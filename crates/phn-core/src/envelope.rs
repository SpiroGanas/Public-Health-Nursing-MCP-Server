//! The response contract shared by every tool.
//!
//! Each tool returns an [`Envelope`]: a markdown `summary` written for the
//! model, a typed `data` object (sent as MCP structured content), the
//! `sources` behind the answer, `caveats`, and a `privacy` block that says
//! whether row-level data is included and whether suppression was applied.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Notice attached to clinical, legal, and regulatory answers.
pub const DECISION_SUPPORT_NOTICE: &str = "Decision support only. Verify against the cited source and \
your agency's protocols before acting. This server does not submit reports or make clinical or \
legal determinations.";

/// How the data behind an answer was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SourceMode {
    /// Fetched from a public API during this call.
    Live,
    /// Served from the local cache of an earlier live call.
    Cache,
    /// Read from a dated snapshot or reviewed content bundled with the server.
    Snapshot,
    /// Read from the nurse's own files in the mounted `/data` folder.
    Local,
    /// Calculated by the server from the inputs provided.
    Computed,
}

impl SourceMode {
    /// Lower-case label used in markdown output.
    pub fn label(self) -> &'static str {
        match self {
            Self::Live => "live",
            Self::Cache => "cache",
            Self::Snapshot => "snapshot",
            Self::Local => "local file",
            Self::Computed => "computed",
        }
    }
}

/// A citation for the data or guidance behind an answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    /// Title of the document, dataset, or method.
    pub title: String,
    /// Publisher or author organization.
    pub publisher: String,
    /// Canonical URL, when one exists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Effective or publication date of the source (ISO 8601), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_date: Option<String>,
    /// Date the server retrieved or built this copy of the source (ISO 8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retrieved: Option<String>,
    /// How the source was accessed for this answer.
    pub mode: SourceMode,
}

impl Source {
    /// A source computed by the server (for example a statistical method).
    pub fn computed(title: impl Into<String>, publisher: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            publisher: publisher.into(),
            url: None,
            effective_date: None,
            retrieved: None,
            mode: SourceMode::Computed,
        }
    }

    /// A file from the nurse's mounted folder.
    pub fn local_file(relative_path: impl Into<String>) -> Self {
        Self {
            title: relative_path.into(),
            publisher: "Your files (/data)".into(),
            url: None,
            effective_date: None,
            retrieved: None,
            mode: SourceMode::Local,
        }
    }

    /// Builder: set the URL.
    pub fn with_url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }
}

/// Privacy facts about a response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Privacy {
    /// True when the response contains record-level (row-level) data from a
    /// local file. False for aggregates, reference content, and calculations.
    pub row_level_data: bool,
    /// True when small-cell suppression was applied to counts in the response.
    pub suppression_applied: bool,
    /// The suppression threshold in force (counts from 1 to threshold − 1 are suppressed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suppression_threshold: Option<u32>,
    /// Categories of possible identifiers detected in the input (never their values).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub identifiers_detected: Vec<String>,
    /// Files written to the output folder that may contain identifiable data.
    /// Their contents are not included in the response.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files_written: Vec<String>,
    /// Other privacy notes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

impl Privacy {
    /// Privacy block for responses built only from reference content or inputs
    /// supplied in the request.
    pub fn none() -> Self {
        Self::default()
    }
}

/// The response returned by every tool.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Envelope<T> {
    /// Markdown summary written for the model.
    pub summary: String,
    /// Structured result.
    pub data: T,
    /// Sources behind the answer.
    pub sources: Vec<Source>,
    /// Data limitations, small numbers, review status, and divergent guidance.
    pub caveats: Vec<String>,
    /// Privacy facts about this response.
    pub privacy: Privacy,
    /// Decision-support notice for clinical, legal, and regulatory answers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
}

impl<T> Envelope<T> {
    /// Start an envelope with a summary and data.
    pub fn new(summary: impl Into<String>, data: T) -> Self {
        Self {
            summary: summary.into(),
            data,
            sources: Vec::new(),
            caveats: Vec::new(),
            privacy: Privacy::none(),
            notice: None,
        }
    }

    /// Add a source.
    pub fn source(mut self, source: Source) -> Self {
        self.sources.push(source);
        self
    }

    /// Add several sources.
    pub fn sources(mut self, sources: impl IntoIterator<Item = Source>) -> Self {
        self.sources.extend(sources);
        self
    }

    /// Add a caveat.
    pub fn caveat(mut self, caveat: impl Into<String>) -> Self {
        self.caveats.push(caveat.into());
        self
    }

    /// Add several caveats.
    pub fn caveats(mut self, caveats: impl IntoIterator<Item = String>) -> Self {
        self.caveats.extend(caveats);
        self
    }

    /// Set the privacy block.
    pub fn privacy(mut self, privacy: Privacy) -> Self {
        self.privacy = privacy;
        self
    }

    /// Attach the standard decision-support notice.
    pub fn decision_support(mut self) -> Self {
        self.notice = Some(DECISION_SUPPORT_NOTICE.to_string());
        self
    }

    /// Render the summary, caveats, sources, and privacy facts as markdown.
    pub fn to_markdown(&self) -> String {
        let mut out = String::with_capacity(self.summary.len() + 512);
        out.push_str(self.summary.trim_end());
        out.push('\n');
        if !self.caveats.is_empty() {
            out.push_str("\n**Caveats**\n");
            for c in &self.caveats {
                out.push_str("- ");
                out.push_str(c);
                out.push('\n');
            }
        }
        if !self.sources.is_empty() {
            out.push_str("\n**Sources**\n");
            for s in &self.sources {
                out.push_str("- ");
                out.push_str(&s.title);
                out.push_str(" — ");
                out.push_str(&s.publisher);
                if let Some(url) = &s.url {
                    out.push_str(" (");
                    out.push_str(url);
                    out.push(')');
                }
                let mut details = vec![s.mode.label().to_string()];
                if let Some(d) = &s.effective_date {
                    details.push(format!("effective {d}"));
                }
                if let Some(d) = &s.retrieved {
                    details.push(format!("retrieved {d}"));
                }
                out.push_str(&format!(" [{}]\n", details.join("; ")));
            }
        }
        let p = &self.privacy;
        let mut privacy = Vec::new();
        privacy.push(if p.row_level_data {
            "row-level data included".to_string()
        } else {
            "no row-level data".to_string()
        });
        if p.suppression_applied {
            match p.suppression_threshold {
                Some(t) => privacy.push(format!("small-cell suppression applied (<{t})")),
                None => privacy.push("small-cell suppression applied".into()),
            }
        }
        if !p.identifiers_detected.is_empty() {
            privacy.push(format!(
                "possible identifiers detected: {}",
                p.identifiers_detected.join(", ")
            ));
        }
        if !p.files_written.is_empty() {
            privacy.push(format!(
                "written to output folder (contents not shown): {}",
                p.files_written.join(", ")
            ));
        }
        out.push_str("\n**Privacy:** ");
        out.push_str(&privacy.join("; "));
        out.push('\n');
        for n in &p.notes {
            out.push_str("- ");
            out.push_str(n);
            out.push('\n');
        }
        if let Some(n) = &self.notice {
            out.push_str("\n_");
            out.push_str(n);
            out.push_str("_\n");
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_includes_all_sections() {
        let env = Envelope::new("Hello", 1)
            .source(Source::computed("Exact Poisson interval", "PHN MCP server"))
            .caveat("Small numbers")
            .privacy(Privacy {
                suppression_applied: true,
                suppression_threshold: Some(5),
                ..Privacy::default()
            })
            .decision_support();
        let md = env.to_markdown();
        assert!(md.starts_with("Hello"));
        assert!(md.contains("**Caveats**\n- Small numbers"));
        assert!(md.contains("Exact Poisson interval — PHN MCP server [computed]"));
        assert!(md.contains("small-cell suppression applied (<5)"));
        assert!(md.contains("Decision support only"));
    }

    #[test]
    fn serializes_mode_snake_case() {
        let s = Source::local_file("a.csv");
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["mode"], "local");
        assert!(v.get("url").is_none());
    }
}
