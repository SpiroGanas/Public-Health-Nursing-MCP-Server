//! Live checks of the Phase 12 data sources. They need the internet and are
//! ignored by default; run them with
//! `cargo test -p phn-tools --test phase12_live -- --ignored --nocapture`
//! when a source may have changed (NWS, OpenFEMA, emPOWER, AirNow, HeatRisk).

use phn_core::{Config, SourceMode};
use phn_tools::Context;

fn online() -> (tempfile::TempDir, Context) {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("data")).unwrap();
    let mut config = Config::for_root(dir.path());
    config.offline = false;
    config.http_timeout = std::time::Duration::from_secs(60);
    (dir, Context::new(config).unwrap())
}

#[tokio::test]
#[ignore = "needs network"]
async fn hazard_alerts_are_live() {
    let (_d, ctx) = online();
    let env = phn_tools::epr::get_active_hazard_alerts(
        &ctx,
        phn_tools::epr::HazardAlertsArgs {
            area: Some("Massachusetts".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    println!("{}", env.to_markdown());
    assert_ne!(env.data.alerts_status, "unavailable");
    assert!(
        env.sources
            .iter()
            .any(|s| s.mode == SourceMode::Live && s.publisher.contains("Federal Emergency"))
    );
}
