//! `phn-mcp-server`: the Public Health Nursing MCP server over stdio.
//!
//! ```text
//! phn-mcp-server            # serve MCP over stdin/stdout
//! phn-mcp-server check      # self-test: configuration, content, snapshots
//! phn-mcp-server --version
//! ```
//!
//! Logs go to stderr only; stdout carries the MCP protocol.

use std::sync::Arc;

use anyhow::Context as _;
use phn_server::PhnServer;
use phn_tools::Context;
use rmcp::ServiceExt;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .with_env_filter(
            EnvFilter::try_from_env("PHN_LOG").unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let arg = std::env::args().nth(1);
    match arg.as_deref() {
        Some("--version") | Some("-V") => {
            println!("{} {}", phn_core::SERVER_NAME, phn_core::VERSION);
            return Ok(());
        }
        Some("check") => return check(),
        Some("--help") | Some("-h") => {
            println!(
                "{} {}\n\nUsage: phn-mcp-server [check | --version]\n\n\
                 Serves the Model Context Protocol over stdin/stdout. See the README for \
                 environment variables (PHN_DATA_DIR, PHN_CACHE_DIR, PHN_PROFILE, PHN_TOOLSETS, \
                 PHN_OFFLINE, PHN_CA_BUNDLE, PHN_SUPPRESSION_THRESHOLD, PHN_LOG).",
                phn_core::SERVER_NAME,
                phn_core::VERSION
            );
            return Ok(());
        }
        Some(other) => anyhow::bail!("unknown argument '{other}' (try --help)"),
        None => {}
    }

    let config = phn_core::Config::from_env().context("reading configuration")?;
    let ctx = Arc::new(Context::new(config).context("starting server")?);
    for w in &ctx.startup_warnings {
        tracing::warn!("{w}");
    }
    // Out-of-range values in the settings file are ignored; say so on stderr.
    for w in ctx.settings().warnings() {
        tracing::warn!("{w}");
    }
    tracing::info!(
        version = phn_core::VERSION,
        toolsets = ?ctx.enabled_toolsets(),
        offline = ctx.config.offline,
        "starting MCP server on stdio"
    );
    let service = PhnServer::new(ctx)
        .serve(rmcp::transport::stdio())
        .await
        .context("MCP initialization failed")?;
    service.waiting().await?;
    Ok(())
}

/// Self-test for administrators: prints configuration and verifies bundled data.
fn check() -> anyhow::Result<()> {
    let config = phn_core::Config::from_env()?;
    let ctx = Context::new(config)?;
    ctx.snapshots.verify_bundled()?;
    let settings = ctx.settings();
    println!("{} {}", phn_core::SERVER_NAME, phn_core::VERSION);
    println!(
        "data folder:   {} ({})",
        ctx.config.paths.data_dir.display(),
        if ctx.config.paths.data_dir.is_dir() {
            "mounted"
        } else {
            "MISSING"
        }
    );
    println!("cache folder:  {}", ctx.config.paths.cache_dir.display());
    println!("settings file: {}", ctx.config.settings_path.display());
    println!("offline mode:  {}", ctx.config.offline);
    // Date calculations ("today", deadlines) use this clock; in the Docker image
    // TZ=America/New_York makes it Massachusetts time (-05:00 or -04:00).
    println!(
        "local time:    {} (TZ={})",
        phn_core::local_time_label(),
        std::env::var("TZ").unwrap_or_else(|_| "not set".into())
    );
    println!(
        "town:          {}",
        settings.jurisdiction.town.as_deref().unwrap_or("not set")
    );
    println!(
        "toolsets:      {}",
        ctx.enabled_toolsets()
            .iter()
            .map(|t| t.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    println!(
        "suppression:   counts of 1-{} suppressed",
        ctx.suppression_threshold().saturating_sub(1)
    );
    println!(
        "municipalities: {}, school districts: {}",
        ctx.ma.municipalities.len(),
        ctx.ma.school_districts.len()
    );
    for item in phn_content::ITEMS {
        let meta = phn_content::item_meta(item).map_err(anyhow::Error::msg)?;
        println!(
            "content: {} v{} ({:?}, review due {})",
            meta.id, meta.version, meta.review.status, meta.review.review_due
        );
    }
    for w in &ctx.startup_warnings {
        println!("warning: {w}");
    }
    for w in settings.warnings() {
        println!("warning: {w}");
    }
    println!("OK");
    Ok(())
}
