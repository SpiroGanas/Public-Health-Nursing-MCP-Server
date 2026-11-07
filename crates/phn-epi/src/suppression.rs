//! Small-cell suppression with complementary suppression.
//!
//! **Primary suppression** hides counts from 1 to `threshold − 1` (and zeros
//! when requested). **Complementary suppression** then hides additional cells
//! so that no suppressed value can be recovered by subtraction from a row or
//! column total: every row and every column of the table that contains a
//! suppressed cell must contain at least two. When totals are part of the
//! table they belong to their row and column; when they are not, the table is
//! still treated as if its margins could be published elsewhere (the
//! conservative choice), unless `complementary` is turned off.
//!
//! Complementary cells are chosen greedily: the smallest non-zero visible
//! interior cell in the offending row or column, then a zero cell, then a
//! total. The same marker is shown for primary and complementary cells so a
//! reader cannot tell them apart.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Options for [`suppress`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuppressionOptions {
    /// Counts from 1 to `threshold − 1` are suppressed.
    pub threshold: u32,
    /// Also suppress zero counts.
    pub suppress_zeros: bool,
    /// Apply complementary suppression.
    pub complementary: bool,
    /// The last column holds row totals.
    pub last_column_is_total: bool,
    /// The last row holds column totals.
    pub last_row_is_total: bool,
}

impl Default for SuppressionOptions {
    fn default() -> Self {
        Self {
            threshold: 5,
            suppress_zeros: false,
            complementary: true,
            last_column_is_total: false,
            last_row_is_total: false,
        }
    }
}

/// Why a cell is hidden or shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CellStatus {
    /// Published as is.
    Shown,
    /// Below the threshold.
    Primary,
    /// Hidden to protect a primary cell.
    Complementary,
    /// No value was supplied.
    Missing,
}

/// The result of suppression for a numeric matrix.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SuppressionResult {
    /// Status of each cell, same shape as the input.
    pub status: Vec<Vec<CellStatus>>,
    /// Number of primary suppressions.
    pub primary: usize,
    /// Number of complementary suppressions.
    pub complementary: usize,
}

impl SuppressionResult {
    /// True when the cell must not be published.
    pub fn is_hidden(&self, r: usize, c: usize) -> bool {
        matches!(
            self.status[r][c],
            CellStatus::Primary | CellStatus::Complementary
        )
    }
}

/// Apply primary and complementary suppression to a matrix of counts.
///
/// `cells[r][c]` is `None` when the cell is empty or not applicable. Rows may
/// have different lengths; missing positions are treated as empty.
pub fn suppress(cells: &[Vec<Option<u64>>], opts: &SuppressionOptions) -> SuppressionResult {
    let nrows = cells.len();
    let ncols = cells.iter().map(Vec::len).max().unwrap_or(0);
    let value = |r: usize, c: usize| cells.get(r).and_then(|row| row.get(c)).copied().flatten();

    let mut status = vec![vec![CellStatus::Missing; ncols]; nrows];
    let mut primary = 0;
    for (r, row) in status.iter_mut().enumerate() {
        for (c, s) in row.iter_mut().enumerate() {
            *s = match value(r, c) {
                None => CellStatus::Missing,
                Some(0) if opts.suppress_zeros => CellStatus::Primary,
                Some(v) if v > 0 && v < u64::from(opts.threshold) => CellStatus::Primary,
                Some(_) => CellStatus::Shown,
            };
            if *s == CellStatus::Primary {
                primary += 1;
            }
        }
    }

    let mut complementary = 0;
    if opts.complementary && primary > 0 {
        let is_total = |r: usize, c: usize| {
            (opts.last_row_is_total && r + 1 == nrows)
                || (opts.last_column_is_total && c + 1 == ncols)
        };
        // Each row and each column is a linear constraint group.
        let mut groups: Vec<Vec<(usize, usize)>> = Vec::new();
        for r in 0..nrows {
            groups.push((0..ncols).map(|c| (r, c)).collect());
        }
        for c in 0..ncols {
            groups.push((0..nrows).map(|r| (r, c)).collect());
        }
        // Iterate to a fixed point; each pass hides at least one cell or stops.
        loop {
            let mut changed = false;
            for g in &groups {
                let present: Vec<(usize, usize)> = g
                    .iter()
                    .copied()
                    .filter(|&(r, c)| status[r][c] != CellStatus::Missing)
                    .collect();
                if present.len() < 2 {
                    continue;
                }
                let hidden = present
                    .iter()
                    .filter(|&&(r, c)| {
                        matches!(
                            status[r][c],
                            CellStatus::Primary | CellStatus::Complementary
                        )
                    })
                    .count();
                if hidden != 1 {
                    continue;
                }
                // Choose the cheapest visible cell to hide.
                let pick = present
                    .iter()
                    .copied()
                    .filter(|&(r, c)| status[r][c] == CellStatus::Shown)
                    .min_by_key(|&(r, c)| {
                        let v = value(r, c).unwrap_or(0);
                        // Order: interior non-zero (by value), interior zero, totals.
                        let class = if is_total(r, c) {
                            2
                        } else if v == 0 {
                            1
                        } else {
                            0
                        };
                        (class, v, r, c)
                    });
                if let Some((r, c)) = pick {
                    status[r][c] = CellStatus::Complementary;
                    complementary += 1;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    SuppressionResult {
        status,
        primary,
        complementary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(rows: &[&[u64]]) -> Vec<Vec<Option<u64>>> {
        rows.iter()
            .map(|r| r.iter().map(|&v| Some(v)).collect())
            .collect()
    }

    /// No row or column may contain exactly one hidden cell.
    fn assert_protected(cells: &[Vec<Option<u64>>], res: &SuppressionResult) {
        let nrows = cells.len();
        let ncols = cells[0].len();
        for r in 0..nrows {
            let n = (0..ncols).filter(|&c| res.is_hidden(r, c)).count();
            assert_ne!(
                n, 1,
                "row {r} has a single hidden cell: {:?}",
                res.status[r]
            );
        }
        for c in 0..ncols {
            let n = (0..nrows).filter(|&r| res.is_hidden(r, c)).count();
            assert_ne!(n, 1, "column {c} has a single hidden cell");
        }
    }

    #[test]
    fn primary_only_without_complementary() {
        let cells = m(&[&[3, 10], &[12, 0]]);
        let res = suppress(
            &cells,
            &SuppressionOptions {
                complementary: false,
                ..Default::default()
            },
        );
        assert_eq!(res.primary, 1);
        assert_eq!(res.complementary, 0);
        assert_eq!(res.status[1][1], CellStatus::Shown); // zero shown by default
    }

    #[test]
    fn complementary_protects_rows_and_columns_with_totals() {
        // Town x year with totals.
        let cells = m(&[
            &[3, 20, 15, 38],
            &[25, 30, 2, 57],
            &[40, 35, 50, 125],
            &[68, 85, 67, 220],
        ]);
        let res = suppress(
            &cells,
            &SuppressionOptions {
                last_column_is_total: true,
                last_row_is_total: true,
                ..Default::default()
            },
        );
        assert_eq!(res.primary, 2);
        assert!(res.complementary >= 2);
        assert_protected(&cells, &res);
        // Totals are only hidden as a last resort.
        assert_eq!(res.status[3][3], CellStatus::Shown);
    }

    #[test]
    fn single_column_hides_next_smallest() {
        let cells = m(&[&[2], &[40], &[9], &[100]]);
        let res = suppress(&cells, &SuppressionOptions::default());
        assert_eq!(res.status[0][0], CellStatus::Primary);
        assert_eq!(res.status[2][0], CellStatus::Complementary);
        assert_eq!(res.status[1][0], CellStatus::Shown);
    }

    #[test]
    fn handles_missing_cells_and_ragged_rows() {
        let cells = vec![vec![Some(1), None], vec![Some(10)], vec![Some(7), Some(8)]];
        let res = suppress(&cells, &SuppressionOptions::default());
        assert_eq!(res.status[0][1], CellStatus::Missing);
        assert_eq!(res.status[1][1], CellStatus::Missing);
        assert_eq!(res.status[0][0], CellStatus::Primary);
        // Column 0 must have a second hidden cell.
        assert!(res.is_hidden(2, 0) || res.is_hidden(1, 0));
    }

    #[test]
    fn randomized_tables_are_always_protected() {
        // Deterministic pseudo-random tables (xorshift) to exercise the fixed point.
        let mut s: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = || {
            s ^= s << 13;
            s ^= s >> 7;
            s ^= s << 17;
            s
        };
        for _ in 0..300 {
            let rows = 2 + (next() % 6) as usize;
            let cols = 2 + (next() % 6) as usize;
            let mut cells: Vec<Vec<Option<u64>>> = (0..rows)
                .map(|_| (0..cols).map(|_| Some(next() % 30)).collect())
                .collect();
            // Append totals.
            for row in cells.iter_mut() {
                let t: u64 = row.iter().map(|v| v.unwrap()).sum();
                row.push(Some(t));
            }
            let totals: Vec<Option<u64>> = (0..=cols)
                .map(|c| Some(cells.iter().map(|r| r[c].unwrap()).sum()))
                .collect();
            cells.push(totals);
            let res = suppress(
                &cells,
                &SuppressionOptions {
                    last_column_is_total: true,
                    last_row_is_total: true,
                    ..Default::default()
                },
            );
            assert_protected(&cells, &res);
        }
    }
}
