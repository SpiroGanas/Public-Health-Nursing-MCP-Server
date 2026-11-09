//! LMS growth-chart math: WHO Child Growth Standards (birth to 24 months) and the CDC 2000
//! growth charts (2 to 20 years) with the 2022 extended BMI-for-age method.
//!
//! The formulas follow CDC's reference SAS programs (`who-source-code.sas` and
//! `cdc-source-code.sas`, revised 2022) and CDC's data-file pages:
//!
//! * z = ((X/M)^L − 1) / (L·S); X = M·(1 + L·S·z)^(1/L).
//! * WHO: weight, BMI, and weight-for-length z-scores beyond ±3 use the distance between the
//!   2 and 3 SD values (WHO's restricted method); length and head circumference do not.
//! * CDC: when |L| < 0.01 the log form z = ln(X/M)/S is used; modified z-scores (half the
//!   distance between 0 and ±2 z extended to the tails) flag extreme values.
//! * Extended BMI (CDC 2022): above the 95th percentile, percentile = 90 + 10·Φ((BMI − P95)/σ)
//!   with σ from CDC's regression on age in years.
//!
//! Tables are interpolated linearly: WHO by day of age (and by 0.1 cm of length), CDC between
//! the half-month rows, as CDC's SAS programs do.

use phn_core::{PhnError, Result};
use serde::{Deserialize, Serialize};
use statrs::distribution::{ContinuousCDF, Normal};

/// Days per month used by WHO and CDC (365.25 / 12).
pub const DAYS_PER_MONTH: f64 = 30.4375;

/// Child's sex as used by the growth charts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    #[serde(alias = "m", alias = "boy", alias = "M", alias = "Male")]
    Male,
    #[serde(alias = "f", alias = "girl", alias = "F", alias = "Female")]
    Female,
}

impl Sex {
    fn idx(self) -> usize {
        match self {
            Sex::Male => 0,
            Sex::Female => 1,
        }
    }

    /// Plain-language label.
    pub fn label(self) -> &'static str {
        match self {
            Sex::Male => "boy",
            Sex::Female => "girl",
        }
    }

    /// Parse a cell from a local file (M/F, male/female, boy/girl, 1/2).
    pub fn parse(s: &str) -> Option<Sex> {
        match s.trim().to_ascii_lowercase().as_str() {
            "m" | "male" | "boy" | "1" => Some(Sex::Male),
            "f" | "female" | "girl" | "2" => Some(Sex::Female),
            _ => None,
        }
    }
}

/// Box-Cox power (L), median (M), and coefficient of variation (S).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lms {
    pub l: f64,
    pub m: f64,
    pub s: f64,
}

impl Lms {
    /// The exact LMS z-score (L = 0 only when L is exactly zero).
    pub fn z(&self, x: f64) -> f64 {
        if self.l == 0.0 {
            (x / self.m).ln() / self.s
        } else {
            ((x / self.m).powf(self.l) - 1.0) / (self.l * self.s)
        }
    }

    /// CDC's z-score: the log form when |L| < 0.01 (`cdc-source-code.sas`).
    pub fn z_cdc(&self, x: f64) -> f64 {
        if self.l.abs() < 0.01 {
            (x / self.m).ln() / self.s
        } else {
            self.z(x)
        }
    }

    /// The measurement at z-score `z`.
    pub fn value(&self, z: f64) -> f64 {
        if self.l == 0.0 {
            self.m * (self.s * z).exp()
        } else {
            self.m * (1.0 + self.l * self.s * z).powf(1.0 / self.l)
        }
    }

    /// WHO's restricted z-score: beyond ±3, the distance between the 2 and 3 SD values.
    pub fn z_who_restricted(&self, x: f64) -> f64 {
        let z = self.z(x);
        if z >= 3.0 {
            let sd3 = self.value(3.0);
            3.0 + (x - sd3) / (sd3 - self.value(2.0))
        } else if z <= -3.0 {
            let sd3 = self.value(-3.0);
            -3.0 + (x - sd3) / (self.value(-2.0) - sd3)
        } else {
            z
        }
    }

    /// CDC modified z-score for extreme values: half the distance between 0 and ±2 z.
    pub fn modified_z(&self, x: f64) -> f64 {
        let sdl = (self.m - self.m * (1.0 - 2.0 * self.l * self.s).powf(1.0 / self.l)) / 2.0;
        let sdh = (self.m * (1.0 + 2.0 * self.l * self.s).powf(1.0 / self.l) - self.m) / 2.0;
        if x < self.m {
            (x - self.m) / sdl
        } else {
            (x - self.m) / sdh
        }
    }

    fn lerp(a: Lms, b: Lms, f: f64) -> Lms {
        Lms {
            l: a.l + (b.l - a.l) * f,
            m: a.m + (b.m - a.m) * f,
            s: a.s + (b.s - a.s) * f,
        }
    }
}

/// Standard normal CDF.
pub fn phi(z: f64) -> f64 {
    Normal::standard().cdf(z)
}

/// Standard normal quantile.
pub fn phi_inv(p: f64) -> f64 {
    Normal::standard().inverse_cdf(p)
}

/// An LMS table for both sexes, keyed by age (days or months) or length (cm).
#[derive(Debug, Clone, Default)]
pub struct Table {
    rows: [Vec<(f64, Lms)>; 2],
}

impl Table {
    /// The smallest and largest x in the table for `sex`.
    pub fn range(&self, sex: Sex) -> Option<(f64, f64)> {
        let r = &self.rows[sex.idx()];
        Some((r.first()?.0, r.last()?.0))
    }

    /// L, M, S at `x`, interpolated linearly; `None` outside the table.
    pub fn at(&self, sex: Sex, x: f64) -> Option<Lms> {
        let r = &self.rows[sex.idx()];
        if !x.is_finite() || r.is_empty() || x < r[0].0 - 1e-9 || x > r[r.len() - 1].0 + 1e-9 {
            return None;
        }
        let i = r.partition_point(|(k, _)| *k <= x);
        if i == 0 {
            return Some(r[0].1);
        }
        if i >= r.len() {
            return Some(r[r.len() - 1].1);
        }
        let (x0, a) = r[i - 1];
        let (x1, b) = r[i];
        if (x - x0).abs() < 1e-12 {
            return Some(a);
        }
        Some(Lms::lerp(a, b, (x - x0) / (x1 - x0)))
    }

    fn push(&mut self, sex: Sex, x: f64, lms: Lms) {
        self.rows[sex.idx()].push((x, lms));
    }

    fn finish(mut self) -> Self {
        for r in &mut self.rows {
            r.sort_by(|a, b| a.0.total_cmp(&b.0));
        }
        self
    }
}

/// The parsed growth tables.
#[derive(Debug, Clone, Default)]
pub struct GrowthTables {
    /// WHO weight-for-age by age in days (0–731).
    pub who_weight: Table,
    /// WHO length-for-age by age in days.
    pub who_length: Table,
    /// WHO head circumference-for-age by age in days.
    pub who_head: Table,
    /// WHO BMI-for-age by age in days.
    pub who_bmi: Table,
    /// WHO weight-for-length by length in cm (45–110).
    pub who_wfl: Table,
    /// CDC weight-for-age by age in months (24–240).
    pub cdc_weight: Table,
    /// CDC stature-for-age by age in months (24–240).
    pub cdc_stature: Table,
    /// CDC BMI-for-age by age in months (24–240.5).
    pub cdc_bmi: Table,
    /// CDC infant head circumference-for-age by age in months (0–36).
    pub cdc_head: Table,
}

fn parse_f(s: &str, what: &str) -> Result<f64> {
    s.trim()
        .parse::<f64>()
        .map_err(|_| PhnError::Internal(format!("growth table {what}: '{s}' is not a number")))
}

fn csv_rows(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(text.as_bytes());
    let headers: Vec<String> = rdr
        .headers()
        .map_err(|e| PhnError::Internal(format!("growth table: {e}")))?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let mut rows = Vec::new();
    for r in rdr.records() {
        let r = r.map_err(|e| PhnError::Internal(format!("growth table: {e}")))?;
        rows.push(r.iter().map(str::to_string).collect());
    }
    Ok((headers, rows))
}

fn col(headers: &[String], name: &str) -> Result<usize> {
    headers
        .iter()
        .position(|h| h == name)
        .ok_or_else(|| PhnError::Internal(format!("growth table has no column {name}")))
}

fn sex_of(s: &str) -> Result<Sex> {
    match s.trim() {
        "1" => Ok(Sex::Male),
        "2" => Ok(Sex::Female),
        other => Err(PhnError::Internal(format!(
            "growth table: unknown sex code {other}"
        ))),
    }
}

/// Parse a CDC data file (Sex, Agemos, L, M, S, ...).
pub fn parse_cdc(text: &str) -> Result<Table> {
    let (h, rows) = csv_rows(text)?;
    let (cs, ca, cl, cm, css) = (
        col(&h, "sex")?,
        col(&h, "agemos")?,
        col(&h, "l")?,
        col(&h, "m")?,
        col(&h, "s")?,
    );
    let mut t = Table::default();
    for r in rows {
        t.push(
            sex_of(&r[cs])?,
            parse_f(&r[ca], "age")?,
            Lms {
                l: parse_f(&r[cl], "L")?,
                m: parse_f(&r[cm], "M")?,
                s: parse_f(&r[css], "S")?,
            },
        );
    }
    Ok(t.finish())
}

/// Parse the WHO by-age snapshot into weight, length, head, and BMI tables.
fn parse_who_age(text: &str) -> Result<[Table; 4]> {
    let (h, rows) = csv_rows(text)?;
    let cs = col(&h, "sex")?;
    let cd = col(&h, "agedays")?;
    let prefixes = ["wt", "len", "hc", "bmi"];
    let mut idx = Vec::new();
    for p in prefixes {
        idx.push((
            col(&h, &format!("{p}_l"))?,
            col(&h, &format!("{p}_m"))?,
            col(&h, &format!("{p}_s"))?,
        ));
    }
    let mut out: [Table; 4] = Default::default();
    for r in rows {
        let sex = sex_of(&r[cs])?;
        let day = parse_f(&r[cd], "agedays")?;
        for (k, (cl, cm, css)) in idx.iter().enumerate() {
            if r[*cm].trim().is_empty() {
                continue;
            }
            out[k].push(
                sex,
                day,
                Lms {
                    l: parse_f(&r[*cl], "L")?,
                    m: parse_f(&r[*cm], "M")?,
                    s: parse_f(&r[*css], "S")?,
                },
            );
        }
    }
    Ok(out.map(Table::finish))
}

/// Parse the WHO weight-for-length snapshot.
fn parse_who_wfl(text: &str) -> Result<Table> {
    let (h, rows) = csv_rows(text)?;
    let (cs, cx, cl, cm, css) = (
        col(&h, "sex")?,
        col(&h, "length_cm")?,
        col(&h, "l")?,
        col(&h, "m")?,
        col(&h, "s")?,
    );
    let mut t = Table::default();
    for r in rows {
        t.push(
            sex_of(&r[cs])?,
            parse_f(&r[cx], "length")?,
            Lms {
                l: parse_f(&r[cl], "L")?,
                m: parse_f(&r[cm], "M")?,
                s: parse_f(&r[css], "S")?,
            },
        );
    }
    Ok(t.finish())
}

/// Snapshot texts for [`GrowthTables::parse`].
pub struct TableTexts<'a> {
    pub who_by_age: &'a str,
    pub who_wfl: &'a str,
    pub cdc_wtage: &'a str,
    pub cdc_statage: &'a str,
    pub cdc_bmi: &'a str,
    pub cdc_hcageinf: &'a str,
}

impl GrowthTables {
    /// Parse every table.
    pub fn parse(t: &TableTexts<'_>) -> Result<Self> {
        let [who_weight, who_length, who_head, who_bmi] = parse_who_age(t.who_by_age)?;
        Ok(Self {
            who_weight,
            who_length,
            who_head,
            who_bmi,
            who_wfl: parse_who_wfl(t.who_wfl)?,
            cdc_weight: parse_cdc(t.cdc_wtage)?,
            cdc_stature: parse_cdc(t.cdc_statage)?,
            cdc_bmi: parse_cdc(t.cdc_bmi)?,
            cdc_head: parse_cdc(t.cdc_hcageinf)?,
        })
    }
}

/// CDC's extended BMI dispersion parameter σ for sex and age in months.
pub fn extended_sigma(sex: Sex, age_months: f64) -> f64 {
    let y = age_months / 12.0;
    match sex {
        Sex::Male => 0.3728 + 0.5196 * y - 0.0091 * y * y,
        Sex::Female => 0.8334 + 0.3712 * y - 0.0011 * y * y,
    }
}

/// Result of CDC BMI scoring (2000 charts with the 2022 extended method).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BmiScore {
    /// Extended z-score (equal to the LMS z-score at or below the 95th percentile).
    pub z: f64,
    /// Extended percentile (0–100).
    pub percentile: f64,
    /// LMS z-score from the 2000 charts.
    pub lms_z: f64,
    /// LMS percentile from the 2000 charts.
    pub lms_percentile: f64,
    /// True when the extended method was used (BMI above the 95th percentile).
    pub extended: bool,
    /// The 95th percentile BMI for sex and age.
    pub p95: f64,
    /// BMI as a percentage of the 95th percentile.
    pub pct_of_p95: f64,
    /// The median BMI for sex and age.
    pub p50: f64,
    pub sigma: f64,
    pub modified_z: f64,
}

/// Score a BMI with the CDC 2000 LMS parameters and the 2022 extended method.
pub fn cdc_bmi_score(lms: Lms, sex: Sex, age_months: f64, bmi: f64) -> BmiScore {
    let lms_z = lms.z_cdc(bmi);
    let lms_percentile = phi(lms_z) * 100.0;
    let p95 = lms.value(phi_inv(0.95));
    let sigma = extended_sigma(sex, age_months);
    let (z, percentile, extended) = if lms_percentile > 95.0 {
        let p = 90.0 + 10.0 * phi((bmi - p95) / sigma);
        // CDC's SAS program caps z at 8.21 where the quantile function cannot resolve p.
        let z = if p <= 99.999_999_999_999_99 {
            phi_inv(p / 100.0)
        } else {
            8.21
        };
        (if z.is_finite() { z } else { 8.21 }, p, true)
    } else {
        (lms_z, lms_percentile, false)
    };
    BmiScore {
        z,
        percentile,
        lms_z,
        lms_percentile,
        extended,
        p95,
        pct_of_p95: 100.0 * bmi / p95,
        p50: lms.m,
        sigma,
        modified_z: lms.modified_z(bmi),
    }
}

/// BMI at an extended percentile (CDC 2022): the LMS value up to the 95th, then
/// P95 + σ·Φ⁻¹((p − 90)/10) above it.
pub fn cdc_bmi_at_percentile(lms: Lms, sex: Sex, age_months: f64, percentile: f64) -> f64 {
    if percentile <= 95.0 {
        lms.value(phi_inv(percentile / 100.0))
    } else {
        let p95 = lms.value(phi_inv(0.95));
        p95 + extended_sigma(sex, age_months) * phi_inv((percentile - 90.0) / 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cdc_worked_example_9_month_boy() {
        // CDC "Percentile Data Files with LMS Values": a 9-month-old boy weighing 9.7 kg,
        // WTAGEINF L=-0.1600954, M=9.476500305, S=0.11218624 -> z = 0.207 (58th
        // percentile); the 5th percentile is 7.90 kg.
        let lms = Lms {
            l: -0.1600954,
            m: 9.476500305,
            s: 0.11218624,
        };
        assert!((lms.z(9.7) - 0.207).abs() < 0.0005);
        assert_eq!((phi(lms.z(9.7)) * 100.0).round(), 58.0);
        assert!((lms.value(-1.645) - 7.90).abs() < 0.005);
    }

    #[test]
    fn who_restricted_z_is_continuous_at_three() {
        let lms = Lms {
            l: 0.3487,
            m: 3.3464,
            s: 0.14602,
        };
        let sd3 = lms.value(3.0);
        assert!((lms.z_who_restricted(sd3) - 3.0).abs() < 1e-9);
        assert!(lms.z_who_restricted(sd3 + 0.5) > 3.0);
        let sdm3 = lms.value(-3.0);
        assert!((lms.z_who_restricted(sdm3) + 3.0).abs() < 1e-9);
    }

    #[test]
    fn interpolation_between_rows() {
        let mut t = Table::default();
        t.push(
            Sex::Male,
            0.0,
            Lms {
                l: 1.0,
                m: 10.0,
                s: 0.1,
            },
        );
        t.push(
            Sex::Male,
            2.0,
            Lms {
                l: 3.0,
                m: 20.0,
                s: 0.3,
            },
        );
        let t = t.finish();
        let m = t.at(Sex::Male, 1.0).unwrap();
        assert!((m.l - 2.0).abs() < 1e-12 && (m.m - 15.0).abs() < 1e-12);
        assert!(t.at(Sex::Male, 2.5).is_none());
        assert!(t.at(Sex::Female, 1.0).is_none());
    }
}
