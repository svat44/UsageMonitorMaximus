//! Small diagnostic report for support and for later modules.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ErrorCategory;
use crate::monitoring::state::{ConnectionState, RunState};
use crate::providers::ProviderHealth;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub database_initialized: bool,
    pub schema_version: Option<i32>,
    pub active_provider: Option<String>,
    pub provider_health: Option<ProviderHealth>,
    pub run_state: RunState,
    pub connection: ConnectionState,
    pub last_success_at: Option<DateTime<Utc>>,
    pub latest_error_category: Option<ErrorCategory>,
    pub data_dir: String,
}
