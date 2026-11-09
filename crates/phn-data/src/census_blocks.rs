//! 2020 census blocks with housing units for Massachusetts (PL 94-171),
//! from the bundled `census-pl2020-blocks-ma` snapshot, for CASPER cluster
//! sampling (EPR-11). See `xtask/scripts/build_epr_snapshots.py` for the
//! format: one line per county, county subdivision (town), and tract,
//! `COUNTY,COUSUB,TRACT,BLOCK:HU[:VACANT] ...`.

use std::sync::OnceLock;

use phn_core::{PhnError, Result, Source};

use crate::snapshots::SnapshotStore;

pub const SNAPSHOT_ID: &str = "census-pl2020-blocks-ma";
pub const SNAPSHOT_FILE: &str = "ma_pl2020_blocks.txt";

/// One census block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Block {
    /// County FIPS (3 digits, as a number).
    pub county: u16,
    /// County subdivision (town) FIPS (5 digits).
    pub cousub: u32,
    /// Tract (6 digits).
    pub tract: u32,
    /// Block (4 digits; the first digit is the block group).
    pub block: u16,
    /// All housing units (H1_001N).
    pub housing_units: u32,
    /// Vacant housing units (H1_003N).
    pub vacant: u32,
}

impl Block {
    /// Occupied housing units (households).
    pub fn occupied(&self) -> u32 {
        self.housing_units - self.vacant
    }

    /// 15-digit block GEOID.
    pub fn geoid(&self) -> String {
        format!("25{:03}{:06}{:04}", self.county, self.tract, self.block)
    }

    /// 10-digit town (county subdivision) GEOID.
    pub fn town_geoid(&self) -> String {
        format!("25{:03}{:05}", self.county, self.cousub)
    }

    /// 5-digit county GEOID.
    pub fn county_geoid(&self) -> String {
        format!("25{:03}", self.county)
    }
}

/// The parsed snapshot.
#[derive(Debug, Clone)]
pub struct BlockFrame {
    pub blocks: Vec<Block>,
    pub source: Source,
}

/// Parse the snapshot text.
pub fn parse(text: &str) -> Result<Vec<Block>> {
    let bad = |l: usize| PhnError::Internal(format!("block snapshot line {}", l + 1));
    let mut out = Vec::new();
    for (ln, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let mut parts = line.splitn(4, ',');
        let county: u16 = parts
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| bad(ln))?;
        let cousub: u32 = parts
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| bad(ln))?;
        let tract: u32 = parts
            .next()
            .and_then(|v| v.parse().ok())
            .ok_or_else(|| bad(ln))?;
        for b in parts.next().ok_or_else(|| bad(ln))?.split(' ') {
            let mut f = b.split(':');
            let block: u16 = f
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| bad(ln))?;
            let hu: u32 = f
                .next()
                .and_then(|v| v.parse().ok())
                .ok_or_else(|| bad(ln))?;
            let vacant: u32 = match f.next() {
                Some(v) => v.parse().map_err(|_| bad(ln))?,
                None => 0,
            };
            if vacant > hu {
                return Err(bad(ln));
            }
            out.push(Block {
                county,
                cousub,
                tract,
                block,
                housing_units: hu,
                vacant,
            });
        }
    }
    Ok(out)
}

impl BlockFrame {
    /// The shared, lazily parsed frame.
    pub fn shared(store: &SnapshotStore) -> Result<&'static BlockFrame> {
        static CELL: OnceLock<std::result::Result<BlockFrame, String>> = OnceLock::new();
        CELL.get_or_init(|| {
            let (text, entry, _) = store
                .file_text(SNAPSHOT_ID, SNAPSHOT_FILE)
                .map_err(|e| e.to_string())?;
            Ok(BlockFrame {
                blocks: parse(&text).map_err(|e| e.to_string())?,
                source: entry.to_source(),
            })
        })
        .as_ref()
        .map_err(|e| PhnError::Internal(format!("block snapshot: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_totals_match_the_2020_census() {
        let f = BlockFrame::shared(&SnapshotStore::bundled()).unwrap();
        assert_eq!(f.blocks.len(), 88_246);
        let hu: u64 = f.blocks.iter().map(|b| b.housing_units as u64).sum();
        assert_eq!(hu, 2_998_537);
        let occ: u64 = f.blocks.iter().map(|b| b.occupied() as u64).sum();
        // 2020 Census: 2,749,225 occupied housing units in Massachusetts.
        assert_eq!(occ, 2_749_225);
        assert_eq!(f.blocks[0].geoid().len(), 15);
        let b = parse("001,55500,010100,1001:12:3 1002:4\n").unwrap();
        assert_eq!(b[0].geoid(), "250010101001001");
        assert_eq!(b[0].occupied(), 9);
        assert_eq!(b[1].vacant, 0);
        assert_eq!(b[0].town_geoid(), "2500155500");
        assert!(parse("001,1,2,3:1:5\n").is_err());
    }
}
