//! Developer tooling for the PHN MCP server.
//!
//! ```text
//! cargo xtask build-snapshots [--check] [--source-dir DIR]
//! cargo xtask build-public-snapshots [--check] [--source-dir DIR] [--only ID]
//! cargo xtask build-surveillance-snapshots
//! cargo xtask build-guide-corpus [--check] [--source-dir DIR]
//! cargo xtask build-name-lists
//! cargo xtask build-service-directories [--check] [--source-dir DIR]
//! cargo xtask build-older-adult-snapshots [--source-dir DIR]
//! cargo xtask build-nursing-home-snapshot
//! cargo xtask validate-content [--today YYYY-MM-DD]
//! ```

mod content;
mod download;
mod guide_corpus;
mod names;
mod nursing_homes;
mod older_adults;
mod public_data;
mod rtf;
mod service_directories;
mod snapshots;
mod surveillance;

use std::path::PathBuf;

use anyhow::{Result, bail};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = workspace_root();
    match args.first().map(String::as_str) {
        Some("build-snapshots") => {
            let check = args.iter().any(|a| a == "--check");
            let source_dir = flag_value(&args, "--source-dir").map(PathBuf::from);
            snapshots::run(&root, check, source_dir)
        }
        Some("build-public-snapshots") => {
            let check = args.iter().any(|a| a == "--check");
            let source_dir = flag_value(&args, "--source-dir").map(PathBuf::from);
            let only = flag_value(&args, "--only");
            public_data::run(&root, check, source_dir, only)
        }
        Some("build-surveillance-snapshots") => surveillance::run(&root),
        Some("build-name-lists") => names::run(&root),
        Some("build-nursing-home-snapshot") => nursing_homes::run(&root),
        Some("build-older-adult-snapshots") => {
            let source_dir = flag_value(&args, "--source-dir").map(PathBuf::from);
            older_adults::run(&root, source_dir.as_deref())
        }
        Some("build-service-directories") => {
            let check = args.iter().any(|a| a == "--check");
            let source_dir = flag_value(&args, "--source-dir").map(PathBuf::from);
            service_directories::run(&root, check, source_dir.as_deref())
        }
        Some("build-guide-corpus") => {
            let check = args.iter().any(|a| a == "--check");
            let source_dir = flag_value(&args, "--source-dir").map(PathBuf::from);
            guide_corpus::run(&root, check, source_dir)
        }
        Some("validate-content") => {
            let today = match flag_value(&args, "--today") {
                Some(d) => chrono::NaiveDate::parse_from_str(&d, "%Y-%m-%d")?,
                None => chrono::Local::now().date_naive(),
            };
            content::run(&root, today)
        }
        _ => {
            eprintln!(
                "usage: cargo xtask <build-snapshots [--check] [--source-dir DIR] | \
                 build-public-snapshots [--check] [--source-dir DIR] [--only ID] | \
                 build-surveillance-snapshots | build-guide-corpus [--check] [--source-dir DIR] | \
                 build-name-lists | build-service-directories [--check] [--source-dir DIR] | \
                 build-nursing-home-snapshot | build-older-adult-snapshots [--source-dir DIR] | \
                 validate-content [--today YYYY-MM-DD]>"
            );
            bail!("unknown or missing command")
        }
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1).cloned())
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives inside the workspace")
        .to_path_buf()
}
