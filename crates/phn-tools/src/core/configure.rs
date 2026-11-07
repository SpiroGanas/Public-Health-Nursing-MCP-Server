//! CORE-09 `configure_jurisdiction`.

use phn_core::config::validate_threshold;
use phn_core::{Envelope, Letterhead, PhnError, Privacy, Profile, Result, Settings, Toolset};
use phn_data::ma::hmcc_region;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Arguments for `configure_jurisdiction`. Omit every field to view the
/// current settings.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ConfigureJurisdictionArgs {
    /// Massachusetts city or town (one of the 351). County and HMCC region are
    /// filled in automatically.
    pub town: Option<String>,
    /// School district name or 8-digit DESE organization code (school nurses).
    pub school_district: Option<String>,
    /// Override the HMCC region derived from the town: 1, 2, 3, 4AB, 4C, or 5.
    pub hmcc_region: Option<String>,
    /// Board of health or health department phone number to show in outputs.
    pub board_of_health_phone: Option<String>,
    /// Small-cell suppression threshold: counts from 1 to threshold−1 are
    /// suppressed (2–30; default 5).
    pub suppression_threshold: Option<u32>,
    /// Preferred languages for materials, most important first.
    pub preferred_languages: Option<Vec<String>>,
    /// Role profile: school_nurse, cd_nurse, phn_leader, preparedness, or "none".
    pub profile: Option<String>,
    /// Toolsets to enable (overrides the profile). An empty list clears it.
    pub toolsets: Option<Vec<String>>,
    /// Letterhead for exported documents. `logo_path` is relative to /data.
    pub letterhead: Option<Letterhead>,
    /// Clear all settings before applying the other fields.
    pub reset: bool,
}

/// Values derived from the jurisdiction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DerivedJurisdiction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hmcc_region_description: Option<String>,
    /// School districts that serve the configured town.
    pub districts_serving_town: Vec<String>,
}

/// Result of `configure_jurisdiction`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ConfigureResult {
    /// Fields changed by this call.
    pub changed: Vec<String>,
    /// Settings now saved.
    pub settings: Settings,
    pub derived: DerivedJurisdiction,
    /// Suppression threshold in force (environment variables can override settings).
    pub suppression_threshold: u32,
    /// Toolsets in force.
    pub enabled_toolsets: Vec<Toolset>,
    /// True when the enabled toolsets changed (the tool list is refreshed).
    pub toolsets_changed: bool,
    /// Environment variables that override the settings file.
    pub env_overrides: Vec<String>,
    pub settings_file: String,
}

/// View or update the nurse's jurisdiction and preferences.
pub fn configure_jurisdiction(
    ctx: &Context,
    args: ConfigureJurisdictionArgs,
) -> Result<Envelope<ConfigureResult>> {
    let before = ctx.settings();
    let toolsets_before = ctx.enabled_toolsets();
    let mut s = if args.reset {
        Settings::default()
    } else {
        before.clone()
    };
    let mut changed = Vec::new();
    let mut caveats = Vec::new();
    if args.reset {
        changed.push("reset".to_string());
    }

    if let Some(town) = &args.town {
        let m = ctx.ma.town(town).ok_or_else(|| {
            let suggestions: Vec<String> = ctx
                .ma
                .suggest_towns(town, 3)
                .into_iter()
                .map(|m| m.name.clone())
                .collect();
            PhnError::InvalidInput(if suggestions.is_empty() {
                format!("'{town}' is not one of the 351 Massachusetts cities and towns")
            } else {
                format!(
                    "'{town}' is not one of the 351 Massachusetts cities and towns. Did you mean: {}?",
                    suggestions.join(", ")
                )
            })
        })?;
        s.jurisdiction.town = Some(m.name.clone());
        s.jurisdiction.county = Some(m.county.clone());
        s.jurisdiction.hmcc_region = Some(m.hmcc_region.clone());
        changed.extend(["town", "county", "hmcc_region"].map(String::from));
    }
    if let Some(r) = &args.hmcc_region {
        let region = hmcc_region(r).ok_or_else(|| {
            PhnError::InvalidInput(format!(
                "'{r}' is not an HMCC region. Use one of: 1, 2, 3, 4AB, 4C, 5"
            ))
        })?;
        s.jurisdiction.hmcc_region = Some(region.id.to_string());
        if !changed.iter().any(|c| c == "hmcc_region") {
            changed.push("hmcc_region".into());
        }
    }
    if let Some(d) = &args.school_district {
        let district = ctx.ma.district(d).ok_or_else(|| {
            let suggestions: Vec<String> = ctx
                .ma
                .suggest_districts(d, 4)
                .into_iter()
                .map(|d| format!("{} ({})", d.name, d.org_code))
                .collect();
            PhnError::InvalidInput(if suggestions.is_empty() {
                format!("school district '{d}' was not found")
            } else {
                format!(
                    "school district '{d}' was not found. Did you mean: {}?",
                    suggestions.join("; ")
                )
            })
        })?;
        s.jurisdiction.school_district = Some(district.name.clone());
        s.jurisdiction.school_district_code = Some(district.org_code.clone());
        changed.push("school_district".into());
    }
    if let Some(p) = &args.board_of_health_phone {
        let digits = p.chars().filter(char::is_ascii_digit).count();
        if digits < 10 {
            return Err(PhnError::InvalidInput(format!(
                "'{p}' does not look like a phone number (expected at least 10 digits)"
            )));
        }
        s.jurisdiction.board_of_health_phone = Some(p.trim().to_string());
        changed.push("board_of_health_phone".into());
    }
    if let Some(t) = args.suppression_threshold {
        s.suppression_threshold = Some(validate_threshold(t)?);
        changed.push("suppression_threshold".into());
        if t < phn_core::config::DEFAULT_SUPPRESSION_THRESHOLD {
            caveats.push(format!(
                "A threshold of {t} is lower than the default of 5; confirm it meets your \
                 agency's data release policy."
            ));
        }
    }
    if let Some(langs) = &args.preferred_languages {
        s.preferred_languages = langs
            .iter()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        changed.push("preferred_languages".into());
    }
    if let Some(p) = &args.profile {
        s.profile = if p.trim().eq_ignore_ascii_case("none") || p.trim().is_empty() {
            None
        } else {
            Some(p.parse::<Profile>().map_err(PhnError::InvalidInput)?)
        };
        changed.push("profile".into());
    }
    if let Some(list) = &args.toolsets {
        let parsed = list
            .iter()
            .map(|t| t.parse::<Toolset>().map_err(PhnError::InvalidInput))
            .collect::<Result<Vec<_>>>()?;
        s.toolsets = (!parsed.is_empty()).then_some(parsed);
        changed.push("toolsets".into());
    }
    if let Some(lh) = &args.letterhead {
        if lh.agency_name.trim().is_empty() {
            return Err(PhnError::InvalidInput(
                "letterhead.agency_name is required".into(),
            ));
        }
        if let Some(logo) = &lh.logo_path {
            let sandbox = ctx.sandbox()?;
            let path = sandbox.resolve_existing(logo)?;
            let ext = path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            if !matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
                return Err(PhnError::InvalidInput(
                    "letterhead logo must be a PNG or JPEG image".into(),
                ));
            }
        }
        s.letterhead = Some(lh.clone());
        changed.push("letterhead".into());
    }

    if s != before {
        ctx.save_settings(s.clone())?;
    }
    let toolsets_after = ctx.enabled_toolsets();
    let toolsets_changed = toolsets_after != toolsets_before;
    let env_overrides: Vec<String> = ctx
        .config
        .env_overrides()
        .into_iter()
        .map(String::from)
        .collect();
    let overridden: Vec<&String> = env_overrides
        .iter()
        .filter(|v| {
            (v.as_str() == "PHN_PROFILE" && changed.iter().any(|c| c == "profile"))
                || (v.as_str() == "PHN_TOOLSETS"
                    && changed.iter().any(|c| c == "toolsets" || c == "profile"))
                || (v.as_str() == "PHN_SUPPRESSION_THRESHOLD"
                    && changed.iter().any(|c| c == "suppression_threshold"))
        })
        .collect();
    if !overridden.is_empty() {
        caveats.push(format!(
            "Saved, but the environment variable(s) {} set in the MCP client configuration \
             take precedence. Remove them there to use the saved value.",
            overridden
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    let derived = DerivedJurisdiction {
        hmcc_region_description: s
            .jurisdiction
            .hmcc_region
            .as_deref()
            .and_then(hmcc_region)
            .map(|r| r.description.to_string()),
        districts_serving_town: s
            .jurisdiction
            .town
            .as_deref()
            .map(|t| {
                ctx.ma
                    .districts_for_town(t)
                    .into_iter()
                    .map(|d| format!("{} ({}, {})", d.name, d.kind, d.org_code))
                    .collect()
            })
            .unwrap_or_default(),
    };
    if changed.iter().any(|c| c == "town") {
        caveats.extend(ctx.ma.caveats.iter().take(1).cloned());
    }

    let threshold = ctx.suppression_threshold();
    let j = &s.jurisdiction;
    let mut summary = if changed.is_empty() {
        "**Current settings**\n\n".to_string()
    } else {
        format!("**Settings saved** (changed: {})\n\n", changed.join(", "))
    };
    let or_unset = |v: &Option<String>| v.clone().unwrap_or_else(|| "not set".into());
    summary.push_str(&format!(
        "- Town: {}\n- County: {}\n- HMCC region: {}\n- School district: {}\n- Board of health phone: {}\n",
        or_unset(&j.town),
        or_unset(&j.county),
        derived
            .hmcc_region_description
            .clone()
            .unwrap_or_else(|| or_unset(&j.hmcc_region)),
        match (&j.school_district, &j.school_district_code) {
            (Some(n), Some(c)) => format!("{n} ({c})"),
            _ => "not set".into(),
        },
        or_unset(&j.board_of_health_phone),
    ));
    summary.push_str(&format!(
        "- Suppression threshold: counts of 1–{} are suppressed\n- Preferred languages: {}\n- Profile: {}\n- Enabled toolsets: {}\n- Letterhead: {}\n",
        threshold - 1,
        if s.preferred_languages.is_empty() {
            "not set".into()
        } else {
            s.preferred_languages.join(", ")
        },
        s.profile.map(|p| p.to_string()).unwrap_or_else(|| "none".into()),
        toolsets_after.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", "),
        s.letterhead
            .as_ref()
            .map(|l| l.agency_name.clone())
            .unwrap_or_else(|| "not set".into()),
    ));
    if j.town.is_some() && j.school_district.is_none() && !derived.districts_serving_town.is_empty()
    {
        summary.push_str(&format!(
            "\nDistricts serving this town: {}\n",
            derived.districts_serving_town.join("; ")
        ));
    }
    if toolsets_changed {
        summary.push_str("\nThe enabled toolsets changed; the tool list has been refreshed.\n");
    }

    let mut env = Envelope::new(
        summary,
        ConfigureResult {
            changed,
            settings: s.clone(),
            derived,
            suppression_threshold: threshold,
            enabled_toolsets: toolsets_after.into_iter().collect(),
            toolsets_changed,
            env_overrides,
            settings_file: ctx.config.settings_path.display().to_string(),
        },
    )
    .caveats(caveats)
    .privacy(Privacy::none());
    if args.town.is_some() || args.school_district.is_some() {
        env = env
            .source(ctx.ma.municipality_source.clone())
            .source(ctx.ma.district_source.clone());
    }
    Ok(env)
}
