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

#[tokio::test]
#[ignore = "needs network"]
async fn air_quality_is_live() {
    let (_d, ctx) = online();
    for area in ["Massachusetts", "Pittsfield", "Middlesex County"] {
        let env = phn_tools::env::air_quality_protective_guidance(
            &ctx,
            phn_tools::env::AirQualityArgs {
                area: Some(area.into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        println!("{}", env.summary);
        assert_ne!(env.data.airnow_status, "unavailable");
        assert!(!env.data.reporting_areas.is_empty());
    }
}

#[tokio::test]
#[ignore = "needs network"]
async fn heat_plan_is_live() {
    let (_d, ctx) = online();
    let env = phn_tools::env::heat_health_action_planner(
        &ctx,
        phn_tools::env::HeatPlanArgs {
            area: Some("Worcester".into()),
            checklist: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    println!("{}", env.to_markdown());
    assert_ne!(env.data.heatrisk_status, "unavailable");
    assert_eq!(env.data.heatrisk_days.len(), 7);
    assert!(env.data.heatrisk_days.iter().all(|d| d.max_level.is_some()));
    assert_ne!(env.data.forecast_status, "unavailable");
}

#[tokio::test]
#[ignore = "needs network"]
async fn empower_is_live() {
    let (_d, ctx) = online();
    let env = phn_tools::epr::get_electricity_dependent_population(
        &ctx,
        phn_tools::epr::ElectricityArgs {
            area: Some("Plymouth County".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    println!("{}", env.summary);
    assert_eq!(env.data.mode, "live");
    assert!(env.data.electricity_dependent > 1000.0);
}
