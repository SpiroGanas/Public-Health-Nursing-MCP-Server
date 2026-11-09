//! AGE-02 `find_ma_aging_services`: for a town, the Aging Services Access
//! Point(s), Council on Aging, SHINE office, senior nutrition project,
//! caregiver and dementia supports, protective services and ombudsman
//! contacts. Town providers come from the `ma-aging-service-directory`
//! snapshot; statewide lines and program descriptions from
//! `content/aging/aging_services.yaml` (pending review).

use std::collections::BTreeMap;
use std::sync::LazyLock;

use phn_content::ContentMeta;
use phn_core::{Envelope, PhnError, Privacy, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::aging::content::{self as c10, SERVICES, SourceRef};
use crate::aging::directory::{Asap, Directory, NutritionProject, ShineOffice};
use crate::context::Context;

#[derive(Debug, Clone, Deserialize)]
struct LineDef {
    id: String,
    category: String,
    name: String,
    phone: String,
    hours: String,
    detail: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ProgramDef {
    id: String,
    category: String,
    name: String,
    text: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct CoaLookup {
    text: String,
    map_url: String,
    source: String,
    section: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ServicesContent {
    #[allow(dead_code)]
    meta: ContentMeta,
    sources: BTreeMap<String, SourceRef>,
    lines: Vec<LineDef>,
    programs: Vec<ProgramDef>,
    coa_lookup: CoaLookup,
}

static CONTENT: LazyLock<ServicesContent> =
    LazyLock::new(|| c10::load(SERVICES).expect("aging services content parses"));

/// What the person needs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Need {
    /// Home care, personal care, homemaker, Frail Elder Waiver.
    HomeCare,
    /// Home-delivered or community meals.
    Meals,
    /// Medicare and health insurance counseling (SHINE).
    Insurance,
    /// Family caregiver support and respite.
    Caregiver,
    /// Dementia supports.
    Dementia,
    /// Abuse, neglect, or self-neglect.
    Protective,
    /// Nursing home, rest home, or assisted living concerns.
    LongTermCare,
    /// Senior center programs, transportation, outreach.
    Community,
    /// Information, referral, and Options Counseling.
    Information,
}

impl Need {
    fn category(self) -> &'static str {
        match self {
            Need::HomeCare => "home_care",
            Need::Meals => "meals",
            Need::Insurance => "insurance",
            Need::Caregiver => "caregiver",
            Need::Dementia => "dementia",
            Need::Protective => "protective",
            Need::LongTermCare => "long_term_care",
            Need::Community => "community",
            Need::Information => "information",
        }
    }
}

/// Arguments for `find_ma_aging_services`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct AgingServicesArgs {
    /// Massachusetts city or town (default: the configured town).
    pub town: Option<String>,
    /// Limit to these needs (default: everything): home_care, meals,
    /// insurance, caregiver, dementia, protective, long_term_care,
    /// community, information.
    pub needs: Vec<Need>,
}

/// A statewide line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LineOut {
    pub name: String,
    pub phone: String,
    pub hours: String,
    pub detail: String,
    pub citation: String,
}

/// A program description.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProgramOut {
    pub name: String,
    pub text: String,
    pub citation: String,
}

/// The town's Council on Aging (contacts are not bundled).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoaOut {
    pub name: String,
    pub how_to_reach: String,
    pub map_url: String,
}

/// Result of `find_ma_aging_services`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AgingServicesResult {
    pub town: String,
    pub county: String,
    /// The ASAP(s) serving the town (Boston: several, by neighborhood).
    pub asaps: Vec<Asap>,
    pub council_on_aging: CoaOut,
    pub shine: Option<ShineOffice>,
    pub meals: Vec<NutritionProject>,
    pub statewide_lines: Vec<LineOut>,
    pub programs: Vec<ProgramOut>,
}

/// Find aging services for a town.
pub fn find_ma_aging_services(
    ctx: &Context,
    args: AgingServicesArgs,
) -> Result<Envelope<AgingServicesResult>> {
    let c = &*CONTENT;
    let town_name = c10::town_or_configured(ctx, args.town.as_deref()).ok_or_else(|| {
        PhnError::InvalidInput(
            "give a Massachusetts city or town (or set one with configure_jurisdiction)".into(),
        )
    })?;
    let m = c10::resolve_town(ctx, &town_name)?;
    let dir = Directory::load(&ctx.snapshots)?;
    let wants = |cat: &str| args.needs.is_empty() || args.needs.iter().any(|n| n.category() == cat);
    let asaps: Vec<Asap> = dir.asaps_for(&m.name).into_iter().cloned().collect();
    let shine = dir.shine_for(&m.name).cloned();
    let meals: Vec<NutritionProject> = if wants("meals") || wants("home_care") {
        dir.nutrition_for(&m.name).into_iter().cloned().collect()
    } else {
        Vec::new()
    };
    let lines: Vec<LineOut> = c
        .lines
        .iter()
        .filter(|l| wants(&l.category) || l.id == "massoptions" || l.id == "massrelay")
        .map(|l| LineOut {
            name: l.name.clone(),
            phone: l.phone.clone(),
            hours: l.hours.clone(),
            detail: l.detail.clone(),
            citation: c10::citation(&c.sources, &l.source, &l.section),
        })
        .collect();
    let programs: Vec<ProgramOut> = c
        .programs
        .iter()
        .filter(|p| wants(&p.category) || p.id == "asap")
        .map(|p| ProgramOut {
            name: p.name.clone(),
            text: p.text.split_whitespace().collect::<Vec<_>>().join(" "),
            citation: c10::citation(&c.sources, &p.source, &p.section),
        })
        .collect();
    let coa = CoaOut {
        name: format!("{} Council on Aging", m.name),
        how_to_reach: c
            .coa_lookup
            .text
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        map_url: c.coa_lookup.map_url.clone(),
    };

    // ---- markdown
    let mut s = format!("**Aging services — {} ({} County)**\n\n", m.name, m.county);
    s.push_str("**Aging Services Access Point (ASAP)**\n");
    for a in &asaps {
        s.push_str(&format!(
            "- **{}**, {} — {}{}{}{}\n",
            a.name,
            a.address,
            a.phone,
            a.phone_toll_free
                .as_deref()
                .map(|t| format!(", toll free {t}"))
                .unwrap_or_default(),
            a.web
                .as_deref()
                .map(|w| format!(", {w}"))
                .unwrap_or_default(),
            if a.neighborhoods.is_empty() || m.name != "Boston" {
                String::new()
            } else {
                format!(" (neighborhoods: {})", a.neighborhoods.join(", "))
            }
        ));
    }
    if wants("community") || args.needs.is_empty() {
        s.push_str(&format!(
            "\n**{}:** {} Map: {}\n",
            coa.name, coa.how_to_reach, coa.map_url
        ));
    }
    if wants("insurance") {
        match &shine {
            Some(o) => s.push_str(&format!("\n**SHINE (Medicare counseling):** {} — {}\n", o.office, o.phone)),
            None => s.push_str("\n**SHINE:** this town has no row in AGE's SHINE list; call MassOptions (800) 243-4636.\n"),
        }
    }
    if !meals.is_empty() {
        s.push_str("\n**Meals (Senior Nutrition Program)**\n");
        for n in &meals {
            s.push_str(&format!("- **{}**, {} — {}\n", n.name, n.address, n.phone));
        }
    }
    s.push_str("\n**Statewide**\n");
    for l in &lines {
        s.push_str(&format!(
            "- **{}**: {}{} — {}\n",
            l.name,
            l.phone,
            if l.hours.is_empty() {
                String::new()
            } else {
                format!(" ({})", l.hours)
            },
            l.detail
        ));
    }
    if !programs.is_empty() {
        s.push_str("\n**Programs**\n");
        for p in &programs {
            s.push_str(&format!("- **{}**: {}\n", p.name, p.text));
        }
    }

    let mut sources = vec![c10::content_source(SERVICES)?, dir.aging_source.clone()];
    sources.extend(c10::sources_for(
        &c.sources,
        lines
            .iter()
            .filter_map(|l| {
                c.lines
                    .iter()
                    .find(|d| d.name == l.name)
                    .map(|d| d.source.as_str())
            })
            .chain(c.programs.iter().map(|p| p.source.as_str()))
            .chain([
                "age-asap",
                "age-nutrition-sites",
                "age-shine",
                c.coa_lookup.source.as_str(),
            ]),
    ));
    let mut caveats = dir.aging_caveats.clone();
    caveats.push(format!(
        "Council on Aging phone numbers are not bundled: AGE publishes them on an online map and the Massachusetts Councils on Aging site, which could not be read when this was built ({}).",
        c10::citation(&c.sources, &c.coa_lookup.source, &c.coa_lookup.section)
    ));
    if meals.is_empty() && (wants("meals") || wants("home_care")) {
        caveats.push(format!(
            "{} is not named in AGE's list of senior nutrition projects; ask the ASAP or MassOptions.",
            m.name
        ));
    }
    if dir.from_cache_override {
        caveats.push("A newer directory snapshot from /cache/snapshots was used.".into());
    }
    caveats.extend(c10::review_caveats(&[SERVICES]));
    Ok(Envelope::new(
        s,
        AgingServicesResult {
            town: m.name.clone(),
            county: m.county.clone(),
            asaps,
            council_on_aging: coa,
            shine,
            meals,
            statewide_lines: lines,
            programs,
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy::none()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let ctx = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, ctx)
    }

    #[test]
    fn content_sources_are_known() {
        let c = &*CONTENT;
        let mut keys: Vec<&str> = c.lines.iter().map(|l| l.source.as_str()).collect();
        keys.extend(c.programs.iter().map(|p| p.source.as_str()));
        keys.push(&c.coa_lookup.source);
        c10::assert_keys_known(&c.sources, &keys, "aging services");
    }

    #[test]
    fn lists_providers_for_a_town() {
        let (_d, ctx) = ctx();
        let r = find_ma_aging_services(
            &ctx,
            AgingServicesArgs {
                town: Some("Amherst".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(r.data.asaps.len(), 1);
        assert!(r.data.shine.is_some());
        assert!(!r.data.meals.is_empty());
        assert!(
            r.data
                .statewide_lines
                .iter()
                .any(|l| l.phone == "(800) 243-4636")
        );
        let r = find_ma_aging_services(
            &ctx,
            AgingServicesArgs {
                town: Some("Boston".into()),
                needs: vec![Need::Insurance],
            },
        )
        .unwrap();
        assert!(r.data.asaps.len() >= 3);
        assert!(r.data.meals.is_empty());
        assert!(
            r.data
                .statewide_lines
                .iter()
                .any(|l| l.phone == "617-357-0226")
        );
        let e = find_ma_aging_services(
            &ctx,
            AgingServicesArgs {
                town: Some("Worchester".into()),
                ..Default::default()
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("Worcester"));
        assert!(find_ma_aging_services(&ctx, AgingServicesArgs::default()).is_err());
    }
}
