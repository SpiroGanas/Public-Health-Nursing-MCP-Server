//! Map boundaries bundled in the `ma-boundaries` snapshot: cities and towns,
//! 2020 census tracts, ZCTAs, and school districts.
//!
//! Geometry is stored in NAD83 / Massachusetts Mainland (EPSG:26986), in
//! 10-metre units, delta-encoded (see `xtask build-public-snapshots`). Decoded
//! coordinates are metres east and north, ready to scale onto a map.

use std::collections::HashMap;
use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::snapshots::SnapshotStore;

/// Snapshot id.
pub const SNAPSHOT_ID: &str = "ma-boundaries";

/// A boundary layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// The 351 cities and towns (id = town name).
    Town,
    /// 2020 census tracts (id = 11-digit GEOID).
    Tract,
    /// ZIP Code Tabulation Areas (id = 5-digit ZCTA).
    Zcta,
    /// School districts (id = DESE 8-digit organization code).
    SchoolDistrict,
}

impl Layer {
    /// Snapshot file.
    pub fn file(self) -> &'static str {
        match self {
            Self::Town => "ma_boundaries_towns.csv",
            Self::Tract => "ma_boundaries_tracts.csv",
            Self::Zcta => "ma_boundaries_zctas.csv",
            Self::SchoolDistrict => "ma_boundaries_school_districts.csv",
        }
    }

    /// Plain-language name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Town => "city or town",
            Self::Tract => "census tract",
            Self::Zcta => "ZIP Code Tabulation Area",
            Self::SchoolDistrict => "school district",
        }
    }
}

/// A ring of (x, y) metres.
pub type Ring = Vec<(f64, f64)>;

/// One feature: polygons, each a list of rings (exterior first).
#[derive(Debug, Clone, PartialEq)]
pub struct Feature {
    pub id: String,
    pub name: String,
    pub polygons: Vec<Vec<Ring>>,
}

impl Feature {
    /// Bounding box (min x, min y, max x, max y).
    pub fn bbox(&self) -> (f64, f64, f64, f64) {
        let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for p in &self.polygons {
            for &(x, y) in p.iter().flatten() {
                b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
            }
        }
        b
    }

    /// Area in square metres (exterior minus holes).
    pub fn area(&self) -> f64 {
        self.polygons
            .iter()
            .map(|rings| {
                rings
                    .iter()
                    .enumerate()
                    .map(|(i, r)| {
                        let a = ring_area(r).abs();
                        if i == 0 { a } else { -a }
                    })
                    .sum::<f64>()
            })
            .sum()
    }

    /// A point for a label: the centroid of the largest polygon's exterior.
    pub fn label_point(&self) -> (f64, f64) {
        let largest = self
            .polygons
            .iter()
            .max_by(|a, b| ring_area(&a[0]).abs().total_cmp(&ring_area(&b[0]).abs()));
        match largest {
            Some(rings) => centroid(&rings[0]),
            None => (0.0, 0.0),
        }
    }
}

fn ring_area(r: &Ring) -> f64 {
    let n = r.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x1, y1) = r[i];
        let (x2, y2) = r[(i + 1) % n];
        a += x1 * y2 - x2 * y1;
    }
    a / 2.0
}

fn centroid(r: &Ring) -> (f64, f64) {
    let a = ring_area(r);
    if a.abs() < 1e-9 {
        let n = r.len().max(1) as f64;
        return (
            r.iter().map(|p| p.0).sum::<f64>() / n,
            r.iter().map(|p| p.1).sum::<f64>() / n,
        );
    }
    let n = r.len();
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let (x1, y1) = r[i];
        let (x2, y2) = r[(i + 1) % n];
        let f = x1 * y2 - x2 * y1;
        cx += (x1 + x2) * f;
        cy += (y1 + y2) * f;
    }
    (cx / (6.0 * a), cy / (6.0 * a))
}

/// Decode the geometry encoding.
pub fn decode_geometry(s: &str) -> Result<Vec<Vec<Ring>>> {
    let mut polys = Vec::new();
    for poly in s.split('|') {
        let mut rings = Vec::new();
        for ring in poly.split(';') {
            let nums: Vec<i64> = ring
                .split_ascii_whitespace()
                .map(|n| n.parse::<i64>())
                .collect::<std::result::Result<_, _>>()
                .map_err(|e| PhnError::Internal(format!("boundary geometry: {e}")))?;
            if nums.len() < 6 || nums.len() % 2 != 0 {
                continue;
            }
            let (mut x, mut y) = (0i64, 0i64);
            let mut pts = Vec::with_capacity(nums.len() / 2);
            for (i, pair) in nums.chunks(2).enumerate() {
                if i == 0 {
                    (x, y) = (pair[0], pair[1]);
                } else {
                    x += pair[0];
                    y += pair[1];
                }
                pts.push((x as f64 * 10.0, y as f64 * 10.0));
            }
            rings.push(pts);
        }
        if !rings.is_empty() {
            polys.push(rings);
        }
    }
    Ok(polys)
}

/// A loaded layer.
#[derive(Debug, Clone)]
pub struct BoundaryLayer {
    pub layer: Layer,
    pub features: Vec<Feature>,
    index: HashMap<String, usize>,
    pub source: Source,
}

impl BoundaryLayer {
    /// Parse a layer.
    pub fn load(store: &SnapshotStore, layer: Layer) -> Result<Self> {
        let (text, entry, _) = store.file_text(SNAPSHOT_ID, layer.file())?;
        let mut features = Vec::new();
        let mut rdr = csv::Reader::from_reader(text.as_bytes());
        for rec in rdr.records() {
            let r = rec.map_err(|e| PhnError::Internal(format!("boundary snapshot: {e}")))?;
            features.push(Feature {
                id: r.get(0).unwrap_or_default().to_string(),
                name: r.get(1).unwrap_or_default().to_string(),
                polygons: decode_geometry(r.get(2).unwrap_or_default())?,
            });
        }
        let index = features
            .iter()
            .enumerate()
            .map(|(i, f)| (f.id.clone(), i))
            .collect();
        Ok(Self {
            layer,
            features,
            index,
            source: entry.to_source(),
        })
    }

    /// The shared, lazily parsed layer.
    pub fn shared(store: &SnapshotStore, layer: Layer) -> Result<&'static BoundaryLayer> {
        static CELLS: [OnceLock<std::result::Result<BoundaryLayer, String>>; 4] =
            [OnceLock::new(), OnceLock::new(), OnceLock::new(), OnceLock::new()];
        let i = match layer {
            Layer::Town => 0,
            Layer::Tract => 1,
            Layer::Zcta => 2,
            Layer::SchoolDistrict => 3,
        };
        CELLS[i]
            .get_or_init(|| Self::load(store, layer).map_err(|e| e.to_string()))
            .as_ref()
            .map_err(|e| PhnError::Internal(format!("boundary snapshot: {e}")))
    }

    /// A feature by id.
    pub fn get(&self, id: &str) -> Option<&Feature> {
        self.index.get(id).map(|&i| &self.features[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_delta_geometry() {
        let p = decode_geometry("10 10 2 0 0 3|1 1 1 0 0 1;5 5 1 0 0 1").unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p[0][0], vec![(100.0, 100.0), (120.0, 100.0), (120.0, 130.0)]);
        assert_eq!(p[1].len(), 2);
    }

    #[test]
    fn every_layer_loads_with_plausible_shapes() {
        let store = SnapshotStore::bundled();
        let towns = BoundaryLayer::load(&store, Layer::Town).unwrap();
        assert_eq!(towns.features.len(), 351);
        let boston = towns.get("Boston").unwrap();
        // Boston covers roughly 125 km² of land.
        let km2 = boston.area() / 1e6;
        assert!((90.0..160.0).contains(&km2), "{km2}");
        let (x, y) = boston.label_point();
        assert!((225_000.0..245_000.0).contains(&x) && (885_000.0..910_000.0).contains(&y));
        let tracts = BoundaryLayer::load(&store, Layer::Tract).unwrap();
        assert!(tracts.features.len() > 1600);
        let zctas = BoundaryLayer::load(&store, Layer::Zcta).unwrap();
        assert!(zctas.get("01002").is_some());
        let districts = BoundaryLayer::load(&store, Layer::SchoolDistrict).unwrap();
        assert!(districts.features.len() > 250);
    }
}
