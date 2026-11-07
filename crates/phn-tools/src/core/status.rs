//! CORE-08 `get_data_source_status`.

use phn_content::sources::{SourceKind, SourceStatus};
use phn_core::{Envelope, Privacy, Result, Source, SourceMode};
use phn_data::http::ProbeResult;
use phn_data::snapshots::SnapshotOrigin;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;
use crate::md;

/// Arguments for `get_data_source_status`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct DataSourceStatusArgs {
    /// Check each live endpoint now (a quick request to each). Default false:
    /// report the last recorded check instead.
    pub check_live: bool,
    /// Include sources planned for later releases. Default false.
    pub include_planned: bool,
    /// Report only this source id.
    pub source_id: Option<String>,
}

/// Status of one source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceStatusRow {
    pub id: String,
    pub name: String,
    pub publisher: String,
    pub kind: SourceKind,
    pub status: SourceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_phase: Option<u32>,
    pub used_by: Vec<String>,
    /// Date of the data (snapshot vintage or content preparation date).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data_date: Option<String>,
    /// When the bundled copy was built or retrieved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retrieved: Option<String>,
    /// Whether the bundled snapshot or a newer cached copy is in use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snapshot_origin: Option<SnapshotOrigin>,
    /// Review status of bundled content.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review_due: Option<String>,
    /// Latest reachability check of the live endpoint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub live_check: Option<ProbeResult>,
    /// True when `live_check` was run during this call.
    pub checked_now: bool,
    pub caveats: Vec<String>,
}

/// Result of `get_data_source_status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DataSourceStatus {
    pub server_version: String,
    pub offline_mode: bool,
    pub sources: Vec<SourceStatusRow>,
    pub general_caveats: Vec<String>,
    pub startup_warnings: Vec<String>,
}

/// List data sources with their dates, review status, and reachability.
pub async fn get_data_source_status(
    ctx: &Context,
    args: DataSourceStatusArgs,
) -> Result<Envelope<DataSourceStatus>> {
    let registry = phn_content::sources::registry();
    let selected: Vec<_> = registry
        .sources
        .iter()
        .filter(|s| args.include_planned || s.status != SourceStatus::Planned)
        .filter(|s| args.source_id.as_ref().is_none_or(|id| &s.id == id))
        .collect();
    if let Some(id) = &args.source_id {
        if selected.is_empty() {
            return Err(phn_core::PhnError::NotFound(format!(
                "data source '{id}'. Known sources: {}",
                registry
                    .sources
                    .iter()
                    .map(|s| s.id.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
    }

    // Run live checks concurrently.
    let mut live_checks = std::collections::HashMap::new();
    if args.check_live {
        let mut set = tokio::task::JoinSet::new();
        for s in &selected {
            if s.status == SourceStatus::Active {
                if let Some(url) = s.probe_url.clone() {
                    let http = ctx.http.clone();
                    let id = s.id.clone();
                    set.spawn(async move { (id, http.probe(&url).await) });
                }
            }
        }
        while let Some(joined) = set.join_next().await {
            if let Ok((id, probe)) = joined {
                if let Some(cache) = &ctx.cache {
                    if let Err(e) = cache.record_probe(&id, &probe) {
                        tracing::warn!(error = %e, "could not record probe");
                    }
                }
                live_checks.insert(id, probe);
            }
        }
    }

    let mut rows = Vec::new();
    for s in selected {
        let mut row = SourceStatusRow {
            id: s.id.clone(),
            name: s.name.clone(),
            publisher: s.publisher.clone(),
            kind: s.kind,
            status: s.status,
            planned_phase: s.planned_phase,
            used_by: s.used_by.clone(),
            data_date: None,
            retrieved: None,
            snapshot_origin: None,
            review_status: None,
            review_due: None,
            live_check: None,
            checked_now: false,
            caveats: s.caveats.clone(),
        };
        if let Some(sid) = &s.snapshot_id {
            if let Some((entry, origin)) = ctx.snapshots.entry(sid) {
                row.data_date = Some(entry.source_vintage.clone());
                row.retrieved = Some(entry.retrieved.clone());
                row.snapshot_origin = Some(origin);
                for c in &entry.caveats {
                    if !row.caveats.contains(c) {
                        row.caveats.push(c.clone());
                    }
                }
            }
        }
        if let Some(cid) = &s.content_id {
            if let Some(item) = phn_content::ITEMS.iter().find(|i| &i.id == cid) {
                if let Ok(meta) = phn_content::item_meta(item) {
                    row.data_date = Some(meta.effective_date.clone().unwrap_or(meta.prepared));
                    row.review_status = Some(
                        match meta.review.status {
                            phn_content::ReviewStatus::Pending => "pending review",
                            phn_content::ReviewStatus::Reviewed => "reviewed",
                        }
                        .into(),
                    );
                    row.review_due = Some(meta.review.review_due.clone());
                }
            }
        }
        if s.kind == SourceKind::Local {
            row.data_date = None;
            let data_ok = ctx.config.paths.data_dir.is_dir();
            row.caveats.push(if data_ok {
                format!(
                    "Data folder is mounted at {}.",
                    ctx.config.paths.data_dir.display()
                )
            } else {
                format!(
                    "No data folder is mounted at {}; local-file tools are unavailable.",
                    ctx.config.paths.data_dir.display()
                )
            });
        }
        if let Some(p) = live_checks.remove(&s.id) {
            row.live_check = Some(p);
            row.checked_now = true;
        } else if let Some(cache) = &ctx.cache {
            row.live_check = cache.last_probe(&s.id).ok().flatten();
        }
        rows.push(row);
    }

    let table_rows: Vec<Vec<String>> = rows
        .iter()
        .map(|r| {
            let kind = match r.kind {
                SourceKind::Content => "reference content",
                SourceKind::Snapshot => "snapshot",
                SourceKind::Live => "live API",
                SourceKind::Local => "your files",
            };
            let status = match (r.status, &r.review_status) {
                (SourceStatus::Planned, _) => {
                    format!("planned (Phase {})", r.planned_phase.unwrap_or_default())
                }
                (_, Some(rs)) => rs.clone(),
                (s, None) => format!("{s:?}").to_lowercase(),
            };
            let live = match &r.live_check {
                Some(p) if p.reachable => format!(
                    "reachable{} ({})",
                    if r.checked_now { "" } else { " at last check" },
                    p.checked_at
                ),
                Some(p) => format!(
                    "NOT reachable: {} ({})",
                    p.error.clone().unwrap_or_default(),
                    p.checked_at
                ),
                None => "not checked".into(),
            };
            vec![
                r.name.clone(),
                kind.into(),
                status,
                r.data_date.clone().unwrap_or_else(|| "—".into()),
                live,
            ]
        })
        .collect();

    let mut summary = format!(
        "**Data sources** (server v{}, {} mode)\n\n",
        phn_core::VERSION,
        if ctx.config.offline {
            "offline"
        } else {
            "online"
        }
    );
    summary.push_str(&md::table(
        &["Source", "Type", "Status", "Data date", "Live endpoint"],
        &table_rows,
    ));
    if !args.check_live {
        summary.push_str("\nRun again with `check_live: true` to test each endpoint now.\n");
    }

    let mut caveats: Vec<String> = registry.general_caveats.clone();
    caveats.extend(ctx.startup_warnings.iter().cloned());
    if ctx.config.offline {
        caveats.push(
            "Offline mode is on (PHN_OFFLINE): only bundled and cached data are used.".into(),
        );
    }
    let data = DataSourceStatus {
        server_version: phn_core::VERSION.into(),
        offline_mode: ctx.config.offline,
        sources: rows,
        general_caveats: registry.general_caveats.clone(),
        startup_warnings: ctx.startup_warnings.clone(),
    };
    Ok(Envelope::new(summary, data)
        .source(Source {
            title: registry.meta.title.clone(),
            publisher: registry.meta.source.publisher.clone(),
            url: registry.meta.source.url.clone(),
            effective_date: Some(registry.meta.prepared.clone()),
            retrieved: None,
            mode: SourceMode::Snapshot,
        })
        .caveats(caveats)
        .privacy(Privacy::none()))
}
