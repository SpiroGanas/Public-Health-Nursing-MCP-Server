//! Typed CDSi supporting data, parsed from CDC's XML with `quick-xml`.
//!
//! The XML files ship inside the binary (`data/supporting-data/`) and are
//! parsed once, on first use ([`SupportingData::get`]).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use chrono::NaiveDate;
use quick_xml::events::{BytesRef, Event};
use quick_xml::reader::Reader;

use crate::duration::{Span, parse_date};

/// Version of the embedded supporting data.
pub const SUPPORTING_DATA_VERSION: &str = "4.65";
/// Publication month of the embedded supporting data.
pub const SUPPORTING_DATA_PUBLISHED: &str = "2026-08";
/// Version of the Logic Specification the engine implements.
pub const LOGIC_SPEC_VERSION: &str = "4.6";
/// Date of the Logic Specification the engine implements.
pub const LOGIC_SPEC_DATE: &str = "2024-12-13";
/// Where CDC publishes the CDSi resources.
pub const CDSI_URL: &str = "https://www.cdc.gov/iis/cdsi/index.html";
/// The supporting-data download.
pub const SUPPORTING_DATA_URL: &str =
    "https://www.cdc.gov/iis/downloads/supporting-data-4.65-508.zip";
/// The Logic Specification PDF.
pub const LOGIC_SPEC_URL: &str = "https://www.cdc.gov/iis/downloads/logic-spec-acip-rec-4.6.pdf";

/// The manifest describing the embedded files (version, URLs, checksums).
pub const MANIFEST_JSON: &str = include_str!("../data/manifest.json");

macro_rules! antigen_files {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../data/supporting-data/", $name)))),*]
    };
}

/// The embedded XML files: (file name, text).
pub const ANTIGEN_FILES: &[(&str, &str)] = antigen_files!(
    "antigen-COVID-19.xml",
    "antigen-Chikungunya.xml",
    "antigen-Cholera.xml",
    "antigen-Dengue.xml",
    "antigen-Diphtheria.xml",
    "antigen-Ebola.xml",
    "antigen-HPV.xml",
    "antigen-HepA.xml",
    "antigen-HepB.xml",
    "antigen-Hib.xml",
    "antigen-Influenza.xml",
    "antigen-JE.xml",
    "antigen-Measles.xml",
    "antigen-Meningococcal-B.xml",
    "antigen-Meningococcal.xml",
    "antigen-Mumps.xml",
    "antigen-Orthopoxvirus.xml",
    "antigen-Pertussis.xml",
    "antigen-Pneumococcal.xml",
    "antigen-Polio.xml",
    "antigen-RSV.xml",
    "antigen-Rabies.xml",
    "antigen-Rotavirus.xml",
    "antigen-Rubella.xml",
    "antigen-TBE.xml",
    "antigen-Tetanus.xml",
    "antigen-Typhoid.xml",
    "antigen-Varicella.xml",
    "antigen-YF.xml",
    "antigen-Zoster.xml",
);

/// The schedule supporting data XML.
pub const SCHEDULE_FILE: (&str, &str) = (
    "schedule.xml",
    include_str!("../data/supporting-data/schedule.xml"),
);

// ------------------------------------------------------------------ model

/// Everything in the supporting data.
#[derive(Debug, Clone)]
pub struct SupportingData {
    /// Antigens by name (e.g. "Diphtheria", "HepB").
    pub antigens: BTreeMap<String, AntigenData>,
    /// Schedule data: CVX map, vaccine groups, conflicts, observations.
    pub schedule: ScheduleData,
}

/// One antigen's supporting data.
#[derive(Debug, Clone)]
pub struct AntigenData {
    /// Antigen (target disease) name.
    pub name: String,
    /// Evidence of immunity.
    pub immunity: Immunity,
    /// Contraindications that apply to every vaccine for the antigen.
    pub antigen_contraindications: Vec<AntigenContraindication>,
    /// Contraindications that apply to specific vaccines.
    pub vaccine_contraindications: Vec<VaccineContraindication>,
    /// Antigen series (paths to immunity).
    pub series: Vec<Series>,
}

/// Evidence of immunity for an antigen.
#[derive(Debug, Clone, Default)]
pub struct Immunity {
    /// Observation codes that are evidence of immunity.
    pub clinical_history: Vec<CodeTitle>,
    /// Birth-date immunity rules.
    pub birth_date: Vec<BirthDateImmunity>,
}

/// A code with its title.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeTitle {
    pub code: String,
    pub title: String,
}

/// "Born before" evidence of immunity.
#[derive(Debug, Clone)]
pub struct BirthDateImmunity {
    pub immunity_birth_date: NaiveDate,
    /// Country of birth required ("U.S."), if any.
    pub birth_country: Option<String>,
    /// Observations that exclude the patient from this evidence.
    pub exclusions: Vec<CodeTitle>,
}

/// An antigen-level contraindication.
#[derive(Debug, Clone)]
pub struct AntigenContraindication {
    pub observation: CodeTitle,
    pub text: String,
    pub guidance: Option<String>,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
}

/// A vaccine-level contraindication.
#[derive(Debug, Clone)]
pub struct VaccineContraindication {
    pub observation: CodeTitle,
    pub text: String,
    pub guidance: Option<String>,
    pub vaccines: Vec<ContraindicatedVaccine>,
}

/// A vaccine named by a vaccine contraindication.
#[derive(Debug, Clone)]
pub struct ContraindicatedVaccine {
    pub vaccine_type: String,
    pub cvx: String,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
}

/// Series type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeriesType {
    Standard,
    Risk,
    EvaluationOnly,
}

impl SeriesType {
    /// Label as written in the supporting data.
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::Risk => "Risk",
            Self::EvaluationOnly => "Evaluation Only",
        }
    }
}

/// An antigen series.
#[derive(Debug, Clone)]
pub struct Series {
    pub name: String,
    pub target_disease: String,
    pub vaccine_group: String,
    pub admin_guidance: Vec<String>,
    pub series_type: SeriesType,
    /// Series groups equivalent to this one.
    pub equivalent_series_groups: Vec<String>,
    /// Genders the series applies to (empty = all).
    pub required_gender: Vec<String>,
    pub default_series: bool,
    pub product_path: bool,
    pub series_group_name: String,
    pub series_group: String,
    /// 'A' (highest) ... 'C'.
    pub series_priority: String,
    /// Lower is preferred; missing = least preferred.
    pub series_preference: Option<u32>,
    pub min_age_to_start: Option<Span>,
    pub max_age_to_start: Option<Span>,
    pub indications: Vec<Indication>,
    pub doses: Vec<SeriesDose>,
}

/// An indication that makes a risk series relevant.
#[derive(Debug, Clone)]
pub struct Indication {
    pub observation: CodeTitle,
    pub description: String,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
    pub guidance: Option<String>,
}

/// Effective/cessation window of a logical component instance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Effective {
    pub effective: Option<NaiveDate>,
    pub cessation: Option<NaiveDate>,
}

impl Effective {
    /// RELEVANT-1 / RELEVANT-2: does the instance apply on `date`?
    pub fn applies(&self, date: NaiveDate) -> bool {
        self.effective.is_none_or(|e| date >= e) && self.cessation.is_none_or(|c| date <= c)
    }
}

/// Age requirements of a series dose.
#[derive(Debug, Clone, Default)]
pub struct AgeSpec {
    pub abs_min_age: Option<Span>,
    pub min_age: Option<Span>,
    pub earliest_rec_age: Option<Span>,
    pub latest_rec_age: Option<Span>,
    pub max_age: Option<Span>,
    pub effective: Effective,
}

/// A preferable interval.
#[derive(Debug, Clone, Default)]
pub struct IntervalSpec {
    pub from_previous: bool,
    pub from_target_dose: Option<usize>,
    pub from_most_recent: Vec<String>,
    pub from_relevant_obs: Option<String>,
    pub abs_min_int: Option<Span>,
    pub min_int: Option<Span>,
    pub earliest_rec_int: Option<Span>,
    pub latest_rec_int: Option<Span>,
    /// Interval priority flag ("override" in the supporting data).
    pub priority: bool,
    pub effective: Effective,
}

/// An allowable interval.
#[derive(Debug, Clone, Default)]
pub struct AllowableIntervalSpec {
    pub from_previous: bool,
    pub from_target_dose: Option<usize>,
    pub abs_min_int: Option<Span>,
    pub effective: Effective,
}

/// A preferable vaccine.
#[derive(Debug, Clone)]
pub struct PreferableVaccine {
    pub vaccine_type: String,
    pub cvx: String,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
    pub trade_name: Option<String>,
    pub mvx: Option<String>,
    pub volume: Option<f64>,
    pub forecast_vaccine_type: bool,
}

/// An allowable vaccine.
#[derive(Debug, Clone)]
pub struct AllowableVaccine {
    pub vaccine_type: String,
    pub cvx: String,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
}

/// Context of a conditional skip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipContext {
    Evaluation,
    Forecast,
    Both,
}

/// AND / OR.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Logic {
    And,
    Or,
}

/// A conditional skip block.
#[derive(Debug, Clone)]
pub struct ConditionalSkip {
    pub context: SkipContext,
    pub set_logic: Logic,
    pub sets: Vec<SkipSet>,
}

/// A set of conditions.
#[derive(Debug, Clone)]
pub struct SkipSet {
    pub id: String,
    pub description: String,
    pub effective: Effective,
    pub condition_logic: Logic,
    pub conditions: Vec<SkipCondition>,
}

/// Condition types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConditionType {
    Age,
    CompletedSeries,
    Interval,
    VaccineCountByAge,
    VaccineCountByDate,
    VaccineCountByDateAndAge,
}

/// Dose count comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountLogic {
    GreaterThan,
    Equal,
    LessThan,
}

/// One condition.
#[derive(Debug, Clone)]
pub struct SkipCondition {
    pub id: String,
    pub kind: ConditionType,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
    pub interval: Option<Span>,
    pub dose_count: u32,
    /// True = only valid doses count; false = total.
    pub valid_only: bool,
    pub count_logic: CountLogic,
    pub vaccine_types: Vec<String>,
    pub series_groups: Vec<String>,
}

/// Seasonal recommendation window.
#[derive(Debug, Clone, Copy, Default)]
pub struct Seasonal {
    pub start: Option<NaiveDate>,
    pub end: Option<NaiveDate>,
}

/// A series dose.
#[derive(Debug, Clone)]
pub struct SeriesDose {
    pub dose_number: String,
    pub ages: Vec<AgeSpec>,
    pub intervals: Vec<IntervalSpec>,
    pub allowable_intervals: Vec<AllowableIntervalSpec>,
    pub preferable_vaccines: Vec<PreferableVaccine>,
    pub allowable_vaccines: Vec<AllowableVaccine>,
    pub inadvertent_vaccines: Vec<String>,
    pub conditional_skips: Vec<ConditionalSkip>,
    pub recurring: bool,
    pub seasonal: Option<Seasonal>,
}

/// Schedule supporting data.
#[derive(Debug, Clone, Default)]
pub struct ScheduleData {
    pub live_virus_conflicts: Vec<VaccineConflict>,
    /// Vaccine groups in supporting-data order.
    pub vaccine_groups: Vec<VaccineGroupData>,
    /// CVX → product and its antigens.
    pub cvx_map: BTreeMap<String, CvxEntry>,
    /// Coded observations by code.
    pub observations: BTreeMap<String, ObservationData>,
}

/// A vaccine type conflict.
#[derive(Debug, Clone)]
pub struct VaccineConflict {
    /// Conflicting (previous) vaccine CVX.
    pub previous_cvx: String,
    /// Impacted (current) vaccine CVX.
    pub current_cvx: String,
    pub begin: Option<Span>,
    pub min_end: Option<Span>,
    pub end: Option<Span>,
}

/// A vaccine group.
#[derive(Debug, Clone)]
pub struct VaccineGroupData {
    pub name: String,
    pub administer_full_vaccine_group: bool,
    pub antigens: Vec<String>,
}

/// A CVX code's antigen associations.
#[derive(Debug, Clone)]
pub struct CvxEntry {
    pub cvx: String,
    pub short_description: String,
    pub associations: Vec<CvxAssociation>,
}

/// One antigen in a CVX code.
#[derive(Debug, Clone)]
pub struct CvxAssociation {
    pub antigen: String,
    pub begin_age: Option<Span>,
    pub end_age: Option<Span>,
}

/// A coded observation (indication, contraindication, immunity evidence).
#[derive(Debug, Clone)]
pub struct ObservationData {
    pub code: String,
    pub title: String,
    pub group: Option<String>,
    pub indication_text: Option<String>,
    pub contraindication_text: Option<String>,
    pub clarifying_text: Option<String>,
    /// SNOMED / CDCPHINVS / CVX codes that map to this observation.
    pub coded_values: Vec<(String, String, String)>,
}

impl SupportingData {
    /// The embedded supporting data, parsed on first use.
    pub fn get() -> &'static SupportingData {
        static DATA: OnceLock<SupportingData> = OnceLock::new();
        DATA.get_or_init(|| Self::parse_embedded().expect("embedded CDSi supporting data parses"))
    }

    /// Parse the embedded XML files.
    pub fn parse_embedded() -> Result<Self, String> {
        let mut antigens = BTreeMap::new();
        for (name, xml) in ANTIGEN_FILES {
            let a = parse_antigen(xml).map_err(|e| format!("{name}: {e}"))?;
            antigens.insert(a.name.clone(), a);
        }
        let schedule =
            parse_schedule(SCHEDULE_FILE.1).map_err(|e| format!("{}: {e}", SCHEDULE_FILE.0))?;
        Ok(Self { antigens, schedule })
    }

    /// The vaccine group that classifies an antigen.
    pub fn vaccine_group_of(&self, antigen: &str) -> Option<&VaccineGroupData> {
        self.schedule
            .vaccine_groups
            .iter()
            .find(|g| g.antigens.iter().any(|a| a == antigen))
    }

    /// A vaccine group by name (case-insensitive).
    pub fn vaccine_group(&self, name: &str) -> Option<&VaccineGroupData> {
        let n = name.trim();
        self.schedule
            .vaccine_groups
            .iter()
            .find(|g| g.name.eq_ignore_ascii_case(n))
    }
}

// ------------------------------------------------------------------ mini DOM

#[derive(Debug, Default)]
struct Node {
    name: String,
    text: String,
    children: Vec<Node>,
}

impl Node {
    fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// Trimmed text of a child, `None` when empty or "n/a".
    fn text_of(&self, name: &str) -> Option<String> {
        self.child(name).and_then(|c| non_empty(&c.text))
    }
    fn span_of(&self, name: &str) -> Option<Span> {
        self.child(name).and_then(|c| Span::parse(&c.text))
    }
    fn date_of(&self, name: &str) -> Option<NaiveDate> {
        self.child(name).and_then(|c| parse_date(&c.text))
    }
    fn effective(&self) -> Effective {
        Effective {
            effective: self.date_of("effectiveDate"),
            cessation: self.date_of("cessationDate"),
        }
    }
}

fn non_empty(s: &str) -> Option<String> {
    let t = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if t.is_empty() || t.eq_ignore_ascii_case("n/a") {
        None
    } else {
        Some(t)
    }
}

fn resolve_ref(r: &BytesRef<'_>) -> String {
    if let Ok(Some(c)) = r.resolve_char_ref() {
        return c.to_string();
    }
    match r.xml10_content().as_ref() {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => "",
    }
    .to_string()
}

fn parse_dom(xml: &str) -> Result<Node, String> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Node> = vec![Node::default()];
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) => stack.push(Node {
                name: e.local_name().as_ref().to_string(),
                ..Node::default()
            }),
            Event::Empty(e) => {
                let node = Node {
                    name: e.local_name().as_ref().to_string(),
                    ..Node::default()
                };
                stack
                    .last_mut()
                    .ok_or("unbalanced XML")?
                    .children
                    .push(node);
            }
            Event::End(_) => {
                let node = stack.pop().ok_or("unbalanced XML")?;
                stack
                    .last_mut()
                    .ok_or("unbalanced XML")?
                    .children
                    .push(node);
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&t.xml10_content());
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(t.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&resolve_ref(&r));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop().ok_or("empty XML")?;
    if !stack.is_empty() {
        return Err("unbalanced XML".into());
    }
    root.children
        .pop()
        .filter(|_| root.children.is_empty())
        .ok_or_else(|| "expected exactly one root element".to_string())
}

fn code_title(node: &Node, code: &str, title: &str) -> CodeTitle {
    CodeTitle {
        code: node.text_of(code).unwrap_or_default(),
        title: node.text_of(title).unwrap_or_default(),
    }
}

fn yes(text: Option<String>) -> bool {
    text.is_some_and(|t| matches!(t.to_ascii_lowercase().as_str(), "y" | "yes"))
}

fn split_list(text: Option<String>) -> Vec<String> {
    text.map(|t| {
        t.split([';', ','])
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    })
    .unwrap_or_default()
}

fn parse_antigen(xml: &str) -> Result<AntigenData, String> {
    let root = parse_dom(xml)?;
    if root.name != "antigenSupportingData" {
        return Err(format!("unexpected root element {}", root.name));
    }
    let mut immunity = Immunity::default();
    if let Some(imm) = root.child("immunity") {
        for ch in imm.children("clinicalHistory") {
            let ct = code_title(ch, "guidelineCode", "guidelineTitle");
            if !ct.code.is_empty() {
                immunity.clinical_history.push(ct);
            }
        }
        for dob in imm.children("dateOfBirth") {
            if let Some(date) = dob.date_of("immunityBirthDate") {
                immunity.birth_date.push(BirthDateImmunity {
                    immunity_birth_date: date,
                    birth_country: dob.text_of("birthCountry"),
                    exclusions: dob
                        .children("exclusion")
                        .map(|e| code_title(e, "exclusionCode", "exclusionTitle"))
                        .filter(|c| !c.code.is_empty())
                        .collect(),
                });
            }
        }
    }
    let mut antigen_contraindications = Vec::new();
    let mut vaccine_contraindications = Vec::new();
    if let Some(ci) = root.child("contraindications") {
        if let Some(vg) = ci.child("vaccineGroup") {
            for c in vg.children("contraindication") {
                let observation = code_title(c, "observationCode", "observationTitle");
                if observation.code.is_empty() {
                    continue;
                }
                antigen_contraindications.push(AntigenContraindication {
                    observation,
                    text: c.text_of("contraindicationText").unwrap_or_default(),
                    guidance: c.text_of("contraindicationGuidance"),
                    begin_age: c.span_of("beginAge"),
                    end_age: c.span_of("endAge"),
                });
            }
        }
        if let Some(v) = ci.child("vaccine") {
            for c in v.children("contraindication") {
                let observation = code_title(c, "observationCode", "observationTitle");
                if observation.code.is_empty() {
                    continue;
                }
                vaccine_contraindications.push(VaccineContraindication {
                    observation,
                    text: c.text_of("contraindicationText").unwrap_or_default(),
                    guidance: c.text_of("contraindicationGuidance"),
                    vaccines: c
                        .children("contraindicatedVaccine")
                        .filter_map(|cv| {
                            Some(ContraindicatedVaccine {
                                vaccine_type: cv.text_of("vaccineType").unwrap_or_default(),
                                cvx: normalize_cvx(&cv.text_of("cvx")?),
                                begin_age: cv.span_of("beginAge"),
                                end_age: cv.span_of("endAge"),
                            })
                        })
                        .collect(),
                });
            }
        }
    }
    let mut series = Vec::new();
    for s in root.children("series") {
        series.push(parse_series(s)?);
    }
    let name = series
        .first()
        .map(|s| s.target_disease.clone())
        .ok_or("no series")?;
    Ok(AntigenData {
        name,
        immunity,
        antigen_contraindications,
        vaccine_contraindications,
        series,
    })
}

fn parse_series(s: &Node) -> Result<Series, String> {
    let name = s.text_of("seriesName").ok_or("series without a name")?;
    let series_type = match s.text_of("seriesType").as_deref() {
        Some("Standard") => SeriesType::Standard,
        Some("Risk") => SeriesType::Risk,
        Some("Evaluation Only") => SeriesType::EvaluationOnly,
        other => return Err(format!("{name}: unknown series type {other:?}")),
    };
    let sel = s.child("selectSeries");
    let sel_text = |n: &str| sel.and_then(|x| x.text_of(n));
    let sel_span = |n: &str| sel.and_then(|x| x.span_of(n));
    let indications = s
        .children("indication")
        .filter_map(|i| {
            let obs = i.child("observationCode")?;
            let code = obs.text_of("code")?;
            Some(Indication {
                observation: CodeTitle {
                    code,
                    title: obs.text_of("text").unwrap_or_default(),
                },
                description: i.text_of("description").unwrap_or_default(),
                begin_age: i.span_of("beginAge"),
                end_age: i.span_of("endAge"),
                guidance: i.text_of("guidance"),
            })
        })
        .collect();
    let mut doses = Vec::new();
    for d in s.children("seriesDose") {
        doses.push(parse_dose(d).map_err(|e| format!("{name}: {e}"))?);
    }
    Ok(Series {
        target_disease: s.text_of("targetDisease").unwrap_or_default(),
        vaccine_group: s.text_of("vaccineGroup").unwrap_or_default(),
        admin_guidance: s
            .children("seriesAdminGuidance")
            .filter_map(|g| non_empty(&g.text))
            .collect(),
        series_type,
        equivalent_series_groups: split_list(s.text_of("equivalentSeriesGroups")),
        required_gender: s
            .children("requiredGender")
            .filter_map(|g| non_empty(&g.text))
            .collect(),
        default_series: yes(sel_text("defaultSeries")),
        product_path: yes(sel_text("productPath")),
        series_group_name: sel_text("seriesGroupName").unwrap_or_default(),
        series_group: sel_text("seriesGroup").unwrap_or_default(),
        series_priority: sel_text("seriesPriority").unwrap_or_else(|| "A".into()),
        series_preference: sel_text("seriesPreference").and_then(|p| p.parse().ok()),
        min_age_to_start: sel_span("minAgeToStart"),
        max_age_to_start: sel_span("maxAgeToStart"),
        indications,
        doses,
        name,
    })
}

fn parse_dose(d: &Node) -> Result<SeriesDose, String> {
    let dose_number = d.text_of("doseNumber").unwrap_or_default();
    let ages = d
        .children("age")
        .filter(|a| !a.children.is_empty())
        .map(|a| AgeSpec {
            abs_min_age: a.span_of("absMinAge"),
            min_age: a.span_of("minAge"),
            earliest_rec_age: a.span_of("earliestRecAge"),
            latest_rec_age: a.span_of("latestRecAge"),
            max_age: a.span_of("maxAge"),
            effective: a.effective(),
        })
        .collect();
    let intervals = d
        .children("interval")
        .filter(|i| !i.children.is_empty())
        .map(|i| IntervalSpec {
            from_previous: yes(i.text_of("fromPrevious")),
            from_target_dose: i.text_of("fromTargetDose").and_then(|t| t.parse().ok()),
            from_most_recent: split_list(i.text_of("fromMostRecent"))
                .iter()
                .map(|c| normalize_cvx(c))
                .collect(),
            from_relevant_obs: i.child("fromRelevantObs").and_then(|o| o.text_of("code")),
            abs_min_int: i.span_of("absMinInt"),
            min_int: i.span_of("minInt"),
            earliest_rec_int: i.span_of("earliestRecInt"),
            latest_rec_int: i.span_of("latestRecInt"),
            priority: i.text_of("intervalPriority").is_some_and(|p| {
                matches!(p.to_ascii_lowercase().as_str(), "override" | "y" | "yes")
            }),
            effective: i.effective(),
        })
        .collect();
    let allowable_intervals = d
        .children("allowableInterval")
        .filter(|i| !i.children.is_empty())
        .map(|i| AllowableIntervalSpec {
            from_previous: yes(i.text_of("fromPrevious")),
            from_target_dose: i.text_of("fromTargetDose").and_then(|t| t.parse().ok()),
            abs_min_int: i.span_of("absMinInt"),
            effective: i.effective(),
        })
        .collect();
    let preferable_vaccines = d
        .children("preferableVaccine")
        .filter_map(|p| {
            Some(PreferableVaccine {
                cvx: normalize_cvx(&p.text_of("cvx")?),
                vaccine_type: p.text_of("vaccineType").unwrap_or_default(),
                begin_age: p.span_of("beginAge"),
                end_age: p.span_of("endAge"),
                trade_name: p.text_of("tradeName"),
                mvx: p.text_of("mvx"),
                volume: p.text_of("volume").and_then(|v| v.parse().ok()),
                forecast_vaccine_type: yes(p.text_of("forecastVaccineType")),
            })
        })
        .collect();
    let allowable_vaccines = d
        .children("allowableVaccine")
        .filter_map(|p| {
            Some(AllowableVaccine {
                cvx: normalize_cvx(&p.text_of("cvx")?),
                vaccine_type: p.text_of("vaccineType").unwrap_or_default(),
                begin_age: p.span_of("beginAge"),
                end_age: p.span_of("endAge"),
            })
        })
        .collect();
    let inadvertent_vaccines = d
        .children("inadvertentVaccine")
        .filter_map(|p| p.text_of("cvx").map(|c| normalize_cvx(&c)))
        .collect();
    let mut conditional_skips = Vec::new();
    for cs in d.children("conditionalSkip") {
        if cs.children.is_empty() {
            continue;
        }
        let context = match cs.text_of("context").as_deref() {
            Some("Evaluation") => SkipContext::Evaluation,
            Some("Forecast") => SkipContext::Forecast,
            Some("Both") => SkipContext::Both,
            _ => continue,
        };
        let set_logic = parse_logic(cs.text_of("setLogic"));
        let mut sets = Vec::new();
        for st in cs.children("set") {
            let mut conditions = Vec::new();
            for c in st.children("condition") {
                conditions.push(parse_condition(c)?);
            }
            sets.push(SkipSet {
                id: st.text_of("setID").unwrap_or_default(),
                description: st.text_of("setDescription").unwrap_or_default(),
                effective: st.effective(),
                condition_logic: parse_logic(st.text_of("conditionLogic")),
                conditions,
            });
        }
        conditional_skips.push(ConditionalSkip {
            context,
            set_logic,
            sets,
        });
    }
    let seasonal = d.child("seasonalRecommendation").and_then(|s| {
        let start = s.date_of("startDate");
        let end = s.date_of("endDate");
        (start.is_some() || end.is_some()).then_some(Seasonal { start, end })
    });
    Ok(SeriesDose {
        dose_number,
        ages,
        intervals,
        allowable_intervals,
        preferable_vaccines,
        allowable_vaccines,
        inadvertent_vaccines,
        conditional_skips,
        recurring: yes(d.text_of("recurringDose")),
        seasonal,
    })
}

fn parse_logic(text: Option<String>) -> Logic {
    match text.as_deref().map(str::to_ascii_uppercase).as_deref() {
        Some("AND") => Logic::And,
        // A single set or condition ("n/a") behaves the same under OR.
        _ => Logic::Or,
    }
}

fn parse_condition(c: &Node) -> Result<SkipCondition, String> {
    let kind_text = c.text_of("conditionType").unwrap_or_default();
    let kind = match kind_text.to_ascii_lowercase().as_str() {
        "age" => ConditionType::Age,
        "completed series" => ConditionType::CompletedSeries,
        "interval" => ConditionType::Interval,
        "vaccine count by age" => ConditionType::VaccineCountByAge,
        "vaccine count by date" => ConditionType::VaccineCountByDate,
        "vaccine count by date and age" => ConditionType::VaccineCountByDateAndAge,
        other => return Err(format!("unknown conditional skip type {other:?}")),
    };
    let count_logic = match c
        .text_of("doseCountLogic")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "less than" => CountLogic::LessThan,
        "equal to" | "equal" => CountLogic::Equal,
        _ => CountLogic::GreaterThan,
    };
    Ok(SkipCondition {
        id: c.text_of("conditionID").unwrap_or_default(),
        kind,
        start_date: c.date_of("startDate"),
        end_date: c.date_of("endDate"),
        begin_age: c.span_of("beginAge"),
        end_age: c.span_of("endAge"),
        interval: c.span_of("interval"),
        dose_count: c
            .text_of("doseCount")
            .and_then(|n| n.parse().ok())
            .unwrap_or(0),
        valid_only: c
            .text_of("doseType")
            .is_some_and(|t| t.eq_ignore_ascii_case("valid")),
        count_logic,
        vaccine_types: split_list(c.text_of("vaccineTypes"))
            .iter()
            .map(|v| normalize_cvx(v))
            .collect(),
        series_groups: split_list(c.text_of("seriesGroups")),
    })
}

/// Normalize a CVX code: trim and drop leading zeros ("03" → "3").
pub fn normalize_cvx(cvx: &str) -> String {
    let t = cvx.trim();
    let stripped = t.trim_start_matches('0');
    if stripped.is_empty() && !t.is_empty() {
        "0".into()
    } else {
        stripped.to_string()
    }
}

fn parse_schedule(xml: &str) -> Result<ScheduleData, String> {
    let root = parse_dom(xml)?;
    if root.name != "scheduleSupportingData" {
        return Err(format!("unexpected root element {}", root.name));
    }
    let mut out = ScheduleData::default();
    if let Some(lvc) = root.child("liveVirusConflicts") {
        for c in lvc.children("liveVirusConflict") {
            let prev = c
                .child("previous")
                .and_then(|p| p.text_of("cvx"))
                .map(|x| normalize_cvx(&x));
            let cur = c
                .child("current")
                .and_then(|p| p.text_of("cvx"))
                .map(|x| normalize_cvx(&x));
            if let (Some(previous_cvx), Some(current_cvx)) = (prev, cur) {
                out.live_virus_conflicts.push(VaccineConflict {
                    previous_cvx,
                    current_cvx,
                    begin: c.span_of("conflictBeginInterval"),
                    min_end: c.span_of("minConflictEndInterval"),
                    end: c.span_of("conflictEndInterval"),
                });
            }
        }
    }
    let mut group_antigens: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(m) = root.child("vaccineGroupToAntigenMap") {
        for g in m.children("vaccineGroupMap") {
            if let Some(name) = g.text_of("name") {
                group_antigens.insert(
                    name,
                    g.children("antigen")
                        .filter_map(|a| non_empty(&a.text))
                        .collect(),
                );
            }
        }
    }
    if let Some(vgs) = root.child("vaccineGroups") {
        for g in vgs.children("vaccineGroup") {
            if let Some(name) = g.text_of("name") {
                out.vaccine_groups.push(VaccineGroupData {
                    antigens: group_antigens.get(&name).cloned().unwrap_or_default(),
                    administer_full_vaccine_group: yes(g.text_of("administerFullVaccineGroup")),
                    name,
                });
            }
        }
    }
    if let Some(m) = root.child("cvxToAntigenMap") {
        for c in m.children("cvxMap") {
            let Some(cvx) = c.text_of("cvx").map(|x| normalize_cvx(&x)) else {
                continue;
            };
            out.cvx_map.insert(
                cvx.clone(),
                CvxEntry {
                    cvx,
                    short_description: c.text_of("shortDescription").unwrap_or_default(),
                    associations: c
                        .children("association")
                        .filter_map(|a| {
                            Some(CvxAssociation {
                                antigen: a.text_of("antigen")?,
                                begin_age: a.span_of("associationBeginAge"),
                                end_age: a.span_of("associationEndAge"),
                            })
                        })
                        .collect(),
                },
            );
        }
    }
    if let Some(obs) = root.child("observations") {
        for o in obs.children("observation") {
            let Some(code) = o.text_of("observationCode") else {
                continue;
            };
            let coded_values = o
                .child("codedValues")
                .map(|cv| {
                    cv.children("codedValue")
                        .filter_map(|v| {
                            Some((
                                v.text_of("code")?,
                                v.text_of("codeSystem").unwrap_or_default(),
                                v.text_of("text").unwrap_or_default(),
                            ))
                        })
                        .collect()
                })
                .unwrap_or_default();
            out.observations.insert(
                code.clone(),
                ObservationData {
                    code,
                    title: o.text_of("observationTitle").unwrap_or_default(),
                    group: o.text_of("group"),
                    indication_text: o.text_of("indicationText"),
                    contraindication_text: o.text_of("contraindicationText"),
                    clarifying_text: o.text_of("clarifyingText"),
                    coded_values,
                },
            );
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_every_embedded_file() {
        let sd = SupportingData::parse_embedded().unwrap();
        assert_eq!(sd.antigens.len(), 30);
        let hib = &sd.antigens["Hib"];
        assert_eq!(hib.series.len(), 8);
        let s = &hib.series[0];
        assert_eq!(s.name, "Hib start at 2 months 4-dose series");
        assert!(s.default_series);
        assert_eq!(s.series_type, SeriesType::Standard);
        assert_eq!(s.doses.len(), 4);
        assert_eq!(
            s.doses[0].ages[0].abs_min_age,
            Span::parse("6 weeks - 4 days")
        );
        assert!(s.doses[0].preferable_vaccines.iter().any(|p| p.cvx == "48"));
        assert_eq!(sd.schedule.cvx_map["3"].associations.len(), 3);
        assert!(sd.schedule.live_virus_conflicts.len() > 100);
        let mmr = sd.vaccine_group("mmr").unwrap();
        assert!(mmr.administer_full_vaccine_group);
        assert_eq!(mmr.antigens, vec!["Measles", "Mumps", "Rubella"]);
        assert_eq!(
            sd.vaccine_group_of("Pertussis").unwrap().name,
            "DTaP/Tdap/Td"
        );
        assert!(sd.schedule.observations.contains_key("080"));
        // Character references are decoded.
        assert!(
            sd.schedule
                .observations
                .values()
                .any(|o| o.title.contains('≥')
                    || o.indication_text.as_deref().unwrap_or("").contains('≥'))
        );
    }

    #[test]
    fn embedded_files_match_the_manifest_checksums() {
        let manifest: serde_json::Value = serde_json::from_str(MANIFEST_JSON).unwrap();
        let files = manifest["files"].as_array().unwrap();
        assert_eq!(files.len(), ANTIGEN_FILES.len() + 1);
        for f in files {
            let path = f["path"].as_str().unwrap();
            let text = ANTIGEN_FILES
                .iter()
                .chain(std::iter::once(&SCHEDULE_FILE))
                .find(|(p, _)| *p == path)
                .map(|(_, t)| *t)
                .unwrap_or_else(|| panic!("{path} not embedded"));
            use sha2::Digest;
            let actual = hex::encode(sha2::Sha256::digest(text.as_bytes()));
            assert_eq!(actual, f["sha256"].as_str().unwrap(), "{path}");
        }
        assert_eq!(manifest["version"], SUPPORTING_DATA_VERSION);
    }
}
