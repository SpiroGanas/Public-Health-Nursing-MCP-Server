//! Full-text index of the agency's own policies and procedures.
//!
//! Documents in the policy folder (PDF, DOCX, Markdown, text, HTML) are split
//! into sections (headings, or pages for PDFs) and indexed with `tantivy` in
//! `/cache/index/policies`. The index is rebuilt automatically when files are
//! added, changed, or removed. Section text is stored after standard
//! redaction, so search snippets never contain Social Security numbers or
//! labeled dates of birth or record numbers that may appear in a policy's
//! examples.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, STORED, STRING, Schema, TEXT, Value};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexReader, TantivyDocument, doc};

use crate::extract::{FileKind, extract};
use crate::phi::{RedactionLevel, redact_text};

const INDEX_FORMAT: u32 = 1;
const CHUNK_CHARS: usize = 2_000;

/// A search hit with its citation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PolicyHit {
    /// File path relative to the policy folder.
    pub file: String,
    /// Section heading (or "Page N").
    pub section: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<u32>,
    pub score: f32,
    /// Matching passage; query terms in **bold**.
    pub snippet: String,
    /// Ready-to-use citation, e.g. "Exposure Control Plan.docx, § Reporting".
    pub citation: String,
}

/// Statistics about the index.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct IndexStats {
    pub rebuilt: bool,
    pub files_indexed: usize,
    pub sections_indexed: usize,
    /// Files that could not be read, with the reason.
    pub skipped: Vec<(String, String)>,
    /// Identifiers redacted from stored text while indexing.
    pub redactions: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
struct Manifest {
    format: u32,
    files: BTreeMap<String, (u64, i64)>,
    stats: IndexStats,
}

#[derive(Clone, Copy)]
struct Fields {
    file: Field,
    section: Field,
    body: Field,
    page: Field,
}

/// An opened policy index.
pub struct PolicyIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    pub stats: IndexStats,
}

fn schema() -> (Schema, Fields) {
    let mut b = Schema::builder();
    let file = b.add_text_field("file", STRING | STORED);
    let section = b.add_text_field("section", TEXT | STORED);
    let body = b.add_text_field("body", TEXT | STORED);
    let page = b.add_u64_field("page", STORED);
    (
        b.build(),
        Fields {
            file,
            section,
            body,
            page,
        },
    )
}

fn supported(path: &Path) -> bool {
    matches!(
        FileKind::of(path),
        FileKind::Pdf | FileKind::Docx | FileKind::Markdown | FileKind::Text | FileKind::Html
    )
}

/// Scan the policy folder: relative path -> (size, modified seconds).
fn scan(policy_dir: &Path) -> BTreeMap<String, (u64, i64)> {
    let mut out = BTreeMap::new();
    for e in walkdir::WalkDir::new(policy_dir)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .flatten()
    {
        if !e.file_type().is_file() || !supported(e.path()) {
            continue;
        }
        let Ok(meta) = e.metadata() else { continue };
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let rel = e
            .path()
            .strip_prefix(policy_dir)
            .unwrap_or(e.path())
            .to_string_lossy()
            .replace('\\', "/");
        out.insert(rel, (meta.len(), mtime));
    }
    out
}

fn idx_err(e: impl std::fmt::Display) -> PhnError {
    PhnError::Internal(format!("policy index: {e}"))
}

/// Split long text into chunks at paragraph boundaries.
fn chunks(text: &str) -> Vec<String> {
    if text.len() <= CHUNK_CHARS {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for para in text.split('\n') {
        if cur.len() + para.len() > CHUNK_CHARS && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push_str(para);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

impl PolicyIndex {
    /// Open the index in `index_dir`, rebuilding it if the policy folder
    /// changed. With `index_dir = None` the index is built in memory.
    pub fn open_or_build(policy_dir: &Path, index_dir: Option<&Path>) -> Result<Self> {
        if !policy_dir.is_dir() {
            return Err(PhnError::NotFound(format!(
                "the policy folder {} does not exist. Create a 'policies' folder in your data \
                 folder and copy your agency's policies and procedures into it.",
                policy_dir.display()
            )));
        }
        let current = scan(policy_dir);
        let (schema, fields) = schema();

        if let Some(dir) = index_dir {
            let manifest_path = dir.join("manifest.json");
            let stored: Option<Manifest> = std::fs::read_to_string(&manifest_path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok());
            if let Some(m) = stored.filter(|m| m.format == INDEX_FORMAT && m.files == current)
                && let Ok(index) = Index::open_in_dir(dir)
            {
                let reader = index.reader().map_err(idx_err)?;
                let mut stats = m.stats;
                stats.rebuilt = false;
                return Ok(Self {
                    index,
                    reader,
                    fields,
                    stats,
                });
            }
            // Rebuild from scratch.
            if dir.exists() {
                std::fs::remove_dir_all(dir)?;
            }
            std::fs::create_dir_all(dir)?;
            let index = Index::create_in_dir(dir, schema).map_err(idx_err)?;
            let stats = Self::fill(&index, fields, policy_dir, &current)?;
            let manifest = Manifest {
                format: INDEX_FORMAT,
                files: current,
                stats: stats.clone(),
            };
            std::fs::write(
                manifest_path,
                serde_json::to_string(&manifest).map_err(idx_err)?,
            )?;
            let reader = index.reader().map_err(idx_err)?;
            return Ok(Self {
                index,
                reader,
                fields,
                stats,
            });
        }

        let index = Index::create_in_ram(schema);
        let stats = Self::fill(&index, fields, policy_dir, &current)?;
        let reader = index.reader().map_err(idx_err)?;
        Ok(Self {
            index,
            reader,
            fields,
            stats,
        })
    }

    fn fill(
        index: &Index,
        f: Fields,
        policy_dir: &Path,
        files: &BTreeMap<String, (u64, i64)>,
    ) -> Result<IndexStats> {
        let mut writer = index
            .writer::<TantivyDocument>(30_000_000)
            .map_err(idx_err)?;
        let mut stats = IndexStats {
            rebuilt: true,
            ..IndexStats::default()
        };
        for rel in files.keys() {
            let path: PathBuf = policy_dir.join(rel);
            let extracted = match extract(&path) {
                Ok(e) => e,
                Err(e) => {
                    stats.skipped.push((rel.clone(), e.to_string()));
                    continue;
                }
            };
            let mut any = false;
            for s in &extracted.sections {
                for chunk in chunks(&s.text) {
                    if chunk.trim().is_empty() && s.heading.is_empty() {
                        continue;
                    }
                    let red = redact_text(&chunk, RedactionLevel::Standard);
                    stats.redactions += red.redacted.values().sum::<usize>();
                    let heading = if s.heading.is_empty() {
                        "(untitled section)".to_string()
                    } else {
                        s.heading.clone()
                    };
                    writer
                        .add_document(doc!(
                            f.file => rel.clone(),
                            f.section => heading,
                            f.body => red.text,
                            f.page => u64::from(s.page.unwrap_or(0)),
                        ))
                        .map_err(idx_err)?;
                    stats.sections_indexed += 1;
                    any = true;
                }
            }
            if any {
                stats.files_indexed += 1;
            } else {
                stats
                    .skipped
                    .push((rel.clone(), "no text found (scanned image?)".into()));
            }
        }
        writer.commit().map_err(idx_err)?;
        Ok(stats)
    }

    /// Search policies. Query syntax: words, "quoted phrases", AND/OR/NOT.
    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<PolicyHit>> {
        let searcher = self.reader.searcher();
        let mut parser =
            QueryParser::for_index(&self.index, vec![self.fields.section, self.fields.body]);
        parser.set_field_boost(self.fields.section, 2.0);
        let (q, _errors) = parser.parse_query_lenient(query);
        let top = searcher
            .search(
                &q,
                &TopDocs::with_limit(limit.clamp(1, 50)).order_by_score(),
            )
            .map_err(idx_err)?;
        let mut snippets =
            SnippetGenerator::create(&searcher, &*q, self.fields.body).map_err(idx_err)?;
        snippets.set_max_num_chars(400);
        let mut out = Vec::new();
        for (score, addr) in top {
            let d: TantivyDocument = searcher.doc(addr).map_err(idx_err)?;
            let get = |field: Field| {
                d.get_first(field)
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string()
            };
            let file = get(self.fields.file);
            let section = get(self.fields.section);
            let page = d
                .get_first(self.fields.page)
                .and_then(|v| v.as_u64())
                .filter(|&p| p > 0)
                .map(|p| p as u32);
            let snip = snippets.snippet_from_doc(&d);
            let snippet = if snip.fragment().is_empty() {
                get(self.fields.body).chars().take(300).collect()
            } else {
                let frag = snip.fragment();
                let mut s = String::new();
                let mut pos = 0;
                for r in snip.highlighted() {
                    s.push_str(&frag[pos..r.start]);
                    s.push_str("**");
                    s.push_str(&frag[r.start..r.end]);
                    s.push_str("**");
                    pos = r.end;
                }
                s.push_str(&frag[pos..]);
                s.split_whitespace().collect::<Vec<_>>().join(" ")
            };
            let file_name = file.rsplit('/').next().unwrap_or(&file).to_string();
            let citation = match page {
                Some(p) => format!("{file_name}, p. {p}"),
                None if section != "(untitled section)" => format!("{file_name}, § {section}"),
                None => file_name,
            };
            out.push(PolicyHit {
                file,
                section,
                page,
                score,
                snippet,
                citation,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexes_searches_and_rebuilds_on_change() {
        let dir = tempfile::tempdir().unwrap();
        let pol = dir.path().join("policies");
        std::fs::create_dir_all(&pol).unwrap();
        std::fs::write(
            pol.join("exposure.md"),
            "# Needlestick exposure\nReport needlestick injuries to the nursing supervisor within one hour.\n\
             Example form: SSN 123-45-6789.\n# Hand hygiene\nUse alcohol-based hand rub.\n",
        )
        .unwrap();
        let idx_dir = dir.path().join("index");
        let idx = PolicyIndex::open_or_build(&pol, Some(&idx_dir)).unwrap();
        assert!(idx.stats.rebuilt);
        assert_eq!(idx.stats.files_indexed, 1);
        assert_eq!(idx.stats.redactions, 1);
        let hits = idx.search("needlestick report", 5).unwrap();
        assert_eq!(hits[0].section, "Needlestick exposure");
        assert_eq!(hits[0].citation, "exposure.md, § Needlestick exposure");
        assert!(hits[0].snippet.contains("**"));
        assert!(hits.iter().all(|h| !h.snippet.contains("123-45-6789")));

        // Unchanged folder: reuse.
        drop(idx);
        let again = PolicyIndex::open_or_build(&pol, Some(&idx_dir)).unwrap();
        assert!(!again.stats.rebuilt);
        drop(again);

        // New file: rebuild.
        std::fs::write(
            pol.join("tb.txt"),
            "Tuberculosis screening for staff annually.",
        )
        .unwrap();
        let rebuilt = PolicyIndex::open_or_build(&pol, Some(&idx_dir)).unwrap();
        assert!(rebuilt.stats.rebuilt);
        assert_eq!(rebuilt.search("tuberculosis", 3).unwrap()[0].file, "tb.txt");
    }

    #[test]
    fn missing_folder_is_helpful() {
        let err = PolicyIndex::open_or_build(Path::new("/nonexistent/policies"), None)
            .err()
            .unwrap();
        assert!(err.to_string().contains("policies"));
    }
}
