//! CD-02 `search_ma_disease_control_guide`: offline full-text search of DPH's
//! *Guide to Surveillance, Reporting and Control* (disease chapters) and DPH
//! fact sheets, from the bundled corpus snapshot (`ma-dph-guide-corpus`,
//! built by `cargo xtask build-guide-corpus`).
//!
//! The tantivy index is built at first use in `/cache/index/dph-guide` (keyed
//! by the corpus checksum) or in memory when the cache is not writable, and
//! kept for the life of the process.

use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

use phn_core::text::normalize;
use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TextFieldIndexing, TextOptions, Value,
};
use tantivy::snippet::SnippetGenerator;
use tantivy::{Index, IndexReader, TantivyDocument, doc};

use crate::context::Context;
use crate::md;

const SNAPSHOT_ID: &str = "ma-dph-guide-corpus";
const FILE: &str = "ma_dph_guide_corpus.txt";
const INDEX_FORMAT: u32 = 2;
const CHUNK_CHARS: usize = 2_500;

/// Kind of document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DocKind {
    /// A chapter of the Guide to Surveillance, Reporting and Control.
    GuideChapter,
    /// A DPH disease fact sheet.
    FactSheet,
    /// Other DPH guidance pages (norovirus, respiratory viruses).
    Guidance,
}

impl DocKind {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "guide_chapter" => Some(Self::GuideChapter),
            "fact_sheet" => Some(Self::FactSheet),
            "guidance" => Some(Self::Guidance),
            _ => None,
        }
    }
    fn as_str(self) -> &'static str {
        match self {
            Self::GuideChapter => "guide_chapter",
            Self::FactSheet => "fact_sheet",
            Self::Guidance => "guidance",
        }
    }
}

/// Arguments for `search_ma_disease_control_guide`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
pub struct GuideSearchArgs {
    /// What to look for: words, "quoted phrases", AND/OR/NOT. E.g. "food handler
    /// exclusion", "incubation period", "\"post-exposure prophylaxis\"".
    #[serde(default)]
    pub query: Option<String>,
    /// Limit to documents about one disease (e.g. "measles", "whooping cough").
    /// With no query, returns that disease's chapter outline.
    #[serde(default)]
    pub disease: Option<String>,
    /// Limit to guide_chapter, fact_sheet, or guidance documents.
    #[serde(default)]
    pub kind: Option<DocKind>,
    /// Passages to return (default 5, at most 20).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Return each matching section's full text (up to about 2,500 characters)
    /// instead of a short highlighted excerpt.
    #[serde(default)]
    pub full_sections: Option<bool>,
    /// List every document in the corpus (titles, dates, URLs) instead of searching.
    #[serde(default)]
    pub list_documents: Option<bool>,
}

/// A document in the corpus.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GuideDocument {
    pub id: String,
    pub title: String,
    pub kind: DocKind,
    pub url: String,
    /// Document date (mass.gov file date for chapters), when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    pub sections: usize,
}

/// A quoted passage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Passage {
    pub document_id: String,
    pub document_title: String,
    pub kind: DocKind,
    /// Section path, e.g. "Section 4: Controlling Further Spread › A. Isolation and Quarantine Requirements".
    pub section: String,
    /// Quoted text from the document (search terms in **bold** in excerpts).
    pub text: String,
    /// True when `text` is a short excerpt rather than the whole section.
    pub excerpt: bool,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_date: Option<String>,
    /// Ready-to-use citation.
    pub citation: String,
    pub score: f32,
}

/// Result of `search_ma_disease_control_guide`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GuideSearchResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    /// Documents searched (after the disease and kind filters).
    pub documents_searched: usize,
    pub passages: Vec<Passage>,
    /// Section headings, when an outline was requested (disease without a query).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outline: Vec<String>,
    /// Corpus documents, when listed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documents: Vec<GuideDocument>,
    /// Corpus build date and size.
    pub corpus: String,
}

struct Section {
    doc: usize,
    heading: String,
    text: String,
}

struct Corpus {
    docs: Vec<GuideDocument>,
    sections: Vec<Section>,
}

fn parse(text: &str) -> Corpus {
    let mut docs: Vec<GuideDocument> = Vec::new();
    let mut sections: Vec<Section> = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') && docs.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix('@') {
            let (k, v) = rest.split_once(' ').unwrap_or((rest, ""));
            match k {
                "doc" => docs.push(GuideDocument {
                    id: v.to_string(),
                    title: String::new(),
                    kind: DocKind::FactSheet,
                    url: String::new(),
                    date: None,
                    sections: 0,
                }),
                "title" | "kind" | "url" | "date" => {
                    if let Some(d) = docs.last_mut() {
                        match k {
                            "title" => d.title = v.to_string(),
                            "kind" => d.kind = DocKind::parse(v).unwrap_or(DocKind::FactSheet),
                            "url" => d.url = v.to_string(),
                            _ => d.date = Some(v.to_string()),
                        }
                    }
                }
                "section" => {
                    if let Some(i) = docs.len().checked_sub(1) {
                        docs[i].sections += 1;
                        sections.push(Section {
                            doc: i,
                            heading: v.to_string(),
                            text: String::new(),
                        });
                    }
                }
                _ => {}
            }
            continue;
        }
        if let Some(s) = sections.last_mut() {
            let l = line.strip_prefix('\\').unwrap_or(line);
            s.text.push_str(l);
            s.text.push('\n');
        }
    }
    Corpus { docs, sections }
}

#[derive(Clone, Copy)]
struct Fields {
    sec: Field,
    doc: Field,
    title: Field,
    section: Field,
    body: Field,
}

struct GuideIndex {
    /// Corpus checksum and index directory.
    key: String,
    corpus: Corpus,
    index: Index,
    reader: IndexReader,
    fields: Fields,
}

fn idx_err(e: impl std::fmt::Display) -> PhnError {
    PhnError::Internal(format!("guide index: {e}"))
}

fn schema() -> (Schema, Fields) {
    // English stemming so "handler" also finds "handlers", "exclude" finds "excluded".
    let stemmed = || {
        TextOptions::default().set_indexing_options(
            TextFieldIndexing::default()
                .set_tokenizer("en_stem")
                .set_index_option(IndexRecordOption::WithFreqsAndPositions),
        )
    };
    let mut b = Schema::builder();
    let sec = b.add_u64_field("sec", STORED);
    let doc = b.add_text_field("doc", STRING | STORED);
    let title = b.add_text_field("title", stemmed());
    let section = b.add_text_field("section", stemmed().set_stored());
    let body = b.add_text_field("body", stemmed().set_stored());
    (
        b.build(),
        Fields {
            sec,
            doc,
            title,
            section,
            body,
        },
    )
}

fn chunks(text: &str) -> Vec<String> {
    if text.len() <= CHUNK_CHARS {
        return vec![text.to_string()];
    }
    let mut out = Vec::new();
    let mut cur = String::new();
    for line in text.lines() {
        if cur.len() + line.len() > CHUNK_CHARS && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        cur.push_str(line);
        cur.push('\n');
    }
    if !cur.trim().is_empty() {
        out.push(cur);
    }
    out
}

fn fill(index: &Index, f: Fields, c: &Corpus) -> Result<()> {
    let mut w = index
        .writer::<TantivyDocument>(30_000_000)
        .map_err(idx_err)?;
    for (i, s) in c.sections.iter().enumerate() {
        let d = &c.docs[s.doc];
        for chunk in chunks(&s.text) {
            w.add_document(doc!(
                f.sec => i as u64,
                f.doc => d.id.clone(),
                f.title => d.title.clone(),
                f.section => s.heading.clone(),
                f.body => chunk,
            ))
            .map_err(idx_err)?;
        }
    }
    w.commit().map_err(idx_err)?;
    Ok(())
}

fn open_or_build(dir: Option<&Path>, sha: &str, corpus: Corpus) -> Result<GuideIndex> {
    let (schema, fields) = schema();
    if let Some(dir) = dir {
        let stamp = dir.join("corpus.sha256");
        let current = format!("{INDEX_FORMAT}:{sha}");
        if std::fs::read_to_string(&stamp).ok().as_deref() == Some(current.as_str())
            && let Ok(index) = Index::open_in_dir(dir)
            && let Ok(reader) = index.reader()
        {
            return Ok(GuideIndex {
                key: String::new(),
                corpus,
                index,
                reader,
                fields,
            });
        }
        let built = (|| -> Result<Index> {
            if dir.exists() {
                std::fs::remove_dir_all(dir)?;
            }
            std::fs::create_dir_all(dir)?;
            let index = Index::create_in_dir(dir, schema.clone()).map_err(idx_err)?;
            fill(&index, fields, &corpus)?;
            std::fs::write(&stamp, &current)?;
            Ok(index)
        })();
        match built {
            Ok(index) => {
                let reader = index.reader().map_err(idx_err)?;
                return Ok(GuideIndex {
                    key: String::new(),
                    corpus,
                    index,
                    reader,
                    fields,
                });
            }
            Err(e) => tracing::warn!("guide index in the cache failed ({e}); using memory"),
        }
    }
    let index = Index::create_in_ram(schema);
    fill(&index, fields, &corpus)?;
    let reader = index.reader().map_err(idx_err)?;
    Ok(GuideIndex {
        key: String::new(),
        corpus,
        index,
        reader,
        fields,
    })
}

static CACHE: OnceLock<Mutex<Option<Arc<GuideIndex>>>> = OnceLock::new();

fn load(ctx: &Context) -> Result<(Arc<GuideIndex>, phn_data::snapshots::SnapshotEntry)> {
    let (text, entry, _origin) = ctx.snapshots.file_text(SNAPSHOT_ID, FILE)?;
    let sha = entry
        .files
        .iter()
        .find(|f| f.path == FILE)
        .map(|f| f.sha256.clone())
        .unwrap_or_default();
    let dir = ctx.config.paths.cache_dir.join("index").join("dph-guide");
    let key = format!("{sha}|{}", dir.display());
    let slot = CACHE.get_or_init(|| Mutex::new(None));
    let mut guard = slot.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(gi) = guard.as_ref()
        && gi.key == key
    {
        return Ok((Arc::clone(gi), entry));
    }
    let corpus = parse(&text);
    let mut built = open_or_build(Some(&dir), &sha, corpus)?;
    built.key = key;
    let gi = Arc::new(built);
    *guard = Some(Arc::clone(&gi));
    Ok((gi, entry))
}

/// Documents about a disease (by title, id, and the disease table's aliases).
fn docs_for_disease(c: &Corpus, q: &str) -> Vec<usize> {
    let mut terms = vec![normalize(q)];
    let mut guide_doc = None;
    if let Some(d) = phn_content::disease_rules::lookup(q) {
        terms.push(normalize(&d.name));
        terms.push(normalize(&d.id.replace('_', " ")));
        terms.extend(d.aliases.iter().map(|a| normalize(a)));
        guide_doc = d.guide_document.clone();
    }
    terms.retain(|t| t.len() >= 3);
    // Use the first word of multi-word names too ("salmonellosis non typhoidal").
    let firsts: Vec<String> = terms
        .iter()
        .filter_map(|t| t.split_whitespace().next())
        .filter(|w| w.len() >= 5)
        .map(str::to_string)
        .collect();
    terms.extend(firsts);
    c.docs
        .iter()
        .enumerate()
        .filter(|(_, d)| {
            let hay = format!(
                "{} {}",
                normalize(&d.title),
                normalize(&d.id.replace('-', " "))
            );
            guide_doc.as_deref() == Some(d.id.as_str())
                || terms.iter().any(|t| hay.contains(t.as_str()))
        })
        .map(|(i, _)| i)
        .collect()
}

fn citation(d: &GuideDocument, section: &str) -> String {
    let prefix = match d.kind {
        DocKind::GuideChapter => "MDPH",
        _ => "MDPH,",
    };
    let date = d
        .date
        .as_ref()
        .map(|x| format!(" (file dated {x})"))
        .unwrap_or_default();
    format!("{prefix} {}{date}, {section}. {}", d.title, d.url)
}

/// Search the DPH guide and fact sheets.
pub fn search_ma_disease_control_guide(
    ctx: &Context,
    args: GuideSearchArgs,
) -> Result<Envelope<GuideSearchResult>> {
    let (gi, entry) = load(ctx)?;
    let c = &gi.corpus;
    let corpus_label = format!(
        "{} documents, {} sections; built {} from mass.gov",
        c.docs.len(),
        c.sections.len(),
        entry.retrieved
    );
    let mut sources = vec![entry.to_source()];
    let caveats_base = vec![
        "Passages are quoted from DPH documents as published; many guide chapters are from \
         2016–2018. Check each passage's date, and confirm current requirements in 105 CMR \
         300.000 and current DPH guidance (calculate_isolation_and_exclusion has the exclusion \
         rules with citations)."
            .to_string(),
        "Search is by keywords, not meaning: try synonyms (for example \"exclusion\" and \
         \"isolation\", \"day care\" and \"child care\") if a passage seems missing."
            .to_string(),
    ];

    if args.list_documents == Some(true) {
        let docs: Vec<GuideDocument> = c
            .docs
            .iter()
            .filter(|d| args.kind.is_none_or(|k| k == d.kind))
            .cloned()
            .collect();
        let rows: Vec<Vec<String>> = docs
            .iter()
            .map(|d| {
                vec![
                    d.title.clone(),
                    d.kind.as_str().replace('_', " "),
                    d.date.clone().unwrap_or_else(|| "—".into()),
                    d.url.clone(),
                ]
            })
            .collect();
        let s = format!(
            "**DPH guide corpus** ({corpus_label})\n\n{}",
            md::table(&["Document", "Kind", "File date", "URL"], &rows)
        );
        return Ok(Envelope::new(
            s,
            GuideSearchResult {
                query: None,
                documents_searched: docs.len(),
                passages: Vec::new(),
                outline: Vec::new(),
                documents: docs,
                corpus: corpus_label,
            },
        )
        .sources(sources)
        .caveats(caveats_base)
        .privacy(Privacy::none()));
    }

    let mut allowed: Vec<usize> = (0..c.docs.len())
        .filter(|&i| args.kind.is_none_or(|k| k == c.docs[i].kind))
        .collect();
    if let Some(dz) = args.disease.as_deref().filter(|s| !s.trim().is_empty()) {
        let m = docs_for_disease(c, dz);
        allowed.retain(|i| m.contains(i));
        if allowed.is_empty() {
            return Err(PhnError::NotFound(format!(
                "no guide chapter or fact sheet matches '{dz}'. Use list_documents to see what \
                 the corpus holds, or search without the disease filter."
            )));
        }
    }
    let query = args
        .query
        .as_deref()
        .map(str::trim)
        .filter(|q| !q.is_empty());
    let Some(query) = query else {
        if args.disease.is_none() {
            return Err(PhnError::InvalidInput(
                "give a query (and optionally a disease), a disease for its outline, or \
                 list_documents"
                    .into(),
            ));
        }
        // Outline of the matching documents.
        let mut outline = Vec::new();
        let mut s = String::new();
        for &i in &allowed {
            let d = &c.docs[i];
            s.push_str(&format!(
                "**{}**{} — {}\n\n",
                d.title,
                d.date
                    .as_ref()
                    .map(|x| format!(" (file dated {x})"))
                    .unwrap_or_default(),
                d.url
            ));
            for sec in c.sections.iter().filter(|x| x.doc == i) {
                s.push_str(&format!("- {}\n", sec.heading));
                outline.push(format!("{}: {}", d.title, sec.heading));
            }
            s.push('\n');
            sources.push(doc_source(d, &entry.retrieved));
        }
        s.push_str(
            "Search within these with a query (for example \"exclusion\" or \"contacts\").\n",
        );
        return Ok(Envelope::new(
            s,
            GuideSearchResult {
                query: None,
                documents_searched: allowed.len(),
                passages: Vec::new(),
                outline,
                documents: Vec::new(),
                corpus: corpus_label,
            },
        )
        .sources(sources)
        .caveats(caveats_base)
        .privacy(Privacy::none())
        .decision_support());
    };
    if query.len() > 500 {
        return Err(PhnError::InvalidInput("the query is too long".into()));
    }
    let limit = args.limit.unwrap_or(5).clamp(1, 20);
    let full = args.full_sections.unwrap_or(false);
    let f = gi.fields;
    let searcher = gi.reader.searcher();
    let mut parser = QueryParser::for_index(&gi.index, vec![f.section, f.body, f.title]);
    parser.set_field_boost(f.section, 2.0);
    parser.set_field_boost(f.title, 1.5);
    let (q, _errors) = parser.parse_query_lenient(query);
    let top = searcher
        .search(&q, &TopDocs::with_limit(400).order_by_score())
        .map_err(idx_err)?;
    let mut snippets = SnippetGenerator::create(&searcher, &*q, f.body).map_err(idx_err)?;
    snippets.set_max_num_chars(450);
    let allowed_ids: std::collections::BTreeSet<&str> =
        allowed.iter().map(|&i| c.docs[i].id.as_str()).collect();
    let mut passages: Vec<Passage> = Vec::new();
    let mut seen_sections = std::collections::BTreeSet::new();
    for (score, addr) in top {
        if passages.len() >= limit {
            break;
        }
        let d: TantivyDocument = searcher.doc(addr).map_err(idx_err)?;
        let doc_id = d
            .get_first(f.doc)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        if !allowed_ids.contains(doc_id.as_str()) {
            continue;
        }
        let sec = d.get_first(f.sec).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        if !seen_sections.insert(sec) {
            continue;
        }
        let Some(section) = c.sections.get(sec) else {
            continue;
        };
        let gd = &c.docs[section.doc];
        let body = d
            .get_first(f.body)
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let (text, excerpt) = if full {
            (body.trim().to_string(), false)
        } else {
            let snip = snippets.snippet_from_doc(&d);
            if snip.fragment().is_empty() {
                (
                    body.chars()
                        .take(400)
                        .collect::<String>()
                        .trim()
                        .to_string(),
                    true,
                )
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
                (s.split_whitespace().collect::<Vec<_>>().join(" "), true)
            }
        };
        passages.push(Passage {
            document_id: gd.id.clone(),
            document_title: gd.title.clone(),
            kind: gd.kind,
            section: section.heading.clone(),
            text,
            excerpt,
            url: gd.url.clone(),
            document_date: gd.date.clone(),
            citation: citation(gd, &section.heading),
            score,
        });
    }

    let mut s = format!(
        "**DPH Guide and fact sheets — \"{query}\"**{} ({} passage(s); {} documents searched)\n\n",
        args.disease
            .as_deref()
            .map(|d| format!(" in documents about {d}"))
            .unwrap_or_default(),
        passages.len(),
        allowed.len()
    );
    if passages.is_empty() {
        s.push_str(
            "No passages matched. Try fewer or different words, remove the disease filter, or \
             list the documents with list_documents.\n",
        );
    }
    for (i, p) in passages.iter().enumerate() {
        s.push_str(&format!(
            "{}. **{}** — {}{}\n\n",
            i + 1,
            p.document_title,
            p.section,
            p.document_date
                .as_ref()
                .map(|d| format!(" (file dated {d})"))
                .unwrap_or_default()
        ));
        for line in p.text.lines() {
            s.push_str(&format!(
                "   > {}{line}{}\n",
                if p.excerpt { "…" } else { "" },
                if p.excerpt { "…" } else { "" }
            ));
        }
        s.push_str(&format!("\n   Source: {}\n\n", p.url));
    }
    let mut cited: Vec<&str> = passages.iter().map(|p| p.document_id.as_str()).collect();
    cited.sort();
    cited.dedup();
    for id in cited {
        if let Some(d) = c.docs.iter().find(|d| d.id == id) {
            sources.push(doc_source(d, &entry.retrieved));
        }
    }
    let data = GuideSearchResult {
        query: Some(query.to_string()),
        documents_searched: allowed.len(),
        passages,
        outline: Vec::new(),
        documents: Vec::new(),
        corpus: corpus_label,
    };
    Ok(Envelope::new(s, data)
        .sources(sources)
        .caveats(caveats_base)
        .privacy(Privacy::none())
        .decision_support())
}

fn doc_source(d: &GuideDocument, retrieved: &str) -> Source {
    Source {
        title: d.title.clone(),
        publisher: "Massachusetts Department of Public Health".into(),
        url: Some(d.url.clone()),
        effective_date: d.date.clone(),
        retrieved: Some(retrieved.to_string()),
        mode: SourceMode::Snapshot,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("cache")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[test]
    fn finds_the_salmonella_incubation_period_with_a_citation() {
        let (_d, c) = ctx();
        let env = search_ma_disease_control_guide(
            &c,
            GuideSearchArgs {
                query: Some("incubation period".into()),
                disease: Some("Salmonella".into()),
                kind: Some(DocKind::GuideChapter),
                full_sections: Some(true),
                ..GuideSearchArgs::default()
            },
        )
        .unwrap();
        let p = &env.data.passages[0];
        assert_eq!(p.document_id, "guide-salmonellosis-non-typhoid");
        assert!(p.section.contains("Incubation Period"), "{}", p.section);
        assert!(p.text.contains("6–72 hours"), "{}", p.text);
        assert!(
            p.citation
                .contains("mass.gov/doc/salmonellosis-non-typhoid")
        );
        assert!(env.notice.is_some());
    }

    #[test]
    fn outline_list_and_errors() {
        let (_d, c) = ctx();
        let env = search_ma_disease_control_guide(
            &c,
            GuideSearchArgs {
                disease: Some("whooping cough".into()),
                kind: Some(DocKind::GuideChapter),
                ..GuideSearchArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.outline.iter().any(|o| o.contains("Pertussis")));
        let env = search_ma_disease_control_guide(
            &c,
            GuideSearchArgs {
                list_documents: Some(true),
                ..GuideSearchArgs::default()
            },
        )
        .unwrap();
        assert!(env.data.documents.len() > 50);
        assert!(search_ma_disease_control_guide(&c, GuideSearchArgs::default()).is_err());
        // Reuses the on-disk index.
        let (gi, _) = load(&c).unwrap();
        assert!(!gi.key.is_empty());
    }
}
