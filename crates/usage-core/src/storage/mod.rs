//! Storage boundary. The engine depends on [`Repository`], never on SQL.

#[cfg(feature = "sqlite")]
pub mod sqlite;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::{ErrorCategory, Result};
use crate::model::{Measurement, MeasurementId};
use crate::providers::ProviderDescriptor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum SyncOutcome {
    Success,
    Failure,
}

/// Everything written for one sync attempt, in a single transaction.
#[derive(Debug, Clone)]
pub struct SyncRecord {
    pub provider_id: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub outcome: SyncOutcome,
    pub error_category: Option<ErrorCategory>,
    /// Measurements to persist (already de-duplicated by the engine).
    pub measurements: Vec<Measurement>,
    /// Count of measurements the sync produced, persisted or not.
    pub measurement_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticEvent {
    pub occurred_at: DateTime<Utc>,
    pub level: String,
    pub provider_id: Option<String>,
    pub operation: String,
    pub category: Option<ErrorCategory>,
    pub message: String,
}

#[derive(Debug, Clone, Copy)]
pub struct RetentionPolicy {
    pub measurements: Duration,
    pub sync_runs: Duration,
    pub diagnostics: Duration,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            measurements: Duration::days(90),
            sync_runs: Duration::days(30),
            diagnostics: Duration::days(30),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub measurements: u64,
    pub sync_runs: u64,
    pub diagnostics: u64,
}

#[async_trait]
pub trait Repository: Send + Sync {
    /// Latest applied migration version (0 if none). Used by diagnostics.
    async fn schema_version(&self) -> Result<i64>;

    async fn get_preference(&self, key: &str) -> Result<Option<String>>;
    async fn set_preference(&self, key: &str, value: &str, now: DateTime<Utc>) -> Result<()>;

    async fn upsert_provider(&self, desc: &ProviderDescriptor, now: DateTime<Utc>) -> Result<()>;

    /// Insert the sync run, its measurements and the provider status update atomically.
    async fn record_sync(&self, rec: &SyncRecord) -> Result<i64>;

    /// Most recent persisted row per (measurement id, scope) for a provider, as `restored`+`stale`.
    async fn latest_measurements(
        &self,
        provider_id: &str,
        newer_than: DateTime<Utc>,
    ) -> Result<Vec<Measurement>>;

    async fn measurement_history(
        &self,
        provider_id: &str,
        id: MeasurementId,
        scope: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Measurement>>;

    async fn record_diagnostic(&self, ev: &DiagnosticEvent) -> Result<()>;
    async fn recent_diagnostics(&self, limit: u32) -> Result<Vec<DiagnosticEvent>>;

    async fn prune(&self, policy: &RetentionPolicy, now: DateTime<Utc>) -> Result<PruneReport>;

    /// Flush and close connections. Idempotent.
    async fn close(&self);
}
