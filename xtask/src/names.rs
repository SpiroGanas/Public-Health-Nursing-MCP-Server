//! `cargo xtask build-name-lists`: build the public name lists used by the
//! PHI detector in `phn-local`.
//!
//! * Surnames: U.S. Census Bureau 2000 surname file (public domain), via the
//!   FiveThirtyEight data repository mirror. The most common 10,000 are kept.
//! * First names: Social Security Administration baby names, 1880–2008, top
//!   1,000 per year and sex (public domain), via Hadley Wickham's
//!   `data-baby-names` mirror. Every distinct name is kept.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result};

use crate::download::fetch;

const SURNAMES_URL: &str =
    "https://raw.githubusercontent.com/fivethirtyeight/data/master/most-common-name/surnames.csv";
const FIRST_NAMES_URL: &str =
    "https://raw.githubusercontent.com/hadley/data-baby-names/master/baby-names.csv";
const SURNAME_LIMIT: usize = 10_000;

pub fn run(root: &Path) -> Result<()> {
    let out_dir = root.join("crates/phn-local/data");
    std::fs::create_dir_all(&out_dir)?;

    let surnames_csv = fetch(SURNAMES_URL, None)?;
    let mut rdr = csv::Reader::from_reader(surnames_csv.as_slice());
    let mut ranked: Vec<(u32, String)> = Vec::new();
    for rec in rdr.records() {
        let rec = rec?;
        let name = rec.get(0).context("surname column")?.trim().to_lowercase();
        let rank: u32 = rec
            .get(1)
            .context("rank column")?
            .trim()
            .parse()
            .unwrap_or(u32::MAX);
        if name.chars().all(|c| c.is_ascii_alphabetic()) && name.len() >= 2 {
            ranked.push((rank, name));
        }
    }
    ranked.sort();
    let surnames: BTreeSet<String> = ranked
        .into_iter()
        .take(SURNAME_LIMIT)
        .map(|(_, n)| n)
        .collect();
    write_list(
        &out_dir.join("surnames.txt"),
        &surnames,
        &[
            "Most common 10,000 surnames, U.S. Census Bureau 2000 surname file (public domain).",
            &format!("Retrieved via {SURNAMES_URL}"),
            "Rebuild with: cargo xtask build-name-lists",
        ],
    )?;

    let first_csv = fetch(FIRST_NAMES_URL, None)?;
    let mut rdr = csv::Reader::from_reader(first_csv.as_slice());
    let mut first: BTreeSet<String> = BTreeSet::new();
    for rec in rdr.records() {
        let rec = rec?;
        let name = rec.get(1).context("name column")?.trim().to_lowercase();
        if name.chars().all(|c| c.is_ascii_alphabetic()) && name.len() >= 2 {
            first.insert(name);
        }
    }
    write_list(
        &out_dir.join("first_names.txt"),
        &first,
        &[
            "Distinct first names from Social Security Administration baby-name data, 1880-2008,",
            "top 1,000 names per year and sex (public domain).",
            &format!("Retrieved via {FIRST_NAMES_URL}"),
            "Rebuild with: cargo xtask build-name-lists",
        ],
    )?;
    println!(
        "wrote {} surnames and {} first names to {}",
        surnames.len(),
        first.len(),
        out_dir.display()
    );
    Ok(())
}

fn write_list(path: &Path, names: &BTreeSet<String>, header: &[&str]) -> Result<()> {
    let mut out = String::new();
    for h in header {
        out.push_str("# ");
        out.push_str(h);
        out.push('\n');
    }
    for n in names {
        out.push_str(n);
        out.push('\n');
    }
    std::fs::write(path, out)?;
    Ok(())
}
