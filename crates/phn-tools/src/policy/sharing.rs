//! POL-07 `health_information_sharing_guide`: a decision guide for which rules govern a record and
//! what questions decide whether it may be shared with someone. It asks structured questions (who
//! holds the record and in what role, what kind of record, the recipient, the purpose, consent)
//! and returns the regimes that may govern (HIPAA, FERPA, 42 CFR Part 2, the Massachusetts public
//! records law, 105 CMR 300.000 and the Massachusetts health confidentiality statutes, the
//! Massachusetts student records provisions), each with provisions, short quotations, cautions, and
//! facts to confirm. It never says that a disclosure is or is not permitted.

use std::collections::BTreeMap;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::epr::content::{self, Confidence, DocSource};
use crate::policy::shared::optional_text;

/// Content id of `content/policy/information_sharing.yaml`.
pub const SHARING: &str = "policy-information-sharing";

const MAX_NOTES: usize = 500;
const MAX_PROVISIONS_PER_REGIME: usize = 8;

// ---------------------------------------------------------------- content

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadScope {
    pub read_in_full: Vec<String>,
    pub read_in_part: Vec<String>,
    pub not_read: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuestionDef {
    pub id: String,
    pub question: String,
    pub why: String,
    pub choices: Vec<Choice>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RegimeDef {
    pub id: String,
    pub name: String,
    pub what_it_is: String,
    pub depends_on: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProvisionDef {
    pub id: String,
    pub regime: String,
    pub citation: String,
    pub source: String,
    pub quote: String,
    pub paraphrase: String,
    pub confidence: Confidence,
    pub purposes: Vec<String>,
    pub recipients: Vec<String>,
    pub record_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossRule {
    pub id: String,
    pub text: String,
    pub regimes: Vec<String>,
    pub sources: Vec<String>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReferralDef {
    pub text: String,
    pub contacts: Vec<String>,
}

/// `content/policy/information_sharing.yaml`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharingContent {
    pub meta: ContentMeta,
    pub sources: BTreeMap<String, DocSource>,
    pub read_scope: ReadScope,
    pub questions: Vec<QuestionDef>,
    pub regimes: Vec<RegimeDef>,
    pub provisions: Vec<ProvisionDef>,
    pub cross_rules: Vec<CrossRule>,
    pub referral: ReferralDef,
    pub limits: Vec<String>,
}

// ---------------------------------------------------------------- arguments

/// Who holds the record, and in what role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum HolderRole {
    /// A health care provider or plan that transmits health information electronically.
    HealthCareProvider,
    /// A local board of health or health department.
    LocalHealthDepartment,
    /// A school nurse in a public school or district.
    SchoolNursePublic,
    /// A school nurse in a private or independent school.
    SchoolNursePrivate,
    /// A program that diagnoses, treats, or refers for substance use disorder.
    SubstanceUseProgram,
    /// A state agency such as the Department of Public Health.
    StateAgency,
    /// Something else or not sure.
    Other,
}

impl HolderRole {
    fn key(self) -> &'static str {
        match self {
            HolderRole::HealthCareProvider => "health_care_provider",
            HolderRole::LocalHealthDepartment => "local_health_department",
            HolderRole::SchoolNursePublic => "school_nurse_public",
            HolderRole::SchoolNursePrivate => "school_nurse_private",
            HolderRole::SubstanceUseProgram => "substance_use_program",
            HolderRole::StateAgency => "state_agency",
            HolderRole::Other => "other",
        }
    }
    fn is_school(self) -> bool {
        matches!(
            self,
            HolderRole::SchoolNursePublic | HolderRole::SchoolNursePrivate
        )
    }
    fn is_public_agency(self) -> bool {
        matches!(
            self,
            HolderRole::LocalHealthDepartment
                | HolderRole::SchoolNursePublic
                | HolderRole::StateAgency
        )
    }
}

/// The kind of record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RecordType {
    /// Clinical or treatment record.
    ClinicalRecord,
    /// Student health record kept by a school.
    SchoolHealthRecord,
    /// Immunization record or registry information.
    ImmunizationRecord,
    /// Disease report, case investigation, or contact tracing record.
    CommunicableDiseaseReport,
    /// HIV test result or the identity of the person tested.
    HivTestResult,
    /// Substance use disorder treatment record from a program.
    SubstanceUseTreatmentRecord,
    /// Agency administrative record (inspection, permit, minutes) that may mention people.
    AdministrativeRecord,
    /// Something else or not sure.
    Other,
}

impl RecordType {
    fn key(self) -> &'static str {
        match self {
            RecordType::ClinicalRecord => "clinical_record",
            RecordType::SchoolHealthRecord => "school_health_record",
            RecordType::ImmunizationRecord => "immunization_record",
            RecordType::CommunicableDiseaseReport => "communicable_disease_report",
            RecordType::HivTestResult => "hiv_test_result",
            RecordType::SubstanceUseTreatmentRecord => "substance_use_treatment_record",
            RecordType::AdministrativeRecord => "administrative_record",
            RecordType::Other => "other",
        }
    }
    fn is_school_kind(self) -> bool {
        matches!(
            self,
            RecordType::SchoolHealthRecord | RecordType::ImmunizationRecord
        )
    }
}

/// Who wants the information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Recipient {
    /// The person the record is about, a parent or guardian, or a personal representative.
    IndividualOrParent,
    /// A school or school nurse.
    School,
    /// Another health care provider.
    HealthCareProvider,
    /// The Department of Public Health or a local board of health.
    PublicHealthAuthority,
    /// Law enforcement, a court, or a party with a subpoena.
    LawEnforcementOrCourt,
    /// An employer.
    Employer,
    /// A researcher or evaluator.
    Researcher,
    /// A member of the public, a reporter, or any requester of public records.
    PublicRequester,
    /// A person who may have been exposed to a disease.
    ExposedPerson,
    /// Another agency or organization.
    OtherAgency,
}

impl Recipient {
    fn key(self) -> &'static str {
        match self {
            Recipient::IndividualOrParent => "individual_or_parent",
            Recipient::School => "school",
            Recipient::HealthCareProvider => "health_care_provider",
            Recipient::PublicHealthAuthority => "public_health_authority",
            Recipient::LawEnforcementOrCourt => "law_enforcement_or_court",
            Recipient::Employer => "employer",
            Recipient::Researcher => "researcher",
            Recipient::PublicRequester => "public_requester",
            Recipient::ExposedPerson => "exposed_person",
            Recipient::OtherAgency => "other_agency",
        }
    }
}

/// The purpose of the sharing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Treatment or coordination of care.
    TreatmentOrCare,
    /// Required public health reporting.
    PublicHealthReporting,
    /// Disease investigation, surveillance, or control.
    DiseaseInvestigation,
    /// Checking school immunization requirements.
    SchoolImmunization,
    /// A health or safety emergency or threat.
    EmergencyThreat,
    /// Research, evaluation, or a study.
    Research,
    /// Responding to a public records request.
    PublicRecordsRequest,
    /// A subpoena, court order, or law enforcement request.
    LegalProcess,
    /// Something else or not sure.
    Other,
}

impl Purpose {
    fn key(self) -> &'static str {
        match self {
            Purpose::TreatmentOrCare => "treatment_or_care",
            Purpose::PublicHealthReporting => "public_health_reporting",
            Purpose::DiseaseInvestigation => "disease_investigation",
            Purpose::SchoolImmunization => "school_immunization",
            Purpose::EmergencyThreat => "emergency_threat",
            Purpose::Research => "research",
            Purpose::PublicRecordsRequest => "public_records_request",
            Purpose::LegalProcess => "legal_process",
            Purpose::Other => "other",
        }
    }
}

/// Whether there is a signed consent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Consent {
    /// Signed and written.
    Written,
    /// Verbal agreement only.
    Verbal,
    /// No consent.
    None,
    /// Not sure.
    Unknown,
}

impl Consent {
    fn key(self) -> &'static str {
        match self {
            Consent::Written => "written",
            Consent::Verbal => "verbal",
            Consent::None => "none",
            Consent::Unknown => "unknown",
        }
    }
}

/// Arguments for `health_information_sharing_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct SharingArgs {
    /// Who holds the record and in what role (health_care_provider, local_health_department,
    /// school_nurse_public, school_nurse_private, substance_use_program, state_agency, other).
    pub holder_role: Option<HolderRole>,
    /// What kind of record it is (clinical_record, school_health_record, immunization_record,
    /// communicable_disease_report, hiv_test_result, substance_use_treatment_record,
    /// administrative_record, other).
    pub record_type: Option<RecordType>,
    /// Who wants or would receive it (individual_or_parent, school, health_care_provider,
    /// public_health_authority, law_enforcement_or_court, employer, researcher, public_requester,
    /// exposed_person, other_agency).
    pub recipient: Option<Recipient>,
    /// The purpose (treatment_or_care, public_health_reporting, disease_investigation,
    /// school_immunization, emergency_threat, research, public_records_request, legal_process,
    /// other).
    pub purpose: Option<Purpose>,
    /// Whether there is a signed consent from the person or parent (written, verbal, none,
    /// unknown).
    pub consent: Option<Consent>,
    /// True if the person the record is about is a minor.
    pub subject_is_minor: Option<bool>,
    /// Anything else about the situation, in general terms (no names, dates of birth, addresses,
    /// phone numbers, or record numbers).
    pub situation: Option<String>,
}

// ---------------------------------------------------------------- results

/// A question and the answer given, if any.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QuestionOut {
    pub id: String,
    pub question: String,
    pub why: String,
    /// Allowed values (value and label).
    pub choices: Vec<String>,
    /// The value you gave, or null.
    pub answer: Option<String>,
}

/// A provision with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProvisionOut {
    pub id: String,
    pub citation: String,
    pub source: String,
    pub url: String,
    /// A quotation of 25 words or fewer.
    pub quote: String,
    /// The guide's summary of the provision (read the source before relying on it).
    pub paraphrase: String,
    pub confidence: Confidence,
}

/// One regime and how it bears on the answers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RegimeOut {
    pub id: String,
    pub name: String,
    pub what_it_is: String,
    /// "likely_governs", "may_apply", "check", or "not_indicated" for the answers given
    /// ("unassessed" until holder_role and record_type are given).
    pub bears_on_your_situation: String,
    pub why: String,
    pub depends_on: String,
    pub provisions: Vec<ProvisionOut>,
    pub cautions: Vec<String>,
}

/// A note on how regimes interact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CrossRuleOut {
    pub text: String,
    pub confidence: Confidence,
}

/// Result of `health_information_sharing_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SharingResult {
    /// Always "governing_rules_and_cautions": the guide never says a disclosure is or is not
    /// permitted.
    pub answer_type: String,
    /// "needs_answers" (holder_role or record_type missing), "partial" (recipient or purpose
    /// missing), or "rules_identified".
    pub status: String,
    pub questions: Vec<QuestionOut>,
    pub regimes: Vec<RegimeOut>,
    pub how_the_rules_interact: Vec<CrossRuleOut>,
    /// Facts about your organization or the record that you must confirm.
    pub facts_to_confirm: Vec<String>,
    pub coverage: Coverage,
    pub refer_to: Referral,
    pub limits: Vec<String>,
}

/// What was and was not read.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Coverage {
    /// Always true: this is a partial snapshot.
    pub partial: bool,
    pub read_in_full: Vec<String>,
    pub read_in_part: Vec<String>,
    pub not_read: Vec<String>,
}

/// Where to take the question.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Referral {
    pub text: String,
    pub contacts: Vec<String>,
}

// ---------------------------------------------------------------- logic

fn flatten(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How a regime bears on the answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bears {
    LikelyGoverns,
    MayApply,
    Check,
    NotIndicated,
}

impl Bears {
    fn label(self) -> &'static str {
        match self {
            Bears::LikelyGoverns => "likely_governs",
            Bears::MayApply => "may_apply",
            Bears::Check => "check",
            Bears::NotIndicated => "not_indicated",
        }
    }
}

/// Decide how a regime bears on the holder and record type, with a plain reason. This is a
/// screening of which rules to read, not a conclusion about a disclosure.
pub fn bears(regime: &str, holder: HolderRole, record: RecordType, recipient: Option<Recipient>) -> (Bears, String) {
    use HolderRole as H;
    use RecordType as R;
    match regime {
        "hipaa" => {
            if holder.is_school() && record.is_school_kind() {
                (
                    Bears::NotIndicated,
                    "A student health record kept by a school is generally an education record, and HIPAA's definition of protected health information leaves out education records covered by FERPA. HIPAA can still apply to a community provider that sends information to the school, and to a covered entity that runs a school-based clinic.".into(),
                )
            } else {
                match holder {
                    H::HealthCareProvider => (
                        Bears::LikelyGoverns,
                        "A provider or plan that conducts covered electronic transactions is a covered entity. Confirm that your organization does.".into(),
                    ),
                    H::SubstanceUseProgram => (
                        Bears::MayApply,
                        "Many treatment programs are also covered entities (for example if they bill electronically). Confirm, and read Part 2 as well.".into(),
                    ),
                    H::LocalHealthDepartment => (
                        Bears::MayApply,
                        "HIPAA applies to a health department only if it (or a designated health care component of a hybrid entity) is a covered entity, for example because it provides care and conducts covered electronic transactions. Many boards of health that only do regulatory and surveillance work are not. Ask counsel.".into(),
                    ),
                    H::StateAgency => (
                        Bears::Check,
                        "Whether a state agency's program is a covered entity or component depends on what it does. A public health authority receives information under 45 CFR 164.512(b); that is different from being bound by the rule.".into(),
                    ),
                    H::SchoolNursePublic | H::SchoolNursePrivate => (
                        Bears::Check,
                        "A school is bound by HIPAA only if it is a covered entity for the record in question (for example a school-based clinic run by a billing provider). Records the school keeps as education records are outside HIPAA.".into(),
                    ),
                    H::Other => (
                        Bears::Check,
                        "Find out whether your organization is a covered entity or a business associate of one.".into(),
                    ),
                }
            }
        }
        "ferpa" => {
            if holder == H::SchoolNursePublic && (record.is_school_kind() || record == R::Other) {
                (
                    Bears::LikelyGoverns,
                    "A public school that receives U.S. Department of Education funds, keeping a record directly related to a student, is covered. Confirm the funding and that the record is maintained by the school.".into(),
                )
            } else if holder == H::SchoolNursePrivate && record.is_school_kind() {
                (
                    Bears::MayApply,
                    "FERPA covers schools that receive funds under a program the Secretary of Education administers; many private schools do not. Ask the school whether it does.".into(),
                )
            } else if recipient == Some(Recipient::School) {
                (
                    Bears::MayApply,
                    "Once a school receives and keeps the information about a student, it can become part of an education record the school must protect under FERPA. Ask the school how it will hold it.".into(),
                )
            } else {
                (
                    Bears::NotIndicated,
                    "Nothing in your answers suggests a school-maintained education record.".into(),
                )
            }
        }
        "part2" => {
            if record == R::SubstanceUseTreatmentRecord && holder == H::SubstanceUseProgram {
                (
                    Bears::LikelyGoverns,
                    "A record that identifies a patient as having a substance use disorder, held by a federally assisted program that holds itself out as providing diagnosis, treatment, or referral, is the core case for 42 CFR Part 2.".into(),
                )
            } else if record == R::SubstanceUseTreatmentRecord {
                (
                    Bears::MayApply,
                    "If the record came from a Part 2 program (or from someone who got it from one), the restrictions can follow it to you as a lawful holder. Find out where it came from and whether the notice about redisclosure came with it.".into(),
                )
            } else if holder == H::SubstanceUseProgram {
                (
                    Bears::MayApply,
                    "Records of a Part 2 program that identify a patient as having a substance use disorder are covered even if you call them clinical records. Confirm that the program is federally assisted.".into(),
                )
            } else {
                (
                    Bears::Check,
                    "Part 2 applies only to records that identify a patient as having a substance use disorder from a federally assisted program. If the record includes substance use treatment information from such a program, read it.".into(),
                )
            }
        }
        "ma_public_records" => {
            if holder.is_public_agency() {
                (
                    Bears::MayApply,
                    "Records made or received by a municipal or state agency (a board of health, a public school district, a state agency) are public records unless an exemption applies. Confidential health information is the kind of record the exemptions are about.".into(),
                )
            } else {
                (
                    Bears::NotIndicated,
                    "The definition in c. 4, section 7 describes records of agencies and political subdivisions; a private clinic, private school, or private treatment program is not described there (unless it is acting for an agency). Records it sends to an agency become the agency's records.".into(),
                )
            }
        }
        "ma_health_confidentiality" => match record {
            R::CommunicableDiseaseReport | R::HivTestResult => (
                Bears::LikelyGoverns,
                "State rules on disease reports and HIV test results (105 CMR 300.120, c. 111 sections 70F and 111) apply to these records.".into(),
            ),
            R::ImmunizationRecord => (
                Bears::LikelyGoverns,
                "State rules on the immunization registry and on school nurses' access to immunization records (c. 111 section 24M, 105 CMR 300.191) apply to immunization records.".into(),
            ),
            R::ClinicalRecord | R::SubstanceUseTreatmentRecord => (
                Bears::MayApply,
                "A patient's right of access to health records (c. 112 section 12CC) and the confidentiality sections for particular conditions (HIV, venereal disease) can apply to clinical records.".into(),
            ),
            R::SchoolHealthRecord | R::AdministrativeRecord | R::Other => (
                Bears::Check,
                "The sections read concern disease reports, HIV test results, immunization registry data, and patient records; check whether the record contains any of them.".into(),
            ),
        },
        "ma_student_records" => {
            if holder == H::SchoolNursePublic && record.is_school_kind() {
                (
                    Bears::LikelyGoverns,
                    "c. 71 sections 34D and 34E address public school student records and parents' and students' access. The student records regulation (603 CMR 23.00) could not be read.".into(),
                )
            } else if holder == H::SchoolNursePrivate {
                (
                    Bears::NotIndicated,
                    "c. 71 section 34E speaks of school committees (public schools). Ask whether the private school has its own student records policy.".into(),
                )
            } else {
                (
                    Bears::NotIndicated,
                    "Nothing in your answers suggests a public school student record.".into(),
                )
            }
        }
        _ => (Bears::Check, String::new()),
    }
}

/// Provisions to show for a regime given the answers: general ones, and those tagged with the
/// purpose, recipient, or record type. Tagged matches come first.
fn pick<'a>(
    c: &'a SharingContent,
    regime: &str,
    record: Option<RecordType>,
    recipient: Option<Recipient>,
    purpose: Option<Purpose>,
) -> Vec<&'a ProvisionDef> {
    let rk = record.map(RecordType::key);
    let ck = recipient.map(Recipient::key);
    let pk = purpose.map(Purpose::key);
    let mut tagged: Vec<(usize, &ProvisionDef)> = Vec::new();
    let mut general: Vec<&ProvisionDef> = Vec::new();
    for p in c.provisions.iter().filter(|p| p.regime == regime) {
        let n = usize::from(pk.is_some_and(|k| p.purposes.iter().any(|x| x == k)))
            + usize::from(ck.is_some_and(|k| p.recipients.iter().any(|x| x == k)))
            + usize::from(rk.is_some_and(|k| p.record_types.iter().any(|x| x == k)));
        let untagged = p.purposes.is_empty() && p.recipients.is_empty() && p.record_types.is_empty();
        if n > 0 {
            tagged.push((n, p));
        } else if untagged {
            general.push(p);
        }
    }
    tagged.sort_by(|a, b| b.0.cmp(&a.0));
    tagged
        .into_iter()
        .map(|(_, p)| p)
        .chain(general)
        .take(MAX_PROVISIONS_PER_REGIME)
        .collect()
}

fn cautions(
    regime: &str,
    a: &SharingArgs,
    bears: Bears,
) -> Vec<String> {
    let mut v = Vec::new();
    if bears == Bears::NotIndicated {
        return v;
    }
    match regime {
        "hipaa" => {
            if matches!(a.purpose, Some(Purpose::PublicHealthReporting | Purpose::DiseaseInvestigation))
                && a.recipient != Some(Recipient::PublicHealthAuthority)
                && a.recipient.is_some()
            {
                v.push("HIPAA's public health permission is for disclosures to a public health authority (or a person at risk of exposure when authorized by law); check that your recipient is one (45 CFR 164.501, 164.512(b)).".into());
            }
            if a.purpose == Some(Purpose::LegalProcess) {
                v.push("A subpoena does not always satisfy 45 CFR 164.512(e); the paragraph sets conditions for each kind of request. Ask counsel before responding.".into());
            }
            if a.consent == Some(Consent::Verbal) {
                v.push("Verbal agreement is mentioned in 45 CFR 164.512 for certain permitted disclosures; other disclosures need a written authorization (164.508, not read).".into());
            }
            v.push("Minimum necessary applies to most disclosures (45 CFR 164.502(b)).".into());
        }
        "ferpa" => {
            if matches!(a.consent, Some(Consent::Verbal | Consent::None)) {
                v.push("FERPA requires signed and dated written consent unless an exception in 34 CFR 99.31 applies (99.30(a)). Verbal agreement does not meet it.".into());
            }
            if a.purpose == Some(Purpose::EmergencyThreat) {
                v.push("The health or safety emergency exception (34 CFR 99.36) turns on a threat to the health or safety of the student or others; the school makes that determination on the information available.".into());
            }
        }
        "part2" => {
            if matches!(a.consent, Some(Consent::Verbal | Consent::None | Consent::Unknown)) {
                v.push("Disclosures with consent need written consent that meets 42 CFR 2.31 (not read here) and carry the notice in 2.32; a verbal agreement is not consent under 2.33.".into());
            }
            if a.purpose == Some(Purpose::LegalProcess) || a.recipient == Some(Recipient::LawEnforcementOrCourt) {
                v.push("Part 2 restrictions apply even when the requester has a subpoena or is a government official (42 CFR 2.13(b)). Court orders are in subpart E, which was not read; ask counsel.".into());
            }
            if matches!(a.purpose, Some(Purpose::PublicHealthReporting | Purpose::DiseaseInvestigation)) {
                v.push("The public health provision (42 CFR 2.54) allows disclosure to a public health authority only of de-identified records.".into());
            }
            v.push("The 2024 final rule's compliance dates and any court rulings were not verified.".into());
        }
        "ma_public_records" => {
            if a.recipient == Some(Recipient::PublicRequester) || a.purpose == Some(Purpose::PublicRecordsRequest) {
                v.push("The agency's records access officer answers the request in writing within 10 business days (c. 66, section 10); if the agency will not provide a record it must say so in writing, and the requester may appeal (section 10A). Ask your records access officer and counsel about exemptions.".into());
            }
        }
        "ma_health_confidentiality" => {
            if a.record_type == Some(RecordType::HivTestResult) {
                v.push("HIV test results and the identity of the person tested need the subject's written informed consent under c. 111, section 70F, with a form that states the purpose and is separate from other releases. A signed general release may not satisfy it.".into());
            }
            if a.record_type == Some(RecordType::CommunicableDiseaseReport)
                && (a.recipient == Some(Recipient::PublicRequester) || a.purpose == Some(Purpose::PublicRecordsRequest))
            {
                v.push("105 CMR 300.120 treats personally identifying disease report information as confidential; do not release it in response to a public records request without counsel's review.".into());
            }
        }
        _ => {}
    }
    if a.subject_is_minor == Some(true) && matches!(regime, "hipaa" | "ferpa" | "ma_student_records") {
        v.push("The person is a minor: check who may exercise the rights (a parent or guardian, the minor, or both). 45 CFR 164.502(g) covers personal representatives and c. 71, section 34E covers school records; Massachusetts law on minors' consent to their own care was not read.".into());
    }
    v
}

// ---------------------------------------------------------------- the tool

/// Run the guide.
pub fn health_information_sharing_guide(
    _ctx: &Context,
    args: SharingArgs,
) -> Result<Envelope<SharingResult>> {
    let c: SharingContent = content::load(SHARING)?;
    // Free text is checked for identifiers and refused without echo.
    let situation = optional_text("situation", &args.situation, MAX_NOTES)?;

    let answers: BTreeMap<&str, Option<&'static str>> = BTreeMap::from([
        ("holder_role", args.holder_role.map(HolderRole::key)),
        ("record_type", args.record_type.map(RecordType::key)),
        ("recipient", args.recipient.map(Recipient::key)),
        ("purpose", args.purpose.map(Purpose::key)),
        ("consent", args.consent.map(Consent::key)),
    ]);
    let questions: Vec<QuestionOut> = c
        .questions
        .iter()
        .map(|q| QuestionOut {
            id: q.id.clone(),
            question: q.question.clone(),
            why: flatten(&q.why),
            choices: q
                .choices
                .iter()
                .map(|x| format!("{}: {}", x.value, x.label))
                .collect(),
            answer: answers.get(q.id.as_str()).copied().flatten().map(String::from),
        })
        .collect();

    let status = match (args.holder_role, args.record_type) {
        (Some(_), Some(_)) => {
            if args.recipient.is_some() && args.purpose.is_some() {
                "rules_identified"
            } else {
                "partial"
            }
        }
        _ => "needs_answers",
    };

    let mut regimes: Vec<RegimeOut> = Vec::new();
    let mut facts: Vec<String> = Vec::new();
    for r in &c.regimes {
        let (bears_label, why, provisions, cautions_v) = match (args.holder_role, args.record_type) {
            (Some(h), Some(rt)) => {
                let (b, why) = bears(&r.id, h, rt, args.recipient);
                let provs: Vec<&ProvisionDef> = if b == Bears::NotIndicated {
                    // Only the provision that explains the exclusion.
                    c.provisions
                        .iter()
                        .filter(|p| p.regime == r.id && p.id == "hipaa_phi_excludes_ferpa")
                        .collect()
                } else {
                    pick(&c, &r.id, Some(rt), args.recipient, args.purpose)
                };
                if matches!(b, Bears::MayApply | Bears::Check | Bears::LikelyGoverns) {
                    facts.push(format!("{}: {}", r.name, flatten(&r.depends_on)));
                }
                (b.label().to_string(), why, provs, cautions(&r.id, &args, b))
            }
            _ => (
                "unassessed".to_string(),
                "Give holder_role and record_type to see how this regime bears on your situation."
                    .to_string(),
                Vec::new(),
                Vec::new(),
            ),
        };
        regimes.push(RegimeOut {
            id: r.id.clone(),
            name: r.name.clone(),
            what_it_is: flatten(&r.what_it_is),
            bears_on_your_situation: bears_label,
            why,
            depends_on: flatten(&r.depends_on),
            provisions: provisions
                .into_iter()
                .map(|p| {
                    let src = c.sources.get(&p.source);
                    ProvisionOut {
                        id: p.id.clone(),
                        citation: p.citation.clone(),
                        source: src
                            .map(|s| match &s.effective_date {
                                Some(d) => format!("{} (as of {d})", s.title),
                                None => s.title.clone(),
                            })
                            .unwrap_or_else(|| p.source.clone()),
                        url: src.map(|s| s.url.clone()).unwrap_or_default(),
                        quote: p.quote.clone(),
                        paraphrase: flatten(&p.paraphrase),
                        confidence: p.confidence,
                    }
                })
                .collect(),
            cautions: cautions_v,
        });
    }
    // Likely-governing regimes first, then may apply, check, not indicated.
    let rank = |s: &str| match s {
        "likely_governs" => 0,
        "may_apply" => 1,
        "check" => 2,
        "unassessed" => 3,
        _ => 4,
    };
    regimes.sort_by_key(|r| rank(&r.bears_on_your_situation));

    let shown_ids: Vec<&str> = regimes
        .iter()
        .filter(|r| r.bears_on_your_situation != "not_indicated" && r.bears_on_your_situation != "unassessed")
        .map(|r| r.id.as_str())
        .collect();
    let cross: Vec<CrossRuleOut> = c
        .cross_rules
        .iter()
        .filter(|x| shown_ids.is_empty() || x.regimes.iter().filter(|g| shown_ids.contains(&g.as_str())).count() >= 1)
        .map(|x| CrossRuleOut {
            text: flatten(&x.text),
            confidence: x.confidence,
        })
        .collect();

    let coverage = Coverage {
        partial: true,
        read_in_full: c.read_scope.read_in_full.clone(),
        read_in_part: c.read_scope.read_in_part.clone(),
        not_read: c.read_scope.not_read.clone(),
    };
    let referral = Referral {
        text: flatten(&c.referral.text),
        contacts: c.referral.contacts.clone(),
    };

    let summary = summary_text(&args, status, &questions, &regimes, &facts, situation.as_deref(), &referral);

    let mut keys: Vec<&str> = Vec::new();
    for r in &regimes {
        for p in &r.provisions {
            if let Some(d) = c.provisions.iter().find(|x| x.id == p.id) {
                keys.push(d.source.as_str());
            }
        }
    }
    if keys.is_empty() {
        keys.extend(["ecfr_160_103", "ecfr_99_3", "ecfr_part2_12", "mgl_4_7", "cmr_300"]);
    }
    let sources = content::sources_for(keys, &c.sources);

    let mut caveats = vec![
        "Decision support, not legal advice. This guide lists the rules that may govern a record and the questions that decide whether it can be shared; it never says that a disclosure is or is not permitted. Take the decision to your agency's counsel or privacy officer."
            .to_string(),
        "PARTIAL snapshot: the FERPA statute, the HHS and Department of Education joint guidance, 603 CMR 23.00 (student records), 42 CFR Part 2 subpart E, and several Massachusetts confidentiality statutes were not read (see coverage)."
            .to_string(),
        "Some HIPAA and Part 2 provisions were amended in 2024 and later; the text read is the eCFR text as of 2026-11-10 and its current status should be confirmed."
            .to_string(),
    ];
    caveats.extend(content::review_caveats(&[SHARING]));

    Ok(Envelope::new(
        summary,
        SharingResult {
            answer_type: "governing_rules_and_cautions".into(),
            status: status.into(),
            questions,
            regimes,
            how_the_rules_interact: cross,
            facts_to_confirm: facts,
            coverage,
            refer_to: referral,
            limits: c.limits.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        notes: vec![
            "No local files are read and nothing is written. The free-text situation is checked for identifiers and refused if it contains any."
                .into(),
        ],
        ..Privacy::none()
    })
    .decision_support())
}

fn summary_text(
    a: &SharingArgs,
    status: &str,
    questions: &[QuestionOut],
    regimes: &[RegimeOut],
    facts: &[String],
    situation: Option<&str>,
    referral: &Referral,
) -> String {
    let mut s = String::from("**Which rules govern this health information? (decision guide)**\n\n");
    s.push_str(
        "This lists rules that may govern the record and the questions that decide the matter. It \
         does not say whether sharing is or is not permitted: that depends on facts and on \
         counsel's reading.\n\n",
    );
    if let Some(t) = situation {
        s.push_str(&format!("Situation: {t}\n\n"));
    }
    if status == "needs_answers" {
        s.push_str("Answer these first (give holder_role and record_type at least):\n\n");
        for q in questions {
            s.push_str(&format!("- **{}** ({})\n", q.question, q.id));
            for ch in &q.choices {
                s.push_str(&format!("  - `{ch}`\n"));
            }
        }
        s.push_str("\nThe regimes this guide covers: ");
        s.push_str(&regimes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>().join("; "));
        s.push_str(".\n");
        return s;
    }
    if status == "partial" {
        let missing: Vec<&str> = [
            ("recipient", a.recipient.is_none()),
            ("purpose", a.purpose.is_none()),
        ]
        .iter()
        .filter(|(_, m)| *m)
        .map(|(k, _)| *k)
        .collect();
        s.push_str(&format!(
            "Not yet answered: {}. Adding them narrows the provisions shown.\n\n",
            missing.join(", ")
        ));
    }
    for r in regimes {
        let label = match r.bears_on_your_situation.as_str() {
            "likely_governs" => "likely governs",
            "may_apply" => "may apply",
            "check" => "check whether it applies",
            "not_indicated" => "not indicated by your answers",
            _ => "not assessed",
        };
        s.push_str(&format!("**{}**: {label}. {}\n\n", r.name, r.why));
        for p in &r.provisions {
            let flag = match p.confidence {
                Confidence::High => "",
                Confidence::Medium => " (medium confidence)",
                Confidence::Low => " (LOW confidence)",
            };
            s.push_str(&format!(
                "- {}: \"{}\" {}{flag}\n",
                p.citation, p.quote, p.paraphrase
            ));
        }
        for c in &r.cautions {
            s.push_str(&format!("- Caution: {c}\n"));
        }
        s.push('\n');
    }
    if !facts.is_empty() {
        s.push_str("**Facts to confirm about your organization and the record**\n\n");
        for f in facts {
            s.push_str(&format!("- {f}\n"));
        }
        s.push('\n');
    }
    s.push_str(&format!("**Next step:** {}\n", referral.text));
    s
}

/// Used by tests and the server: names of the allowed enum values must match the content's
/// question choices.
pub fn answer_keys() -> Result<BTreeMap<String, Vec<String>>> {
    let c: SharingContent = content::load(SHARING)?;
    Ok(c.questions
        .iter()
        .map(|q| (q.id.clone(), q.choices.iter().map(|x| x.value.clone()).collect()))
        .collect())
}

/// Convenience for the unit tests: an error for an unknown question id.
#[allow(dead_code)]
fn unknown(id: &str) -> PhnError {
    PhnError::InvalidInput(format!("unknown question {id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enum_values_match_the_content_choices() {
        let keys = answer_keys().unwrap();
        let holders: Vec<&str> = [
            HolderRole::HealthCareProvider,
            HolderRole::LocalHealthDepartment,
            HolderRole::SchoolNursePublic,
            HolderRole::SchoolNursePrivate,
            HolderRole::SubstanceUseProgram,
            HolderRole::StateAgency,
            HolderRole::Other,
        ]
        .iter()
        .map(|h| h.key())
        .collect();
        assert_eq!(keys["holder_role"], holders);
        let records: Vec<&str> = [
            RecordType::ClinicalRecord,
            RecordType::SchoolHealthRecord,
            RecordType::ImmunizationRecord,
            RecordType::CommunicableDiseaseReport,
            RecordType::HivTestResult,
            RecordType::SubstanceUseTreatmentRecord,
            RecordType::AdministrativeRecord,
            RecordType::Other,
        ]
        .iter()
        .map(|h| h.key())
        .collect();
        assert_eq!(keys["record_type"], records);
        let recipients: Vec<&str> = [
            Recipient::IndividualOrParent,
            Recipient::School,
            Recipient::HealthCareProvider,
            Recipient::PublicHealthAuthority,
            Recipient::LawEnforcementOrCourt,
            Recipient::Employer,
            Recipient::Researcher,
            Recipient::PublicRequester,
            Recipient::ExposedPerson,
            Recipient::OtherAgency,
        ]
        .iter()
        .map(|h| h.key())
        .collect();
        assert_eq!(keys["recipient"], recipients);
        let purposes: Vec<&str> = [
            Purpose::TreatmentOrCare,
            Purpose::PublicHealthReporting,
            Purpose::DiseaseInvestigation,
            Purpose::SchoolImmunization,
            Purpose::EmergencyThreat,
            Purpose::Research,
            Purpose::PublicRecordsRequest,
            Purpose::LegalProcess,
            Purpose::Other,
        ]
        .iter()
        .map(|h| h.key())
        .collect();
        assert_eq!(keys["purpose"], purposes);
        assert_eq!(keys["consent"], ["written", "verbal", "none", "unknown"]);
    }

    #[test]
    fn school_health_records_fall_outside_hipaa_and_inside_ferpa() {
        let (h, _) = bears("hipaa", HolderRole::SchoolNursePublic, RecordType::SchoolHealthRecord, None);
        assert_eq!(h, Bears::NotIndicated);
        let (f, _) = bears("ferpa", HolderRole::SchoolNursePublic, RecordType::SchoolHealthRecord, None);
        assert_eq!(f, Bears::LikelyGoverns);
        let (f, _) = bears("ferpa", HolderRole::SchoolNursePrivate, RecordType::SchoolHealthRecord, None);
        assert_eq!(f, Bears::MayApply);
    }

    #[test]
    fn every_provision_names_a_known_regime_and_source_and_quotes_briefly() {
        let c: SharingContent = content::load(SHARING).unwrap();
        let regimes: Vec<&str> = c.regimes.iter().map(|r| r.id.as_str()).collect();
        for p in &c.provisions {
            assert!(regimes.contains(&p.regime.as_str()), "{}", p.id);
            assert!(c.sources.contains_key(&p.source), "{}", p.id);
            assert!(p.quote.split_whitespace().count() <= 25, "{} quote too long", p.id);
        }
    }
}
