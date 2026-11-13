#![allow(dead_code)]
use std::sync::Arc;

use chrono::{Duration, TimeZone, Utc};
use tempfile::TempDir;
use usage_core::clock::ManualClock;
use usage_core::config::AppConfig;
use usage_core::events::RecordingSink;
use usage_core::monitoring::{EngineDeps, MonitoringEngine};
use usage_core::providers::mock::MockProvider;
use usage_core::providers::{Provider, ProviderRegistry};
use usage_core::storage::sqlite::SqliteRepository;
use usage_core::storage::RetentionPolicy;

pub struct Harness {
    pub engine: MonitoringEngine,
    pub sink: Arc<RecordingSink>,
    pub clock: Arc<ManualClock>,
    pub repo: Arc<SqliteRepository>,
    pub provider: Arc<MockProvider>,
    pub dir: TempDir,
}

pub fn t0() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
}

pub async fn harness(provider: MockProvider) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    harness_in(dir, provider).await
}

pub async fn harness_in(dir: TempDir, provider: MockProvider) -> Harness {
    let provider = Arc::new(provider);
    let repo = Arc::new(SqliteRepository::open(&dir.path().join("t.sqlite")).await.unwrap());
    let clock = Arc::new(ManualClock::new(t0()));
    let sink = Arc::new(RecordingSink::default());
    let mut reg = ProviderRegistry::new();
    reg.register(provider.clone() as Arc<dyn Provider>).unwrap();
    let cfg = AppConfig { selected_provider: provider.descriptor().id.clone(), ..AppConfig::default() };
    let engine = MonitoringEngine::new(EngineDeps {
        registry: reg,
        repo: repo.clone(),
        events: sink.clone(),
        clock: clock.clone(),
        config: cfg,
        data_dir: dir.path().to_path_buf(),
        retention: RetentionPolicy::default(),
    });
    engine.initialize().await.unwrap();
    Harness { engine, sink, clock, repo, provider, dir }
}

pub fn secs(n: i64) -> Duration {
    Duration::seconds(n)
}
