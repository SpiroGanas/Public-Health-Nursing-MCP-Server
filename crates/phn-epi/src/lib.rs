//! Epidemiology and biostatistics engine.
//!
//! * [`rates`]: crude and category-specific rates with exact Poisson
//!   confidence intervals (EPI-01).
//! * [`suppression`]: small-cell suppression with complementary suppression
//!   (EPI-06), used automatically by tools that summarize local data.
//! * [`two_by_two`]: 2×2 tables, exact tests, and Mantel–Haenszel analysis (EPI-03).
//! * [`dist`]: distribution helpers.
//!
//! Every calculation is checked against reference values from R or SciPy in
//! `tests/golden.rs`.

pub mod dist;
pub mod rates;
pub mod suppression;
pub mod two_by_two;
