//! Epidemiology and biostatistics engine.
//!
//! * [`rates`]: crude and category-specific rates with exact Poisson
//!   confidence intervals (EPI-01).
//! * [`suppression`]: small-cell suppression with complementary suppression
//!   (EPI-06), used automatically by tools that summarize local data.
//! * [`two_by_two`]: 2×2 tables, exact tests, and Mantel–Haenszel analysis (EPI-03).
//! * [`aberration`]: EARS C1–C3, CUSUM, and Farrington-type (Noufaily)
//!   aberration detection for count series (CD-10).
//! * [`dist`]: distribution helpers.
//!
//! Every calculation is checked against reference values from R or SciPy in
//! `tests/golden.rs`.

pub mod aberration;
pub mod dist;
pub mod rates;
pub mod suppression;
pub mod two_by_two;
