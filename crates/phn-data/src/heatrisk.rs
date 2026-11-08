//! NWS HeatRisk (experimental), from the Weather Prediction Center
//! (www.wpc.ncep.noaa.gov/heatrisk).
//!
//! HeatRisk rates each day of the 7-day forecast from 0 (green, little to no
//! risk) to 4 (magenta, extreme), from how unusual the heat is for the time of
//! year, its duration (day and night), and CDC data on heat-related impacts.
//! WPC publishes each day as a KML ground overlay: a palette PNG whose palette
//! index is the HeatRisk category (index 5 = no data) on a latitude/longitude
//! box given in the KML. This module reads those files; dates come from
//! `data/FileTimes.js`. There is no snapshot: a forecast is useful only when
//! current.

use std::io::Cursor;

use phn_core::{PhnError, Result, SourceMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::apifetch::fetch_validated;
use crate::cache::Cache;
use crate::http::HttpClient;

pub const HOME_URL: &str = "https://www.wpc.ncep.noaa.gov/heatrisk/";
pub const DATA_BASE: &str = "https://www.wpc.ncep.noaa.gov/heatrisk/data";
/// Cache for an hour (WPC updates the forecast about twice a day).
pub const TTL_SECS: u64 = 3600;
/// Palette index meaning "no data".
pub const NO_DATA: u8 = 5;

/// A latitude/longitude box.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LatLonBox {
    pub north: f64,
    pub south: f64,
    pub east: f64,
    pub west: f64,
}

/// One day's HeatRisk grid.
#[derive(Debug, Clone, PartialEq)]
pub struct HeatRiskGrid {
    /// Forecast day (1 = today).
    pub day: u8,
    /// Valid date (YYYY-MM-DD) when known.
    pub date: Option<String>,
    pub bbox: LatLonBox,
    pub width: usize,
    pub height: usize,
    /// Row-major category per pixel (0–4, or [`NO_DATA`]).
    pub cells: Vec<u8>,
}

impl HeatRiskGrid {
    /// Category at a point, or None outside the grid or where there is no data.
    pub fn at(&self, lat: f64, lon: f64) -> Option<u8> {
        let b = &self.bbox;
        if !(b.south..=b.north).contains(&lat) || !(b.west..=b.east).contains(&lon) {
            return None;
        }
        let x = ((lon - b.west) / (b.east - b.west) * self.width as f64) as usize;
        let y = ((b.north - lat) / (b.north - b.south) * self.height as f64) as usize;
        let v = *self
            .cells
            .get(y.min(self.height - 1) * self.width + x.min(self.width - 1))?;
        (v < NO_DATA).then_some(v)
    }

    /// Centres of the cells inside a latitude/longitude box.
    pub fn cell_centres(&self, south: f64, north: f64, west: f64, east: f64) -> Vec<(f64, f64)> {
        let b = &self.bbox;
        let dx = (b.east - b.west) / self.width as f64;
        let dy = (b.north - b.south) / self.height as f64;
        let x0 = (((west - b.west) / dx).floor().max(0.0)) as usize;
        let x1 = (((east - b.west) / dx).ceil() as usize).min(self.width);
        let y0 = (((b.north - north) / dy).floor().max(0.0)) as usize;
        let y1 = (((b.north - south) / dy).ceil() as usize).min(self.height);
        let mut out = Vec::new();
        for y in y0..y1 {
            for x in x0..x1 {
                out.push((
                    b.north - (y as f64 + 0.5) * dy,
                    b.west + (x as f64 + 0.5) * dx,
                ));
            }
        }
        out
    }
}

/// Decode a HeatRisk palette PNG (palette indices are categories).
pub fn decode_png(bytes: &[u8]) -> Result<(usize, usize, Vec<u8>)> {
    let mut decoder = png::Decoder::new(Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::IDENTITY);
    let mut reader = decoder
        .read_info()
        .map_err(|e| PhnError::Network(format!("HeatRisk image could not be read ({e})")))?;
    let info = reader.info();
    if info.color_type != png::ColorType::Indexed || info.bit_depth != png::BitDepth::Eight {
        return Err(PhnError::Network(
            "HeatRisk image is not an 8-bit palette image; WPC may have changed the format".into(),
        ));
    }
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| PhnError::Network("HeatRisk image is too large".into()))?;
    let mut buf = vec![0u8; size];
    let out = reader
        .next_frame(&mut buf)
        .map_err(|e| PhnError::Network(format!("HeatRisk image could not be decoded ({e})")))?;
    let (w, h) = (out.width as usize, out.height as usize);
    let stride = out.line_size;
    let mut cells = Vec::with_capacity(w * h);
    for row in buf.chunks(stride).take(h) {
        cells.extend_from_slice(&row[..w]);
    }
    Ok((w, h, cells))
}

fn tag(text: &str, name: &str) -> Option<f64> {
    let open = format!("<{name}>");
    let start = text.find(&open)? + open.len();
    let end = text[start..].find(&format!("</{name}>"))? + start;
    text[start..end].trim().parse().ok()
}

/// The ground-overlay box from a HeatRisk KML file.
pub fn parse_kml_box(kml: &str) -> Result<LatLonBox> {
    match (
        tag(kml, "north"),
        tag(kml, "south"),
        tag(kml, "east"),
        tag(kml, "west"),
    ) {
        (Some(north), Some(south), Some(east), Some(west)) if north > south && east > west => {
            Ok(LatLonBox {
                north,
                south,
                east,
                west,
            })
        }
        _ => Err(PhnError::Network(
            "HeatRisk KML has no usable LatLonBox".into(),
        )),
    }
}

/// Dates for days 1–7 and the last update, from `FileTimes.js`.
pub fn parse_file_times(js: &str) -> (Vec<(u8, String)>, Option<String>) {
    let mut days = Vec::new();
    if let Some(start) = js.find('{')
        && let Some(end) = js[start..].find('}')
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(&js[start..=start + end])
        && let Some(o) = v.as_object()
    {
        for (k, d) in o {
            if let (Ok(day), Some(d)) = (k.parse::<u8>(), d.as_str()) {
                // MM/DD/YYYY -> YYYY-MM-DD
                let p: Vec<&str> = d.split('/').collect();
                if p.len() == 3 {
                    days.push((day, format!("{}-{}-{}", p[2], p[0], p[1])));
                }
            }
        }
    }
    days.sort();
    let updated = js
        .find("last_updated")
        .and_then(|i| js[i..].split('\'').nth(1))
        .map(str::to_string);
    (days, updated)
}

/// The 7-day HeatRisk forecast.
#[derive(Debug, Clone)]
pub struct HeatRiskForecast {
    pub grids: Vec<HeatRiskGrid>,
    /// WPC's "last updated" time.
    pub updated: Option<String>,
    pub mode: SourceMode,
    pub as_of: String,
}

fn not_html(body: &[u8]) -> Result<()> {
    let start: Vec<u8> = body
        .iter()
        .skip_while(|b| b.is_ascii_whitespace())
        .take(9)
        .map(u8::to_ascii_lowercase)
        .collect();
    if start.starts_with(b"<!doctype") || start.starts_with(b"<html") {
        return Err(PhnError::Network(
            "WPC returned a web page instead of HeatRisk data".into(),
        ));
    }
    Ok(())
}

/// Fetch the HeatRisk grids for days 1..=`days` (live → cache).
pub async fn fetch(http: &HttpClient, cache: Option<&Cache>, days: u8) -> Result<HeatRiskForecast> {
    let times = fetch_validated(
        http,
        cache,
        &format!("{DATA_BASE}/FileTimes.js"),
        None,
        TTL_SECS,
        &not_html,
        None,
    )
    .await?;
    let (dates, updated) = parse_file_times(&String::from_utf8_lossy(&times.body));
    let kml = fetch_validated(
        http,
        cache,
        &format!("{DATA_BASE}/HeatRisk_Day1_Fcst.kml"),
        None,
        TTL_SECS,
        &not_html,
        None,
    )
    .await?;
    let bbox = parse_kml_box(&String::from_utf8_lossy(&kml.body))?;
    let mut grids = Vec::new();
    let mut mode = times.mode;
    for day in 1..=days.clamp(1, 7) {
        let png = fetch_validated(
            http,
            cache,
            &format!("{DATA_BASE}/HeatRisk_{day}.png"),
            None,
            TTL_SECS,
            &|b: &[u8]| decode_png(b).map(|_| ()),
            None,
        )
        .await?;
        if png.mode != SourceMode::Live {
            mode = png.mode;
        }
        let (width, height, cells) = decode_png(&png.body)?;
        grids.push(HeatRiskGrid {
            day,
            date: dates
                .iter()
                .find(|(d, _)| *d == day)
                .map(|(_, s)| s.clone()),
            bbox,
            width,
            height,
            cells,
        });
    }
    Ok(HeatRiskForecast {
        grids,
        updated,
        mode,
        as_of: times.as_of,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = include_bytes!("../tests/fixtures/heatrisk_day1.png");
    const KML: &str = include_str!("../tests/fixtures/heatrisk_day1.kml");

    #[test]
    fn decodes_the_palette_grid() {
        let (w, h, cells) = decode_png(PNG).unwrap();
        assert_eq!((w, h), (2372, 1083));
        assert_eq!(cells.len(), w * h);
        assert!(cells.iter().all(|c| *c <= NO_DATA));
        let bbox = parse_kml_box(KML).unwrap();
        let g = HeatRiskGrid {
            day: 1,
            date: None,
            bbox,
            width: w,
            height: h,
            cells,
        };
        // November 8, 2026: no heat risk in Boston; minor (yellow) risk in Phoenix.
        assert_eq!(g.at(42.36, -71.06), Some(0));
        assert_eq!(g.at(33.45, -112.07), Some(1));
        assert_eq!(g.at(60.0, -71.0), None);
        let centres = g.cell_centres(42.2, 42.4, -71.2, -70.9);
        assert!(centres.len() > 50, "{}", centres.len());
    }

    #[test]
    fn parses_file_times() {
        let (days, updated) = parse_file_times(
            "var file_times = {\"1\":\"11/08/2026\",\"2\":\"11/09/2026\"};\nvar last_updated = '2026-11-08 19:49 UTC';",
        );
        assert_eq!(days[0], (1, "2026-11-08".to_string()));
        assert_eq!(days.len(), 2);
        assert_eq!(updated.as_deref(), Some("2026-11-08 19:49 UTC"));
    }
}
