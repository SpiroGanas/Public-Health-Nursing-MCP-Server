//! CORE-05 `search_agency_policies`.

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_local::policy_index::{IndexStats, PolicyHit, PolicyIndex};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::context::Context;

/// Arguments for `search_agency_policies`.
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchPoliciesArgs {
    /// What to look for, e.g. "needlestick exposure reporting". Supports
    /// "quoted phrases" and AND/OR/NOT.
    pub query: String,
    /// Policy folder relative to /data (default: the configured policy
    /// folder, normally /data/policies).
    #[serde(default)]
    pub folder: Option<String>,
    /// Number of passages to return (1–20, default 5).
    #[serde(default)]
    pub max_results: Option<usize>,
}

/// Result of `search_agency_policies`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PolicySearchResult {
    pub query: String,
    pub folder: String,
    pub hits: Vec<PolicyHit>,
    pub index: IndexStats,
}

/// Search the agency's own policies and procedures, with file and section citations.
pub fn search_agency_policies(
    ctx: &Context,
    args: SearchPoliciesArgs,
) -> Result<Envelope<PolicySearchResult>> {
    if args.query.trim().is_empty() {
        return Err(PhnError::InvalidInput("query must not be empty".into()));
    }
    let sandbox = ctx.sandbox()?;
    let (folder_path, folder_label, index_name) = match args.folder.as_deref().map(str::trim) {
        Some(f) if !f.is_empty() => {
            let p = sandbox.resolve_existing(f)?;
            let label = sandbox.display_path(&p);
            let key: String = label
                .chars()
                .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                .collect();
            (p, label, format!("policies-{key}"))
        }
        _ => {
            let p = ctx.config.paths.policy_dir.clone();
            let label = sandbox.display_path(&p);
            (
                p,
                if label.is_empty() {
                    "policies".into()
                } else {
                    label
                },
                "policies".to_string(),
            )
        }
    };
    let index_dir = ctx.config.paths.cache_dir.join("index").join(index_name);
    let writable = std::fs::create_dir_all(&index_dir).is_ok();
    let index = PolicyIndex::open_or_build(&folder_path, writable.then_some(index_dir.as_path()))?;
    let hits = index.search(&args.query, args.max_results.unwrap_or(5).clamp(1, 20))?;

    let mut summary = if hits.is_empty() {
        format!(
            "No passages in /data/{folder_label} match \"{}\". Try other words or a broader query.\n",
            args.query
        )
    } else {
        format!(
            "**{} passage(s) from your agency's policies** for \"{}\":\n\n",
            hits.len(),
            args.query
        )
    };
    for (i, h) in hits.iter().enumerate() {
        summary.push_str(&format!("{}. **{}** — {}\n", i + 1, h.citation, h.snippet));
    }
    let mut caveats = vec![
        "Quote the cited file and section when answering; confirm the policy is the current \
         version before relying on it."
            .to_string(),
    ];
    if !index.stats.skipped.is_empty() {
        caveats.push(format!(
            "{} file(s) could not be indexed: {}",
            index.stats.skipped.len(),
            index
                .stats
                .skipped
                .iter()
                .take(5)
                .map(|(f, why)| format!("{f} ({why})"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    if !writable {
        caveats.push(
            "The /cache volume is not writable, so the index is rebuilt on every search.".into(),
        );
    }
    let mut privacy = Privacy::none();
    if index.stats.redactions > 0 {
        privacy.notes.push(format!(
            "{} Social Security, record, or labeled birth-date value(s) in the policy files were \
             redacted when indexing.",
            index.stats.redactions
        ));
    }
    let mut sources: Vec<Source> = Vec::new();
    for h in &hits {
        if !sources.iter().any(|s| s.title == h.file) {
            sources.push(Source {
                title: h.file.clone(),
                publisher: "Your agency's policies (/data)".into(),
                url: None,
                effective_date: None,
                retrieved: None,
                mode: SourceMode::Local,
            });
        }
    }
    Ok(Envelope::new(
        summary,
        PolicySearchResult {
            query: args.query,
            folder: folder_label,
            hits,
            index: index.stats.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(privacy))
}
