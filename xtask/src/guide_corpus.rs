//! `cargo xtask build-guide-corpus [--check] [--source-dir DIR]`: download
//! DPH's *Guide to Surveillance, Reporting and Control* chapters (DOCX/RTF)
//! and DPH disease fact sheets (mass.gov pages), extract their text, split it
//! into sections, and write `snapshots/ma_dph_guide_corpus.txt` plus its
//! manifest entry (URLs, dates, SHA-256 of every source document).
//!
//! The server indexes the corpus with tantivy at first use (in /cache); the
//! index is not committed. Text is quoted as extracted: no editing.
//!
//! With `--source-dir DIR`, documents are read from `DIR/<doc id>` (and the
//! `Last-Modified` date from `DIR/<doc id>.date` when present) instead of the
//! network, for offline rebuilds.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use scraper::{ElementRef, Html, Node, Selector};

use crate::download::sha256;
use crate::snapshots::{Manifest, SnapshotEntry, SnapshotFile};

pub const SNAPSHOT_ID: &str = "ma-dph-guide-corpus";
pub const FILE: &str = "ma_dph_guide_corpus.txt";
const HANDBOOK: &str = "https://www.mass.gov/handbook/guide-to-surveillance-reporting-and-control";

/// Guide chapters: (mass.gov doc slug, title as listed on the handbook page).
const CHAPTERS: &[(&str, &str)] = &[
    ("amebiasis-2016", "Amebiasis (2016)"),
    ("botulism-2018", "Botulism (2018)"),
    ("campylobacter-enteritis-2016-0", "Campylobacter Enteritis (2016)"),
    ("chickenpox-2023", "Chickenpox (2023)"),
    ("cryptosporidiosis-2016", "Cryptosporidiosis (2016)"),
    ("cyclosporiasis-2016", "Cyclosporiasis (2016)"),
    ("giardiasis-2016", "Giardiasis (2016)"),
    (
        "group-a-streptococcus-invasive-2018",
        "Group A Streptococcus (Invasive) (2018)",
    ),
    ("hansens-disease-2016", "Hansen's Disease (2016)"),
    ("hepatitis-a-2016", "Hepatitis A (2016)"),
    (
        "invasive-meningococcal-disease-2018",
        "Invasive Meningococcal Disease (2018)",
    ),
    ("legionellosis-2016", "Legionellosis (2016)"),
    ("listeriosis-2018", "Listeriosis (2018)"),
    ("measles-2022", "Measles (2022)"),
    ("mumps-2022", "Mumps (2022)"),
    ("pertussis-2022", "Pertussis (2022)"),
    ("rubella-and-crs-2022", "Rubella and CRS (2022)"),
    ("salmonellosis-non-typhoid", "Salmonellosis (Non-Typhoid)"),
    (
        "sexually-transmitted-infections-2018",
        "Sexually Transmitted Infections (2018)",
    ),
    (
        "shiga-toxin-producing-escherichia-coli-2018",
        "Shiga Toxin-Producing Escherichia coli (2018)",
    ),
    ("shigellosis-2018", "Shigellosis (2018)"),
    (
        "streptococcus-pneumoniae-invasive-disease-2016",
        "Streptococcus pneumoniae - Invasive Disease (2016)",
    ),
    ("tetanus-2022", "Tetanus (2022)"),
    ("toxic-shock-syndrome-2018", "Toxic Shock Syndrome (2018)"),
    ("vibriosis-2018", "Vibriosis (2018)"),
    ("yersiniosis-2016", "Yersiniosis (2016)"),
];

/// DPH fact sheets and guidance pages (paths under https://www.mass.gov/).
const PAGES: &[&str] = &[
    "info-details/anaplasmosis-hga",
    "info-details/anthrax",
    "info-details/babesiosis",
    "info-details/borrelia-miyamotoi",
    "info-details/botulism",
    "info-details/brucellosis",
    "info-details/campylobacter",
    "info-details/chickenpox",
    "info-details/chikungunya-virus",
    "info-details/cholera",
    "info-details/clostridium-difficile",
    "info-details/cryptosporidiosis",
    "info-details/cyclospora",
    "info-details/diseases-spread-by-mosquitoes-information-for-travelers",
    "info-details/eee-eastern-equine-encephalitis",
    "info-details/enterovirus-d68-ev-68",
    "info-details/fifth-disease",
    "info-details/giardia",
    "info-details/group-a-streptococcal-disease",
    "info-details/haemophilus-influenzae-type-b-hib",
    "info-details/hantavirus",
    "info-details/head-lice-pediculosis",
    "info-details/hepatitis-a",
    "info-details/hepatitis-b",
    "info-details/hepatitis-c",
    "info-details/influenza-birds-pandemics-and-protecting-yourself",
    "info-details/legionellosis-legionnaires-disease",
    "info-details/leptospirosis",
    "info-details/listeria-listeriosis",
    "info-details/lyme-disease",
    "info-details/measles",
    "info-details/methicillin-resistant-staphylococcus-aureus-mrsa",
    "info-details/meningitis",
    "info-details/meningococcal-disease",
    "info-details/meningococcal-disease-and-college-students",
    "info-details/mosquito-repellents",
    "info-details/mumps",
    "info-details/norovirus",
    "info-details/pertussis",
    "info-details/plague",
    "info-details/pneumococcal-disease",
    "info-details/polio",
    "info-details/powassan-virus",
    "info-details/psittacosis-in-birds-and-people",
    "info-details/rabies",
    "info-details/rocky-mountain-spotted-fever",
    "info-details/rubella-german-measles",
    "info-details/salmonella",
    "info-details/salmonellosis-from-pet-reptiles-and-amphibians",
    "info-details/scabies",
    "info-details/shiga-toxin-producing-escherichia-coli",
    "info-details/shigella",
    "info-details/smallpox",
    "info-details/tick-repellents",
    "info-details/tuberculosis",
    "info-details/tularemia",
    "info-details/vibrio-parahaemolyticus",
    "info-details/west-nile-virus-wnv",
    "info-details/what-should-i-know-about-flu",
    "info-details/about-covid-19-symptoms-and-spread",
    "info-details/staying-home-to-prevent-the-spread-of-respiratory-viruses",
    "info-details/flu-information-for-school-administrators-and-childcare-professionals",
    "info-details/norovirus-information-for-child-care-programs-and-k-12-schools",
    "info-details/norovirus-information-for-health-care-facilities",
    "info-details/reporting-norovirus",
    "guidance/guidance-for-health-care-personnel-with-a-respiratory-viral-pathogen-infection-or-exposure",
];

struct Doc {
    id: String,
    title: String,
    kind: &'static str,
    url: String,
    date: Option<String>,
    sha256: String,
    sections: Vec<(String, Vec<String>)>,
}

struct Fetched {
    bytes: Vec<u8>,
    last_modified: Option<String>,
}

fn client() -> Result<reqwest::blocking::Client> {
    Ok(reqwest::blocking::Client::builder()
        .user_agent(concat!(
            "phn-mcp-server-xtask/",
            env!("CARGO_PKG_VERSION"),
            " (+https://github.com/SpiroGanas/Public-Health-Nursing-MCP-Server)"
        ))
        .timeout(Duration::from_secs(300))
        .build()?)
}

fn http_date_to_iso(s: &str) -> Option<String> {
    chrono::DateTime::parse_from_rfc2822(s.trim())
        .ok()
        .map(|d| d.date_naive().to_string())
}

fn get(
    client: &Option<reqwest::blocking::Client>,
    id: &str,
    url: &str,
    source_dir: Option<&Path>,
) -> Result<Fetched> {
    if let Some(dir) = source_dir {
        let p = dir.join(id);
        let bytes = std::fs::read(&p).with_context(|| format!("reading {}", p.display()))?;
        let last_modified = std::fs::read_to_string(dir.join(format!("{id}.date")))
            .ok()
            .map(|s| s.trim().to_string());
        return Ok(Fetched {
            bytes,
            last_modified,
        });
    }
    let c = client.as_ref().expect("client when online");
    eprintln!("downloading {url}");
    std::thread::sleep(Duration::from_millis(400));
    let resp = c
        .get(url)
        .send()
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("GET {url}"))?;
    let last_modified = resp
        .headers()
        .get(reqwest::header::LAST_MODIFIED)
        .and_then(|v| v.to_str().ok())
        .and_then(http_date_to_iso);
    Ok(Fetched {
        bytes: resp.bytes()?.to_vec(),
        last_modified,
    })
}

/// Normalize whitespace and typographic oddities in a line.
fn clean(s: &str) -> String {
    let s = s.replace(['\u{a0}', '\t', '\u{2007}', '\u{202f}'], " ");
    let s: String = s
        .chars()
        .filter(|c| !c.is_control() && *c != '\u{fffd}' && *c != '\u{200b}')
        .collect();
    let mut out = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if out.starts_with('@') {
        out.insert(0, '\\');
    }
    out
}

const ACRONYMS: &[&str] = &[
    "MDPH", "DPH", "CDC", "MA", "HIV", "STI", "STD", "TB", "HCW", "HCP", "US", "U.S.", "PEP",
    "IG", "MAVEN", "SPHL", "LBOH", "CRS", "STEC", "HUS", "ACIP",
];

fn title_case(s: &str) -> String {
    if s.chars().any(|c| c.is_lowercase()) {
        return s.to_string();
    }
    let small = ["and", "or", "of", "the", "in", "for", "to", "a", "an", "on", "by"];
    s.split_whitespace()
        .enumerate()
        .map(|(i, w)| {
            let lw = w.to_lowercase();
            if i > 0 && small.contains(&lw.as_str()) {
                lw
            } else if ACRONYMS.contains(&w.trim_matches(|c: char| !c.is_alphanumeric())) {
                w.to_string()
            } else {
                let mut c = lw.chars();
                c.next()
                    .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                    .unwrap_or_default()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_subsection(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 4
        && b.len() <= 90
        && b[0].is_ascii_uppercase()
        && b[1] == b'.'
        && b[2] == b' '
        && b[3].is_ascii_uppercase()
        && !line.ends_with('.')
        && !line.ends_with(':')
        && line.split_whitespace().count() <= 12
}

/// Split guide chapter lines into sections ("Section 1: About the Disease ›
/// E. Incubation Period").
fn sectionize_chapter(lines: &[String]) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![("Introduction".into(), Vec::new())];
    let mut top = String::new();
    let mut i = 0;
    while i < lines.len() {
        let l = &lines[i];
        let lower = l.to_lowercase();
        let num = lower
            .strip_prefix("section ")
            .map(|r| r.trim().trim_end_matches(':').trim());
        let is_section =
            num.is_some_and(|n| !n.is_empty() && n.len() <= 2 && n.chars().all(|c| c.is_ascii_digit()));
        if is_section {
            let title = lines
                .get(i + 1)
                .filter(|t| t.len() <= 80 && !t.ends_with('.'))
                .map(|t| title_case(t));
            let head = l.trim().trim_end_matches(':');
            top = match &title {
                Some(t) => format!("{head}: {t}"),
                None => head.to_string(),
            };
            out.push((top.clone(), Vec::new()));
            i += if title.is_some() { 2 } else { 1 };
            continue;
        }
        if !top.is_empty() && is_subsection(l) {
            out.push((format!("{top} › {l}"), Vec::new()));
            i += 1;
            continue;
        }
        if let Some(last) = out.last_mut() {
            last.1.push(l.clone());
        }
        i += 1;
    }
    out.retain(|(_, body)| !body.is_empty());
    out
}

fn chapter_lines(bytes: &[u8]) -> Result<Vec<String>> {
    let text = if bytes.starts_with(b"PK") {
        let doc = phn_local::docx::parse_docx(bytes).map_err(anyhow::Error::msg)?;
        let mut out = Vec::new();
        for b in &doc.blocks {
            match b {
                phn_local::docx::Block::Paragraph(p) if !p.in_table => out.push(p.text.clone()),
                phn_local::docx::Block::Table(t) => {
                    for r in &t.rows {
                        out.push(r.join(" | "));
                    }
                }
                _ => {}
            }
        }
        out.join("\n")
    } else if bytes.starts_with(b"{\\rtf") {
        crate::rtf::to_text(bytes)
    } else {
        bail!("unsupported format (not DOCX or RTF)");
    };
    Ok(text
        .lines()
        .map(clean)
        .filter(|l| !l.is_empty() && l != "|" && !l.chars().all(|c| c == '|' || c == ' '))
        .collect())
}

/// Text of a mass.gov page: (title, sections by h2/h3 heading).
fn page_sections(html: &str) -> Result<(String, Vec<(String, Vec<String>)>)> {
    let doc = Html::parse_document(html);
    let sel = |s: &str| Selector::parse(s).expect("static selector");
    let title = doc
        .select(&sel("h1.ma__page-header__title, h1"))
        .next()
        .map(|h| clean(&h.text().collect::<Vec<_>>().join(" ")))
        .filter(|t| !t.is_empty())
        .context("page has no title")?;
    let body = [".ma__information-details__content", ".page-content", "main"]
        .iter()
        .find_map(|q| doc.select(&sel(q)).next())
        .context("page has no main content")?;
    let mut sections: Vec<(String, Vec<String>)> = vec![("Overview".into(), Vec::new())];
    let mut cur = String::new();
    walk(body, &mut sections, &mut cur);
    flush(&mut sections, &mut cur);
    for s in &mut sections {
        s.1.retain(|l| !l.is_empty());
    }
    sections.retain(|(_, b)| !b.is_empty());
    // Drop the feedback form and anything after it.
    if let Some(pos) = sections
        .iter()
        .position(|(h, _)| h.starts_with("Help Us Improve"))
    {
        sections.truncate(pos);
    }
    Ok((title, sections))
}

fn flush(sections: &mut [(String, Vec<String>)], cur: &mut String) {
    let line = clean(cur);
    cur.clear();
    if !line.is_empty()
        && let Some(last) = sections.last_mut()
    {
        last.1.push(line);
    }
}

fn walk(el: ElementRef<'_>, sections: &mut Vec<(String, Vec<String>)>, cur: &mut String) {
    for child in el.children() {
        match child.value() {
            Node::Text(t) => cur.push_str(t),
            Node::Element(e) => {
                let name = e.name();
                if matches!(
                    name,
                    "script" | "style" | "svg" | "button" | "nav" | "form" | "aside" | "noscript"
                ) {
                    continue;
                }
                let Some(ce) = ElementRef::wrap(child) else {
                    continue;
                };
                if matches!(name, "h2" | "h3" | "h4") {
                    flush(sections, cur);
                    let h = clean(&ce.text().collect::<Vec<_>>().join(" "));
                    if !h.is_empty() {
                        sections.push((h, Vec::new()));
                    }
                    continue;
                }
                let block = matches!(
                    name,
                    "p" | "li" | "div" | "ul" | "ol" | "table" | "tr" | "section" | "br" | "dd"
                        | "dt" | "blockquote"
                );
                if block {
                    flush(sections, cur);
                }
                if matches!(name, "td" | "th") && !cur.trim().is_empty() {
                    cur.push_str(" | ");
                }
                if name == "li" {
                    cur.push_str("- ");
                }
                walk(ce, sections, cur);
                if block {
                    flush(sections, cur);
                }
            }
            _ => {}
        }
    }
}

/// Render the corpus text.
fn render(docs: &[Doc], built: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# PHN MCP server corpus, format 1: Massachusetts DPH Guide to Surveillance, Reporting \
         and Control (disease chapters) and DPH fact sheets.\n# Built by `cargo xtask \
         build-guide-corpus` on {built}. Text is extracted verbatim from each source document; \
         see snapshots/manifest.json for checksums.\n"
    ));
    for d in docs {
        out.push_str(&format!("@doc {}\n@title {}\n@kind {}\n@url {}\n", d.id, d.title, d.kind, d.url));
        if let Some(date) = &d.date {
            out.push_str(&format!("@date {date}\n"));
        }
        out.push_str(&format!("@sha256 {}\n", d.sha256));
        for (h, body) in &d.sections {
            out.push_str(&format!("@section {h}\n"));
            for l in body {
                out.push_str(l);
                out.push('\n');
            }
        }
    }
    out
}

pub fn run(root: &Path, check: bool, source_dir: Option<PathBuf>) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let mut manifest: Manifest =
        serde_json::from_str(&std::fs::read_to_string(&manifest_path)?).context("manifest")?;
    let today = chrono::Local::now().date_naive().to_string();
    let client = if source_dir.is_none() {
        Some(client()?)
    } else {
        None
    };
    let mut docs = Vec::new();
    let mut skipped: Vec<String> = Vec::new();
    let mut source_sha = BTreeMap::new();
    let mut urls = vec![HANDBOOK.to_string()];
    for (slug, title) in CHAPTERS {
        let url = format!("https://www.mass.gov/doc/{slug}/download");
        let id = format!("guide-{}", slug.trim_end_matches("-0"));
        let f = get(&client, &id, &url, source_dir.as_deref())?;
        let sha = sha256(&f.bytes);
        match chapter_lines(&f.bytes) {
            Ok(lines) => {
                let sections = sectionize_chapter(&lines);
                if sections.len() < 3 {
                    skipped.push(format!("{title}: too little text extracted"));
                    continue;
                }
                source_sha.insert(id.clone(), sha.clone());
                urls.push(url.clone());
                docs.push(Doc {
                    id,
                    title: format!("Guide to Surveillance, Reporting and Control: {title}"),
                    kind: "guide_chapter",
                    url,
                    date: f.last_modified,
                    sha256: sha,
                    sections,
                });
            }
            Err(e) => skipped.push(format!("{title} ({url}): {e}")),
        }
    }
    for path in PAGES {
        let url = format!("https://www.mass.gov/{path}");
        let id = format!(
            "{}-{}",
            if path.starts_with("guidance/") {
                "guidance"
            } else {
                "factsheet"
            },
            path.rsplit('/').next().unwrap_or(path)
        );
        let f = get(&client, &id, &url, source_dir.as_deref())?;
        let sha = sha256(&f.bytes);
        let html = String::from_utf8_lossy(&f.bytes);
        match page_sections(&html) {
            Ok((title, sections)) if !sections.is_empty() => {
                source_sha.insert(id.clone(), sha.clone());
                urls.push(url.clone());
                docs.push(Doc {
                    id,
                    title,
                    kind: if path.starts_with("guidance/") || path.contains("information-for") {
                        "guidance"
                    } else {
                        "fact_sheet"
                    },
                    url,
                    // mass.gov pages carry no content date; Last-Modified is the cache time.
                    date: None,
                    sha256: sha,
                    sections,
                });
            }
            Ok(_) => skipped.push(format!("{url}: no text")),
            Err(e) => skipped.push(format!("{url}: {e}")),
        }
    }
    let text = render(&docs, &today);
    let committed = std::fs::read_to_string(snap_dir.join(FILE)).unwrap_or_default();
    // The header line carries the build date; compare the rest.
    let body = |s: &str| s.lines().skip(2).collect::<Vec<_>>().join("\n");
    let changed = body(&committed) != body(&text);
    let sections: usize = docs.iter().map(|d| d.sections.len()).sum();
    let old_entry = manifest.snapshots.iter().find(|e| e.id == SNAPSHOT_ID).cloned();
    let retrieved = match &old_entry {
        Some(o) if !changed && o.source_sha256 == source_sha => o.retrieved.clone(),
        _ => today.clone(),
    };
    let chapters = docs.iter().filter(|d| d.kind == "guide_chapter").count();
    let mut caveats = vec![
        "Most guide chapters are dated 2016–2018 (a few 2022–2023); DPH guidance for some \
         diseases (for example respiratory viruses) has changed since. Check the date of each \
         passage and confirm current practice with DPH."
            .to_string(),
        "Text was extracted automatically from DOCX, RTF, and web pages: tables are flattened \
         to lines with ' | ' between cells, and images (including flowcharts) are not included."
            .into(),
    ];
    if !skipped.is_empty() {
        caveats.push(format!("Not included: {}.", skipped.join("; ")));
    }
    let final_text = if changed { text.clone() } else { committed.clone() };
    let entry = SnapshotEntry {
        id: SNAPSHOT_ID.into(),
        title: "DPH Guide to Surveillance, Reporting and Control (disease chapters) and DPH \
                infectious disease fact sheets (text corpus)"
            .into(),
        publisher: "Massachusetts Department of Public Health".into(),
        source_urls: urls,
        source_sha256: source_sha.clone(),
        source_vintage: format!(
            "{chapters} guide chapters (2016–2023, dates in the corpus) and {} DPH web pages",
            docs.len() - chapters
        ),
        retrieved,
        license: "Massachusetts state government publications (public records); quoted with \
                  citation."
            .into(),
        derivation: "Downloaded from mass.gov; DOCX and RTF chapters and HTML page bodies \
                     converted to plain text and split into sections (chapter sections and \
                     lettered subsections; page headings). Built by `cargo xtask \
                     build-guide-corpus`."
            .into(),
        caveats,
        files: vec![SnapshotFile {
            path: FILE.into(),
            sha256: sha256(final_text.as_bytes()),
            bytes: final_text.len() as u64,
            rows: sections as u64,
        }],
    };
    let entry_changed = old_entry.as_ref() != Some(&entry);
    if check {
        if changed || entry_changed {
            bail!(
                "the guide corpus is out of date ({} documents changed or manifest differs); run \
                 `cargo xtask build-guide-corpus` and review the diff",
                if changed { "text" } else { "no text, but" }
            );
        }
        println!("Guide corpus is up to date ({} documents).", docs.len());
        return Ok(());
    }
    if changed {
        std::fs::write(snap_dir.join(FILE), &text)?;
    }
    match manifest.snapshots.iter_mut().find(|e| e.id == SNAPSHOT_ID) {
        Some(e) => *e = entry,
        None => manifest.snapshots.push(entry),
    }
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!(
        "Guide corpus: {} documents ({chapters} chapters), {sections} sections, {} bytes{}.",
        docs.len(),
        final_text.len(),
        if changed { " (updated)" } else { " (unchanged)" }
    );
    for s in &skipped {
        println!("skipped: {s}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_follow_the_guide_layout() {
        let lines: Vec<String> = [
            "Massachusetts Department of Public Health",
            "Section 1:",
            "ABOUT THE DISEASE",
            "A. Etiologic Agent",
            "Salmonella bacteria.",
            "E. Incubation Period",
            "The incubation period is 6–72 hours.",
            "Section 2",
            "REPORTING CRITERIA AND LABORATORY TESTING",
            "A. What to Report to the Massachusetts Department of Public Health (MDPH)",
            "Report isolation of Salmonella.",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let s = sectionize_chapter(&lines);
        let heads: Vec<&str> = s.iter().map(|x| x.0.as_str()).collect();
        assert_eq!(
            heads,
            vec![
                "Introduction",
                "Section 1: About the Disease › A. Etiologic Agent",
                "Section 1: About the Disease › E. Incubation Period",
                "Section 2: Reporting Criteria and Laboratory Testing › A. What to Report to the Massachusetts Department of Public Health (MDPH)",
            ]
        );
    }

    #[test]
    fn page_text_uses_headings() {
        let html = r#"<html><body><main><h1 class="ma__page-header__title"><span>Norovirus</span></h1>
            <div class="ma__information-details__content"><h2>What is norovirus?</h2><p>A very
            contagious virus.</p><ul><li>Wash hands.</li></ul><table><tr><td>A</td><td>B</td></tr></table>
            <h2>Help Us Improve Mass.gov</h2><p>Feedback</p></div></main></body></html>"#;
        let (t, s) = page_sections(html).unwrap();
        assert_eq!(t, "Norovirus");
        assert_eq!(s[0].0, "What is norovirus?");
        assert_eq!(s[0].1, vec!["A very contagious virus.", "- Wash hands.", "A | B"]);
        assert_eq!(s.len(), 1);
    }
}
