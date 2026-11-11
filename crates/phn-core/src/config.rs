//! Configuration.
//!
//! Two layers:
//!
//! 1. **Environment variables**, set in the MCP client configuration
//!    (`docker run -e ...`). They describe the deployment: folders, offline
//!    mode, proxy CA bundle, and optionally the profile, toolsets, and
//!    suppression threshold.
//! 2. **The settings file** (`/cache/phn-settings.toml` by default), written by
//!    the `configure_jurisdiction` tool. It holds the nurse's town, school
//!    district, preferred languages, letterhead, and the same optional
//!    profile/toolset/threshold choices.
//!
//! Environment variables win over the settings file so an administrator can
//! pin a value in the client configuration.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::error::{PhnError, Result};
use crate::toolsets::{self, Profile, Toolset};

/// Default small-cell suppression threshold: counts from 1 to 4 are suppressed.
pub const DEFAULT_SUPPRESSION_THRESHOLD: u32 = 5;

/// Lowest threshold accepted (suppress only counts of 1).
pub const MIN_SUPPRESSION_THRESHOLD: u32 = 2;

/// Highest threshold accepted.
pub const MAX_SUPPRESSION_THRESHOLD: u32 = 30;

/// Folders the server may use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// The nurse's mounted folder (`/data`). Tools read only inside it.
    pub data_dir: PathBuf,
    /// Writable cache volume (`/cache`): settings, HTTP cache, indexes.
    pub cache_dir: PathBuf,
    /// Where generated files are written (`/data/output`).
    pub output_dir: PathBuf,
    /// Where agency policies live for `search_agency_policies` (`/data/policies`).
    pub policy_dir: PathBuf,
}

impl Paths {
    /// Paths rooted at `data_dir` and `cache_dir` with the default subfolders.
    pub fn new(data_dir: impl Into<PathBuf>, cache_dir: impl Into<PathBuf>) -> Self {
        let data_dir = data_dir.into();
        Self {
            output_dir: data_dir.join("output"),
            policy_dir: data_dir.join("policies"),
            cache_dir: cache_dir.into(),
            data_dir,
        }
    }
}

/// The nurse's jurisdiction.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Jurisdiction {
    /// Massachusetts city or town (one of the 351 municipalities).
    pub town: Option<String>,
    /// County, derived from the town.
    pub county: Option<String>,
    /// Health and Medical Coordinating Coalition (HMCC) region, e.g. `"4AB"`.
    pub hmcc_region: Option<String>,
    /// School district name, for school nurses.
    pub school_district: Option<String>,
    /// DESE organization code of the school district.
    pub school_district_code: Option<String>,
    /// Board of health or health department phone number shown in outputs.
    pub board_of_health_phone: Option<String>,
}

/// Agency letterhead used by document exports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Letterhead {
    /// Agency name, e.g. "Town of Amherst Health Department".
    pub agency_name: String,
    /// Optional division or program name.
    pub department: Option<String>,
    /// Street address lines.
    pub address_lines: Vec<String>,
    /// Main phone number.
    pub phone: Option<String>,
    /// Fax number.
    pub fax: Option<String>,
    /// Public email address.
    pub email: Option<String>,
    /// Website.
    pub website: Option<String>,
    /// Logo image (PNG or JPEG), relative to the data folder.
    pub logo_path: Option<String>,
    /// Footer text, such as a confidentiality statement.
    pub footer_text: Option<String>,
}

/// Settings persisted by `configure_jurisdiction`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Settings {
    /// Town, county, HMCC region, and school district.
    pub jurisdiction: Jurisdiction,
    /// Small-cell suppression threshold.
    pub suppression_threshold: Option<u32>,
    /// Preferred languages for materials, in priority order (e.g. "English", "Spanish").
    pub preferred_languages: Vec<String>,
    /// Role profile.
    pub profile: Option<Profile>,
    /// Explicitly enabled toolsets (overrides the profile).
    pub toolsets: Option<Vec<Toolset>>,
    /// Letterhead for exported documents.
    pub letterhead: Option<Letterhead>,
    /// Which body's vaccine recommendation the immunization tools treat as the
    /// basis (default: DPH, then the professional societies, then CDC/ACIP).
    /// Every answer still shows each body's position side by side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vaccine_recommendation_basis: Option<VaccineRecommendationBasis>,
}

impl Settings {
    /// Problems with values in the settings file that the server corrects by
    /// falling back to a default. Shown on stderr at startup and by `check`.
    pub fn warnings(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(t) = self.suppression_threshold
            && validate_threshold(t).is_err()
        {
            out.push(format!(
                "the settings file has suppression_threshold = {t}, which is outside \
                 {MIN_SUPPRESSION_THRESHOLD}-{MAX_SUPPRESSION_THRESHOLD}; it is ignored and the \
                 default of {DEFAULT_SUPPRESSION_THRESHOLD} is used unless \
                 PHN_SUPPRESSION_THRESHOLD is set"
            ));
        }
        out
    }
}

/// Order of precedence used to pick the "basis" recommendation when federal
/// (CDC/ACIP) vaccine guidance differs from Massachusetts DPH and the
/// professional societies (AAP, ACOG, AAFP).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VaccineRecommendationBasis {
    /// DPH's position where it has one; otherwise the professional society for
    /// the population (AAP for children and adolescents, ACOG for pregnancy,
    /// AAFP for adults); otherwise CDC/ACIP. The default.
    #[default]
    DphThenProfessionalThenCdc,
    /// CDC/ACIP (the federal schedule) first.
    CdcAcip,
    /// The professional societies first, then DPH, then CDC/ACIP.
    ProfessionalSocieties,
    /// Do not pick a basis: show every body's position without a headline.
    ShowAll,
}

impl VaccineRecommendationBasis {
    /// Every option.
    pub const ALL: [VaccineRecommendationBasis; 4] = [
        Self::DphThenProfessionalThenCdc,
        Self::CdcAcip,
        Self::ProfessionalSocieties,
        Self::ShowAll,
    ];

    /// Identifier used in settings and tool arguments.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DphThenProfessionalThenCdc => "dph_then_professional_then_cdc",
            Self::CdcAcip => "cdc_acip",
            Self::ProfessionalSocieties => "professional_societies",
            Self::ShowAll => "show_all",
        }
    }

    /// Plain-language description for summaries.
    pub fn describe(self) -> &'static str {
        match self {
            Self::DphThenProfessionalThenCdc => {
                "Massachusetts DPH where DPH has a position; otherwise the professional society \
                 (AAP, ACOG, or AAFP); otherwise CDC/ACIP"
            }
            Self::CdcAcip => "CDC/ACIP (federal) first",
            Self::ProfessionalSocieties => {
                "professional societies (AAP, ACOG, AAFP) first, then DPH, then CDC/ACIP"
            }
            Self::ShowAll => "no single basis; every body's position is shown side by side",
        }
    }
}

impl std::fmt::Display for VaccineRecommendationBasis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for VaccineRecommendationBasis {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let norm = s.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        Self::ALL
            .into_iter()
            .find(|b| b.as_str() == norm)
            .or(match norm.as_str() {
                "default" | "dph" => Some(Self::DphThenProfessionalThenCdc),
                "cdc" | "acip" => Some(Self::CdcAcip),
                "professional" | "aap" => Some(Self::ProfessionalSocieties),
                "all" => Some(Self::ShowAll),
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "unknown vaccine recommendation basis '{s}'. Use one of: {}",
                    Self::ALL.map(|b| b.as_str()).join(", ")
                )
            })
    }
}

/// Deployment configuration read from environment variables.
#[derive(Debug, Clone)]
pub struct Config {
    /// Folders.
    pub paths: Paths,
    /// Where the settings file lives.
    pub settings_path: PathBuf,
    /// Never make network calls (`PHN_OFFLINE=1`).
    pub offline: bool,
    /// Extra PEM CA bundle for agency TLS inspection (`PHN_CA_BUNDLE`).
    pub ca_bundle: Option<PathBuf>,
    /// Timeout for outbound HTTP requests.
    pub http_timeout: Duration,
    /// Profile pinned by `PHN_PROFILE`.
    pub env_profile: Option<Profile>,
    /// Toolsets pinned by `PHN_TOOLSETS`.
    pub env_toolsets: Option<BTreeSet<Toolset>>,
    /// Suppression threshold pinned by `PHN_SUPPRESSION_THRESHOLD`.
    pub env_suppression_threshold: Option<u32>,
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> Result<Self> {
        Self::from_lookup(|k| std::env::var(k).ok())
    }

    /// Read configuration through a lookup function (testable).
    pub fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Result<Self> {
        let get = |k: &str| {
            lookup(k)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        };

        let data_dir = get("PHN_DATA_DIR").unwrap_or_else(|| "/data".into());
        let cache_dir = get("PHN_CACHE_DIR").unwrap_or_else(|| "/cache".into());
        let mut paths = Paths::new(data_dir, cache_dir);
        if let Some(o) = get("PHN_OUTPUT_DIR") {
            paths.output_dir = PathBuf::from(o);
        }
        if let Some(p) = get("PHN_POLICY_DIR") {
            paths.policy_dir = PathBuf::from(p);
        }
        let settings_path = get("PHN_SETTINGS_FILE")
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.cache_dir.join("phn-settings.toml"));

        let offline = get("PHN_OFFLINE")
            .map(|v| parse_bool("PHN_OFFLINE", &v))
            .transpose()?
            .unwrap_or(false);
        let ca_bundle = get("PHN_CA_BUNDLE").map(PathBuf::from);
        let http_timeout = match get("PHN_HTTP_TIMEOUT_SECS") {
            Some(v) => Duration::from_secs(v.parse::<u64>().map_err(|_| {
                PhnError::Config(format!(
                    "PHN_HTTP_TIMEOUT_SECS must be a whole number, got '{v}'"
                ))
            })?),
            None => Duration::from_secs(20),
        };

        let env_profile = get("PHN_PROFILE")
            .map(|p| p.parse::<Profile>().map_err(PhnError::Config))
            .transpose()?;
        let env_toolsets = get("PHN_TOOLSETS")
            .map(|t| -> Result<BTreeSet<Toolset>> {
                let set = toolsets::parse_toolset_list(&t).map_err(PhnError::Config)?;
                // A list with no names (for example ",") would otherwise count as a
                // pin while enabling every toolset.
                if set.is_empty() {
                    return Err(PhnError::Config(format!(
                        "PHN_TOOLSETS names no toolsets, got '{t}'. List toolsets separated by \
                         commas (for example cd,epi), use 'all', or remove the variable."
                    )));
                }
                Ok(set)
            })
            .transpose()?;
        let env_suppression_threshold = get("PHN_SUPPRESSION_THRESHOLD")
            .map(|v| {
                v.parse::<u32>()
                    .map_err(|_| {
                        PhnError::Config(format!(
                            "PHN_SUPPRESSION_THRESHOLD must be a whole number, got '{v}'"
                        ))
                    })
                    .and_then(validate_threshold)
            })
            .transpose()?;

        Ok(Self {
            paths,
            settings_path,
            offline,
            ca_bundle,
            http_timeout,
            env_profile,
            env_toolsets,
            env_suppression_threshold,
        })
    }

    /// Configuration rooted in a temporary directory, for tests.
    pub fn for_root(root: &Path) -> Self {
        let paths = Paths::new(root.join("data"), root.join("cache"));
        Self {
            settings_path: paths.cache_dir.join("phn-settings.toml"),
            paths,
            offline: true,
            ca_bundle: None,
            http_timeout: Duration::from_secs(5),
            env_profile: None,
            env_toolsets: None,
            env_suppression_threshold: None,
        }
    }

    /// Load the settings file; a missing file yields default settings.
    pub fn load_settings(&self) -> Result<Settings> {
        match std::fs::read_to_string(&self.settings_path) {
            Ok(text) => toml::from_str(&text).map_err(|e| {
                PhnError::Config(format!(
                    "settings file {} is not valid: {e}",
                    self.settings_path.display()
                ))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings::default()),
            Err(e) => Err(e.into()),
        }
    }

    /// Write the settings file atomically.
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        let text = toml::to_string_pretty(settings)
            .map_err(|e| PhnError::Internal(format!("could not serialize settings: {e}")))?;
        if let Some(parent) = self.settings_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                PhnError::Config(format!(
                    "cannot create {} (is the /cache volume mounted and writable?): {e}",
                    parent.display()
                ))
            })?;
        }
        let tmp = self.settings_path.with_extension("toml.tmp");
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, &self.settings_path)?;
        Ok(())
    }

    /// The suppression threshold in force.
    ///
    /// A value in the settings file outside the accepted range (for example
    /// after a hand edit) is ignored, so the file cannot switch suppression
    /// off; [`Settings::warnings`] reports it.
    pub fn suppression_threshold(&self, settings: &Settings) -> u32 {
        self.env_suppression_threshold
            .or(settings
                .suppression_threshold
                .filter(|t| validate_threshold(*t).is_ok()))
            .unwrap_or(DEFAULT_SUPPRESSION_THRESHOLD)
    }

    /// The toolsets in force.
    pub fn enabled_toolsets(&self, settings: &Settings) -> BTreeSet<Toolset> {
        if self.env_toolsets.is_some() || self.env_profile.is_some() {
            return toolsets::resolve_enabled(self.env_profile, self.env_toolsets.as_ref());
        }
        let from_settings: Option<BTreeSet<Toolset>> = settings
            .toolsets
            .as_ref()
            .map(|t| t.iter().copied().collect());
        toolsets::resolve_enabled(settings.profile, from_settings.as_ref())
    }

    /// Names of environment variables that override settings-file values.
    pub fn env_overrides(&self) -> Vec<&'static str> {
        let mut v = Vec::new();
        if self.env_profile.is_some() {
            v.push("PHN_PROFILE");
        }
        if self.env_toolsets.is_some() {
            v.push("PHN_TOOLSETS");
        }
        if self.env_suppression_threshold.is_some() {
            v.push("PHN_SUPPRESSION_THRESHOLD");
        }
        v
    }
}

/// Check a suppression threshold is within the accepted range.
pub fn validate_threshold(t: u32) -> Result<u32> {
    if (MIN_SUPPRESSION_THRESHOLD..=MAX_SUPPRESSION_THRESHOLD).contains(&t) {
        Ok(t)
    } else {
        Err(PhnError::InvalidInput(format!(
            "suppression threshold must be between {MIN_SUPPRESSION_THRESHOLD} and \
             {MAX_SUPPRESSION_THRESHOLD}, got {t}"
        )))
    }
}

/// Parse an on/off environment variable. Anything unrecognized is an error, so
/// a typo cannot silently leave a switch such as `PHN_OFFLINE` off.
fn parse_bool(name: &str, v: &str) -> Result<bool> {
    match v.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(PhnError::Config(format!(
            "{name} must be 1, true, yes, or on (or 0, false, no, or off), got '{v}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let m: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| m.get(k).cloned()
    }

    #[test]
    fn defaults_follow_container_layout() {
        let c = Config::from_lookup(lookup(&[])).unwrap();
        assert_eq!(c.paths.data_dir, PathBuf::from("/data"));
        assert_eq!(c.paths.output_dir, PathBuf::from("/data/output"));
        assert_eq!(c.settings_path, PathBuf::from("/cache/phn-settings.toml"));
        assert!(!c.offline);
        assert_eq!(c.suppression_threshold(&Settings::default()), 5);
    }

    #[test]
    fn env_overrides_settings() {
        let c = Config::from_lookup(lookup(&[
            ("PHN_PROFILE", "school_nurse"),
            ("PHN_SUPPRESSION_THRESHOLD", "11"),
            ("PHN_OFFLINE", "true"),
        ]))
        .unwrap();
        let s = Settings {
            suppression_threshold: Some(5),
            profile: Some(Profile::CdNurse),
            ..Settings::default()
        };
        assert_eq!(c.suppression_threshold(&s), 11);
        assert!(c.enabled_toolsets(&s).contains(&Toolset::School));
        assert!(c.offline);
        assert_eq!(
            c.env_overrides(),
            vec!["PHN_PROFILE", "PHN_SUPPRESSION_THRESHOLD"]
        );
    }

    #[test]
    fn rejects_bad_values() {
        assert!(Config::from_lookup(lookup(&[("PHN_PROFILE", "wizard")])).is_err());
        assert!(Config::from_lookup(lookup(&[("PHN_SUPPRESSION_THRESHOLD", "1")])).is_err());
        assert!(Config::from_lookup(lookup(&[("PHN_TOOLSETS", "cd,nope")])).is_err());
    }

    #[test]
    fn rejects_unrecognized_offline_values_and_empty_toolset_lists() {
        // A typo must not silently leave offline mode off.
        for bad in ["y", "enabled", "offline"] {
            assert!(
                Config::from_lookup(lookup(&[("PHN_OFFLINE", bad)])).is_err(),
                "PHN_OFFLINE={bad} should be rejected"
            );
        }
        for (v, expect) in [("1", true), ("TRUE", true), ("on", true), ("0", false)] {
            let c = Config::from_lookup(lookup(&[("PHN_OFFLINE", v)])).unwrap();
            assert_eq!(c.offline, expect, "PHN_OFFLINE={v}");
        }
        assert!(
            !Config::from_lookup(lookup(&[("PHN_OFFLINE", "off")]))
                .unwrap()
                .offline
        );
        // An unset or blank variable still means "not offline".
        assert!(
            !Config::from_lookup(lookup(&[("PHN_OFFLINE", " ")]))
                .unwrap()
                .offline
        );
        // A list with no names is not a pin on every toolset.
        assert!(Config::from_lookup(lookup(&[("PHN_TOOLSETS", ",")])).is_err());
        assert!(Config::from_lookup(lookup(&[("PHN_TOOLSETS", " , ,")])).is_err());
        let c = Config::from_lookup(lookup(&[("PHN_TOOLSETS", "all")])).unwrap();
        assert_eq!(c.env_toolsets.unwrap().len(), Toolset::ALL.len());
    }

    #[test]
    fn out_of_range_threshold_in_the_settings_file_is_ignored() {
        let c = Config::from_lookup(lookup(&[])).unwrap();
        for bad in ["0", "1", "31", "1000"] {
            let s: Settings = toml::from_str(&format!("suppression_threshold = {bad}\n")).unwrap();
            assert_eq!(
                c.suppression_threshold(&s),
                DEFAULT_SUPPRESSION_THRESHOLD,
                "threshold {bad} from the settings file must not be used"
            );
            assert_eq!(s.warnings().len(), 1, "threshold {bad} should be reported");
        }
        let ok: Settings = toml::from_str("suppression_threshold = 11\n").unwrap();
        assert_eq!(c.suppression_threshold(&ok), 11);
        assert!(ok.warnings().is_empty());
        assert!(Settings::default().warnings().is_empty());
        // The environment variable still wins over a bad file value.
        let pinned = Config::from_lookup(lookup(&[("PHN_SUPPRESSION_THRESHOLD", "10")])).unwrap();
        let bad: Settings = toml::from_str("suppression_threshold = 1\n").unwrap();
        assert_eq!(pinned.suppression_threshold(&bad), 10);
    }

    #[test]
    fn settings_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::for_root(dir.path());
        assert_eq!(c.load_settings().unwrap(), Settings::default());
        let s = Settings {
            jurisdiction: Jurisdiction {
                town: Some("Amherst".into()),
                ..Jurisdiction::default()
            },
            toolsets: Some(vec![Toolset::Cd]),
            ..Settings::default()
        };
        c.save_settings(&s).unwrap();
        assert_eq!(c.load_settings().unwrap(), s);
        let enabled = c.enabled_toolsets(&s);
        assert_eq!(enabled, [Toolset::Core, Toolset::Cd].into());
    }

    #[test]
    fn vaccine_basis_is_optional_and_round_trips() {
        // Settings files written before the field existed still load.
        let old: Settings = toml::from_str("suppression_threshold = 11\n").unwrap();
        assert_eq!(old.vaccine_recommendation_basis, None);
        let dir = tempfile::tempdir().unwrap();
        let c = Config::for_root(dir.path());
        let s = Settings {
            vaccine_recommendation_basis: Some(VaccineRecommendationBasis::CdcAcip),
            ..Settings::default()
        };
        c.save_settings(&s).unwrap();
        assert_eq!(c.load_settings().unwrap(), s);
        assert_eq!(
            "show-all".parse::<VaccineRecommendationBasis>().unwrap(),
            VaccineRecommendationBasis::ShowAll
        );
        assert!("federal".parse::<VaccineRecommendationBasis>().is_err());
        assert_eq!(
            VaccineRecommendationBasis::default().as_str(),
            "dph_then_professional_then_cdc"
        );
    }
}
