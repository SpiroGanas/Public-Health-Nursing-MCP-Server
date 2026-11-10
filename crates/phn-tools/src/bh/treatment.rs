//! MH-05 `find_substance_use_treatment`: SAMHSA FindTreatment.gov facilities
//! near a Massachusetts town, filtered by medications for opioid use disorder
//! (methadone at opioid treatment programs, buprenorphine, naltrexone),
//! withdrawal management, setting, recovery supports, special groups, payment
//! (including MassHealth/Medicaid), and language, with the Massachusetts
//! Substance Use Helpline. Live → cache → bundled statewide snapshot.

use phn_core::{Envelope, PhnError, Privacy, Result};
use phn_data::boundaries::{BoundaryLayer, Layer};
use phn_data::findtreatment::{self, Facility};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::harm_reduction::{CitedText, Contact, RESOURCES};
use crate::comms::content::{self, SUBSTANCE_USE};
use crate::context::Context;
use crate::md;

/// Arguments for `find_substance_use_treatment`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct TreatmentArgs {
    /// A Massachusetts city or town to search from (default: your configured town).
    pub town: Option<String>,
    /// Search radius in miles (default 15, at most 75).
    pub radius_miles: Option<f64>,
    /// Services every facility must offer: "methadone" (opioid treatment program),
    /// "buprenorphine", "naltrexone", "withdrawal_management", "residential", "outpatient",
    /// "intensive_outpatient", "recovery_support", "sober_housing", "adolescents",
    /// "pregnant_postpartum", "co_occurring", "veterans".
    pub services: Vec<String>,
    /// Payment the facility must accept: "masshealth" (Medicaid), "medicare", "private",
    /// "self_pay", or "sliding_fee" (sliding fee scale or payment assistance).
    pub payment: Option<String>,
    /// A language the facility must offer: "spanish", "asl", or another language such as
    /// "portuguese", "haitian creole", "vietnamese", "chinese".
    pub language: Option<String>,
    /// Most facilities to list (default 15, at most 50).
    pub max_results: Option<usize>,
}

/// A facility in the results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TreatmentFacility {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub program: String,
    pub address: String,
    pub city: String,
    pub phone: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub intake_phone: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub website: String,
    pub miles: f64,
    /// Medications for opioid use disorder offered (methadone, buprenorphine, naltrexone).
    pub moud: Vec<String>,
    /// Settings (outpatient, residential, ...).
    pub settings: Vec<String>,
    pub payment: Vec<String>,
    pub languages: Vec<String>,
    /// Requested services this facility matched.
    pub matched: Vec<String>,
}

/// Result of `find_substance_use_treatment`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TreatmentResult {
    pub town: String,
    pub radius_miles: f64,
    /// Facilities within the radius before filtering.
    pub facilities_in_radius: usize,
    /// Facilities that meet every filter.
    pub matching: usize,
    pub facilities: Vec<TreatmentFacility>,
    pub filters: Vec<String>,
    pub helpline: Vec<Contact>,
    pub about_treatment: Vec<CitedText>,
    pub locator_url: String,
}

/// A service filter: (name, description, predicate).
type Filter = (&'static str, &'static str, fn(&Facility) -> bool);

const SERVICES: &[Filter] = &[
    (
        "methadone",
        "methadone at a federally certified opioid treatment program",
        |f| f.has("OT", "federally-certified opioid treatment program") || f.has("OM", "methadone"),
    ),
    ("buprenorphine", "buprenorphine", |f| {
        // "Use methadone/buprenorphine for pain management or emergency dosing" is not
        // treatment of opioid use disorder.
        f.has("OM", "buprenorphine")
            || f.services.get("OT").is_some_and(|v| {
                v.iter().any(|s| {
                    let l = s.to_lowercase();
                    l.contains("buprenorphine") && !l.contains("pain management")
                })
            })
    }),
    ("naltrexone", "naltrexone", |f| {
        f.has("OM", "naltrexone") || f.has("OT", "naltrexone")
    }),
    (
        "withdrawal_management",
        "withdrawal management (detoxification)",
        |f| {
            f.has("TC", "detoxification")
                || f.has("SET", "detox")
                || f.services.contains_key("DETOX")
        },
    ),
    ("residential", "residential treatment", |f| {
        f.has("SET", "residential")
    }),
    ("outpatient", "outpatient treatment", |f| {
        f.has("SET", "outpatient")
    }),
    (
        "intensive_outpatient",
        "intensive outpatient treatment",
        |f| f.has("SET", "intensive outpatient"),
    ),
    (
        "recovery_support",
        "recovery supports (peer support, recovery coach, self-help groups)",
        |f| {
            f.has("RSS", "peer support")
                || f.has("RSS", "recovery coach")
                || f.has("RSS", "self-help")
        },
    ),
    (
        "sober_housing",
        "transitional housing, halfway house, or sober home",
        |f| f.has("TC", "sober home"),
    ),
    ("adolescents", "programs for adolescents", |f| {
        f.has("AGE", "adolescents") || f.has("SG", "adolescents")
    }),
    (
        "pregnant_postpartum",
        "programs for pregnant or postpartum women",
        |f| f.has("SG", "pregnant"),
    ),
    (
        "co_occurring",
        "co-occurring mental health and substance use disorders",
        |f| f.has("TC", "co-occurring") || f.has("SG", "co-occurring mental"),
    ),
    ("veterans", "programs for veterans", |f| {
        f.has("SG", "veterans")
    }),
];

fn service_key(raw: &str) -> Result<&'static Filter> {
    let k = raw.trim().to_lowercase().replace([' ', '-', '/'], "_");
    let k = match k.as_str() {
        "otp" | "opioid_treatment_program" | "methadone_maintenance" => "methadone",
        "bup" | "suboxone" | "buprenorphine_naloxone" => "buprenorphine",
        "vivitrol" | "xr_naltrexone" => "naltrexone",
        "detox" | "detoxification" | "withdrawal" => "withdrawal_management",
        "inpatient" => "residential",
        "iop" => "intensive_outpatient",
        "recovery" | "peer_support" | "recovery_coach" => "recovery_support",
        "sober_home" | "halfway_house" | "transitional_housing" => "sober_housing",
        "youth" | "teens" | "adolescent" => "adolescents",
        "pregnant" | "pregnancy" | "postpartum" => "pregnant_postpartum",
        "dual_diagnosis" | "co_occurring_disorders" => "co_occurring",
        "veteran" => "veterans",
        other => other,
    }
    .to_string();
    SERVICES.iter().find(|s| s.0 == k).ok_or_else(|| {
        PhnError::InvalidInput(format!(
            "service '{raw}' is not recognized; use {}",
            SERVICES.iter().map(|s| s.0).collect::<Vec<_>>().join(", ")
        ))
    })
}

/// True when a value in category `code` is exactly `value` (case-insensitive).
/// Substring matching is wrong for payment and language: SAMHSA's values
/// include "State-financed health insurance plan other than Medicaid" and
/// "Other languages (excluding Spanish)".
fn has_exact(f: &Facility, code: &str, value: &str) -> bool {
    f.services
        .get(code)
        .is_some_and(|v| v.iter().any(|s| s.trim().eq_ignore_ascii_case(value)))
}

/// True when the facility's ZIP code is not a Massachusetts one (010-027).
fn outside_massachusetts(f: &Facility) -> bool {
    let z = f.zip.trim();
    !z.is_empty() && !z.starts_with("01") && !z.starts_with("02")
}

fn moud(f: &Facility) -> Vec<String> {
    let mut v = Vec::new();
    if SERVICES[0].2(f) {
        v.push("methadone (opioid treatment program)".to_string());
    }
    if SERVICES[1].2(f) {
        v.push("buprenorphine".into());
    }
    if SERVICES[2].2(f) {
        v.push("naltrexone".into());
    }
    v
}

/// Find substance use treatment near a town.
pub async fn find_substance_use_treatment(
    ctx: &Context,
    args: TreatmentArgs,
) -> Result<Envelope<TreatmentResult>> {
    let town_q = args
        .town
        .clone()
        .filter(|t| !t.trim().is_empty())
        .or_else(|| ctx.settings().jurisdiction.town)
        .ok_or_else(|| {
            PhnError::InvalidInput(
                "give a town, or set your town with configure_jurisdiction".into(),
            )
        })?;
    let town = ctx.ma.town(&town_q).ok_or_else(|| {
        let s: Vec<String> = ctx
            .ma
            .suggest_towns(&town_q, 3)
            .into_iter()
            .map(|t| t.name.clone())
            .collect();
        PhnError::NotFound(format!(
            "'{town_q}' is not a Massachusetts city or town{}",
            if s.is_empty() {
                String::new()
            } else {
                format!(". Did you mean: {}?", s.join(", "))
            }
        ))
    })?;
    let radius = args.radius_miles.unwrap_or(15.0);
    if !(1.0..=75.0).contains(&radius) {
        return Err(PhnError::InvalidInput(
            "radius_miles must be between 1 and 75".into(),
        ));
    }
    let max = args.max_results.unwrap_or(15).clamp(1, 50);
    let filters: Vec<&Filter> = args
        .services
        .iter()
        .map(|s| service_key(s))
        .collect::<Result<_>>()?;
    let payment: Option<(&str, &str)> = match args
        .payment
        .as_deref()
        .map(|p| p.trim().to_lowercase().replace([' ', '-'], "_"))
    {
        None => None,
        Some(p) if p.is_empty() => None,
        Some(p) => Some(match p.as_str() {
            "masshealth" | "medicaid" | "mass_health" => ("Medicaid", "MassHealth (Medicaid)"),
            "medicare" => ("Medicare", "Medicare"),
            "private" | "private_insurance" | "commercial" => {
                ("Private health insurance", "private insurance")
            }
            "self_pay" | "cash" | "selfpay" => ("Cash or self-payment", "cash or self-payment"),
            "sliding_fee" | "sliding_scale" | "free" | "payment_assistance" | "uninsured" => {
                ("PYAS", "sliding fee scale or payment assistance")
            }
            other => {
                return Err(PhnError::InvalidInput(format!(
                    "payment '{other}' is not recognized; use masshealth, medicare, private, self_pay, or sliding_fee"
                )));
            }
        }),
    };
    let language = args
        .language
        .as_deref()
        .map(|l| l.trim().to_lowercase())
        .filter(|l| !l.is_empty() && l != "english");

    // Town point: the label point of the town's boundary, converted to latitude/longitude.
    let layer = BoundaryLayer::shared(&ctx.snapshots, Layer::Town)?;
    let feature = layer
        .get(&town.name)
        .ok_or_else(|| PhnError::Internal(format!("no boundary for {}", town.name)))?;
    let (x, y) = feature.label_point();
    let (lat, lng) = findtreatment::state_plane_to_lat_lng(x, y);

    let store = ctx.snapshots.clone();
    let fb: phn_data::fetch::SnapshotFallback = Box::new(move || {
        let (text, entry, _) = store
            .file_text(findtreatment::SNAPSHOT_ID, findtreatment::SNAPSHOT_FILE)
            .ok()?;
        Some((text.as_bytes().to_vec(), entry.retrieved))
    });
    let found =
        findtreatment::search(&ctx.http, ctx.cache.as_ref(), lat, lng, radius, Some(fb)).await?;
    let in_radius = found.facilities.len();

    let lang_ok = |f: &Facility| -> bool {
        match language.as_deref() {
            None => true,
            Some("spanish") | Some("espanol") | Some("español") => has_exact(f, "SL", "Spanish"),
            Some("asl") | Some("sign") | Some("sign language") | Some("american sign language") => {
                f.has("SL", "sign language")
            }
            Some(l) => {
                let l = l
                    .replace("haitian ", "")
                    .replace("cantonese", "chinese")
                    .replace("mandarin", "chinese");
                f.has("OL", &l)
            }
        }
    };
    let pay_ok = |f: &Facility| -> bool {
        match payment {
            None => true,
            Some(("PYAS", _)) => f.services.contains_key("PYAS"),
            Some((v, _)) => has_exact(f, "PAY", v),
        }
    };
    let mut matching: Vec<&Facility> = found
        .facilities
        .iter()
        .filter(|f| filters.iter().all(|flt| (flt.2)(f)))
        .filter(|f| pay_ok(f))
        .filter(|f| lang_ok(f))
        .collect();
    matching.sort_by(|a, b| a.miles.total_cmp(&b.miles).then(a.name.cmp(&b.name)));
    let n_match = matching.len();
    let facilities: Vec<TreatmentFacility> = matching
        .iter()
        .take(max)
        .map(|f| {
            let mut langs: Vec<String> = f.services.get("SL").cloned().unwrap_or_default();
            langs.retain(|l| !l.starts_with("Other languages"));
            langs.extend(f.services.get("OL").cloned().unwrap_or_default());
            TreatmentFacility {
                name: f.name.clone(),
                program: f.name2.clone(),
                address: f.address.clone(),
                city: f.city.clone(),
                phone: f.phone.clone(),
                intake_phone: f.intake_phone.clone(),
                website: f.website.clone(),
                miles: (f.miles * 10.0).round() / 10.0,
                moud: moud(f),
                settings: f.services.get("SET").cloned().unwrap_or_default(),
                payment: f.services.get("PAY").cloned().unwrap_or_default(),
                languages: langs,
                matched: filters.iter().map(|flt| flt.1.to_string()).collect(),
            }
        })
        .collect();

    let mut filter_text: Vec<String> = filters.iter().map(|f| f.1.to_string()).collect();
    if let Some((_, label)) = payment {
        filter_text.push(format!("accepts {label}"));
    }
    if let Some(l) = &language {
        filter_text.push(format!("offers {l}"));
    }
    let helpline: Vec<Contact> = RESOURCES
        .contacts
        .iter()
        .filter(|c| c.name.contains("Substance Use Helpline") || c.name.contains("988"))
        .cloned()
        .collect();

    // ---- summary
    let mut summary = format!(
        "**Substance use treatment within {radius:.0} miles of {}** (SAMHSA FindTreatment.gov)\n\n{} facilit{} in the area; {} match{}.\n\n",
        town.name,
        in_radius,
        if in_radius == 1 { "y" } else { "ies" },
        n_match,
        if filter_text.is_empty() {
            String::new()
        } else {
            format!(" ({})", filter_text.join("; "))
        }
    );
    if facilities.is_empty() {
        summary.push_str("No facility matched. Widen the radius, drop a filter, or call the Massachusetts Substance Use Helpline.\n\n");
    } else {
        let rows: Vec<Vec<String>> = facilities
            .iter()
            .map(|f| {
                vec![
                    if f.program.is_empty() {
                        f.name.clone()
                    } else {
                        format!("{} ({})", f.name, f.program)
                    },
                    format!("{}, {}", f.address, f.city),
                    f.phone.clone(),
                    format!("{:.1}", f.miles),
                    if f.moud.is_empty() {
                        "—".into()
                    } else {
                        f.moud.join(", ")
                    },
                ]
            })
            .collect();
        summary.push_str(&md::table(
            &[
                "Facility",
                "Address",
                "Phone",
                "Miles",
                "Medications for OUD",
            ],
            &rows,
        ));
        if n_match > facilities.len() {
            summary.push_str(&format!(
                "\n{} more match; raise max_results to see them.\n",
                n_match - facilities.len()
            ));
        }
        summary.push('\n');
    }
    summary.push_str("**Help finding treatment:**\n");
    for c in &helpline {
        summary.push_str(&format!("- {}: {}\n", c.name, c.detail));
    }
    let locator_url = RESOURCES
        .sources
        .get("findtreatment")
        .and_then(|s| s.url.clone())
        .unwrap_or_default();
    summary.push_str(&format!("- FindTreatment.gov: {locator_url}\n"));

    let mut caveats = Vec::new();
    match found.mode {
        phn_core::SourceMode::Snapshot => caveats.push(format!(
            "FindTreatment.gov could not be reached, so the bundled statewide copy from {} was used; distances are from the center of {}.",
            found.as_of.chars().take(10).collect::<String>(),
            town.name
        )),
        phn_core::SourceMode::Cache if found.fallback_reason.is_some() => caveats.push(format!(
            "FindTreatment.gov could not be reached, so a cached copy from {} was used.",
            found.as_of.chars().take(10).collect::<String>()
        )),
        _ => {}
    }
    caveats.push("Facilities report their own services to SAMHSA. Call ahead to confirm services, openings, insurance, language, and intake hours.".into());
    caveats.push(format!(
        "Distances are straight-line miles from the center of {}, not travel distance.",
        town.name
    ));
    let out_of_state = matching
        .iter()
        .take(max)
        .filter(|f| outside_massachusetts(f))
        .count();
    if out_of_state > 0 {
        caveats.push(format!(
            "{out_of_state} listed facilit{} outside Massachusetts by ZIP code (the search is by distance). \"Medicaid\" at an out-of-state facility is that state's program, not MassHealth; confirm coverage before referring.",
            if out_of_state == 1 { "y is" } else { "ies are" }
        ));
    }
    caveats.extend(content::review_caveats(&[SUBSTANCE_USE]));
    let mut sources = vec![found.source()];
    sources.extend(content::cite(
        &RESOURCES.sources,
        &["findtreatment_api", "fda_moud", "bsas_contacts"],
        &RESOURCES.meta.prepared,
    ));
    let mut privacy = Privacy::none();
    privacy
        .notes
        .push("Public facility listings only; no client information is used.".into());
    Ok(Envelope::new(
        summary,
        TreatmentResult {
            town: town.name.clone(),
            radius_miles: radius,
            facilities_in_radius: in_radius,
            matching: n_match,
            facilities,
            filters: filter_text,
            helpline,
            about_treatment: RESOURCES.treatment.clone(),
            locator_url,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_aliases_resolve() {
        assert_eq!(service_key("Suboxone").unwrap().0, "buprenorphine");
        assert_eq!(service_key("OTP").unwrap().0, "methadone");
        assert_eq!(service_key("detox").unwrap().0, "withdrawal_management");
        assert!(service_key("acupuncture").is_err());
    }

    /// A facility with services given as (category code, "value; value").
    fn facility(zip: &str, services: &[(&str, &str)]) -> Facility {
        Facility {
            name: "Test Program".into(),
            name2: String::new(),
            address: "1 Main St".into(),
            city: "Lee".into(),
            zip: zip.into(),
            phone: String::new(),
            intake_phone: String::new(),
            website: String::new(),
            latitude: 42.3,
            longitude: -73.25,
            miles: 1.0,
            services: services
                .iter()
                .map(|(k, v)| {
                    (
                        k.to_string(),
                        v.split("; ").map(str::to_string).collect::<Vec<String>>(),
                    )
                })
                .collect(),
        }
    }

    #[test]
    fn payment_and_language_filters_do_not_match_negated_values() {
        let other = facility(
            "01238",
            &[
                ("SL", "Other languages (excluding Spanish)"),
                (
                    "PAY",
                    "State-financed health insurance plan other than Medicaid",
                ),
                (
                    "OT",
                    "Use methadone/buprenorphine for pain management or emergency dosing",
                ),
            ],
        );
        assert!(!has_exact(&other, "SL", "Spanish"));
        assert!(!has_exact(&other, "PAY", "Medicaid"));
        assert!(!SERVICES[1].2(&other));
        let yes = facility(
            "02903",
            &[
                ("SL", "Spanish"),
                ("PAY", "Medicaid; Medicare"),
                ("OT", "Buprenorphine maintenance"),
            ],
        );
        assert!(has_exact(&yes, "SL", "spanish"));
        assert!(has_exact(&yes, "PAY", "Medicaid"));
        assert!(SERVICES[1].2(&yes));
        assert!(!outside_massachusetts(&other));
        assert!(outside_massachusetts(&yes));
    }
}
