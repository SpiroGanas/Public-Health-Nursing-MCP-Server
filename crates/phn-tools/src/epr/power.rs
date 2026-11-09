//! EPR-04 `get_electricity_dependent_population`: HHS emPOWER counts of
//! Medicare beneficiaries who rely on electricity-dependent medical equipment,
//! by county and ZIP code, for power outage and evacuation planning.

use std::collections::BTreeSet;

use phn_core::{Envelope, PhnError, Privacy, Result, Source, SourceMode};
use phn_data::census::GeoLevel;
use phn_data::empower::{self, EmpowerLevel, EmpowerRow};
use phn_data::geography::MaGeographies;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::area::{self, Area, AreaLevel};
use super::content::{self, CitedOut, Power};
use crate::context::Context;
use crate::md;

/// Arguments for `get_electricity_dependent_population`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, JsonSchema)]
#[serde(default)]
pub struct ElectricityArgs {
    /// A Massachusetts city or town, a county, or "Massachusetts". Default: your saved town;
    /// otherwise the whole state.
    pub area: Option<String>,
    /// Specific 5-digit ZIP codes (instead of an area).
    pub zip_codes: Vec<String>,
    /// Optional: an aggregate count file in /data (for example a ZIP-level export of the
    /// emPOWER Emergency Planning Dataset from DPH) with a ZIP column and count columns.
    /// Individual-level files are refused.
    pub local_path: Option<String>,
}

/// One county or ZIP code.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct EmpowerOut {
    /// County FIPS or ZIP code.
    pub id: String,
    pub name: String,
    pub county: Option<String>,
    pub medicare_beneficiaries: u64,
    pub electricity_dependent: u64,
    pub per_1000_beneficiaries: Option<f64>,
    /// True when HHS shows 11 (the true count is 1-11).
    pub suppressed: bool,
    /// Share of the ZIP code's land area in the town (towns only).
    pub town_share: Option<f64>,
}

/// Totals from a local aggregate file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LocalTotals {
    pub file: String,
    pub zip_codes_matched: usize,
    /// Column → total over the matched ZIP codes ("<11" when 1-10).
    pub totals: Vec<(String, String)>,
}

/// Result of `get_electricity_dependent_population`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ElectricityResult {
    pub area: Area,
    /// "live", "cache", or "snapshot".
    pub mode: String,
    pub as_of: String,
    pub electricity_dependent: f64,
    pub medicare_beneficiaries: f64,
    pub per_1000_beneficiaries: Option<f64>,
    pub massachusetts_per_1000: Option<f64>,
    pub us_per_1000: Option<f64>,
    /// How the area total was formed.
    pub method: String,
    pub counties: Vec<EmpowerOut>,
    pub zip_codes: Vec<EmpowerOut>,
    pub local: Option<LocalTotals>,
    pub definition: String,
    pub equipment: Vec<String>,
    pub actions: Vec<CitedOut>,
    pub datasets: Vec<CitedOut>,
    pub utilities: Vec<String>,
}

fn per_1000(dme: f64, benes: f64) -> Option<f64> {
    (benes > 0.0).then(|| (dme / benes * 10_000.0).round() / 10.0)
}

fn out(r: &EmpowerRow, share: Option<f64>) -> EmpowerOut {
    EmpowerOut {
        id: r.geoid.clone(),
        name: r.name.clone(),
        county: r.county.clone(),
        medicare_beneficiaries: r.medicare_beneficiaries,
        electricity_dependent: r.electricity_dependent,
        per_1000_beneficiaries: per_1000(
            r.electricity_dependent as f64,
            r.medicare_beneficiaries as f64,
        ),
        suppressed: r.dme_suppressed(),
        town_share: share,
    }
}

/// emPOWER counts for an area.
pub async fn get_electricity_dependent_population(
    ctx: &Context,
    args: ElectricityArgs,
) -> Result<Envelope<ElectricityResult>> {
    let c: Power = content::load(content::POWER)?;
    let zips_in: Vec<String> = args
        .zip_codes
        .iter()
        .map(|z| z.trim().to_string())
        .collect();
    if zips_in
        .iter()
        .any(|z| z.len() != 5 || !z.chars().all(|c| c.is_ascii_digit()))
    {
        return Err(PhnError::InvalidInput("ZIP codes must be 5 digits".into()));
    }
    if zips_in.len() > 100 {
        return Err(PhnError::InvalidInput("give at most 100 ZIP codes".into()));
    }
    let area = if zips_in.is_empty() {
        area::resolve(ctx, args.area.as_deref(), true)?
    } else {
        Area {
            level: AreaLevel::State,
            name: format!("ZIP codes {}", zips_in.join(", ")),
            geoid: "25".into(),
            county_fips: None,
            county: None,
            town: None,
        }
    };
    let mut caveats = Vec::new();
    let counties_f = empower::fetch_counties(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await?;
    let zips_f = empower::fetch_zips(&ctx.http, ctx.cache.as_ref(), &ctx.snapshots).await?;
    let mode = if counties_f.mode == SourceMode::Snapshot || zips_f.mode == SourceMode::Snapshot {
        SourceMode::Snapshot
    } else if counties_f.mode == SourceMode::Cache || zips_f.mode == SourceMode::Cache {
        SourceMode::Cache
    } else {
        SourceMode::Live
    };
    if mode == SourceMode::Snapshot {
        caveats.push(format!(
            "emPOWER could not be reached live, so the bundled snapshot (retrieved {}) was used; \
             HHS updates the data monthly.",
            zips_f.as_of
        ));
    }
    let geos = MaGeographies::shared(&ctx.snapshots)?;
    let all_counties: Vec<&EmpowerRow> = counties_f
        .rows
        .iter()
        .filter(|r| r.level == EmpowerLevel::County)
        .collect();
    let (ma_dme, ma_benes) = all_counties.iter().fold((0.0, 0.0), |a, r| {
        (
            a.0 + r.electricity_dependent as f64,
            a.1 + r.medicare_beneficiaries as f64,
        )
    });
    let us = empower::snapshot_totals(&ctx.snapshots)
        .ok()
        .and_then(|(_, us)| us);

    let mut counties = Vec::new();
    let mut zips = Vec::new();
    let (dme, benes, method) = if !zips_in.is_empty() {
        for z in &zips_in {
            match zips_f.rows.iter().find(|r| &r.geoid == z) {
                Some(r) => zips.push(out(r, None)),
                None => caveats.push(format!(
                    "ZIP code {z} is not in the Massachusetts emPOWER data."
                )),
            }
        }
        let t = zips.iter().fold((0.0, 0.0), |a, z| {
            (
                a.0 + z.electricity_dependent as f64,
                a.1 + z.medicare_beneficiaries as f64,
            )
        });
        (t.0, t.1, "sum of the ZIP codes".to_string())
    } else {
        match area.level {
            AreaLevel::State => {
                counties = all_counties.iter().map(|r| out(r, None)).collect();
                counties.sort_by_key(|c| std::cmp::Reverse(c.electricity_dependent));
                let mut z: Vec<&EmpowerRow> = zips_f.rows.iter().collect();
                z.sort_by_key(|r| std::cmp::Reverse(r.electricity_dependent));
                zips = z.into_iter().take(20).map(|r| out(r, None)).collect();
                (
                    ma_dme,
                    ma_benes,
                    "sum of the 14 counties; the 20 ZIP codes with the most are listed".to_string(),
                )
            }
            AreaLevel::County => {
                let cty = area.county.clone().unwrap_or_default();
                if let Some(r) = all_counties
                    .iter()
                    .find(|r| Some(r.geoid.as_str()) == area.county_fips.as_deref())
                {
                    counties.push(out(r, None));
                }
                zips = zips_f
                    .rows
                    .iter()
                    .filter(|r| r.county.as_deref() == Some(cty.as_str()))
                    .map(|r| out(r, None))
                    .collect();
                zips.sort_by_key(|z| std::cmp::Reverse(z.electricity_dependent));
                let c0 = counties.first();
                (
                    c0.map(|c| c.electricity_dependent as f64).unwrap_or(0.0),
                    c0.map(|c| c.medicare_beneficiaries as f64).unwrap_or(0.0),
                    "county total; ZIP codes are those HHS assigns to the county".to_string(),
                )
            }
            AreaLevel::Town => {
                let town = area.town.clone().unwrap_or_default();
                let shares: Vec<(String, f64)> = geos
                    .at_level(GeoLevel::Zcta)
                    .filter_map(|z| {
                        z.town_shares
                            .iter()
                            .find(|(t, _)| *t == town)
                            .map(|(_, s)| (z.geoid.clone(), *s))
                    })
                    .collect();
                for (z, s) in &shares {
                    if let Some(r) = zips_f.rows.iter().find(|r| &r.geoid == z) {
                        zips.push(out(r, Some((s * 100.0).round() / 100.0)));
                    }
                }
                zips.sort_by(|a, b| {
                    b.town_share
                        .partial_cmp(&a.town_share)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                let t = zips.iter().fold((0.0, 0.0), |a, z| {
                    let s = z.town_share.unwrap_or(0.0);
                    (
                        a.0 + z.electricity_dependent as f64 * s,
                        a.1 + z.medicare_beneficiaries as f64 * s,
                    )
                });
                if let Some(r) = all_counties
                    .iter()
                    .find(|r| Some(r.geoid.as_str()) == area.county_fips.as_deref())
                {
                    counties.push(out(r, None));
                }
                caveats.push(
                    "Town totals allocate each ZIP code's count by the share of the ZIP Code \
                     Tabulation Area's land in the town; ZIP codes and towns do not line up, so \
                     use the ZIP code list for outreach."
                        .into(),
                );
                (
                    t.0.round(),
                    t.1.round(),
                    "ZIP codes allocated to the town by land-area share".to_string(),
                )
            }
        }
    };
    if zips.iter().any(|z| z.suppressed) {
        caveats.push(c.suppression.text.clone());
    }

    // Local aggregate file.
    let threshold = ctx
        .suppression_threshold()
        .max(empower::SUPPRESSED_VALUE as u32);
    let mut local = None;
    let mut identifiers = Vec::new();
    let mut local_src = None;
    if let Some(p) = &args.local_path {
        let lt = crate::localdata::load(ctx, p, None)?;
        identifiers = lt.identifier_categories();
        if lt.findings.iter().any(|f| f.kind.is_direct()) {
            return Err(PhnError::Privacy(format!(
                "'{}' appears to contain individual-level identifiers ({}). This tool reads only \
                 ZIP- or county-level counts. Never load the emPOWER Emergency Response Outreach \
                 (individual) dataset here; use it only in your agency's secure systems.",
                lt.label,
                lt.identifier_categories().join(", ")
            )));
        }
        let norm = |s: &str| phn_core::text::normalize(s);
        let zip_col = lt
            .table
            .headers
            .iter()
            .position(|h| {
                ["zip", "zip code", "zipcode", "zcta", "postal code"].contains(&norm(h).as_str())
            })
            .ok_or_else(|| {
                PhnError::InvalidInput(format!("'{}' needs a ZIP code column", lt.label))
            })?;
        let wanted: BTreeSet<String> = if !zips_in.is_empty() || area.level != AreaLevel::State {
            zips.iter().map(|z| z.id.clone()).collect()
        } else {
            zips_f.rows.iter().map(|r| r.geoid.clone()).collect()
        };
        let rows: Vec<&Vec<String>> = lt
            .table
            .rows
            .iter()
            .filter(|r| {
                r.get(zip_col)
                    .is_some_and(|z| wanted.contains(&format!("{:0>5}", z.trim())))
            })
            .collect();
        let mut totals = Vec::new();
        for (i, h) in lt.table.headers.iter().enumerate() {
            if i == zip_col {
                continue;
            }
            let (_, ok) = lt.numeric(i);
            if ok < 0.9 {
                continue;
            }
            let sum: f64 = rows
                .iter()
                .filter_map(|r| r.get(i))
                .filter_map(|v| v.trim().replace(',', "").parse::<f64>().ok())
                .sum();
            let n = sum.round() as u64;
            totals.push((
                h.clone(),
                if n > 0 && n < threshold as u64 {
                    format!("<{threshold}")
                } else {
                    n.to_string()
                },
            ));
        }
        local = Some(LocalTotals {
            file: lt.label.clone(),
            zip_codes_matched: rows.len(),
            totals,
        });
        local_src = Some(lt.source());
    }

    // Summary.
    let rate = per_1000(dme, benes);
    let ma_rate = per_1000(ma_dme, ma_benes);
    let us_rate = us.as_ref().and_then(|u| {
        per_1000(
            u.electricity_dependent as f64,
            u.medicare_beneficiaries as f64,
        )
    });
    let mut s = format!(
        "**Electricity-dependent Medicare beneficiaries — {}** (HHS emPOWER, {})\n\nAbout **{}** of {} Medicare beneficiaries rely on electricity-dependent medical equipment ({} per 1,000; Massachusetts {}, U.S. {}). Method: {method}.\n\n",
        area.name,
        mode.label(),
        md::num(dme, 0),
        md::num(benes, 0),
        rate.map(|r| r.to_string()).unwrap_or_else(|| "—".into()),
        ma_rate.map(|r| r.to_string()).unwrap_or_else(|| "—".into()),
        us_rate.map(|r| r.to_string()).unwrap_or_else(|| "—".into()),
    );
    if area.level == AreaLevel::State && zips_in.is_empty() {
        let rows: Vec<Vec<String>> = counties
            .iter()
            .map(|c| {
                vec![
                    c.name.clone(),
                    md::num(c.electricity_dependent as f64, 0),
                    md::num(c.medicare_beneficiaries as f64, 0),
                    c.per_1000_beneficiaries
                        .map(|v| v.to_string())
                        .unwrap_or_default(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "County",
                "Electricity-dependent",
                "Medicare beneficiaries",
                "Per 1,000",
            ],
            &rows,
        ));
        s.push('\n');
    }
    if !zips.is_empty() {
        let rows: Vec<Vec<String>> = zips
            .iter()
            .take(40)
            .map(|z| {
                vec![
                    z.id.clone(),
                    z.county.clone().unwrap_or_default(),
                    if z.suppressed {
                        "1–11".into()
                    } else {
                        md::num(z.electricity_dependent as f64, 0)
                    },
                    md::num(z.medicare_beneficiaries as f64, 0),
                    z.town_share
                        .map(|s| format!("{:.0}%", s * 100.0))
                        .unwrap_or_default(),
                ]
            })
            .collect();
        s.push_str(&md::table(
            &[
                "ZIP code",
                "County",
                "Electricity-dependent",
                "Medicare beneficiaries",
                "Share of ZIP in town",
            ],
            &rows,
        ));
        if zips.len() > 40 {
            s.push_str(&format!(
                "…and {} more ZIP codes (see data).\n",
                zips.len() - 40
            ));
        }
    }
    if let Some(l) = &local {
        s.push_str(&format!(
            "\nFrom your file `{}` ({} ZIP codes matched): {}.\n",
            l.file,
            l.zip_codes_matched,
            l.totals
                .iter()
                .map(|(k, v)| format!("{k} {v}"))
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    let actions = content::cite_all(&c.actions, &c.sources);
    s.push_str("\n**Planning actions**\n");
    for a in &actions {
        s.push_str(&content::bullet(a));
    }
    s.push_str(&format!(
        "\nUtilities (report outages to the utility, not 9-1-1): {}.\n",
        c.utilities.items.join("; ")
    ));

    caveats.push(c.coverage.text.clone());
    caveats.push(
        "Counts are for planning; they do not identify people. To reach individuals in an \
         emergency, DPH can request the restricted emPOWER outreach dataset; that file must stay \
         in secure agency systems."
            .into(),
    );
    caveats.extend(content::review_caveats(&[content::POWER]));
    let mut sources = vec![Source {
        title: "HHS emPOWER Map data: Massachusetts counties and ZIP codes".into(),
        publisher: "HHS Administration for Strategic Preparedness and Response (ASPR) and CMS"
            .into(),
        url: Some(empower::MAP_URL.into()),
        effective_date: None,
        retrieved: Some(zips_f.as_of.chars().take(10).collect()),
        mode,
    }];
    if mode == SourceMode::Snapshot
        && let Some((e, _)) = ctx.snapshots.entry(empower::SNAPSHOT_ID)
    {
        sources.push(e.to_source());
    }
    if area.level == AreaLevel::Town {
        sources.push(geos.source.clone());
    }
    if let Some(l) = local_src {
        sources.push(l);
    }
    sources.extend(content::sources_for(
        c.sources.keys().map(String::as_str),
        &c.sources,
    ));

    Ok(Envelope::new(
        s,
        ElectricityResult {
            area,
            mode: mode.label().into(),
            as_of: zips_f.as_of.clone(),
            electricity_dependent: dme,
            medicare_beneficiaries: benes,
            per_1000_beneficiaries: rate,
            massachusetts_per_1000: ma_rate,
            us_per_1000: us_rate,
            method,
            counties,
            zip_codes: zips,
            local,
            definition: c.definition.text.clone(),
            equipment: c.equipment.items.clone(),
            actions,
            datasets: content::cite_all(&c.datasets, &c.sources),
            utilities: c.utilities.items.clone(),
        },
    )
    .sources(sources)
    .caveats(caveats)
    .privacy(Privacy {
        row_level_data: false,
        suppression_applied: args.local_path.is_some(),
        suppression_threshold: args.local_path.as_ref().map(|_| threshold),
        identifiers_detected: identifiers,
        notes: vec![
            "emPOWER publishes de-identified counts; values of 1-11 are shown by HHS as 11.".into(),
        ],
        ..Privacy::none()
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("data")).unwrap();
        let c = Context::new(phn_core::Config::for_root(dir.path())).unwrap();
        (dir, c)
    }

    #[tokio::test]
    async fn county_town_state_and_zip_offline() {
        let (dir, c) = ctx();
        let run = |a: ElectricityArgs| get_electricity_dependent_population(&c, a);
        let e = run(ElectricityArgs {
            area: Some("Hampshire County".into()),
            ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(e.data.electricity_dependent, 1179.0);
        assert_eq!(e.data.mode, "snapshot");
        assert!(e.data.zip_codes.iter().any(|z| z.id == "01002"));
        assert!(
            e.data
                .zip_codes
                .iter()
                .all(|z| z.county.as_deref() == Some("Hampshire"))
        );
        let e = run(ElectricityArgs {
            area: Some("Amherst".into()),
            ..Default::default()
        })
        .await
        .unwrap();
        assert!(e.data.electricity_dependent > 0.0 && e.data.electricity_dependent < 1179.0);
        assert!(e.data.zip_codes.iter().all(|z| z.town_share.is_some()));
        let e = run(ElectricityArgs::default()).await.unwrap();
        assert_eq!(e.data.counties.len(), 14);
        assert!(e.data.us_per_1000.is_some());
        let e = run(ElectricityArgs {
            zip_codes: vec!["01002".into(), "01003".into()],
            ..Default::default()
        })
        .await
        .unwrap();
        assert_eq!(e.data.zip_codes.len(), 2);
        assert!(
            run(ElectricityArgs {
                zip_codes: vec!["1002".into()],
                ..Default::default()
            })
            .await
            .is_err()
        );

        // Aggregate local file: summed and suppressed.
        std::fs::write(
            dir.path().join("data/epd.csv"),
            "ZIP,Oxygen,Dialysis\n01002,40,3\n01007,12,4\n",
        )
        .unwrap();
        let e = run(ElectricityArgs {
            area: Some("Hampshire County".into()),
            local_path: Some("epd.csv".into()),
            ..Default::default()
        })
        .await
        .unwrap();
        let l = e.data.local.unwrap();
        assert_eq!(l.zip_codes_matched, 2);
        assert_eq!(
            l.totals,
            vec![
                ("Oxygen".to_string(), "52".to_string()),
                ("Dialysis".to_string(), "<11".to_string())
            ]
        );
        // Individual-level files are refused.
        std::fs::write(
            dir.path().join("data/people.csv"),
            "Name,Address,ZIP,Phone\nPat Example,1 Main St,01002,555-0100\n",
        )
        .unwrap();
        assert!(
            run(ElectricityArgs {
                local_path: Some("people.csv".into()),
                ..Default::default()
            })
            .await
            .is_err()
        );
    }
}
