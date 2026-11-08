//! Developer tooling for the PHN MCP server.
//!
//! ```text
//! cargo xtask build-snapshots [--check] [--source-dir DIR]
//! cargo xtask build-guide-corpus [--check] [--source-dir DIR]
//! cargo xtask build-name-lists
//! cargo xtask validate-content [--today YYYY-MM-DD]
//! ```

mod content;
mod download;
mod guide_corpus;
mod names;
mod rtf;
mod snapshots;

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
        Some("build-name-lists") => names::run(&root),
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
                 build-guide-corpus [--check] [--source-dir DIR] | build-name-lists | \
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
