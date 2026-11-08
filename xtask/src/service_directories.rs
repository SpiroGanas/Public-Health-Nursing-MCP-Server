//! `cargo xtask build-service-directories [--check]`: rebuild the Phase 10
//! town-to-service directory snapshots from mass.gov and update their entries
//! in `snapshots/manifest.json` (entries built elsewhere are kept).
//!
//! * `ma-aging-service-directory`:
//!   * `ma_asap_directory.csv`: the Aging Services Access Points (ASAPs) and
//!     the cities and towns each serves, from the Executive Office of Aging &
//!     Independence page "Find Your Regional Aging Services Access Point".
//!   * `ma_shine_directory.csv`: the SHINE (State Health Insurance Assistance
//!     Program) regional office and phone for each town, from AGE's
//!     "towns by SHINE regional office" CSV.
//! * `ma-cbhc-directory` (`ma_cbhc_directory.csv`): the Community Behavioral
//!   Health Centers, their addresses, phones, coordinates, and the towns each
//!   lists as "Serving", from mass.gov's "Find a CBHC Locations" listing.
//!
//! Published town names are matched to the 351 MassGIS municipality names.
//! Villages and neighborhoods (Hyannis, Turners Falls, Allston) are mapped to
//! their town and kept in a separate column; spelling variants (Tyngsboro,
//! Foxboro) are mapped through a fixed table. Any other name that does not
//! match stops the build, so an upstream change is noticed and reviewed.
//! `--check` builds everything and reports differences without writing.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use anyhow::{Context, Result, bail};
use scraper::{ElementRef, Html, Selector};

use crate::download::{fetch, sha256};
use crate::snapshots::{Manifest, SnapshotEntry, SnapshotFile};

const ASAP_URL: &str =
    "https://www.mass.gov/info-details/find-your-regional-aging-services-access-point-asap";
const SHINE_PAGE: &str = "https://www.mass.gov/info-details/find-a-shine-counselor-near-you";
const CBHC_URL: &str = "https://www.mass.gov/find-a-cbhc/locations";
const NUTRITION_URL: &str = "https://www.mass.gov/info-details/nutrition-program-sites";
const LICENSE: &str = "Public information published by the Commonwealth of Massachusetts on mass.gov.";

/// Published spellings that differ from the MassGIS town name.
const SPELLINGS: &[(&str, &str)] = &[
    ("attleborough", "Attleboro"),
    ("merrimack", "Merrimac"),
    ("tyngsboro", "Tyngsborough"),
    ("foxboro", "Foxborough"),
    ("lanesboro", "Lanesborough"),
    ("boxboro", "Boxborough"),
    ("marlboro", "Marlborough"),
    ("middleboro", "Middleborough"),
    ("northboro", "Northborough"),
    ("southboro", "Southborough"),
    ("westboro", "Westborough"),
    ("egrement", "Egremont"),
    ("new marlboro", "New Marlborough"),
    ("gay head", "Aquinnah"),
    ("lunenberg", "Lunenburg"),
    ("north attleboro", "North Attleborough"),
    ("manchester by the sea", "Manchester-by-the-Sea"),
    ("manchester", "Manchester-by-the-Sea"),
];

/// Villages and neighborhoods: (published name, towns it is part of).
const VILLAGES: &[(&str, &[&str])] = &[
    ("allston", &["Boston"]),
    ("brighton", &["Boston"]),
    ("charlestown", &["Boston"]),
    ("back bay", &["Boston"]),
    ("beacon hill", &["Boston"]),
    ("chinatown", &["Boston"]),
    ("dorchester", &["Boston"]),
    ("east boston", &["Boston"]),
    ("fenway kenmore", &["Boston"]),
    ("hyde park", &["Boston"]),
    ("jamaica plain", &["Boston"]),
    ("mattapan", &["Boston"]),
    ("north end", &["Boston"]),
    ("roslindale", &["Boston"]),
    ("roxbury", &["Boston"]),
    ("south boston", &["Boston"]),
    ("west end", &["Boston"]),
    ("west roxbury", &["Boston"]),
    ("buzzards bay", &["Bourne"]),
    ("hyannis", &["Barnstable"]),
    ("vineyard haven", &["Tisbury"]),
    ("southwest boston", &["Boston"]),
    ("chestnut hill", &["Newton", "Brookline", "Boston"]),
    ("shelburne falls", &["Shelburne", "Buckland"]),
    ("turner falls", &["Montague"]),
    ("turners falls", &["Montague"]),
    ("millers falls", &["Montague", "Erving"]),
    ("florence", &["Northampton"]),
    ("indian orchard", &["Springfield"]),
    ("bondsville", &["Palmer", "Belchertown"]),
    ("thorndike", &["Palmer"]),
    ("three rivers", &["Palmer"]),
    ("whitinsville", &["Northbridge"]),
    (
        "marthas vineyard",
        &["Aquinnah", "Chilmark", "Edgartown", "Oak Bluffs", "Tisbury", "West Tisbury"],
    ),
];

fn norm(s: &str) -> String {
    phn_core::text::normalize(s)
}

/// Match published names to MassGIS towns.
struct Towns {
    by_norm: BTreeMap<String, String>,
    county: BTreeMap<String, String>,
}

impl Towns {
    fn load(root: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(root.join("snapshots/ma_municipalities.csv"))?;
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        let mut by_norm = BTreeMap::new();
        let mut county = BTreeMap::new();
        for r in rdr.records() {
            let r = r?;
            let name = r.get(1).unwrap_or_default().to_string();
            county.insert(name.clone(), r.get(3).unwrap_or_default().to_string());
            by_norm.insert(norm(&name), name);
        }
        if by_norm.len() != 351 {
            bail!("expected 351 municipalities, found {}", by_norm.len());
        }
        Ok(Self { by_norm, county })
    }

    /// (towns, villages-as-published) for one published name, or an error.
    fn resolve(&self, published: &str) -> Result<(Vec<String>, Option<String>)> {
        // "Allston (Boston)" -> try the part in parentheses as the town.
        let p = published.trim().trim_end_matches('.').trim();
        if let Some((village, rest)) = p.split_once('(') {
            let town = rest.trim_end_matches(')').trim();
            if let Some(t) = self.by_norm.get(&norm(town)) {
                return Ok((vec![t.clone()], Some(village.trim().to_string())));
            }
        }
        let n = norm(p);
        if let Some(t) = self.by_norm.get(&n) {
            return Ok((vec![t.clone()], None));
        }
        if let Some((_, t)) = SPELLINGS.iter().find(|(k, _)| *k == n) {
            return Ok((vec![(*t).to_string()], None));
        }
        if let Some((_, ts)) = VILLAGES.iter().find(|(k, _)| *k == n) {
            return Ok((
                ts.iter().map(|t| (*t).to_string()).collect(),
                Some(p.to_string()),
            ));
        }
        bail!("'{published}' does not match a Massachusetts town, a known spelling variant, or a known village; add it to SPELLINGS or VILLAGES after checking")
    }
}

/// Split "A, B, C, and D" into names.
fn split_list(s: &str) -> Vec<String> {
    s.replace(" and ", ", ")
        .replace(" & ", ", ")
        .replace('&', ",")
        .split(',')
        .map(|x| x.trim().trim_end_matches('.').trim().to_string())
        .filter(|x| !x.is_empty() && x != "and")
        .collect()
}

/// Text of an element with `<br>` and block boundaries as newlines.
fn text_lines(el: ElementRef) -> String {
    let mut out = String::new();
    for node in el.descendants() {
        match node.value() {
            scraper::Node::Text(t) => out.push_str(t),
            scraper::Node::Element(e) if e.name() == "br" || e.name() == "p" => out.push('\n'),
            _ => {}
        }
    }
    out.replace('\u{a0}', " ")
}

fn sel(s: &str) -> Selector {
    Selector::parse(s).expect("valid selector")
}

fn looks_like_phone(s: &str) -> bool {
    let digits = s.chars().filter(char::is_ascii_digit).count();
    digits >= 10 && !s.contains(", MA") && !s.contains(" MA ")
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn file_entry(path: &str, text: &str) -> SnapshotFile {
    SnapshotFile {
        path: path.to_string(),
        sha256: sha256(text.as_bytes()),
        bytes: text.len() as u64,
        rows: text.lines().count().saturating_sub(1) as u64,
    }
}

// ------------------------------------------------------------------ ASAPs

struct Asap {
    id: String,
    name: String,
    address: String,
    phone: String,
    toll_free: String,
    web: String,
    towns: Vec<String>,
    neighborhoods: Vec<String>,
    served: String,
}

fn parse_asaps(html: &str, towns: &Towns) -> Result<Vec<Asap>> {
    let doc = Html::parse_document(html);
    let h3s = sel("div.ma__rich-text h3");
    let mut out = Vec::new();
    for h in doc.select(&h3s) {
        let id = h.value().id().unwrap_or_default();
        let Some((num, _)) = id.split_once('-') else {
            continue;
        };
        if num.parse::<u32>().is_err() {
            continue;
        }
        let title = h.text().collect::<String>();
        let name = title
            .split_once(". ")
            .map(|(_, n)| n.trim().to_string())
            .unwrap_or_else(|| title.trim().to_string());
        let mut address = Vec::new();
        let mut phones = Vec::new();
        let mut toll_free = String::new();
        let mut web = String::new();
        let mut served = String::new();
        let mut sib = h.next_sibling();
        while let Some(node) = sib {
            sib = node.next_sibling();
            let Some(el) = ElementRef::wrap(node) else {
                continue;
            };
            if el.value().name() == "h3" {
                break;
            }
            if el.value().name() != "p" {
                continue;
            }
            let text = text_lines(el);
            let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
            if let Some(rest) = flat.strip_prefix("Areas served:") {
                served = rest.trim().to_string();
            } else if flat.starts_with("Web:") {
                if let Some(a) = el.select(&sel("a")).next() {
                    web = a.value().attr("href").unwrap_or_default().to_string();
                }
            } else {
                for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    if let Some(t) = line.strip_prefix("Toll Free:") {
                        toll_free = t.trim().to_string();
                    } else if looks_like_phone(line) {
                        phones.push(line.split_whitespace().collect::<Vec<_>>().join(" "));
                    } else {
                        address.push(line.to_string());
                    }
                }
            }
        }
        if served.is_empty() {
            bail!("ASAP '{name}' has no 'Areas served' line");
        }
        let mut t: BTreeSet<String> = BTreeSet::new();
        let mut hoods = Vec::new();
        if let Some(rest) = served.strip_prefix("Boston neighborhoods of") {
            t.insert("Boston".into());
            hoods = split_list(rest);
        } else {
            for n in split_list(&served) {
                let (ts, village) = towns.resolve(&n)?;
                t.extend(ts);
                if let Some(v) = village {
                    hoods.push(v);
                }
            }
        }
        out.push(Asap {
            id: format!("asap-{num}"),
            name,
            address: address.join(", "),
            phone: phones.first().cloned().unwrap_or_default(),
            toll_free,
            web,
            towns: t.into_iter().collect(),
            neighborhoods: hoods,
            served,
        });
    }
    if out.len() < 20 {
        bail!("only {} ASAPs parsed from {ASAP_URL}; the page layout may have changed", out.len());
    }
    let covered: BTreeSet<&String> = out.iter().flat_map(|a| a.towns.iter()).collect();
    let missing: Vec<&String> = towns
        .by_norm
        .values()
        .filter(|t| !covered.contains(t))
        .collect();
    if !missing.is_empty() {
        bail!("towns without an ASAP: {missing:?}");
    }
    Ok(out)
}

fn asap_csv(asaps: &[Asap]) -> String {
    let mut s = String::from(
        "id,name,address,phone,phone_toll_free,web,towns,neighborhoods,served_as_published\n",
    );
    for a in asaps {
        let row = [
            a.id.clone(),
            a.name.clone(),
            a.address.clone(),
            a.phone.clone(),
            a.toll_free.clone(),
            a.web.clone(),
            a.towns.join(";"),
            a.neighborhoods.join(";"),
            a.served.clone(),
        ];
        s.push_str(&row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        s.push('\n');
    }
    s
}

// ------------------------------------------------------------------ SHINE

fn shine_csv_url(page: &str) -> Result<String> {
    let doc = Html::parse_document(page);
    for a in doc.select(&sel("a")) {
        if let Some(h) = a.value().attr("href")
            && h.contains("shine")
            && h.to_ascii_lowercase().ends_with(".csv")
        {
            return Ok(if h.starts_with("http") {
                h.to_string()
            } else {
                format!("https://www.mass.gov{h}")
            });
        }
    }
    bail!("no SHINE CSV link found on {SHINE_PAGE}")
}

fn build_shine_csv(bytes: &[u8], towns: &Towns) -> Result<(String, Vec<String>)> {
    let text = String::from_utf8_lossy(bytes)
        .trim_start_matches('\u{feff}')
        .to_string();
    let mut rdr = csv::Reader::from_reader(text.as_bytes());
    let mut rows: BTreeMap<String, (String, String, Vec<String>)> = BTreeMap::new();
    let mut statewide = Vec::new();
    for r in rdr.records() {
        let r = r?;
        let place = r.get(0).unwrap_or_default().trim();
        let phone = r.get(1).unwrap_or_default().trim().to_string();
        let office = r.get(2).unwrap_or_default().trim().to_string();
        if place.is_empty() {
            continue;
        }
        let (ts, village) = towns.resolve(place)?;
        for t in ts {
            let e = rows
                .entry(t)
                .or_insert_with(|| (phone.clone(), office.clone(), Vec::new()));
            if let Some(v) = &village {
                e.2.push(v.clone());
            }
            if e.0 != phone {
                statewide.push(format!("{place}: {phone} ({office})"));
            }
        }
    }
    let missing: Vec<String> = towns
        .by_norm
        .values()
        .filter(|t| !rows.contains_key(*t))
        .cloned()
        .collect();
    let mut s = String::from("town,phone,office,places_as_published\n");
    for (t, (p, o, v)) in &rows {
        s.push_str(&format!(
            "{},{},{},{}\n",
            csv_field(t),
            csv_field(p),
            csv_field(o),
            csv_field(&v.join(";"))
        ));
    }
    if !statewide.is_empty() {
        eprintln!("note: SHINE rows with a different phone for the same town: {statewide:?}");
    }
    Ok((s, missing))
}

// ------------------------------------------------------------------ Nutrition

struct Nutrition {
    name: String,
    address: String,
    phone: String,
    web: String,
    towns: Vec<String>,
    served: String,
}

fn parse_nutrition(html: &str, towns: &Towns) -> Result<Vec<Nutrition>> {
    let doc = Html::parse_document(html);
    let mut out = Vec::new();
    for tr in doc.select(&sel("table.ma__table tbody tr")) {
        let tds: Vec<ElementRef> = tr.select(&sel("td")).collect();
        if tds.len() < 2 {
            continue;
        }
        let web = tds[0]
            .select(&sel("a"))
            .next()
            .and_then(|a| a.value().attr("href"))
            .unwrap_or_default()
            .to_string();
        let mut lines: Vec<String> = Vec::new();
        let mut cur = String::new();
        for node in tds[0].descendants() {
            match node.value() {
                scraper::Node::Text(t) => cur.push_str(t),
                scraper::Node::Element(e) if e.name() == "br" => {
                    lines.push(std::mem::take(&mut cur));
                }
                _ => {}
            }
        }
        lines.push(cur);
        let lines: Vec<String> = lines
            .into_iter()
            .map(|l| l.replace('\u{a0}', " ").split_whitespace().collect::<Vec<_>>().join(" "))
            .filter(|l| !l.is_empty())
            .collect();
        let Some(mut name) = lines.first().cloned() else {
            continue;
        };
        let mut rest = 1;
        // "Health & Social Services Consortium" / "Inc. (HESSCO)"
        if let Some(next) = lines.get(1)
            && next.starts_with("Inc.")
        {
            name = format!("{name}, {next}");
            rest = 2;
        }
        let mut address = Vec::new();
        let mut phone = String::new();
        for l in lines.iter().skip(rest) {
            let lower = l.to_ascii_lowercase();
            if lower.starts_with("fax") || lower.starts_with("tty") || lower.starts_with("tdd") {
                continue;
            }
            if looks_like_phone(l) || lower.starts_with("toll free") {
                if phone.is_empty() {
                    phone = l.clone();
                }
            } else {
                address.push(l.clone());
            }
        }
        let served = tds[1].text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
        let mut t = BTreeSet::new();
        for n in split_list(&served.replace(['(', ')'], ",")) {
            if n.to_ascii_lowercase().starts_with("boston neighborhoods") || n.is_empty() {
                continue;
            }
            let (ts, _) = towns.resolve(&n)?;
            t.extend(ts);
        }
        out.push(Nutrition {
            name,
            address: address.join(", "),
            phone,
            web,
            towns: t.into_iter().collect(),
            served,
        });
    }
    if out.len() < 15 {
        bail!("only {} nutrition projects parsed from {NUTRITION_URL}", out.len());
    }
    Ok(out)
}

fn nutrition_csv(items: &[Nutrition]) -> String {
    let mut s = String::from("id,name,address,phone,web,towns,served_as_published\n");
    for (i, n) in items.iter().enumerate() {
        let row = [
            format!("meals-{:02}", i + 1),
            n.name.clone(),
            n.address.clone(),
            n.phone.clone(),
            n.web.clone(),
            n.towns.join(";"),
            n.served.clone(),
        ];
        s.push_str(&row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        s.push('\n');
    }
    s
}

// ------------------------------------------------------------------ CBHCs

struct Cbhc {
    name: String,
    address: String,
    city: String,
    county: String,
    phone: String,
    lat: Option<f64>,
    lon: Option<f64>,
    towns: Vec<String>,
    villages: Vec<String>,
    served: String,
}

fn leaflet_markers(html: &str) -> BTreeMap<String, (f64, f64)> {
    let mut out = BTreeMap::new();
    let Some(start) = html.find("ma.leafletMapData.push(") else {
        return out;
    };
    let rest = &html[start + "ma.leafletMapData.push(".len()..];
    let Some(end) = rest.find(");\n").or_else(|| rest.find(");")) else {
        return out;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&rest[..end]) else {
        return out;
    };
    for m in v["markers"].as_array().into_iter().flatten() {
        let p = &m["position"];
        if let (Some(a), Some(lat), Some(lng)) =
            (p["alt"].as_str(), p["lat"].as_f64(), p["lng"].as_f64())
        {
            out.insert(norm(a), (lat, lng));
        }
    }
    out
}

fn parse_cbhc_page(html: &str, towns: &Towns) -> Result<Vec<Cbhc>> {
    let doc = Html::parse_document(html);
    let coords = leaflet_markers(html);
    let mut out = Vec::new();
    for item in doc.select(&sel("li.ma__image-promo")) {
        let Some(h) = item.select(&sel("h3")).next() else {
            continue;
        };
        let name = h.text().collect::<String>().trim().to_string();
        if name.is_empty() {
            continue;
        }
        let address = item
            .select(&sel(".ma__image-promo__location"))
            .next()
            .map(|e| {
                e.text()
                    .collect::<String>()
                    .replace("Directions", "")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .replace(" ,", ",")
            })
            .unwrap_or_default();
        let served = item
            .select(&sel(".ma__image-promo__description"))
            .next()
            .map(|e| e.text().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();
        let phone = item
            .select(&sel(".ma__image-promo__phone-link"))
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .unwrap_or_default();
        // City: the part before ", MA".
        let city = address
            .split(',')
            .rev()
            .nth(1)
            .map(|c| c.trim().to_string())
            .unwrap_or_default();
        let city_town = towns
            .resolve(&city)
            .map(|(t, _)| t.first().cloned().unwrap_or_default())
            .unwrap_or_default();
        let county = towns.county.get(&city_town).cloned().unwrap_or_default();
        let mut t = BTreeSet::new();
        let mut villages = Vec::new();
        let list = served.strip_prefix("Serving:").unwrap_or(&served).trim();
        if !list.is_empty() {
            for n in split_list(list) {
                let (ts, v) = towns.resolve(&n)?;
                t.extend(ts);
                if let Some(v) = v {
                    villages.push(v);
                }
            }
        }
        let (lat, lon) = coords
            .get(&norm(&name))
            .map(|(a, b)| (Some(*a), Some(*b)))
            .unwrap_or((None, None));
        out.push(Cbhc {
            name,
            address,
            city: city_town,
            county,
            phone,
            lat,
            lon,
            towns: t.into_iter().collect(),
            villages,
            served: list.to_string(),
        });
    }
    Ok(out)
}

fn cbhc_csv(items: &[Cbhc]) -> String {
    let mut s = String::from(
        "id,name,address,city,county,phone,lat,lon,towns,villages,served_as_published\n",
    );
    for (i, c) in items.iter().enumerate() {
        let row = [
            format!("cbhc-{:02}", i + 1),
            c.name.clone(),
            c.address.clone(),
            c.city.clone(),
            c.county.clone(),
            c.phone.clone(),
            c.lat.map(|v| format!("{v:.5}")).unwrap_or_default(),
            c.lon.map(|v| format!("{v:.5}")).unwrap_or_default(),
            c.towns.join(";"),
            c.villages.join(";"),
            c.served.clone(),
        ];
        s.push_str(&row.iter().map(|f| csv_field(f)).collect::<Vec<_>>().join(","));
        s.push('\n');
    }
    s
}

// ------------------------------------------------------------------ run

pub fn run(root: &Path, check: bool, source_dir: Option<&Path>) -> Result<()> {
    let snap_dir = root.join("snapshots");
    let manifest_path = snap_dir.join("manifest.json");
    let mut manifest: Manifest = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).context("reading snapshots/manifest.json")?,
    )?;
    let today = chrono::Local::now().date_naive().to_string();
    let towns = Towns::load(root)?;
    let mut hashes = BTreeMap::new();

    // ASAPs
    let asap_html = fetch(ASAP_URL, source_dir)?;
    hashes.insert("find-your-regional-aging-services-access-point-asap.html".to_string(), sha256(&asap_html));
    let asaps = parse_asaps(&String::from_utf8_lossy(&asap_html), &towns)?;
    let asap_text = asap_csv(&asaps);

    // SHINE
    let shine_page = fetch(SHINE_PAGE, source_dir)?;
    let shine_url = shine_csv_url(&String::from_utf8_lossy(&shine_page))?;
    let shine_bytes = fetch(&shine_url, source_dir)?;
    hashes.insert(
        shine_url.rsplit('/').next().unwrap_or("shine.csv").to_string(),
        sha256(&shine_bytes),
    );
    let (shine_text, shine_missing) = build_shine_csv(&shine_bytes, &towns)?;

    // Senior nutrition projects (home-delivered and community meals)
    let nut_html = fetch(NUTRITION_URL, source_dir)?;
    hashes.insert("nutrition-program-sites.html".to_string(), sha256(&nut_html));
    let nutrition = parse_nutrition(&String::from_utf8_lossy(&nut_html), &towns)?;
    let nutrition_text = nutrition_csv(&nutrition);

    // CBHCs (paged listing)
    let mut cbhcs: Vec<Cbhc> = Vec::new();
    let mut cbhc_hashes = BTreeMap::new();
    for page in 0..10 {
        let url = if page == 0 {
            CBHC_URL.to_string()
        } else {
            format!("{CBHC_URL}?page={page}")
        };
        let html = match source_dir {
            Some(dir) => {
                let p = dir.join(format!("cbhc_locations_{page}.html"));
                if !p.exists() {
                    break;
                }
                std::fs::read(p)?
            }
            None => match fetch(&url, None) {
                Ok(b) => b,
                // mass.gov answers 404 past the last page.
                Err(_) if page > 0 => break,
                Err(e) => return Err(e),
            },
        };
        let items = parse_cbhc_page(&String::from_utf8_lossy(&html), &towns)?;
        if items.is_empty() || items.iter().all(|i| cbhcs.iter().any(|c| c.name == i.name)) {
            break;
        }
        cbhc_hashes.insert(format!("cbhc_locations_{page}.html"), sha256(&html));
        cbhcs.extend(items);
    }
    if cbhcs.len() < 25 {
        bail!("only {} CBHC locations parsed from {CBHC_URL}", cbhcs.len());
    }
    let cbhc_text = cbhc_csv(&cbhcs);
    let listed: BTreeSet<&String> = cbhcs.iter().flat_map(|c| c.towns.iter()).collect();
    let not_listed: Vec<&String> = towns
        .by_norm
        .values()
        .filter(|t| !listed.contains(t))
        .collect();

    let aging_caveats = {
        let mut v = vec![
            "Service areas are as published on mass.gov; AGE notes that the areas served by an ASAP's protective services may differ. Confirm with the ASAP or call MassOptions at (800) 243-4636.".to_string(),
            "Boston is served by several ASAPs by neighborhood; the neighborhoods are listed for each.".to_string(),
        ];
        if !shine_missing.is_empty() {
            v.push(format!(
                "The SHINE town list has no row for: {}. Use MassOptions (800) 243-4636 to find a SHINE counselor there.",
                shine_missing.join(", ")
            ));
        }
        v
    };
    let aging_entry = SnapshotEntry {
        id: "ma-aging-service-directory".into(),
        title: "Massachusetts Aging Services Access Points, SHINE offices, and senior nutrition projects by town".into(),
        publisher: "Massachusetts Executive Office of Aging & Independence (mass.gov)".into(),
        source_urls: vec![
            ASAP_URL.into(),
            SHINE_PAGE.into(),
            shine_url.clone(),
            NUTRITION_URL.into(),
        ],
        source_sha256: hashes,
        source_vintage: today.clone(),
        retrieved: today.clone(),
        license: LICENSE.into(),
        derivation: "ASAP names, addresses, phones, web sites, and 'Areas served' parsed from the AGE page's 'A Complete List of ASAPs'; SHINE regional office and phone by town from AGE's CSV; senior nutrition projects and the communities they serve from the 'Nutrition Program Sites' table. Place names are matched to the 351 MassGIS municipalities; villages and Boston neighborhoods are mapped to their town (kept in their own column) and known spelling variants are corrected (xtask/src/service_directories.rs).".into(),
        caveats: aging_caveats,
        files: vec![
            file_entry("ma_asap_directory.csv", &asap_text),
            file_entry("ma_shine_directory.csv", &shine_text),
            file_entry("ma_nutrition_directory.csv", &nutrition_text),
        ],
    };
    let cbhc_entry = SnapshotEntry {
        id: "ma-cbhc-directory".into(),
        title: "Massachusetts Community Behavioral Health Centers (CBHCs) and the towns they list".into(),
        publisher: "Massachusetts Executive Office of Health and Human Services (mass.gov)".into(),
        source_urls: vec![CBHC_URL.into()],
        source_sha256: cbhc_hashes,
        source_vintage: today.clone(),
        retrieved: today.clone(),
        license: LICENSE.into(),
        derivation: "Every location in the paged 'Find a CBHC Locations' listing: name, address, phone, and the 'Serving' town list; coordinates from the page's map data. Place names are matched to MassGIS municipalities as for the ASAP directory.".into(),
        caveats: vec![
            format!(
                "{} of 351 towns are not named in any CBHC's 'Serving' list (some CBHC listings, for example in the Framingham, Waltham, Quincy, Hyannis, and Martha's Vineyard areas, publish no town list); for those towns the tool lists the CBHCs in the same county and the statewide numbers that route to the nearest CBHC.",
                not_listed.len()
            ),
            "mass.gov's listing is the source of record; CBHC catchment areas can change. Confirm with the Behavioral Health Help Line, (833) 773-2445.".into(),
        ],
        files: vec![file_entry("ma_cbhc_directory.csv", &cbhc_text)],
    };

    let outputs = [
        ("ma_asap_directory.csv", &asap_text),
        ("ma_shine_directory.csv", &shine_text),
        ("ma_nutrition_directory.csv", &nutrition_text),
        ("ma_cbhc_directory.csv", &cbhc_text),
    ];
    println!(
        "{} ASAPs, {} SHINE town rows, {} CBHC locations; {} towns not in any CBHC list",
        asaps.len(),
        shine_text.lines().count() - 1,
        cbhcs.len(),
        not_listed.len()
    );
    if check {
        let mut changed = false;
        for (path, text) in outputs {
            let old = std::fs::read_to_string(snap_dir.join(path)).unwrap_or_default();
            if old != *text {
                changed = true;
                println!("CHANGED snapshots/{path}");
            } else {
                println!("same    snapshots/{path}");
            }
        }
        if changed {
            bail!("service directories changed upstream; rebuild and review the diff");
        }
        return Ok(());
    }
    for (path, text) in outputs {
        std::fs::write(snap_dir.join(path), text)?;
        println!("wrote snapshots/{path}");
    }
    for e in [aging_entry, cbhc_entry] {
        match manifest.snapshots.iter_mut().find(|x| x.id == e.id) {
            Some(x) => *x = e,
            None => manifest.snapshots.push(e),
        }
    }
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest)? + "\n",
    )?;
    println!("Updated snapshots/manifest.json.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_lists_and_resolves_variants() {
        assert_eq!(
            split_list("Fall River, Freetown, Somerset, and Westport"),
            vec!["Fall River", "Freetown", "Somerset", "Westport"]
        );
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
        let t = Towns::load(&root).unwrap();
        assert_eq!(t.resolve("Tyngsboro").unwrap().0, vec!["Tyngsborough"]);
        assert_eq!(t.resolve("Allston (Boston)").unwrap().0, vec!["Boston"]);
        assert_eq!(t.resolve("Turners Falls").unwrap().0, vec!["Montague"]);
        assert_eq!(t.resolve("Manchester by the Sea").unwrap().0, vec!["Manchester-by-the-Sea"]);
        assert!(t.resolve("Atlantis").is_err());
    }
}
