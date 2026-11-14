//! The budget arithmetic for `build_program_budget` (PLAN-09): line items by year with
//! escalation, fringe, the modified total direct cost (MTDC) base, indirect costs on the right
//! base, cost share, and the match check.
//!
//! Every rounding step mirrors the XLSX formulas (`ROUND(x, 2)` on each line-year amount, fringe,
//! and indirect), so the workbook and this module agree to the cent. The golden fixture
//! `tests/fixtures/budget_golden.json` is computed by an independent Python program with exact
//! decimals (`tests/fixtures/generators/budget_golden.py`), and
//! `tests/fixtures/generators/check_budget_workbook.py` recalculates the workbook formulas in
//! LibreOffice and compares them with it.

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Most budget years a workbook holds.
pub const MAX_YEARS: usize = 5;

/// Object class of a line item (fringe benefits are calculated from the personnel lines).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    /// Salaries and wages (give `annual_salary` and `effort_percent`).
    Personnel,
    Travel,
    /// Items at or above the equipment threshold; not in the MTDC base.
    Equipment,
    Supplies,
    /// Contracts for services and subawards (set `subaward` for a subaward).
    Contractual,
    /// Other direct costs, including participant support costs.
    Other,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Personnel,
        Category::Travel,
        Category::Equipment,
        Category::Supplies,
        Category::Contractual,
        Category::Other,
    ];

    /// The label used in the workbook and its formulas.
    pub fn label(self) -> &'static str {
        match self {
            Category::Personnel => "Personnel",
            Category::Travel => "Travel",
            Category::Equipment => "Equipment",
            Category::Supplies => "Supplies",
            Category::Contractual => "Contractual",
            Category::Other => "Other",
        }
    }

    /// Match a label or a plain word ("personnel", "contract", "participant support").
    pub fn parse(s: &str) -> Option<Category> {
        let w = s.trim().to_lowercase();
        match w.as_str() {
            "personnel" | "salary" | "salaries" | "staff" | "labor" | "wages" => {
                Some(Category::Personnel)
            }
            "travel" => Some(Category::Travel),
            "equipment" => Some(Category::Equipment),
            "supplies" | "supply" | "materials" => Some(Category::Supplies),
            "contractual" | "contract" | "contracts" | "subaward" | "subawards" | "consultant"
            | "consultants" => Some(Category::Contractual),
            "other" | "other direct costs" | "odc" => Some(Category::Other),
            _ => None,
        }
    }
}

/// Who pays for a line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Funding {
    /// Requested from the funder.
    Federal,
    /// Provided by your organization or a partner (cost share or match).
    CostShare,
}

impl Funding {
    pub fn label(self) -> &'static str {
        match self {
            Funding::Federal => "Federal",
            Funding::CostShare => "Cost share",
        }
    }

    pub fn parse(s: &str) -> Option<Funding> {
        match s.trim().to_lowercase().replace(['_', '-'], " ").as_str() {
            "federal" | "funder" | "grant" | "requested" | "request" => Some(Funding::Federal),
            "cost share" | "costshare" | "match" | "matching" | "in kind" | "in-kind"
            | "non federal" | "nonfederal" | "cash match" => Some(Funding::CostShare),
            _ => None,
        }
    }
}

/// Why a line is left out of the MTDC base (2 CFR 200.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MtdcExclusion {
    ParticipantSupport,
    Rental,
    PatientCare,
    TuitionOrScholarship,
    CapitalExpenditure,
}

impl MtdcExclusion {
    pub fn label(self) -> &'static str {
        match self {
            MtdcExclusion::ParticipantSupport => "participant support costs",
            MtdcExclusion::Rental => "rental costs",
            MtdcExclusion::PatientCare => "charges for patient care",
            MtdcExclusion::TuitionOrScholarship => {
                "tuition remission, scholarships, or fellowships"
            }
            MtdcExclusion::CapitalExpenditure => "capital expenditures",
        }
    }

    pub fn parse(s: &str) -> Option<MtdcExclusion> {
        match s.trim().to_lowercase().replace(['_', '-'], " ").as_str() {
            "participant support" | "participant support costs" | "participant" => {
                Some(MtdcExclusion::ParticipantSupport)
            }
            "rental" | "rent" | "rental costs" => Some(MtdcExclusion::Rental),
            "patient care" | "charges for patient care" => Some(MtdcExclusion::PatientCare),
            "tuition"
            | "scholarship"
            | "scholarships"
            | "tuition or scholarship"
            | "tuition remission"
            | "fellowship"
            | "fellowships" => Some(MtdcExclusion::TuitionOrScholarship),
            "capital expenditure" | "capital expenditures" | "capital" => {
                Some(MtdcExclusion::CapitalExpenditure)
            }
            _ => None,
        }
    }
}

/// How a line counts in the MTDC base.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Treatment {
    Included,
    Excluded,
    /// Only the first `subaward_cap` dollars of the subaward over the whole award.
    Subaward,
}

impl Treatment {
    /// The word written to the workbook's MTDC column.
    pub fn label(self) -> &'static str {
        match self {
            Treatment::Included => "Included",
            Treatment::Excluded => "Excluded",
            Treatment::Subaward => "Subaward",
        }
    }
}

/// The base an indirect rate is applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum IndirectBase {
    /// Modified total direct costs (the base for the de minimis rate).
    Mtdc,
    /// All direct costs including equipment.
    TotalDirect,
    /// Salaries and wages only.
    SalariesAndWages,
    /// Salaries and wages plus fringe benefits.
    SalariesAndFringe,
}

impl IndirectBase {
    pub fn label(self) -> &'static str {
        match self {
            IndirectBase::Mtdc => "Modified total direct costs (MTDC)",
            IndirectBase::TotalDirect => "Total direct costs",
            IndirectBase::SalariesAndWages => "Salaries and wages",
            IndirectBase::SalariesAndFringe => "Salaries and wages plus fringe",
        }
    }

    pub const ALL: [IndirectBase; 4] = [
        IndirectBase::Mtdc,
        IndirectBase::TotalDirect,
        IndirectBase::SalariesAndWages,
        IndirectBase::SalariesAndFringe,
    ];

    /// 1-based position in [`IndirectBase::ALL`] (the workbook's `MATCH` result).
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|b| *b == self).unwrap_or(0) + 1
    }
}

/// One line of the budget.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub category: Category,
    /// Role (personnel) or item description. Never a person's name.
    pub item: String,
    pub funding: Funding,
    /// Personnel: fraction of time (0.5 = 50 percent). Others: units per year.
    pub quantity: f64,
    /// Personnel: annual base salary. Others: cost per unit.
    pub unit_cost: f64,
    pub unit: String,
    /// Fringe rate for this line as a fraction; `None` uses the default.
    pub fringe: Option<f64>,
    pub escalate: bool,
    /// 1-based first and last budget year the line is active.
    pub first_year: usize,
    pub last_year: usize,
    pub treatment: Treatment,
    pub exclusion: Option<MtdcExclusion>,
}

/// Rates and settings for the whole budget.
#[derive(Debug, Clone, PartialEq)]
pub struct Rates {
    pub years: usize,
    /// Annual growth of personnel lines, as a fraction.
    pub salary_escalation: f64,
    /// Annual growth of other lines that escalate.
    pub other_escalation: f64,
    pub fringe: f64,
    /// Indirect rate as a fraction (0 when no indirect costs).
    pub indirect_rate: f64,
    pub indirect_base: IndirectBase,
    /// Charge indirect costs on the cost share lines too.
    pub indirect_on_cost_share: bool,
    /// Dollars of each subaward counted in MTDC.
    pub subaward_cap: f64,
}

/// Required cost share.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MatchBasis {
    /// The required percent applies to the total project cost (federal plus cost share).
    TotalProject,
    /// The required percent applies to the federal request (a match ratio).
    FederalRequest,
}

/// Results for one line (per year).
#[derive(Debug, Clone, PartialEq)]
pub struct RowCalc {
    pub amount: Vec<f64>,
    pub fringe: Vec<f64>,
    pub mtdc: Vec<f64>,
}

/// Totals for one funding source, per year.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Block {
    pub personnel: Vec<f64>,
    pub fringe: Vec<f64>,
    pub travel: Vec<f64>,
    pub equipment: Vec<f64>,
    pub supplies: Vec<f64>,
    pub contractual: Vec<f64>,
    pub other: Vec<f64>,
    pub direct: Vec<f64>,
    pub mtdc: Vec<f64>,
    pub indirect_base: Vec<f64>,
    pub indirect: Vec<f64>,
    pub total: Vec<f64>,
}

impl Block {
    /// A category's per-year amounts.
    pub fn category(&self, c: Category) -> &[f64] {
        match c {
            Category::Personnel => &self.personnel,
            Category::Travel => &self.travel,
            Category::Equipment => &self.equipment,
            Category::Supplies => &self.supplies,
            Category::Contractual => &self.contractual,
            Category::Other => &self.other,
        }
    }
}

/// The match requirement check.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchCheck {
    pub percent: f64,
    pub basis: MatchBasis,
    pub required: f64,
    pub budgeted: f64,
    /// Positive when the budgeted cost share is below the requirement.
    pub shortfall: f64,
}

/// The whole computed budget.
#[derive(Debug, Clone, PartialEq)]
pub struct Computed {
    pub rows: Vec<RowCalc>,
    pub federal: Block,
    pub cost_share: Block,
    pub match_check: Option<MatchCheck>,
}

/// Excel-style `ROUND(x, 2)` (half away from zero, tolerant of binary noise on ties).
pub fn round2(x: f64) -> f64 {
    let y = x * 100.0;
    let f = y.fract().abs();
    let r = if (f - 0.5).abs() < 1e-7 {
        y.trunc() + y.signum()
    } else {
        y.round()
    };
    r / 100.0
}

fn sum_years(v: &[f64]) -> f64 {
    v.iter().sum()
}

/// Line treatment from the category and flags.
pub fn treatment_for(
    category: Category,
    subaward: bool,
    exclusion: Option<MtdcExclusion>,
) -> Treatment {
    if category == Category::Equipment || exclusion.is_some() {
        Treatment::Excluded
    } else if category == Category::Contractual && subaward {
        Treatment::Subaward
    } else {
        Treatment::Included
    }
}

fn rate_ok(name: &str, r: f64) -> Result<()> {
    if !r.is_finite() || !(0.0..=1.0).contains(&r) {
        return Err(PhnError::InvalidInput(format!(
            "{name} must be between 0 and 100 percent"
        )));
    }
    Ok(())
}

/// Validate the inputs and compute every line, year, and total.
pub fn compute(
    lines: &[Line],
    rates: &Rates,
    required_match: Option<(f64, MatchBasis)>,
) -> Result<Computed> {
    if rates.years == 0 || rates.years > MAX_YEARS {
        return Err(PhnError::InvalidInput(format!(
            "years must be from 1 to {MAX_YEARS}"
        )));
    }
    for (n, r) in [
        ("fringe rate", rates.fringe),
        ("indirect rate", rates.indirect_rate),
    ] {
        rate_ok(n, r)?;
    }
    for (n, r) in [
        ("salary escalation", rates.salary_escalation),
        ("other escalation", rates.other_escalation),
    ] {
        if !r.is_finite() || !(-0.5..=1.0).contains(&r) {
            return Err(PhnError::InvalidInput(format!(
                "{n} must be between -50 and 100 percent a year"
            )));
        }
    }
    let n = rates.years;
    let mut rows = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        if l.first_year == 0 || l.last_year < l.first_year || l.last_year > n {
            return Err(PhnError::InvalidInput(format!(
                "line {}: first_year and last_year must satisfy 1 <= first_year <= last_year <= {n}",
                i + 1
            )));
        }
        if !l.quantity.is_finite()
            || !l.unit_cost.is_finite()
            || l.quantity < 0.0
            || l.unit_cost < 0.0
        {
            return Err(PhnError::InvalidInput(format!(
                "line {}: quantity and cost must be zero or more",
                i + 1
            )));
        }
        if let Some(f) = l.fringe {
            rate_ok("a fringe rate", f)?;
        }
        let esc = if l.category == Category::Personnel {
            rates.salary_escalation
        } else {
            rates.other_escalation
        };
        let mut amount = vec![0.0; n];
        let mut fringe = vec![0.0; n];
        let mut mtdc = vec![0.0; n];
        let mut cum_prev = 0.0;
        for y in 1..=n {
            let k = y - 1;
            if y >= l.first_year && y <= l.last_year {
                let factor = if l.escalate {
                    (1.0 + esc).powi(k as i32)
                } else {
                    1.0
                };
                amount[k] = round2(l.quantity * l.unit_cost * factor);
            }
            if l.category == Category::Personnel {
                fringe[k] = round2(amount[k] * l.fringe.unwrap_or(rates.fringe));
            }
            let cum = cum_prev + amount[k];
            mtdc[k] = match l.treatment {
                Treatment::Excluded => 0.0,
                Treatment::Included => amount[k] + fringe[k],
                Treatment::Subaward => {
                    cum.min(rates.subaward_cap) - cum_prev.min(rates.subaward_cap)
                }
            };
            cum_prev = cum;
        }
        if amount.iter().any(|a| !a.is_finite() || a.abs() > 1e11) {
            return Err(PhnError::InvalidInput(format!(
                "line {}: the amount is too large",
                i + 1
            )));
        }
        rows.push(RowCalc {
            amount,
            fringe,
            mtdc,
        });
    }

    let block = |f: Funding| -> Block {
        let mut b = Block {
            personnel: vec![0.0; n],
            fringe: vec![0.0; n],
            travel: vec![0.0; n],
            equipment: vec![0.0; n],
            supplies: vec![0.0; n],
            contractual: vec![0.0; n],
            other: vec![0.0; n],
            direct: vec![0.0; n],
            mtdc: vec![0.0; n],
            indirect_base: vec![0.0; n],
            indirect: vec![0.0; n],
            total: vec![0.0; n],
        };
        for (l, r) in lines.iter().zip(&rows) {
            if l.funding != f {
                continue;
            }
            for k in 0..n {
                let slot = match l.category {
                    Category::Personnel => &mut b.personnel,
                    Category::Travel => &mut b.travel,
                    Category::Equipment => &mut b.equipment,
                    Category::Supplies => &mut b.supplies,
                    Category::Contractual => &mut b.contractual,
                    Category::Other => &mut b.other,
                };
                slot[k] += r.amount[k];
                b.fringe[k] += r.fringe[k];
                b.mtdc[k] += r.mtdc[k];
            }
        }
        let charge = f == Funding::Federal || rates.indirect_on_cost_share;
        for k in 0..n {
            b.direct[k] = b.personnel[k]
                + b.fringe[k]
                + b.travel[k]
                + b.equipment[k]
                + b.supplies[k]
                + b.contractual[k]
                + b.other[k];
            b.indirect_base[k] = match rates.indirect_base {
                IndirectBase::Mtdc => b.mtdc[k],
                IndirectBase::TotalDirect => b.direct[k],
                IndirectBase::SalariesAndWages => b.personnel[k],
                IndirectBase::SalariesAndFringe => b.personnel[k] + b.fringe[k],
            };
            b.indirect[k] = if charge {
                round2(rates.indirect_rate * b.indirect_base[k])
            } else {
                0.0
            };
            b.total[k] = b.direct[k] + b.indirect[k];
        }
        b
    };
    let federal = block(Funding::Federal);
    let cost_share = block(Funding::CostShare);
    let match_check = match required_match {
        None => None,
        Some((pct, basis)) => {
            if !pct.is_finite() || !(0.0..=100.0).contains(&pct) {
                return Err(PhnError::InvalidInput(
                    "the required cost share must be between 0 and 100 percent".into(),
                ));
            }
            let fed = sum_years(&federal.total);
            let cs = sum_years(&cost_share.total);
            let required = match basis {
                MatchBasis::FederalRequest => pct / 100.0 * fed,
                MatchBasis::TotalProject => pct / 100.0 * (fed + cs),
            };
            Some(MatchCheck {
                percent: pct,
                basis,
                required,
                budgeted: cs,
                shortfall: (required - cs).max(0.0),
            })
        }
    };
    Ok(Computed {
        rows,
        federal,
        cost_share,
        match_check,
    })
}

impl Computed {
    /// Sum of every line's direct amount and fringe, from the rows alone (for the arithmetic
    /// self-check against the blocks).
    pub fn rows_total(&self, lines: &[Line], f: Funding) -> f64 {
        lines
            .iter()
            .zip(&self.rows)
            .filter(|(l, _)| l.funding == f)
            .map(|(_, r)| sum_years(&r.amount) + sum_years(&r.fringe))
            .sum()
    }

    /// True when the blocks' direct totals equal the rows' totals (within a tenth of a cent).
    pub fn self_check(&self, lines: &[Line]) -> bool {
        let close = |a: f64, b: f64| (a - b).abs() < 0.001;
        close(
            sum_years(&self.federal.direct),
            self.rows_total(lines, Funding::Federal),
        ) && close(
            sum_years(&self.cost_share.direct),
            self.rows_total(lines, Funding::CostShare),
        )
    }

    /// Total of a per-year series.
    pub fn total(v: &[f64]) -> f64 {
        sum_years(v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(cat: Category, q: f64, u: f64) -> Line {
        Line {
            category: cat,
            item: "x".into(),
            funding: Funding::Federal,
            quantity: q,
            unit_cost: u,
            unit: String::new(),
            fringe: None,
            escalate: true,
            first_year: 1,
            last_year: 1,
            treatment: treatment_for(cat, false, None),
            exclusion: None,
        }
    }

    fn rates(years: usize) -> Rates {
        Rates {
            years,
            salary_escalation: 0.0,
            other_escalation: 0.0,
            fringe: 0.25,
            indirect_rate: 0.15,
            indirect_base: IndirectBase::Mtdc,
            indirect_on_cost_share: false,
            subaward_cap: 50_000.0,
        }
    }

    #[test]
    fn one_year_de_minimis_on_mtdc() {
        let lines = vec![
            line(Category::Personnel, 0.5, 80_000.0),
            line(Category::Equipment, 1.0, 12_000.0),
            line(Category::Supplies, 1.0, 3_000.0),
        ];
        let c = compute(&lines, &rates(1), None).unwrap();
        assert_eq!(c.federal.personnel[0], 40_000.0);
        assert_eq!(c.federal.fringe[0], 10_000.0);
        assert_eq!(c.federal.direct[0], 65_000.0);
        // MTDC excludes the equipment: 40,000 + 10,000 + 3,000.
        assert_eq!(c.federal.mtdc[0], 53_000.0);
        assert_eq!(c.federal.indirect[0], 7_950.0);
        assert_eq!(c.federal.total[0], 72_950.0);
        assert!(c.self_check(&lines));
    }

    #[test]
    fn subaward_cap_counts_the_first_fifty_thousand_over_the_whole_award() {
        let mut sub = line(Category::Contractual, 1.0, 30_000.0);
        sub.last_year = 3;
        sub.treatment = Treatment::Subaward;
        let c = compute(&[sub], &rates(3), None).unwrap();
        assert_eq!(c.federal.mtdc, vec![30_000.0, 20_000.0, 0.0]);
        assert_eq!(c.federal.contractual, vec![30_000.0; 3]);
    }

    #[test]
    fn escalation_and_rounding() {
        let mut p = line(Category::Personnel, 1.0, 61_234.56);
        p.last_year = 3;
        let mut r = rates(3);
        r.salary_escalation = 0.03;
        let c = compute(&[p], &r, None).unwrap();
        assert_eq!(c.federal.personnel[0], 61_234.56);
        assert_eq!(c.federal.personnel[1], 63_071.6);
        assert_eq!(c.federal.personnel[2], 64_963.74);
    }

    #[test]
    fn cost_share_and_match_check() {
        let mut cs = line(Category::Personnel, 0.1, 100_000.0);
        cs.funding = Funding::CostShare;
        let fed = line(Category::Supplies, 1.0, 100_000.0);
        let c = compute(
            &[fed, cs],
            &rates(1),
            Some((20.0, MatchBasis::FederalRequest)),
        )
        .unwrap();
        let m = c.match_check.unwrap();
        // Federal: 100,000 + 15,000 indirect; cost share: 10,000 + 2,500 fringe (no indirect).
        assert_eq!(c.federal.total[0], 115_000.0);
        assert_eq!(c.cost_share.total[0], 12_500.0);
        assert_eq!(m.required, 23_000.0);
        assert_eq!(m.shortfall, 10_500.0);
    }

    #[test]
    fn bad_inputs_are_refused() {
        let mut l = line(Category::Travel, 1.0, 10.0);
        l.last_year = 2;
        assert!(compute(&[l], &rates(1), None).is_err());
        assert!(compute(&[], &rates(6), None).is_err());
        let mut r = rates(1);
        r.indirect_rate = 1.5;
        assert!(compute(&[], &r, None).is_err());
    }
}
