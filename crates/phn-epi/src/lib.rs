//! Epidemiology and biostatistics engine.
//!
//! * [`rates`]: crude and category-specific rates with exact Poisson
//!   confidence intervals (EPI-01).
//! * [`suppression`]: small-cell suppression with complementary suppression
//!   (EPI-06), used automatically by tools that summarize local data.
//!
//! Later phases add age adjustment, 2x2 tables, trend tests, and SPC.

pub mod rates;
pub mod suppression;
