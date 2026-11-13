//! Authoritative monitoring state, owned by the engine and published to the frontend.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ErrorInfo;
use crate::model::{CategoryCoverage, Measurement};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ConnectionState {
    /// Engine not running / no sync attempted yet.
    NotStarted,
    /// Real provider, last sync succeeded and data is fresh.
    Current,
    /// Last sync succeeded but the source's data is older than its freshness limit.
    Stale,
    AuthenticationRequired,
    RateLimited,
    /// Last sync failed (network, timeout, provider down…). Previous values are kept, marked stale.
    TemporarilyUnavailable,
    /// Provider can never work here; polling stopped.
    PermanentlyUnsupported,
    /// Last sync succeeded, but the provider is a simulator. Not real data.
    Simulated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RunState {
    Stopped,
    Running,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MonitoringSnapshot {
    pub run_state: RunState,
    pub active_provider: Option<String>,
    pub connection: ConnectionState,
    /// True when the active provider is a simulator/mock.
    pub simulated: bool,
    pub measurements: Vec<Measurement>,
    /// What the user may believe about each category (derived from `measurements`).
    pub coverage: Vec<CategoryCoverage>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    /// When the engine will next contact the provider; `None` = waiting for user action.
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub rate_limited_until: Option<DateTime<Utc>>,
    pub consecutive_failures: u32,
    pub latest_error: Option<ErrorInfo>,
}

impl MonitoringSnapshot {
    pub fn empty() -> Self {
        Self {
            run_state: RunState::Stopped,
            active_provider: None,
            connection: ConnectionState::NotStarted,
            simulated: false,
            measurements: vec![],
            coverage: crate::model::compute_coverage(&[]),
            last_attempt_at: None,
            last_success_at: None,
            next_attempt_at: None,
            rate_limited_until: None,
            consecutive_failures: 0,
            latest_error: None,
        }
    }
}
