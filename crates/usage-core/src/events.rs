//! Event contract between the engine and whoever renders it (Tauri → React).
//! Names are stable; see docs/ARCHITECTURE.md "Event contract".

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::ErrorInfo;
use crate::monitoring::state::{ConnectionState, MonitoringSnapshot};

pub const EVENT_STARTED: &str = "monitoring://started";
pub const EVENT_UPDATED: &str = "monitoring://updated";
pub const EVENT_PROVIDER_STATUS_CHANGED: &str = "monitoring://provider-status-changed";
pub const EVENT_AUTHENTICATION_REQUIRED: &str = "monitoring://authentication-required";
pub const EVENT_RATE_LIMITED: &str = "monitoring://rate-limited";
pub const EVENT_ERROR: &str = "monitoring://error";
pub const EVENT_STOPPED: &str = "monitoring://stopped";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderStatusChanged {
    pub provider_id: String,
    pub previous: ConnectionState,
    pub current: ConnectionState,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RateLimitedPayload {
    pub provider_id: String,
    pub retry_after_secs: Option<u32>,
    /// Earliest time the engine will contact the provider again.
    pub resume_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AuthenticationRequiredPayload {
    pub provider_id: String,
    pub error: ErrorInfo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LifecyclePayload {
    pub at: DateTime<Utc>,
    pub provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MonitoringEvent {
    Started(LifecyclePayload),
    /// Carries the full authoritative snapshot so the frontend needn't poll.
    Updated(MonitoringSnapshot),
    ProviderStatusChanged(ProviderStatusChanged),
    AuthenticationRequired(AuthenticationRequiredPayload),
    RateLimited(RateLimitedPayload),
    Error(ErrorInfo),
    Stopped(LifecyclePayload),
}

impl MonitoringEvent {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Started(_) => EVENT_STARTED,
            Self::Updated(_) => EVENT_UPDATED,
            Self::ProviderStatusChanged(_) => EVENT_PROVIDER_STATUS_CHANGED,
            Self::AuthenticationRequired(_) => EVENT_AUTHENTICATION_REQUIRED,
            Self::RateLimited(_) => EVENT_RATE_LIMITED,
            Self::Error(_) => EVENT_ERROR,
            Self::Stopped(_) => EVENT_STOPPED,
        }
    }

    pub fn payload(&self) -> serde_json::Value {
        let r = match self {
            Self::Started(p) | Self::Stopped(p) => serde_json::to_value(p),
            Self::Updated(p) => serde_json::to_value(p),
            Self::ProviderStatusChanged(p) => serde_json::to_value(p),
            Self::AuthenticationRequired(p) => serde_json::to_value(p),
            Self::RateLimited(p) => serde_json::to_value(p),
            Self::Error(p) => serde_json::to_value(p),
        };
        r.unwrap_or(serde_json::Value::Null)
    }
}

/// Implemented by the Tauri shell (emits to the webview) and by test recorders.
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &MonitoringEvent);
}

/// Discards events.
pub struct NullSink;
impl EventSink for NullSink {
    fn emit(&self, _: &MonitoringEvent) {}
}

/// Records events; used by tests.
#[derive(Default)]
pub struct RecordingSink(pub std::sync::Mutex<Vec<MonitoringEvent>>);

impl RecordingSink {
    pub fn events(&self) -> Vec<MonitoringEvent> {
        self.0.lock().unwrap().clone()
    }
    pub fn names(&self) -> Vec<&'static str> {
        self.0.lock().unwrap().iter().map(|e| e.name()).collect()
    }
}
impl EventSink for RecordingSink {
    fn emit(&self, e: &MonitoringEvent) {
        self.0.lock().unwrap().push(e.clone());
    }
}
