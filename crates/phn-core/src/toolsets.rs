//! Toolsets and role profiles.
//!
//! Exposing every tool at once crowds the model's context and hurts tool
//! selection, so tools are grouped into toolsets. A nurse enables toolsets
//! directly (`PHN_TOOLSETS=cd,epi`) or picks a role profile
//! (`PHN_PROFILE=cd_nurse`). The `core` toolset is always enabled.

use std::collections::BTreeSet;
use std::fmt;
use std::str::FromStr;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A group of related tools.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Toolset {
    /// Platform utilities: configuration, data sources, local files, export.
    Core,
    /// Communicable disease surveillance and control.
    Cd,
    /// Immunizations and vaccine-preventable disease.
    Vax,
    /// School health.
    School,
    /// Maternal, child, and family health.
    Mch,
    /// Healthy aging and older adults.
    Aging,
    /// Mental health and substance use.
    Bh,
    /// Emergency preparedness and response.
    Epr,
    /// Environmental health and climate.
    Env,
    /// Chronic disease prevention.
    Chronic,
    /// Health equity and community assessment.
    Equity,
    /// Epidemiology, biostatistics, and data handling.
    Epi,
    /// Program planning, evaluation, and grants.
    Planning,
    /// Leadership, finance, and administration.
    Leadership,
    /// Policy, law, and ethics.
    Policy,
    /// Health communication and language access.
    Comms,
    /// Evidence-based practice and quality improvement.
    Ebp,
}

impl Toolset {
    /// Every toolset, in display order.
    pub const ALL: [Toolset; 17] = [
        Toolset::Core,
        Toolset::Cd,
        Toolset::Vax,
        Toolset::School,
        Toolset::Mch,
        Toolset::Aging,
        Toolset::Bh,
        Toolset::Epr,
        Toolset::Env,
        Toolset::Chronic,
        Toolset::Equity,
        Toolset::Epi,
        Toolset::Planning,
        Toolset::Leadership,
        Toolset::Policy,
        Toolset::Comms,
        Toolset::Ebp,
    ];

    /// The identifier used in `PHN_TOOLSETS`.
    pub fn as_str(self) -> &'static str {
        match self {
            Toolset::Core => "core",
            Toolset::Cd => "cd",
            Toolset::Vax => "vax",
            Toolset::School => "school",
            Toolset::Mch => "mch",
            Toolset::Aging => "aging",
            Toolset::Bh => "bh",
            Toolset::Epr => "epr",
            Toolset::Env => "env",
            Toolset::Chronic => "chronic",
            Toolset::Equity => "equity",
            Toolset::Epi => "epi",
            Toolset::Planning => "planning",
            Toolset::Leadership => "leadership",
            Toolset::Policy => "policy",
            Toolset::Comms => "comms",
            Toolset::Ebp => "ebp",
        }
    }
}

impl fmt::Display for Toolset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Toolset {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = s.trim().to_ascii_lowercase();
        Toolset::ALL
            .iter()
            .copied()
            .find(|t| t.as_str() == key)
            .ok_or_else(|| {
                format!(
                    "unknown toolset '{s}'. Valid toolsets: {}",
                    Toolset::ALL.map(Toolset::as_str).join(", ")
                )
            })
    }
}

/// A role profile: a named bundle of toolsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Profile {
    /// School nurse.
    SchoolNurse,
    /// Communicable disease / epidemiology nurse at a board of health.
    CdNurse,
    /// Public health nurse leader or director.
    PhnLeader,
    /// Emergency preparedness coordinator.
    Preparedness,
}

impl Profile {
    /// Every profile.
    pub const ALL: [Profile; 4] = [
        Profile::SchoolNurse,
        Profile::CdNurse,
        Profile::PhnLeader,
        Profile::Preparedness,
    ];

    /// Identifier used in `PHN_PROFILE`.
    pub fn as_str(self) -> &'static str {
        match self {
            Profile::SchoolNurse => "school_nurse",
            Profile::CdNurse => "cd_nurse",
            Profile::PhnLeader => "phn_leader",
            Profile::Preparedness => "preparedness",
        }
    }

    /// Toolsets enabled by this profile (`core` is always added).
    pub fn toolsets(self) -> &'static [Toolset] {
        use Toolset::*;
        match self {
            Profile::SchoolNurse => &[Core, School, Vax, Cd, Bh, Comms, Epi],
            Profile::CdNurse => &[Core, Cd, Vax, Epi, Comms, Equity],
            Profile::PhnLeader => &[
                Core, Epi, Equity, Planning, Leadership, Policy, Comms, Ebp, Chronic,
            ],
            Profile::Preparedness => &[Core, Epr, Env, Epi, Equity, Comms, Cd],
        }
    }
}

impl fmt::Display for Profile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Profile {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = s.trim().to_ascii_lowercase().replace('-', "_");
        Profile::ALL
            .iter()
            .copied()
            .find(|p| p.as_str() == key)
            .ok_or_else(|| {
                format!(
                    "unknown profile '{s}'. Valid profiles: {}",
                    Profile::ALL.map(Profile::as_str).join(", ")
                )
            })
    }
}

/// Parse a comma-separated toolset list. `all` expands to every toolset.
pub fn parse_toolset_list(list: &str) -> Result<BTreeSet<Toolset>, String> {
    let mut out = BTreeSet::new();
    for item in list.split(',').map(str::trim).filter(|s| !s.is_empty()) {
        if item.eq_ignore_ascii_case("all") {
            out.extend(Toolset::ALL);
        } else {
            out.insert(item.parse::<Toolset>()?);
        }
    }
    Ok(out)
}

/// Work out which toolsets are enabled.
///
/// Explicit toolsets win over a profile. With neither, every toolset is
/// enabled. `core` is always enabled.
pub fn resolve_enabled(
    profile: Option<Profile>,
    toolsets: Option<&BTreeSet<Toolset>>,
) -> BTreeSet<Toolset> {
    let mut enabled: BTreeSet<Toolset> = match (toolsets, profile) {
        (Some(t), _) if !t.is_empty() => t.clone(),
        (_, Some(p)) => p.toolsets().iter().copied().collect(),
        _ => Toolset::ALL.into_iter().collect(),
    };
    enabled.insert(Toolset::Core);
    enabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lists_and_all() {
        let s = parse_toolset_list("cd, epi").unwrap();
        assert_eq!(s.len(), 2);
        assert!(parse_toolset_list("all").unwrap().len() == Toolset::ALL.len());
        assert!(parse_toolset_list("cd,bogus").is_err());
    }

    #[test]
    fn toolsets_override_profile_and_core_always_on() {
        let explicit: BTreeSet<_> = [Toolset::Epi].into();
        let e = resolve_enabled(Some(Profile::SchoolNurse), Some(&explicit));
        assert_eq!(e, [Toolset::Core, Toolset::Epi].into());
        let p = resolve_enabled(Some(Profile::CdNurse), None);
        assert!(p.contains(&Toolset::Cd) && !p.contains(&Toolset::School));
        assert_eq!(resolve_enabled(None, None).len(), Toolset::ALL.len());
    }

    #[test]
    fn profile_parsing_accepts_hyphens() {
        assert_eq!("cd-nurse".parse::<Profile>().unwrap(), Profile::CdNurse);
        assert!("nurse".parse::<Profile>().is_err());
    }
}
