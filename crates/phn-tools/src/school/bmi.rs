//! BMI-for-age percentiles and weight categories from the CDC growth chart
//! reference (`content/school/cdc_bmi_for_age.yaml`, LMS method).
//!
//! `z = ((BMI / M)^L - 1) / (L * S)` and the percentile is the standard
//! normal CDF of `z`, with L, M, and S interpolated linearly between CDC's
//! half-month rows. Categories compare the BMI with CDC's published 5th, 85th,
//! and 95th percentiles for the same sex and age.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::function::erf::erf;

use crate::school::content::BMI_REF;

/// Sex as recorded for growth charts.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Sex {
    Male,
    Female,
}

impl Sex {
    /// Parse "M", "male", "boy", "F", "female", "girl", "1", "2".
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "m" | "male" | "boy" | "b" | "1" | "man" => Some(Sex::Male),
            "f" | "female" | "girl" | "g" | "2" | "woman" => Some(Sex::Female),
            _ => None,
        }
    }

    fn code(self) -> f64 {
        match self {
            Sex::Male => 1.0,
            Sex::Female => 2.0,
        }
    }

    /// Label.
    pub fn label(self) -> &'static str {
        match self {
            Sex::Male => "male",
            Sex::Female => "female",
        }
    }
}

/// CDC weight status category.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum BmiCategory {
    /// Below the 5th percentile.
    Underweight,
    /// 5th to below the 85th percentile.
    HealthyWeight,
    /// 85th to below the 95th percentile.
    Overweight,
    /// 95th percentile or above.
    Obese,
}

impl BmiCategory {
    /// Label.
    pub fn label(self) -> &'static str {
        match self {
            BmiCategory::Underweight => "underweight",
            BmiCategory::HealthyWeight => "healthy weight",
            BmiCategory::Overweight => "overweight",
            BmiCategory::Obese => "obese",
        }
    }

    /// All categories in order.
    pub const ALL: [BmiCategory; 4] = [
        BmiCategory::Underweight,
        BmiCategory::HealthyWeight,
        BmiCategory::Overweight,
        BmiCategory::Obese,
    ];

    /// Parse a category written in a file.
    pub fn parse(s: &str) -> Option<Self> {
        let t = phn_core::text::normalize(s);
        if t.contains("under") {
            Some(BmiCategory::Underweight)
        } else if t.contains("healthy") || t.contains("normal") {
            Some(BmiCategory::HealthyWeight)
        } else if t.contains("over") {
            Some(BmiCategory::Overweight)
        } else if t.contains("obes") {
            Some(BmiCategory::Obese)
        } else {
            None
        }
    }

    /// Category for a percentile (0-100).
    pub fn from_percentile(p: f64) -> Self {
        if p < 5.0 {
            BmiCategory::Underweight
        } else if p < 85.0 {
            BmiCategory::HealthyWeight
        } else if p < 95.0 {
            BmiCategory::Overweight
        } else {
            BmiCategory::Obese
        }
    }
}

/// A computed BMI result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BmiResult {
    pub bmi: f64,
    pub percentile: f64,
    pub category: BmiCategory,
}

/// Interpolated [L, M, S, P5, P85, P95] for a sex and age in months.
fn params(sex: Sex, age_months: f64) -> Option<[f64; 6]> {
    let rows: Vec<&[f64; 8]> = BMI_REF.rows.iter().filter(|r| r[0] == sex.code()).collect();
    let first = rows.first()?;
    let last = rows.last()?;
    if age_months < first[1] || age_months > last[1] + 0.5 {
        return None;
    }
    let pick = |r: &[f64; 8]| [r[2], r[3], r[4], r[5], r[6], r[7]];
    for w in rows.windows(2) {
        let (a, b) = (w[0], w[1]);
        if age_months >= a[1] && age_months <= b[1] {
            let t = if b[1] > a[1] {
                (age_months - a[1]) / (b[1] - a[1])
            } else {
                0.0
            };
            let pa = pick(a);
            let pb = pick(b);
            let mut out = [0.0; 6];
            for i in 0..6 {
                out[i] = pa[i] + t * (pb[i] - pa[i]);
            }
            return Some(out);
        }
    }
    Some(pick(last))
}

fn phi(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

/// BMI percentile (0-100) for a BMI, sex, and age in months (24 to 240.5).
pub fn percentile(sex: Sex, age_months: f64, bmi: f64) -> Option<f64> {
    if !(bmi.is_finite() && bmi > 0.0) {
        return None;
    }
    let [l, m, s, ..] = params(sex, age_months)?;
    let z = if l.abs() < 1e-12 {
        (bmi / m).ln() / s
    } else {
        ((bmi / m).powf(l) - 1.0) / (l * s)
    };
    Some(100.0 * phi(z))
}

/// BMI, percentile, and category. Height in inches and weight in pounds
/// unless `metric` (centimeters and kilograms).
pub fn evaluate(
    sex: Sex,
    age_months: f64,
    height: f64,
    weight: f64,
    metric: bool,
) -> Option<BmiResult> {
    if !(height > 0.0 && weight > 0.0) {
        return None;
    }
    let bmi = if metric {
        weight / (height / 100.0).powi(2)
    } else {
        703.0 * weight / height.powi(2)
    };
    from_bmi(sex, age_months, bmi)
}

/// Percentile and category for a BMI value.
pub fn from_bmi(sex: Sex, age_months: f64, bmi: f64) -> Option<BmiResult> {
    let p = percentile(sex, age_months, bmi)?;
    let [_, _, _, p5, p85, p95] = params(sex, age_months)?;
    let category = if bmi < p5 {
        BmiCategory::Underweight
    } else if bmi < p85 {
        BmiCategory::HealthyWeight
    } else if bmi < p95 {
        BmiCategory::Overweight
    } else {
        BmiCategory::Obese
    };
    Some(BmiResult {
        bmi,
        percentile: p,
        category,
    })
}

/// Age in months (with fraction) between two dates, as CDC computes it
/// (days / 30.4375).
pub fn age_months(dob: chrono::NaiveDate, on: chrono::NaiveDate) -> f64 {
    (on - dob).num_days() as f64 / 30.4375
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Golden test against CDC's own table: a BMI equal to CDC's published
    /// 5th, 85th, or 95th percentile value must give that percentile.
    #[test]
    fn lms_reproduces_cdc_published_percentiles() {
        let mut checked = 0;
        for r in BMI_REF.rows.iter().step_by(17) {
            let sex = if r[0] == 1.0 { Sex::Male } else { Sex::Female };
            for (bmi, want) in [(r[5], 5.0), (r[6], 85.0), (r[7], 95.0)] {
                let p = percentile(sex, r[1], bmi).unwrap();
                assert!(
                    (p - want).abs() < 0.05,
                    "sex {} age {} bmi {bmi}: {p} vs {want}",
                    r[0],
                    r[1]
                );
                checked += 1;
            }
        }
        assert!(checked > 60);
    }

    #[test]
    fn categories_and_units() {
        // A 10-year-old boy, 54 in, 70 lb: BMI 16.9 (healthy weight).
        let r = evaluate(Sex::Male, 120.0, 54.0, 70.0, false).unwrap();
        assert!((r.bmi - 16.876).abs() < 0.01);
        assert_eq!(r.category, BmiCategory::HealthyWeight);
        // Same child in metric units.
        let m = evaluate(Sex::Male, 120.0, 137.16, 31.7515, true).unwrap();
        assert!((m.bmi - r.bmi).abs() < 0.01);
        // 30 BMI at 10 years is obese; 13 is underweight.
        assert_eq!(
            from_bmi(Sex::Female, 120.0, 30.0).unwrap().category,
            BmiCategory::Obese
        );
        assert_eq!(
            from_bmi(Sex::Female, 120.0, 13.0).unwrap().category,
            BmiCategory::Underweight
        );
        assert!(percentile(Sex::Male, 12.0, 17.0).is_none());
        assert_eq!(
            BmiCategory::parse("Healthy Weight"),
            Some(BmiCategory::HealthyWeight)
        );
        assert_eq!(BmiCategory::from_percentile(85.0), BmiCategory::Overweight);
    }
}
