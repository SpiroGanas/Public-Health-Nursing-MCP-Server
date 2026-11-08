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
//! * [`trend`]: percent change, joinpoint-style segmented regression,
//!   interrupted time series, seasonality, Mann–Kendall (EPI-09).
//! * [`sample_size`]: survey, two-group, and cluster sample size and power (EPI-04).
//! * [`hypothesis`]: t, rank, ANOVA, chi-square, correlation, normality, and
//!   equal-variance tests (EPI-10).
//! * [`spc`]: run charts (Anhøj rules) and XmR, p, u, c control charts (EBP-06).
//! * [`dist`]: distribution helpers.
//!
//! Every calculation is checked against reference values from R or SciPy in
//! `tests/golden.rs`.

pub mod dist;
pub mod glm;
pub mod hypothesis;
pub mod linalg;
pub mod rates;
pub mod sample_size;
pub mod spc;
pub mod standardize;
pub mod suppression;
pub mod trend;
pub mod two_by_two;
