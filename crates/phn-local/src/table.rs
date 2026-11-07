//! Tabular files: CSV/TSV (any common delimiter and encoding) and XLSX/XLS/ODS.

use std::path::Path;

use calamine::{Data, Reader};
use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Hard limit on rows read from one sheet.
pub const MAX_ROWS: usize = 1_000_000;

/// A rectangular table of text cells.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Table {
    /// Sheet name (spreadsheets) or file name (CSV).
    pub name: String,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// True when the file had more rows than [`MAX_ROWS`].
    pub truncated: bool,
}

impl Table {
    /// Number of data rows.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// Values of one column.
    pub fn column(&self, index: usize) -> impl Iterator<Item = &str> {
        self.rows
            .iter()
            .map(move |r| r.get(index).map(String::as_str).unwrap_or(""))
    }

    /// Index of a column by case-insensitive header name.
    pub fn column_index(&self, name: &str) -> Option<usize> {
        self.headers
            .iter()
            .position(|h| h.trim().eq_ignore_ascii_case(name.trim()))
    }
}

/// File types read as tables.
pub fn is_tabular(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "csv" | "tsv" | "txt" | "xlsx" | "xlsm" | "xls" | "ods"
    )
}

/// Lower-case file extension.
pub fn extension(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default()
}

/// Read every table in a file (one per sheet for spreadsheets).
pub fn read_tables(path: &Path) -> Result<Vec<Table>> {
    match extension(path).as_str() {
        "xlsx" | "xlsm" | "xls" | "ods" => read_spreadsheet(path),
        _ => Ok(vec![read_delimited(path)?]),
    }
}

/// Read one table: the named sheet, or the first non-empty sheet.
pub fn read_table(path: &Path, sheet: Option<&str>) -> Result<Table> {
    let tables = read_tables(path)?;
    if let Some(name) = sheet {
        return tables
            .into_iter()
            .find(|t| t.name.eq_ignore_ascii_case(name))
            .ok_or_else(|| PhnError::NotFound(format!("sheet '{name}' in {}", path.display())));
    }
    let first_nonempty = tables.iter().position(|t| !t.headers.is_empty());
    match first_nonempty {
        Some(i) => Ok(tables.into_iter().nth(i).expect("index exists")),
        None => Err(PhnError::parse(path, "no data found")),
    }
}

/// Decode bytes as UTF-8 (dropping a BOM) or, failing that, Windows-1252,
/// which is what Excel on Windows writes for "CSV" files.
pub fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => {
            let (cow, _, _) = encoding_rs::WINDOWS_1252.decode(bytes);
            cow.into_owned()
        }
    }
}

/// Guess the delimiter from the first lines.
fn sniff_delimiter(text: &str) -> u8 {
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(10)
        .collect();
    let mut best = (b',', 0usize);
    for d in *b",\t;|" {
        let counts: Vec<usize> = lines
            .iter()
            .map(|l| l.bytes().filter(|&b| b == d).count())
            .collect();
        let Some(&first) = counts.first() else {
            continue;
        };
        if first == 0 {
            continue;
        }
        let consistent = counts.iter().filter(|&&c| c == first).count();
        let score = consistent * 100 + first;
        if score > best.1 {
            best = (d, score);
        }
    }
    best.0
}

fn read_delimited(path: &Path) -> Result<Table> {
    let bytes = std::fs::read(path)?;
    let text = decode_text(&bytes);
    let delimiter = if extension(path) == "tsv" {
        b'\t'
    } else {
        sniff_delimiter(&text)
    };
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .flexible(true)
        .has_headers(false)
        .from_reader(text.as_bytes());
    let mut records = rdr.records();
    let headers: Vec<String> = match records.next() {
        Some(r) => r
            .map_err(|e| PhnError::parse(path, e))?
            .iter()
            .map(|s| s.trim().to_string())
            .collect(),
        None => Vec::new(),
    };
    let headers = clean_headers(headers);
    let width = headers.len();
    let mut rows = Vec::new();
    let mut truncated = false;
    for rec in records {
        let rec = rec.map_err(|e| PhnError::parse(path, e))?;
        if rec.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if rows.len() >= MAX_ROWS {
            truncated = true;
            break;
        }
        let mut row: Vec<String> = rec.iter().map(|s| s.trim().to_string()).collect();
        row.resize(width, String::new());
        rows.push(row);
    }
    Ok(Table {
        name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        headers,
        rows,
        truncated,
    })
}

fn read_spreadsheet(path: &Path) -> Result<Vec<Table>> {
    let mut wb = calamine::open_workbook_auto(path).map_err(|e| PhnError::parse(path, e))?;
    let mut out = Vec::new();
    for name in wb.sheet_names().to_owned() {
        let range = wb
            .worksheet_range(&name)
            .map_err(|e| PhnError::parse(path, e))?;
        let mut rows_iter = range
            .rows()
            .map(|r| r.iter().map(cell_to_string).collect::<Vec<String>>())
            .skip_while(|r| r.iter().all(|c| c.is_empty()));
        let headers = clean_headers(rows_iter.next().unwrap_or_default());
        let width = headers.len();
        let mut rows = Vec::new();
        let mut truncated = false;
        for mut r in rows_iter {
            if r.iter().all(|c| c.is_empty()) {
                continue;
            }
            if rows.len() >= MAX_ROWS {
                truncated = true;
                break;
            }
            r.resize(width, String::new());
            rows.push(r);
        }
        out.push(Table {
            name,
            headers,
            rows,
            truncated,
        });
    }
    Ok(out)
}

/// Convert a spreadsheet cell to text. Dates become ISO 8601.
fn cell_to_string(d: &Data) -> String {
    match d {
        Data::Empty => String::new(),
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) => {
            if f.fract() == 0.0 && f.abs() < 1e15 {
                format!("{}", *f as i64)
            } else {
                format!("{f}")
            }
        }
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => excel_serial_to_iso(dt.as_f64()),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Error(e) => format!("#{e:?}"),
        #[allow(unreachable_patterns)]
        _ => String::new(),
    }
}

/// Excel serial date (1900 system) to `YYYY-MM-DD` or `YYYY-MM-DDTHH:MM`.
pub fn excel_serial_to_iso(serial: f64) -> String {
    let base = chrono::NaiveDate::from_ymd_opt(1899, 12, 30).expect("valid date");
    let days = serial.floor() as i64;
    let date = base + chrono::Duration::days(days);
    let secs = ((serial - serial.floor()) * 86_400.0).round() as i64;
    if secs == 0 {
        date.format("%Y-%m-%d").to_string()
    } else {
        format!(
            "{}T{:02}:{:02}",
            date.format("%Y-%m-%d"),
            secs / 3600,
            (secs % 3600) / 60
        )
    }
}

/// Fill blank headers and make duplicates unique.
fn clean_headers(raw: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashMap::new();
    raw.into_iter()
        .enumerate()
        .map(|(i, h)| {
            let base = if h.trim().is_empty() {
                format!("column_{}", i + 1)
            } else {
                h.trim().to_string()
            };
            let n = seen.entry(base.to_lowercase()).or_insert(0);
            *n += 1;
            if *n == 1 { base } else { format!("{base}_{n}") }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_semicolon_csv_with_bom_and_windows_1252() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.csv");
        let mut bytes = b"\xEF\xBB\xBFTown;Cases;\n".to_vec();
        bytes.extend_from_slice(b"Amherst;3;x\n\n;;\nBoston;10\n");
        std::fs::write(&p, &bytes).unwrap();
        let t = read_table(&p, None).unwrap();
        assert_eq!(t.headers, vec!["Town", "Cases", "column_3"]);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[1], vec!["Boston", "10", ""]);

        let p2 = dir.path().join("b.csv");
        std::fs::write(&p2, b"Name,City\nJos\xe9,Lowell\n").unwrap();
        let t2 = read_table(&p2, None).unwrap();
        assert_eq!(t2.rows[0][0], "José");
    }

    #[test]
    fn excel_dates_and_duplicate_headers() {
        assert_eq!(excel_serial_to_iso(45292.0), "2024-01-01");
        assert_eq!(excel_serial_to_iso(45292.5), "2024-01-01T12:00");
        assert_eq!(
            clean_headers(vec!["Date".into(), "date".into(), "".into()]),
            vec!["Date", "date_2", "column_3"]
        );
    }

    #[test]
    fn reads_xlsx_written_by_rust_xlsxwriter() {
        // A tiny workbook built in code keeps the test hermetic.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("w.xlsx");
        let mut wb = rust_xlsxwriter::Workbook::new();
        let ws = wb.add_worksheet().set_name("Cases").unwrap();
        ws.write_string(0, 0, "Town").unwrap();
        ws.write_string(0, 1, "Count").unwrap();
        ws.write_string(1, 0, "Amherst").unwrap();
        ws.write_number(1, 1, 4.0).unwrap();
        wb.save(&p).unwrap();
        let t = read_table(&p, Some("cases")).unwrap();
        assert_eq!(t.headers, vec!["Town", "Count"]);
        assert_eq!(t.rows[0], vec!["Amherst", "4"]);
    }
}
