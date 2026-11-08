//! Engine inputs and results.

use chrono::NaiveDate;
use serde::Serialize;

/// Patient sex as used by the supporting data's "required gender".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Gender {
    Female,
    Male,
    Unknown,
}

impl Gender {
    /// Label as written in the supporting data.
    pub fn label(self) -> &'static str {
        match self {
            Self::Female => "Female",
            Self::Male => "Male",
            Self::Unknown => "Unknown",
        }
    }
}

/// An active patient observation (CDSi coded observation, e.g. "007"
/// Pregnant, "080" adverse reaction to a vaccine component).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Observation {
    /// Three-digit CDSi observation code.
    pub code: String,
    /// Date of the observation, when known (used by intervals measured from
    /// an observation, such as Tdap in pregnancy).
    pub date: Option<NaiveDate>,
}

/// A vaccine dose administered.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DoseInput {
    pub date: NaiveDate,
    /// CVX code.
    pub cvx: String,
    /// MVX (manufacturer) code, when known.
    pub mvx: Option<String>,
    /// Trade name, when known.
    pub trade_name: Option<String>,
    /// Volume in mL, when known (only produces a reason, never invalidates).
    pub volume_ml: Option<f64>,
    /// Dose condition flag (expired, sub-potent, recalled, cold-chain break).
    pub sub_standard: bool,
    /// Lot expiration date, when known.
    pub lot_expiration: Option<NaiveDate>,
}

impl DoseInput {
    /// A dose with only a date and CVX code.
    pub fn new(date: NaiveDate, cvx: impl Into<String>) -> Self {
        Self {
            date,
            cvx: crate::data::normalize_cvx(&cvx.into()),
            mvx: None,
            trade_name: None,
            volume_ml: None,
            sub_standard: false,
            lot_expiration: None,
        }
    }
}

/// Everything the engine needs about one patient.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatientInput {
    pub dob: NaiveDate,
    pub gender: Gender,
    /// Born in the United States (`None` = unknown). Some birth-date
    /// evidence of immunity applies only to U.S.-born patients.
    pub born_in_us: Option<bool>,
    pub observations: Vec<Observation>,
    pub doses: Vec<DoseInput>,
    pub assessment_date: NaiveDate,
}

/// Evaluation status of a dose (Table 3-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum EvalStatus {
    Valid,
    NotValid,
    Extraneous,
    SubStandard,
}

impl EvalStatus {
    /// Label as written in the Logic Specification.
    pub fn label(self) -> &'static str {
        match self {
            Self::Valid => "Valid",
            Self::NotValid => "Not Valid",
            Self::Extraneous => "Extraneous",
            Self::SubStandard => "Sub-standard",
        }
    }
}

/// Target dose status (Table 3-2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum TargetStatus {
    NotSatisfied,
    Satisfied,
    Skipped,
}

/// Patient series status (Table 3-3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum SeriesStatus {
    AgedOut,
    Complete,
    Contraindicated,
    Immune,
    NotComplete,
    NotRecommended,
}

impl SeriesStatus {
    /// Label as written in the Logic Specification.
    pub fn label(self) -> &'static str {
        match self {
            Self::AgedOut => "Aged Out",
            Self::Complete => "Complete",
            Self::Contraindicated => "Contraindicated",
            Self::Immune => "Immune",
            Self::NotComplete => "Not Complete",
            Self::NotRecommended => "Not Recommended",
        }
    }
}

/// One dose's evaluation in one patient series.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DoseEvaluation {
    /// Index of the dose in [`PatientInput::doses`].
    pub dose_index: usize,
    pub date: NaiveDate,
    pub cvx: String,
    pub antigen: String,
    pub status: EvalStatus,
    /// Target dose number it was evaluated against (1-based), if any.
    pub target_dose: Option<usize>,
    /// Reasons (e.g. "Age: too young", "Interval: grace period").
    pub reasons: Vec<String>,
}

/// Forecast dates for the next target dose.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ForecastDates {
    pub earliest: Option<NaiveDate>,
    /// Adjusted recommended date.
    pub recommended: Option<NaiveDate>,
    /// Adjusted past-due date.
    pub past_due: Option<NaiveDate>,
    pub latest: Option<NaiveDate>,
    pub unadjusted_recommended: Option<NaiveDate>,
    pub unadjusted_past_due: Option<NaiveDate>,
}

/// A recommended vaccine type for the next dose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RecommendedVaccine {
    pub cvx: String,
    pub vaccine_type: String,
}

/// The result for one relevant patient series.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PatientSeriesResult {
    pub antigen: String,
    pub series_name: String,
    pub series_type: String,
    pub series_group: String,
    pub status: SeriesStatus,
    pub forecast_reasons: Vec<String>,
    /// Next target dose number (FORECASTDN-1), when a dose is forecast.
    pub forecast_dose_number: Option<usize>,
    pub dates: ForecastDates,
    pub recommended_vaccines: Vec<RecommendedVaccine>,
    /// Number of satisfied target doses.
    pub valid_doses: usize,
    /// Number of target doses in the series (after recurring doses).
    pub target_doses: usize,
    pub evaluations: Vec<DoseEvaluation>,
    /// Remaining doses with minimum intervals, for a catch-up schedule:
    /// (target dose number, minimum interval from the previous dose).
    pub remaining_doses: Vec<RemainingDose>,
    pub admin_guidance: Vec<String>,
    /// True when this is a best patient series for its antigen.
    pub best: bool,
    /// FORECASTPRIORITY-1: every preferable interval of the forecast target
    /// dose has the interval priority flag.
    pub priority_forecast: bool,
}

/// A target dose still to be given.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RemainingDose {
    pub dose_number: usize,
    /// Minimum interval from the previous dose, as written in the data.
    pub min_interval: Option<String>,
    /// Minimum age, as written in the data.
    pub min_age: Option<String>,
    /// Recommended age, as written in the data.
    pub recommended_age: Option<String>,
}

/// A vaccine group forecast (Chapter 9).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VaccineGroupForecast {
    pub vaccine_group: String,
    pub series_group: String,
    /// "Standard", "Risk", or "Evaluation Only" of the contained series.
    pub series_type: String,
    pub status: SeriesStatus,
    pub forecast_dose_number: Option<usize>,
    pub dates: ForecastDates,
    pub forecast_reasons: Vec<String>,
    /// Antigens that need a dose.
    pub recommended_antigens: Vec<String>,
    pub recommended_vaccines: Vec<RecommendedVaccine>,
    /// Indices into [`EngineResult::patient_series`] of the contained
    /// best patient series.
    pub patient_series: Vec<usize>,
}

/// A contraindication or indication message for the clinician.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ClinicalNote {
    pub antigen: String,
    pub observation_code: String,
    pub text: String,
}

/// Full engine output.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EngineResult {
    pub assessment_date: NaiveDate,
    pub patient_series: Vec<PatientSeriesResult>,
    pub vaccine_groups: Vec<VaccineGroupForecast>,
    /// CVX codes that are not in the supporting data's CVX map.
    pub unrecognized_cvx: Vec<String>,
    /// Contraindications that apply to the patient.
    pub contraindications: Vec<ClinicalNote>,
    /// Evidence of immunity found (antigen, reason).
    pub immunity: Vec<ClinicalNote>,
}

impl EngineResult {
    /// Vaccine group forecasts for a vaccine group.
    pub fn group_forecasts(&self, name: &str) -> Vec<&VaccineGroupForecast> {
        self.vaccine_groups
            .iter()
            .filter(|g| g.vaccine_group.eq_ignore_ascii_case(name))
            .collect()
    }
}
