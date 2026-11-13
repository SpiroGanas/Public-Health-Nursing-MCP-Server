//! The law corpus behind `search_public_health_law` (POL-02).
//!
//! * Three bundled snapshots built by `xtask/scripts/build_law_snapshots.py`: M.G.L. c. 111,
//!   105 CMR, and 244 CMR (a section index for everything, full text for part).
//! * An optional larger corpus in the same format (`--extended` of the builder), installed as
//!   `law_corpus/` in the data folder or the cache folder (in that order). It is used only when
//!   its manifest is the expected format and every file matches its SHA-256; otherwise it is
//!   ignored and the reason is returned as a note (the same pattern as the offline geocoder,
//!   ADR 0011).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use phn_core::{PhnError, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::snapshots::{SnapshotEntry, SnapshotOrigin, SnapshotStore};

/// Format name written into every corpus file.
pub const FORMAT: &str = "phn-law-corpus";
/// Format version this reader understands.
pub const FORMAT_VERSION: u32 = 1;
/// Folder name of the optional larger corpus.
pub const EXTENDED_DIR: &str = "law_corpus";

/// One bundled source: corpus source id, snapshot id, file.
pub const SOURCES: &[(&str, &str, &str)] = &[
    ("mgl_c111", "ma-law-mgl-c111", "ma_law_mgl_c111.json"),
    ("cmr_105", "ma-law-105-cmr", "ma_law_105_cmr.json"),
    ("cmr_244", "ma-law-244-cmr", "ma_law_244_cmr.json"),
];

/// A chapter of a regulation (or the one chapter of a statute).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Chapter {
    pub ch: String,
    pub title: String,
    /// For example "105 CMR 300.00".
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub pdf_url: Option<String>,
    /// Date shown on the source's page (ISO), when it shows one.
    #[serde(default)]
    pub date: Option<String>,
    /// True when at least one section of the chapter is in full text.
    #[serde(default)]
    pub full_text: bool,
}

/// A section (statute section or regulation section).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Provision {
    /// Citation, for example "105 CMR 300.200" or "M.G.L. c. 111, § 95".
    pub c: String,
    /// Section number as printed (for example "300.200" or "5A 1/2").
    pub n: String,
    /// Chapter key.
    pub ch: String,
    /// Section title.
    pub t: String,
    /// Link to the section, when it has its own page.
    #[serde(default)]
    pub u: Option<String>,
    /// Section text (whitespace collapsed); absent when only the index entry is bundled.
    #[serde(default)]
    pub x: Option<String>,
}

/// One source's corpus file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Corpus {
    pub format: String,
    pub format_version: u32,
    pub source_id: String,
    pub title: String,
    pub kind: String,
    pub retrieved: String,
    pub index_url: String,
    #[serde(default)]
    pub currency: String,
    #[serde(default)]
    pub repealed_sections_omitted: Option<u32>,
    pub chapters: Vec<Chapter>,
    pub provisions: Vec<Provision>,
}

impl Corpus {
    /// Provisions with full text.
    pub fn full_text_count(&self) -> usize {
        self.provisions.iter().filter(|p| p.x.is_some()).count()
    }
}

/// A corpus with where it came from.
#[derive(Debug, Clone)]
pub struct LoadedCorpus {
    pub corpus: Corpus,
    /// The bundled snapshot's manifest entry (for citation).
    pub entry: Option<SnapshotEntry>,
    /// "bundled", "cache snapshot override", or "extended".
    pub origin: String,
}

/// Parse and check a corpus file.
pub fn parse(text: &str, expected_source: &str) -> Result<Corpus> {
    let c: Corpus = serde_json::from_str(text)
        .map_err(|e| PhnError::Internal(format!("law corpus {expected_source}: {e}")))?;
    if c.format != FORMAT || c.format_version != FORMAT_VERSION {
        return Err(PhnError::Internal(format!(
            "law corpus {expected_source}: unsupported format {} v{}",
            c.format, c.format_version
        )));
    }
    if c.source_id != expected_source {
        return Err(PhnError::Internal(format!(
            "law corpus {expected_source}: file says it is {}",
            c.source_id
        )));
    }
    let mut seen = std::collections::BTreeSet::new();
    for p in &c.provisions {
        if !seen.insert(p.c.as_str()) {
            return Err(PhnError::Internal(format!(
                "law corpus {expected_source}: {} appears twice",
                p.c
            )));
        }
    }
    Ok(c)
}

/// The bundled corpora (or newer checksum-verified cache copies of the snapshots).
pub fn load_bundled(store: &SnapshotStore) -> Result<Vec<LoadedCorpus>> {
    let mut out = Vec::new();
    for (source, snap, file) in SOURCES {
        let (text, entry, origin) = store.file_text(snap, file)?;
        out.push(LoadedCorpus {
            corpus: parse(&text, source)?,
            entry: Some(entry),
            origin: match origin {
                SnapshotOrigin::Bundled => "bundled".into(),
                SnapshotOrigin::CacheOverride => "cache snapshot override".into(),
            },
        });
    }
    Ok(out)
}

#[derive(Debug, Deserialize)]
struct ExtManifest {
    format: String,
    format_version: u32,
    #[serde(default)]
    built: String,
    files: Vec<ExtFile>,
}

#[derive(Debug, Deserialize)]
struct ExtFile {
    path: String,
    source_id: String,
    sha256: String,
}

/// Result of looking for the optional larger corpus.
#[derive(Debug, Clone, Default)]
pub struct ExtendedLoad {
    pub corpora: Vec<Corpus>,
    /// Folder it was read from.
    pub folder: Option<PathBuf>,
    pub built: Option<String>,
    /// Why a folder was ignored, or how many files were used.
    pub notes: Vec<String>,
}

/// Look for `law_corpus/` in each of `roots` (data folder, then cache folder) and use the first
/// one whose manifest and checksums are valid.
pub fn load_extended(roots: &[PathBuf]) -> ExtendedLoad {
    let mut result = ExtendedLoad::default();
    for root in roots {
        let dir = root.join(EXTENDED_DIR);
        if !dir.is_dir() {
            continue;
        }
        match read_extended(&dir) {
            Ok((corpora, built)) => {
                result.notes.push(format!(
                    "Larger law corpus in use from {} ({} source file(s), built {}).",
                    dir.display(),
                    corpora.len(),
                    built
                ));
                result.corpora = corpora;
                result.folder = Some(dir);
                result.built = Some(built);
                return result;
            }
            Err(why) => result.notes.push(format!(
                "The larger law corpus in {} was ignored: {why}. The bundled corpus was used.",
                dir.display()
            )),
        }
    }
    result
}

fn read_extended(dir: &Path) -> std::result::Result<(Vec<Corpus>, String), String> {
    let text = std::fs::read_to_string(dir.join("manifest.json"))
        .map_err(|_| "it has no manifest.json".to_string())?;
    let m: ExtManifest =
        serde_json::from_str(&text).map_err(|_| "manifest.json is not valid".to_string())?;
    if m.format != format!("{FORMAT}-manifest") || m.format_version != FORMAT_VERSION {
        return Err(format!(
            "its format ({} v{}) is not the one this release reads; rebuild it with this release's script",
            m.format, m.format_version
        ));
    }
    let mut out = Vec::new();
    for f in &m.files {
        // File names are plain names inside the folder.
        if f.path.contains('/') || f.path.contains('\\') || f.path.starts_with('.') {
            return Err("a file name in the manifest is not a plain file name".into());
        }
        let bytes = std::fs::read(dir.join(&f.path))
            .map_err(|_| format!("{} is missing", f.path))?;
        let sum = hex(&Sha256::digest(&bytes));
        if !sum.eq_ignore_ascii_case(&f.sha256) {
            return Err(format!("{} does not match its checksum", f.path));
        }
        let text = String::from_utf8(bytes).map_err(|_| format!("{} is not UTF-8", f.path))?;
        let c = parse(&text, &f.source_id).map_err(|e| e.to_string())?;
        out.push(c);
    }
    if out.is_empty() {
        return Err("the manifest lists no files".into());
    }
    Ok((out, m.built))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// Overlay an extended corpus on a bundled one: sections of the same citation are replaced (so a
/// section gains its full text), new sections are added, and chapters are replaced by key.
pub fn merge(base: &Corpus, ext: &Corpus) -> Corpus {
    let mut out = base.clone();
    let mut index: BTreeMap<String, usize> = out
        .provisions
        .iter()
        .enumerate()
        .map(|(i, p)| (p.c.clone(), i))
        .collect();
    for p in &ext.provisions {
        match index.get(&p.c) {
            Some(&i) => {
                // keep the bundled text if the extended entry has none
                let keep = out.provisions[i].x.clone();
                out.provisions[i] = p.clone();
                if out.provisions[i].x.is_none() {
                    out.provisions[i].x = keep;
                }
            }
            None => {
                index.insert(p.c.clone(), out.provisions.len());
                out.provisions.push(p.clone());
            }
        }
    }
    for c in &ext.chapters {
        match out.chapters.iter_mut().find(|x| x.ch == c.ch) {
            Some(slot) => *slot = c.clone(),
            None => out.chapters.push(c.clone()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_corpora_parse_and_are_bounded() {
        let store = SnapshotStore::bundled();
        let all = load_bundled(&store).unwrap();
        assert_eq!(all.len(), 3);
        for l in &all {
            let bytes: usize = l
                .corpus
                .provisions
                .iter()
                .map(|p| p.x.as_deref().map(str::len).unwrap_or(0))
                .sum();
            assert!(bytes < 1_000_000, "{} text is {bytes} bytes", l.corpus.source_id);
            assert!(!l.corpus.chapters.is_empty());
            assert_eq!(l.origin, "bundled");
        }
        let mgl = &all[0].corpus;
        assert!(mgl.provisions.len() > 400 && mgl.full_text_count() == 45);
        assert!(mgl.provisions.iter().any(|p| p.c == "M.G.L. c. 111, § 95" && p.x.is_some()));
        let c105 = &all[1].corpus;
        assert!(c105.chapters.len() >= 85);
        assert!(c105.provisions.iter().any(|p| p.c == "105 CMR 300.200" && p.x.is_some()));
        // a chapter that is index-only has titles and no text
        assert!(
            c105.provisions
                .iter()
                .any(|p| p.ch == "130" && p.x.is_none() && !p.t.is_empty())
        );
        let c244 = &all[2].corpus;
        assert_eq!(c244.chapters.len(), 8);
        assert_eq!(c244.full_text_count(), c244.provisions.len());
    }

    #[test]
    fn extended_corpus_needs_matching_checksums() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().to_path_buf();
        let law = root.join(EXTENDED_DIR);
        std::fs::create_dir_all(&law).unwrap();
        let body = serde_json::json!({
            "format": FORMAT, "format_version": FORMAT_VERSION, "source_id": "mgl_c111",
            "title": "t", "kind": "statute", "retrieved": "2026-11-12", "index_url": "u",
            "chapters": [], "provisions": [
              {"c": "M.G.L. c. 111, § 95", "n": "95", "ch": "111", "t": "Powers", "x": "full text"}]
        })
        .to_string();
        std::fs::write(law.join("a.json"), &body).unwrap();
        let sum = hex(&Sha256::digest(body.as_bytes()));
        let manifest = |sha: &str| {
            serde_json::json!({"format": "phn-law-corpus-manifest", "format_version": 1,
              "built": "2026-11-13", "files": [{"path": "a.json", "source_id": "mgl_c111", "sha256": sha}]})
            .to_string()
        };
        std::fs::write(law.join("manifest.json"), manifest("00")).unwrap();
        let bad = load_extended(std::slice::from_ref(&root));
        assert!(bad.corpora.is_empty() && bad.notes[0].contains("checksum"), "{:?}", bad.notes);
        std::fs::write(law.join("manifest.json"), manifest(&sum)).unwrap();
        let ok = load_extended(&[root]);
        assert_eq!(ok.corpora.len(), 1);
        assert_eq!(ok.built.as_deref(), Some("2026-11-13"));
    }

    #[test]
    fn merge_adds_text_and_sections() {
        let store = SnapshotStore::bundled();
        let all = load_bundled(&store).unwrap();
        let base = &all[0].corpus;
        let mut ext = base.clone();
        let idx = ext.provisions.iter().position(|p| p.x.is_none()).unwrap();
        ext.provisions[idx].x = Some("new text".into());
        ext.provisions.push(Provision {
            c: "M.G.L. c. 111, § 999".into(),
            n: "999".into(),
            ch: "111".into(),
            t: "Added".into(),
            u: None,
            x: None,
        });
        let m = merge(base, &ext);
        assert_eq!(m.provisions.len(), base.provisions.len() + 1);
        assert_eq!(m.provisions[idx].x.as_deref(), Some("new text"));
    }
}
