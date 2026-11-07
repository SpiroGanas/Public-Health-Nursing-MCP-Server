//! Shared state handed to every tool.

use std::collections::BTreeSet;
use std::sync::RwLock;

use phn_core::{Config, PhnError, Result, Settings, Toolset};
use phn_data::ma::MaReference;
use phn_data::{Cache, HttpClient, SnapshotStore};
use phn_local::Sandbox;

/// Everything a tool needs: configuration, settings, HTTP, cache, snapshots,
/// and Massachusetts reference lists.
#[derive(Debug)]
pub struct Context {
    pub config: Config,
    settings: RwLock<Settings>,
    pub http: HttpClient,
    pub cache: Option<Cache>,
    pub snapshots: SnapshotStore,
    pub ma: MaReference,
    /// Problems found at startup, reported by `get_data_source_status`.
    pub startup_warnings: Vec<String>,
}

impl Context {
    /// Build the context. Problems with optional pieces (settings file,
    /// cache) become warnings so the server still starts.
    pub fn new(config: Config) -> Result<Self> {
        let mut startup_warnings = Vec::new();
        let settings = match config.load_settings() {
            Ok(s) => s,
            Err(e) => {
                startup_warnings.push(format!("{e}; using default settings"));
                Settings::default()
            }
        };
        let cache_path = config.paths.cache_dir.join("phn-cache.sqlite");
        let cache = match Cache::open(&cache_path) {
            Ok(c) => Some(c),
            Err(e) => {
                startup_warnings.push(format!(
                    "cache at {} is unavailable ({e}); using an in-memory cache. Mount a \
                     writable volume at /cache to keep settings and cached data.",
                    cache_path.display()
                ));
                Cache::in_memory().ok()
            }
        };
        let http = HttpClient::new(&config)?;
        let snapshots =
            SnapshotStore::bundled().with_override_dir(config.paths.cache_dir.join("snapshots"));
        let ma = MaReference::load(&snapshots)?;
        Ok(Self {
            config,
            settings: RwLock::new(settings),
            http,
            cache,
            snapshots,
            ma,
            startup_warnings,
        })
    }

    /// A copy of the current settings.
    pub fn settings(&self) -> Settings {
        self.settings
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    /// Replace the settings and persist them to the settings file.
    pub fn save_settings(&self, new: Settings) -> Result<()> {
        self.config.save_settings(&new)?;
        *self.settings.write().unwrap_or_else(|p| p.into_inner()) = new;
        Ok(())
    }

    /// The suppression threshold in force.
    pub fn suppression_threshold(&self) -> u32 {
        self.config.suppression_threshold(&self.settings())
    }

    /// The toolsets in force.
    pub fn enabled_toolsets(&self) -> BTreeSet<Toolset> {
        self.config.enabled_toolsets(&self.settings())
    }

    /// The `/data` sandbox. Fails with a helpful message when nothing is mounted.
    pub fn sandbox(&self) -> Result<Sandbox> {
        Sandbox::new(&self.config.paths.data_dir, &self.config.paths.output_dir)
    }

    /// A short label for the configured jurisdiction, if any.
    pub fn jurisdiction_label(&self) -> Option<String> {
        let s = self.settings();
        s.jurisdiction.town.map(|t| match s.jurisdiction.county {
            Some(c) => format!("{t} ({c} County)"),
            None => t,
        })
    }
}

/// Convert a `PhnError` into the message shown to the model.
pub fn error_message(e: &PhnError) -> String {
    e.to_string()
}
