//! CSV writer for one table.

use phn_core::{PhnError, Result};

use crate::model::{Document, TableBlock};

/// True for a number as a spreadsheet reads it: an optional sign, digits with
/// optional thousands separators and decimal point, and an optional percent sign.
fn is_plain_number(s: &str) -> bool {
    let body = s.strip_prefix(['+', '-']).unwrap_or(s);
    let body = body.strip_suffix('%').unwrap_or(body);
    body.bytes().any(|b| b.is_ascii_digit())
        && body
            .bytes()
            .all(|b| b.is_ascii_digit() || b == b'.' || b == b',')
}

/// Keep a spreadsheet from running a cell as a formula (CSV injection): text
/// that starts with `=`, `+`, `-`, `@`, a tab, or a carriage return gets a
/// leading apostrophe. Numbers, including negative numbers, are left alone.
fn neutralize(cell: &str) -> String {
    let risky = cell.len() > 1 && cell.starts_with(['=', '+', '-', '@', '\t', '\r']);
    if risky && !is_plain_number(cell.trim()) {
        format!("'{cell}")
    } else {
        cell.to_string()
    }
}

/// The same protection for code that writes its own CSV: text a spreadsheet would run as a
/// formula gets a leading apostrophe; plain numbers are unchanged.
pub fn neutralize_cell(cell: &str) -> String {
    neutralize(cell)
}

/// Write a table as UTF-8 CSV with a byte-order mark so Excel opens it correctly.
///
/// Cells that a spreadsheet would read as formulas are written as text, with
/// a leading apostrophe; numbers are unchanged.
pub fn table_to_csv(t: &TableBlock) -> Result<Vec<u8>> {
    let mut w = csv::WriterBuilder::new()
        .flexible(true)
        .from_writer(vec![0xEF, 0xBB, 0xBF]);
    let cerr = |e: csv::Error| PhnError::Internal(format!("CSV: {e}"));
    if !t.headers.is_empty() {
        w.write_record(t.headers.iter().map(|c| neutralize(c)))
            .map_err(cerr)?;
    }
    for r in &t.rows {
        w.write_record(r.iter().map(|c| neutralize(c)))
            .map_err(cerr)?;
    }
    w.into_inner()
        .map_err(|e| PhnError::Internal(format!("CSV: {e}")))
}

/// Export the document's only table (or the `index`-th, 1-based) as CSV.
pub fn to_csv(doc: &Document, index: Option<usize>) -> Result<Vec<u8>> {
    let tables = doc.tables();
    let t = match (tables.len(), index) {
        (0, _) => {
            return Err(PhnError::InvalidInput(
                "CSV export needs a table in the document (a Markdown table)".into(),
            ));
        }
        (_, Some(i)) => *tables.get(i.saturating_sub(1)).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "the document has {} table(s); table {i} does not exist",
                tables.len()
            ))
        })?,
        (1, None) => tables[0],
        (n, None) => {
            return Err(PhnError::InvalidInput(format!(
                "the document has {n} tables; choose one with table_index, or export to XLSX"
            )));
        }
    };
    table_to_csv(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_like_cells_are_written_as_text() {
        let t = TableBlock {
            caption: None,
            headers: vec!["Town".into(), "Change".into()],
            rows: vec![
                vec!["=HYPERLINK(\"http://example.org\")".into(), "-5".into()],
                vec!["@SUM(A1)".into(), "+1-508-555-0100".into()],
                vec!["Amherst".into(), "-1,234.5".into()],
                vec!["-".into(), "+12%".into()],
            ],
        };
        let bytes = table_to_csv(&t).unwrap();
        let text = String::from_utf8(bytes[3..].to_vec()).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines[0], "Town,Change");
        assert_eq!(lines[1], "\"'=HYPERLINK(\"\"http://example.org\"\")\",-5");
        assert_eq!(lines[2], "'@SUM(A1),'+1-508-555-0100");
        assert_eq!(lines[3], "Amherst,\"-1,234.5\"");
        assert_eq!(lines[4], "-,+12%");
    }
}
