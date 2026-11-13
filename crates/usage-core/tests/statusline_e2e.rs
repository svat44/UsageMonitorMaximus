//! Bridge file → provider → engine → persisted history → "restart", using the real
//! status-line provider (not a mock) and fixtures that follow the *documented* schema.

use std::sync::Arc;

use chrono::{Duration, Utc};
use usage_core::clock::SystemClock;
use usage_core::config::AppConfig;
use usage_core::events::NullSink;
use usage_core::model::*;
use usage_core::monitoring::{ConnectionState, EngineDeps, MonitoringEngine, Trigger};
use usage_core::providers::statusline::{ClaudeCodeStatusLineProvider, STATUSLINE_PROVIDER_ID};
use usage_core::providers::{Provider, ProviderRegistry};
use usage_core::statusline_protocol::*;
use usage_core::storage::sqlite::SqliteRepository;
use usage_core::storage::RetentionPolicy;

async fn engine(dir: &std::path::Path) -> (MonitoringEngine, Arc<SqliteRepository>) {
    let repo = Arc::new(SqliteRepository::open(&dir.join("db.sqlite")).await.unwrap());
    let mut reg = ProviderRegistry::new();
    reg.register(Arc::new(ClaudeCodeStatusLineProvider::new(dir.join("statusline"))) as Arc<dyn Provider>)
        .unwrap();
    let e = MonitoringEngine::new(EngineDeps {
        registry: reg,
        repo: repo.clone(),
        events: Arc::new(NullSink),
        clock: Arc::new(SystemClock),
        config: AppConfig::default(),
        data_dir: dir.to_path_buf(),
        retention: RetentionPolicy::default(),
    });
    e.initialize().await.unwrap();
    (e, repo)
}

#[tokio::test]
async fn real_provider_end_to_end_with_documented_fixture() {
    let d = tempfile::tempdir().unwrap();
    let (e, _repo) = engine(d.path()).await;

    // 1. Nothing from Claude Code yet: "connected" source, but subscription is NOT connected.
    e.sync_once(Trigger::Manual).await.unwrap();
    let s = e.snapshot();
    assert_eq!(s.active_provider.as_deref(), Some(STATUSLINE_PROVIDER_ID));
    assert_eq!(s.connection, ConnectionState::Current);
    assert!(!s.simulated);
    assert!(s.measurements.iter().all(|m| m.value.value().is_none()));
    let sub = s
        .coverage
        .iter()
        .find(|c| c.category == MeasurementCategory::SubscriptionUsage)
        .unwrap();
    assert_eq!(sub.state, CoverageState::NotConnected);

    // 2. Claude Code (via the bridge) reports the documented JSON.
    let now = Utc::now();
    let json = format!(
        r#"{{"session_id":"abc123","model":{{"display_name":"X"}},"cost":{{"total_cost_usd":0.0123}},
        "context_window":{{"used_percentage":8,"context_window_size":200000}},
        "rate_limits":{{"five_hour":{{"used_percentage":23.5,"resets_at":{}}},
                        "seven_day":{{"used_percentage":41.2,"resets_at":{}}}}}}}"#,
        (now + Duration::hours(2)).timestamp(),
        (now + Duration::days(3)).timestamp()
    );
    write_snapshot(
        &d.path().join("statusline"),
        &SnapshotEnvelope {
            schema_version: SCHEMA_VERSION,
            received_at: now,
            payload: StatusLinePayload::from_claude_code_json(&json).unwrap(),
        },
    )
    .unwrap();
    e.sync_once(Trigger::Manual).await.unwrap();
    let s = e.snapshot();
    let get = |id| s.measurements.iter().find(|m| m.id == id).unwrap().clone();
    assert_eq!(
        get(MeasurementId::SubscriptionFiveHourUtilization).value.value(),
        Some(23.5)
    );
    assert_eq!(
        get(MeasurementId::SubscriptionWeeklyUtilization).value.value(),
        Some(41.2)
    );
    assert_eq!(
        get(MeasurementId::LocalContextWindowUtilization).value.value(),
        Some(8.0)
    );
    // The two percentages mean different things and stay in different categories.
    assert_ne!(
        get(MeasurementId::SubscriptionFiveHourUtilization).category,
        get(MeasurementId::LocalContextWindowUtilization).category
    );
    let sub = s
        .coverage
        .iter()
        .find(|c| c.category == MeasurementCategory::SubscriptionUsage)
        .unwrap();
    assert_eq!(sub.state, CoverageState::Live);
    let api = s
        .coverage
        .iter()
        .find(|c| c.category == MeasurementCategory::ApiConsumption)
        .unwrap();
    assert_eq!(
        api.state,
        CoverageState::NotConnected,
        "no real API provider exists"
    );
    assert!(s
        .measurements
        .iter()
        .all(|m| m.verification == Verification::OfficialLocalInterface && !m.simulated));

    // 3. Restart: history comes back restored+stale, subscription coverage is Stale, not Live.
    e.shutdown().await;
    let (e2, _r2) = engine(d.path()).await;
    let s2 = e2.snapshot();
    let five = s2
        .measurements
        .iter()
        .find(|m| m.id == MeasurementId::SubscriptionFiveHourUtilization)
        .unwrap();
    assert_eq!(five.value.value(), Some(23.5));
    assert!(five.restored && five.stale);
    let sub = s2
        .coverage
        .iter()
        .find(|c| c.category == MeasurementCategory::SubscriptionUsage)
        .unwrap();
    assert_eq!(sub.state, CoverageState::Stale);
}
