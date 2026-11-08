//! Direct and indirect standardization.
//!
//! * Direct age adjustment to the 2000 U.S. standard population with the
//!   gamma-method confidence interval of Fay and Feuer (Stat Med
//!   1997;16:791–801), as used by NCHS and R `epitools::ageadjust.direct`.
//! * Indirect standardization (standardized incidence or mortality ratio,
//!   SIR/SMR) with the exact Poisson interval for the observed count and the
//!   exact two-sided p-value of R `poisson.test`.
//!
//! The standard population is the U.S. Census Bureau's 2000 projection
//! (P25-1130) by single year of age, published by the NCI SEER program
//! (<https://seer.cancer.gov/stdpopulations/stdpop.singleages.html>,
//! retrieved 2026-11-07). Aggregating it to any whole-year age groups gives
//! exactly the NCHS standard weights (for example the 11-group and 19-group
//! "standard million"), which a test checks against SEER's 19-group file.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use statrs::distribution::{ContinuousCDF, Discrete, DiscreteCDF, Gamma, Poisson};

use crate::rates::poisson_exact_ci;

/// 2000 U.S. standard population (Census P25-1130), single years of age 0–84
/// and 85 and over (index 85). Total 274,633,642.
pub const US2000_SINGLE_AGES: [u64; 86] = [
    3_794_901, 3_758_562, 3_773_025, 3_791_001, 3_869_031, 3_896_081, 3_917_855, 3_978_143,
    3_903_983, 4_223_778, 4_230_322, 4_027_959, 3_941_299, 3_923_270, 3_933_929, 3_952_423,
    3_853_629, 4_012_263, 3_936_904, 4_064_299, 4_037_599, 3_764_802, 3_555_718, 3_489_233,
    3_409_873, 3_421_099, 3_328_203, 3_434_987, 3_450_602, 4_087_176, 3_999_004, 3_810_183,
    3_774_385, 3_840_938, 4_086_860, 4_288_078, 4_349_620, 4_469_476, 4_290_207, 4_782_575,
    4_666_685, 4_493_582, 4_487_560, 4_424_004, 4_407_398, 4_268_017, 4_033_859, 3_958_468,
    3_681_489, 3_863_960, 3_720_935, 3_504_329, 3_475_657, 3_754_218, 2_769_220, 2_749_739,
    2_786_795, 2_947_472, 2_404_462, 2_418_766, 2_259_141, 2_179_759, 2_132_873, 2_030_730,
    2_051_769, 2_033_933, 1_862_107, 1_849_893, 1_788_769, 1_875_238, 1_843_087, 1_784_744,
    1_802_080, 1_674_285, 1_621_378, 1_610_943, 1_530_137, 1_450_062, 1_456_186, 1_367_231,
    1_172_978, 1_065_672, 963_587, 890_893, 807_104, 4_259_173,
];

/// The NCHS "Master List" (Klein and Schoenborn, Healthy People 2010
/// Statistical Notes No. 20, NCHS 2001, Table 1): the 2000 projected U.S.
/// population in thousands in 24 age groups. NCHS derives every published
/// age-adjustment weight from this list, so groups built from it reproduce
/// NCHS (and DPH) weights exactly.
pub const NCHS_MASTER_LIST: [(u32, Option<u32>, u64); 24] = [
    (0, Some(0), 3_795),
    (1, Some(1), 3_759),
    (2, Some(4), 11_433),
    (5, Some(5), 3_896),
    (6, Some(8), 11_800),
    (9, Some(9), 4_224),
    (10, Some(11), 8_258),
    (12, Some(14), 11_799),
    (15, Some(17), 11_819),
    (18, Some(19), 8_001),
    (20, Some(24), 18_257),
    (25, Some(29), 17_722),
    (30, Some(34), 19_511),
    (35, Some(39), 22_180),
    (40, Some(44), 22_479),
    (45, Some(49), 19_806),
    (50, Some(54), 17_224),
    (55, Some(59), 13_307),
    (60, Some(64), 10_654),
    (65, Some(69), 9_410),
    (70, Some(74), 8_726),
    (75, Some(79), 7_415),
    (80, Some(84), 4_900),
    (85, None, 4_259),
];

/// Which form of the 2000 standard supplied the weights.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandardBasis {
    /// Sums of the NCHS Master List (the groups align with its boundaries);
    /// identical to NCHS published weights.
    NchsMasterList,
    /// Sums of the single-year P25-1130 population (a group splits a Master
    /// List group); within a few millionths of NCHS-style weights.
    SingleAges,
}

/// Standard population for each group: the NCHS Master List when every
/// group starts and ends on its boundaries, otherwise the single-year
/// population. Errors as in [`us2000_population`].
pub fn us2000_weights(groups: &[AgeGroup]) -> Result<(Vec<f64>, StandardBasis), String> {
    let pops = us2000_population(groups)?;
    let aligned = groups.iter().all(|g| {
        NCHS_MASTER_LIST.iter().any(|(s, _, _)| *s == g.start)
            && match g.end {
                None => true,
                Some(e) => NCHS_MASTER_LIST.iter().any(|(_, end, _)| *end == Some(e)),
            }
    });
    if !aligned {
        return Ok((
            pops.iter().map(|p| *p as f64).collect(),
            StandardBasis::SingleAges,
        ));
    }
    let w = groups
        .iter()
        .map(|g| {
            NCHS_MASTER_LIST
                .iter()
                .filter(|(s, e, _)| {
                    *s >= g.start
                        && match (g.end, e) {
                            (None, _) => true,
                            (Some(ge), Some(e)) => *e <= ge,
                            (Some(_), None) => false,
                        }
                })
                .map(|(_, _, m)| *m as f64 * 1000.0)
                .sum()
        })
        .collect();
    Ok((w, StandardBasis::NchsMasterList))
}

/// Citation for the standard population.
pub const US2000_SOURCE: &str = "2000 U.S. standard population (U.S. Census Bureau P25-1130), \
single ages, from the NCI SEER Program, https://seer.cancer.gov/stdpopulations/";

/// An age group in whole years: `start` to `end` inclusive (`end = None` means "and over").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AgeGroup {
    pub start: u32,
    #[serde(default)]
    pub end: Option<u32>,
}

impl AgeGroup {
    /// A label such as "45–54" or "85+".
    pub fn label(&self) -> String {
        match self.end {
            Some(e) if e == self.start => format!("{}", self.start),
            Some(e) => format!("{}–{}", self.start, e),
            None => format!("{}+", self.start),
        }
    }
}

/// Parse labels like "<1", "0", "1-4", "1–4", "85+", "85 and over", "65 plus".
pub fn parse_age_group(s: &str) -> Option<AgeGroup> {
    let t = s
        .trim()
        .to_ascii_lowercase()
        .replace(['–', '—'], "-")
        .replace("years", "")
        .replace("year", "")
        .replace("yrs", "")
        .replace(" to ", "-")
        .replace(' ', "");
    if let Some(n) = t.strip_prefix('<') {
        let n: u32 = n.parse().ok()?;
        return (n >= 1).then_some(AgeGroup {
            start: 0,
            end: Some(n - 1),
        });
    }
    for suffix in ["+", "andover", "andolder", "plus", "ormore", "orolder"] {
        if let Some(n) = t.strip_suffix(suffix) {
            return Some(AgeGroup {
                start: n.parse().ok()?,
                end: None,
            });
        }
    }
    if let Some((a, b)) = t.split_once('-') {
        let (a, b): (u32, u32) = (a.parse().ok()?, b.parse().ok()?);
        return (b >= a).then_some(AgeGroup {
            start: a,
            end: Some(b),
        });
    }
    let n: u32 = t.parse().ok()?;
    Some(AgeGroup {
        start: n,
        end: Some(n),
    })
}

/// The 2000 U.S. standard population in each group. Groups must not overlap.
/// An open-ended group must start at or below 85; a closed group must end at
/// or below 84 (the standard's single ages stop at 84, then 85+).
pub fn us2000_population(groups: &[AgeGroup]) -> Result<Vec<u64>, String> {
    let mut out = Vec::with_capacity(groups.len());
    let mut used = [false; 86];
    for g in groups {
        let end = match g.end {
            Some(e) if e > 84 => {
                return Err(format!(
                    "age group {} ends above 84; the 2000 standard population is by single year \
                     only through 84, so use an open-ended group such as 85+",
                    g.label()
                ));
            }
            Some(e) => e as usize,
            None if g.start > 85 => {
                return Err(format!(
                    "age group {} starts above 85; the oldest group the standard supports is 85+",
                    g.label()
                ));
            }
            None => 85,
        };
        let mut sum = 0;
        for (a, slot) in used
            .iter_mut()
            .enumerate()
            .take(end + 1)
            .skip(g.start as usize)
        {
            if *slot {
                return Err(format!("age group {} overlaps another group", g.label()));
            }
            *slot = true;
            sum += US2000_SINGLE_AGES[a];
        }
        out.push(sum);
    }
    Ok(out)
}

/// Inputs for one age stratum.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stratum {
    pub count: u64,
    pub population: f64,
    /// Standard population (any scale; normalized internally).
    pub standard: f64,
}

/// A directly age-adjusted rate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DirectAdjusted {
    /// Age-adjusted rate per `per`.
    pub rate: f64,
    pub lower: f64,
    pub upper: f64,
    /// Crude rate per `per`, with its exact Poisson interval.
    pub crude_rate: f64,
    pub crude_lower: f64,
    pub crude_upper: f64,
    pub total_count: u64,
    pub total_population: f64,
    /// Standard error of the adjusted rate (per `per`), √Σ wᵢ² xᵢ / nᵢ².
    pub standard_error: f64,
    /// Age groups with a zero population (excluded; their standard weight is
    /// dropped and the remaining weights renormalized).
    pub empty_strata: usize,
}

/// Direct standardization with the Fay–Feuer gamma interval.
///
/// # Panics
/// Panics if `confidence` is not in (0, 1) or no stratum has a population.
pub fn direct_adjust(strata: &[Stratum], per: f64, confidence: f64) -> DirectAdjusted {
    assert!(confidence > 0.0 && confidence < 1.0);
    let used: Vec<&Stratum> = strata.iter().filter(|s| s.population > 0.0).collect();
    assert!(!used.is_empty(), "at least one stratum needs a population");
    let wsum: f64 = used.iter().map(|s| s.standard).sum();
    let (mut y, mut v, mut wm) = (0.0f64, 0.0f64, 0.0f64);
    for s in &used {
        let w = s.standard / wsum;
        let x = s.count as f64;
        y += w * x / s.population;
        v += w * w * x / (s.population * s.population);
        wm = wm.max(w / s.population);
    }
    let alpha = 1.0 - confidence;
    let lower = if y > 0.0 && v > 0.0 {
        gamma_quantile(y * y / v, v / y, alpha / 2.0)
    } else {
        0.0
    };
    let (vu, yu) = (v + wm * wm, y + wm);
    let upper = gamma_quantile(yu * yu / vu, vu / yu, 1.0 - alpha / 2.0);
    let total_count: u64 = strata.iter().map(|s| s.count).sum();
    let total_population: f64 = strata.iter().map(|s| s.population).sum();
    let (clo, chi) = poisson_exact_ci(total_count, confidence);
    DirectAdjusted {
        rate: y * per,
        lower: lower * per,
        upper: upper * per,
        crude_rate: total_count as f64 / total_population * per,
        crude_lower: clo / total_population * per,
        crude_upper: chi / total_population * per,
        total_count,
        total_population,
        standard_error: v.sqrt() * per,
        empty_strata: strata.len() - used.len(),
    }
}

fn gamma_quantile(shape: f64, scale: f64, p: f64) -> f64 {
    Gamma::new(shape, 1.0 / scale)
        .map(|g| g.inverse_cdf(p))
        .unwrap_or(f64::NAN)
}

/// Comparison of two adjusted rates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RateComparison {
    /// Ratio of adjusted rates (this group ÷ reference).
    pub ratio: f64,
    pub ratio_lower: f64,
    pub ratio_upper: f64,
    /// Difference of adjusted rates (this group − reference), per `per`.
    pub difference: f64,
    pub difference_lower: f64,
    pub difference_upper: f64,
    /// Two-sided p-value of the z-test for the difference.
    pub p_value: f64,
}

/// Compare two independent adjusted rates with normal approximations (the
/// NCHS z-test for the difference; log scale for the ratio).
pub fn compare(
    a: &DirectAdjusted,
    reference: &DirectAdjusted,
    confidence: f64,
) -> Option<RateComparison> {
    if a.rate <= 0.0 || reference.rate <= 0.0 {
        return None;
    }
    let z = crate::dist::z_two_sided(confidence);
    let ratio = a.rate / reference.rate;
    let se_log = ((a.standard_error / a.rate).powi(2)
        + (reference.standard_error / reference.rate).powi(2))
    .sqrt();
    let diff = a.rate - reference.rate;
    let se_diff = (a.standard_error.powi(2) + reference.standard_error.powi(2)).sqrt();
    Some(RateComparison {
        ratio,
        ratio_lower: (ratio.ln() - z * se_log).exp(),
        ratio_upper: (ratio.ln() + z * se_log).exp(),
        difference: diff,
        difference_lower: diff - z * se_diff,
        difference_upper: diff + z * se_diff,
        p_value: crate::dist::p_normal_two_sided(diff / se_diff),
    })
}

/// An indirectly standardized ratio (SIR or SMR).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Indirect {
    pub observed: u64,
    pub expected: f64,
    /// Observed ÷ expected.
    pub ratio: f64,
    pub lower: f64,
    pub upper: f64,
    /// Exact two-sided p-value (R `poisson.test` method).
    pub p_value: f64,
    /// Observed − expected.
    pub excess: f64,
}

/// Indirect standardization: expected = Σ local populationᵢ × reference rateᵢ.
pub fn indirect(observed: u64, expected: f64, confidence: f64) -> Indirect {
    assert!(expected > 0.0 && confidence > 0.0 && confidence < 1.0);
    let (lo, hi) = poisson_exact_ci(observed, confidence);
    Indirect {
        observed,
        expected,
        ratio: observed as f64 / expected,
        lower: lo / expected,
        upper: hi / expected,
        p_value: poisson_two_sided_p(observed, expected),
        excess: observed as f64 - expected,
    }
}

/// Two-sided exact Poisson p-value for observing `x` when `m` is expected,
/// summing the probabilities no larger than that of `x` (as R `poisson.test`).
pub fn poisson_two_sided_p(x: u64, m: f64) -> f64 {
    let Ok(pois) = Poisson::new(m) else {
        return f64::NAN;
    };
    let rel = 1.0 + 1e-7;
    let d = pois.pmf(x);
    let xf = x as f64;
    if (xf - m).abs() < 1e-12 {
        return 1.0;
    }
    let p = if xf < m {
        let mut n = (2.0 * m - xf).ceil() as u64;
        while pois.pmf(n) > d {
            n *= 2;
        }
        let y = (m.ceil() as u64..=n)
            .filter(|i| pois.pmf(*i) <= d * rel)
            .count() as u64;
        pois.cdf(x) + if n + 1 > y { pois.sf(n - y) } else { 1.0 }
    } else {
        let y = (0..=m.floor() as u64)
            .filter(|i| pois.pmf(*i) <= d * rel)
            .count() as u64;
        let lower = if y == 0 { 0.0 } else { pois.cdf(y - 1) };
        lower + if x == 0 { 1.0 } else { pois.sf(x - 1) }
    };
    p.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn weights(groups: &[(u32, Option<u32>)]) -> Vec<f64> {
        let g: Vec<AgeGroup> = groups
            .iter()
            .map(|(s, e)| AgeGroup { start: *s, end: *e })
            .collect();
        let (p, basis) = us2000_weights(&g).unwrap();
        assert_eq!(basis, StandardBasis::NchsMasterList);
        let t: f64 = p.iter().sum();
        p.iter().map(|x| x / t).collect()
    }

    #[test]
    fn weights_match_nchs_statistical_notes_20() {
        // Klein RJ, Schoenborn CA. Age adjustment using the 2000 projected U.S.
        // population. Healthy People 2010 Statistical Notes No. 20. NCHS; 2001.
        // Table 2. Weights are proportions rounded to 6 decimals; 75–84 was
        // raised from 0.044841 to 0.044842 so the weights sum to 1.
        assert_eq!(US2000_SINGLE_AGES.iter().sum::<u64>(), 274_633_642);
        let d1 = weights(&[
            (0, Some(0)),
            (1, Some(4)),
            (5, Some(14)),
            (15, Some(24)),
            (25, Some(34)),
            (35, Some(44)),
            (45, Some(54)),
            (55, Some(64)),
            (65, Some(74)),
            (75, Some(84)),
            (85, None),
        ]);
        let nchs1 = [
            0.013818, 0.055317, 0.145565, 0.138646, 0.135573, 0.162613, 0.134834, 0.087247,
            0.066037, 0.044842, 0.015508,
        ];
        for (w, n) in d1.iter().zip(nchs1) {
            assert!((w - n).abs() < 1.1e-6, "{w} vs {n}");
        }
        // Distribution #4: <18, 18–44, 45–64, 65–74, 75+.
        let d4 = weights(&[
            (0, Some(17)),
            (18, Some(44)),
            (45, Some(64)),
            (65, Some(74)),
            (75, None),
        ]);
        for (w, n) in d4
            .iter()
            .zip([0.257736, 0.393797, 0.222081, 0.066037, 0.060349])
        {
            assert!((w - n).abs() < 1.1e-6, "{w} vs {n}");
        }
        // Distribution #18: 65+ only (weights renormalized within the range).
        let d18 = weights(&[(65, Some(74)), (75, None)]);
        assert!((d18[0] - 0.522501).abs() < 1e-6 && (d18[1] - 0.477499).abs() < 1e-6);
        // Groups that split a Master List group fall back to single ages,
        // which stay within a few millionths of the NCHS-style weights.
        let split = [
            AgeGroup {
                start: 0,
                end: Some(6),
            },
            AgeGroup {
                start: 7,
                end: None,
            },
        ];
        let (w, basis) = us2000_weights(&split).unwrap();
        assert_eq!(basis, StandardBasis::SingleAges);
        assert_eq!(w[0] as u64, US2000_SINGLE_AGES[..=6].iter().sum::<u64>());
        let master: u64 = NCHS_MASTER_LIST.iter().map(|x| x.2).sum();
        assert_eq!(master, 274_634);
        for (s, e, m) in NCHS_MASTER_LIST {
            let p = us2000_population(&[AgeGroup { start: s, end: e }]).unwrap()[0];
            assert!(
                (p as f64 / 1000.0 - m as f64).abs() <= 1.0,
                "{s}: {p} vs {m} thousand"
            );
        }
    }

    #[test]
    fn parses_age_labels_and_rejects_bad_groups() {
        assert_eq!(
            parse_age_group("<1"),
            Some(AgeGroup {
                start: 0,
                end: Some(0)
            })
        );
        assert_eq!(
            parse_age_group("1–4 years"),
            Some(AgeGroup {
                start: 1,
                end: Some(4)
            })
        );
        assert_eq!(
            parse_age_group("85+"),
            Some(AgeGroup {
                start: 85,
                end: None
            })
        );
        assert_eq!(
            parse_age_group("65 and over"),
            Some(AgeGroup {
                start: 65,
                end: None
            })
        );
        assert_eq!(
            parse_age_group("18 to 44"),
            Some(AgeGroup {
                start: 18,
                end: Some(44)
            })
        );
        assert!(parse_age_group("adults").is_none());
        assert!(
            us2000_population(&[AgeGroup {
                start: 80,
                end: Some(89)
            }])
            .is_err()
        );
        assert!(
            us2000_population(&[
                AgeGroup {
                    start: 0,
                    end: Some(10)
                },
                AgeGroup {
                    start: 5,
                    end: Some(20)
                }
            ])
            .is_err()
        );
        assert_eq!(
            AgeGroup {
                start: 85,
                end: None
            }
            .label(),
            "85+"
        );
    }

    #[test]
    fn indirect_ratio_and_p() {
        let r = indirect(18, 10.5, 0.95);
        assert!((r.ratio - 18.0 / 10.5).abs() < 1e-12);
        assert!(r.lower > 1.0 && r.p_value < 0.05);
        assert_eq!(poisson_two_sided_p(10, 10.0), 1.0);
    }
}
