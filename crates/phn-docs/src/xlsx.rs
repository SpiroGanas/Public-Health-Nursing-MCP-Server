//! XLSX writer: each table becomes a worksheet holding an Excel table (which
//! screen readers announce with its column headers), with document
//! properties and the letterhead in the print header.

use phn_core::{PhnError, Result};
use rust_xlsxwriter::{DocProperties, Format, Table, TableColumn, Workbook};

use crate::model::{Block, Document};

fn sheet_name(base: &str, used: &mut Vec<String>) -> String {
    let mut clean: String = base
        .chars()
        .filter(|c| !matches!(c, '[' | ']' | ':' | '*' | '?' | '/' | '\\'))
        .collect();
    let edge = |c: char| c == '\'' || c.is_whitespace();
    // Excel's limit is 31 characters (not bytes); 28 leaves room for " 99".
    clean = clean.trim_matches(edge).chars().take(28).collect();
    clean = clean.trim_matches(edge).to_string();
    if clean.is_empty() {
        clean = "Sheet".into();
    }
    let mut name = clean.clone();
    let mut n = 2;
    while used.iter().any(|u| u.eq_ignore_ascii_case(&name)) {
        name = format!("{clean} {n}");
        n += 1;
    }
    used.push(name.clone());
    name
}

/// Excel tables need column names that differ (ignoring case), so repeated
/// headers such as "n", "%", "n", "%" get a number: "n (2)", "% (2)".
fn unique_headers(headers: &mut [String]) {
    let mut seen: Vec<String> = Vec::new();
    for h in headers.iter_mut() {
        let base = h.clone();
        let mut n = 2;
        while seen.contains(&h.to_lowercase()) {
            *h = format!("{base} ({n})");
            n += 1;
        }
        seen.push(h.to_lowercase());
    }
}

/// The number a cell holds, when its text is a plain number: digits with an
/// optional minus sign, decimal part, and thousands separators. Codes stay
/// text: leading zeros (ZIP codes), lists such as "1,2", exponent forms such
/// as "1E5", and identifiers longer than 15 digits.
fn as_number(v: &str) -> Option<f64> {
    let body = v.strip_prefix('-').unwrap_or(v);
    let (int, frac) = match body.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (body, None),
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let int_ok = if int.is_empty() {
        frac.is_some()
    } else if int.contains(',') {
        let mut groups = int.split(',');
        let first = groups.next().unwrap_or("");
        all_digits(first) && first.len() <= 3 && groups.all(|g| g.len() == 3 && all_digits(g))
    } else {
        all_digits(int)
    };
    if !int_ok || frac.is_some_and(|f| !all_digits(f)) {
        return None;
    }
    let digits = int.bytes().filter(|b| b.is_ascii_digit()).count() + frac.map_or(0, str::len);
    if digits > 15 || (int.len() > 1 && int.starts_with('0')) {
        return None;
    }
    v.replace(',', "").parse::<f64>().ok()
}

fn xerr(e: rust_xlsxwriter::XlsxError) -> PhnError {
    PhnError::Internal(format!("XLSX: {e}"))
}

/// Render the document's tables (and its text, on a "Notes" sheet) as XLSX.
pub fn to_xlsx(doc: &Document) -> Result<Vec<u8>> {
    let tables = doc.tables();
    if tables.is_empty() {
        return Err(PhnError::InvalidInput(
            "XLSX export needs at least one table in the document (a Markdown table)".into(),
        ));
    }
    let mut wb = Workbook::new();
    let mut props = DocProperties::new().set_title(&doc.title);
    if let Some(l) = &doc.letterhead {
        props = props.set_company(&l.agency_name).set_author(&l.agency_name);
    }
    wb.set_properties(&props);
    let header = doc
        .letterhead
        .as_ref()
        .map(|l| {
            format!(
                "&L{}&R{}",
                l.agency_name.replace('&', "&&"),
                doc.title.replace('&', "&&")
            )
        })
        .unwrap_or_else(|| format!("&L{}", doc.title.replace('&', "&&")));
    let footer = "&LPage &P of &N&R&D";
    let wrap = Format::new().set_text_wrap();
    let mut used = Vec::new();

    for (i, t) in tables.iter().enumerate() {
        let name = sheet_name(
            t.caption.as_deref().unwrap_or(&format!("Table {}", i + 1)),
            &mut used,
        );
        let ws = wb.add_worksheet();
        ws.set_name(&name).map_err(xerr)?;
        ws.set_header(&header);
        ws.set_footer(footer);
        let cols = t
            .headers
            .len()
            .max(t.rows.iter().map(Vec::len).max().unwrap_or(0))
            .max(1);
        let mut headers: Vec<String> = (0..cols)
            .map(|c| {
                t.headers
                    .get(c)
                    .filter(|h| !h.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| format!("Column {}", c + 1))
            })
            .collect();
        unique_headers(&mut headers);
        for (c, h) in headers.iter().enumerate() {
            ws.write_string(0, c as u16, h).map_err(xerr)?;
        }
        for (r, row) in t.rows.iter().enumerate() {
            for c in 0..cols {
                let v = row.get(c).map(String::as_str).unwrap_or("");
                let rr = (r + 1) as u32;
                // Write plain numbers as numbers (but keep codes like ZIPs
                // with leading zeros, lists, and long identifiers as text).
                match as_number(v) {
                    Some(n) => {
                        ws.write_number(rr, c as u16, n).map_err(xerr)?;
                    }
                    None => {
                        ws.write_string_with_format(rr, c as u16, v, &wrap)
                            .map_err(xerr)?;
                    }
                }
            }
        }
        let columns: Vec<TableColumn> = headers
            .iter()
            .map(|h| TableColumn::new().set_header(h))
            .collect();
        let table = Table::new()
            .set_columns(&columns)
            .set_name(format!("Table{}", i + 1));
        let last_row = t.rows.len().max(1) as u32;
        ws.add_table(0, 0, last_row, (cols - 1) as u16, &table)
            .map_err(xerr)?;
        ws.set_freeze_panes(1, 0).map_err(xerr)?;
        ws.set_autofit_max_width(400);
        ws.autofit();
    }

    // Text content goes on a Notes sheet so nothing is lost.
    let notes: Vec<String> = doc
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Heading { text, .. } => Some(text.clone()),
            Block::Paragraph(r) => Some(r.iter().map(|i| i.text.as_str()).collect()),
            Block::List { items, .. } => Some(
                items
                    .iter()
                    .map(|i| {
                        format!(
                            "• {}",
                            i.iter().map(|x| x.text.as_str()).collect::<String>()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        })
        .collect();
    if !notes.is_empty() {
        let name = sheet_name("Notes", &mut used);
        let ws = wb.add_worksheet();
        ws.set_name(&name).map_err(xerr)?;
        ws.write_string(0, 0, &doc.title).map_err(xerr)?;
        for (i, n) in notes.iter().enumerate() {
            ws.write_string_with_format((i + 2) as u32, 0, n, &wrap)
                .map_err(xerr)?;
        }
        ws.set_column_width(0, 100).map_err(xerr)?;
    }
    wb.save_to_buffer().map_err(xerr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::from_markdown;

    #[test]
    fn writes_tables_that_read_back() {
        let doc = Document {
            title: "Cases".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: None,
            logo: None,
            blocks: from_markdown(
                "Intro text.\n\nTable: By town\n\n| Town | Cases | ZIP |\n|---|---|---|\n| Amherst | 3 | 01002 |\n",
            ),
        };
        let bytes = to_xlsx(&doc).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("x.xlsx");
        std::fs::write(&p, bytes).unwrap();
        let t = phn_local::table::read_table(&p, Some("By town")).unwrap();
        assert_eq!(t.headers, vec!["Town", "Cases", "ZIP"]);
        assert_eq!(t.rows[0], vec!["Amherst", "3", "01002"]);
        assert!(phn_local::table::read_table(&p, Some("Notes")).is_ok());
    }

    #[test]
    fn sheet_names_are_cut_by_characters_not_bytes() {
        let mut used = Vec::new();
        // Byte 28 of each caption falls inside the dash.
        for caption in [
            "Immunization rates by town — 2025",
            "Cases by school and grade — fall 2025",
        ] {
            let name = sheet_name(caption, &mut used);
            assert!(name.chars().count() <= 31, "{name}");
            assert!(!name.ends_with(' ') && !name.ends_with('\''));
        }
        assert_eq!(sheet_name("'Quoted' ", &mut used), "Quoted");
    }

    #[test]
    fn repeated_headers_are_numbered() {
        let mut h: Vec<String> = ["Town", "n", "%", "N", "%"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        unique_headers(&mut h);
        assert_eq!(h, vec!["Town", "n", "%", "N (2)", "% (2)"]);
        let doc = Document {
            title: "Cases".into(),
            subtitle: None,
            language: "en-US".into(),
            author: None,
            letterhead: None,
            logo: None,
            blocks: from_markdown("| Town | n | % | n | % |\n|---|---|---|---|---|\n| Amherst | 3 | 1.5 | 4 | 2.0 |\n"),
        };
        assert!(to_xlsx(&doc).is_ok());
    }

    #[test]
    fn only_plain_numbers_become_numbers() {
        for (text, number) in [
            ("3", 3.0),
            ("-5", -5.0),
            ("0", 0.0),
            ("0.5", 0.5),
            (".5", 0.5),
            ("1,234", 1234.0),
            ("-1,234,567.5", -1_234_567.5),
        ] {
            assert_eq!(as_number(text), Some(number), "{text}");
        }
        for text in [
            "", "-", "01002", "1,2", "5,10", "1E5", "12e3", "NaN", "inf", "+15085550100", "12.5%",
            "1234567890123456", "2026-11-08",
        ] {
            assert_eq!(as_number(text), None, "{text}");
        }
    }
}
