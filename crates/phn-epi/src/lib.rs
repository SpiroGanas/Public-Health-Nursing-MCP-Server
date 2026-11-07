//! Epidemiology and biostatistics engine.
//!
//! * [`rates`]: crude and category-specific rates with exact Poisson
//!   confidence intervals (EPI-01).
//! * [`suppression`]: small-cell suppression with complementary suppression
//!   (EPI-06), used automatically by tools that summarize local data.
//! * [`two_by_two`]: 2×2 tables, exact tests, and Mantel–Haenszel analysis (EPI-03).
//! * [`standardize`]: direct age adjustment (2000 U.S. standard, gamma CIs)
//!   and indirect standardization (SIR/SMR) (EPI-02, EPI-05).
//! * [`glm`]: Gaussian, Poisson, binomial, and negative binomial GLMs (IRLS),
//!   used by trend analysis and regression; [`linalg`] supports it.
//! * [`dist`]: distribution helpers.
//!
//! Every calculation is checked against reference values from R or SciPy in
//! `tests/golden.rs`.

pub mod dist;
pub mod glm;
pub mod linalg;
pub mod rates;
pub mod standardize;
pub mod suppression;
pub mod two_by_two;
