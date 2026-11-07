//! CSV writer for one table.

use phn_core::{PhnError, Result};

use crate::model::{Document, TableBlock};

/// Write a table as UTF-8 CSV with a byte-order mark so Excel opens it correctly.
pub fn table_to_csv(t: &TableBlock) -> Result<Vec<u8>> {
    let mut w = csv::WriterBuilder::new()
        .flexible(true)
        .from_writer(vec![0xEF, 0xBB, 0xBF]);
    let cerr = |e: csv::Error| PhnError::Internal(format!("CSV: {e}"));
    if !t.headers.is_empty() {
        w.write_record(&t.headers).map_err(cerr)?;
    }
    for r in &t.rows {
        w.write_record(r).map_err(cerr)?;
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
