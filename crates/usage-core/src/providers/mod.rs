//! Provider abstraction. The monitoring engine only ever talks to `dyn Provider`.

pub mod mock;
pub mod simulated;
pub mod statusline;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::errors::{Result, UsageError};
use crate::model::{Measurement, MeasurementId};

/// Per-provider refresh policy. The engine clamps any user setting to `min_interval_secs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefreshPolicy {
    pub min_interval_secs: u32,
    pub recommended_interval_secs: u32,
    pub polling_supported: bool,
    /// Source pushes updates (informational; the engine still polls the local cache).
    pub event_driven: bool,
    pub request_timeout_secs: u32,
    /// Data whose `source_at` is older than this is flagged stale.
    pub stale_after_secs: u32,
    /// Documented rate limit, human-readable. `None` = none documented (use conservative defaults).
    pub documented_rate_limit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderDescriptor {
    pub id: String,
    pub display_name: String,
    pub description: String,
    /// True for simulators and mocks. Their output can never count as real monitoring.
    pub simulated: bool,
    /// Measurements this provider can ever return. The engine rejects anything else.
    pub capabilities: Vec<MeasurementId>,
    pub refresh: RefreshPolicy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "snake_case")]
#[ts(export)]
pub enum Availability {
    Available,
    /// Temporarily not usable (e.g. source directory unreadable).
    Unavailable { reason: String },
    /// Will never work in this environment (e.g. unsupported OS).
    Unsupported { reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum AuthState {
    /// The provider needs no credentials (e.g. local file source).
    NotRequired,
    Authenticated,
    Required,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unhealthy,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProviderHealth {
    pub state: HealthState,
    pub detail: Option<String>,
    pub last_success_at: Option<DateTime<Utc>>,
}

/// Passed to `fetch`; providers stamp `collected_at` from `now` (deterministic tests).
#[derive(Debug, Clone, Copy)]
pub struct FetchContext {
    pub now: DateTime<Utc>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;

    /// Cheap check, no side effects. Called before `initialize`.
    async fn check_availability(&self) -> Availability;

    /// Acquire resources / validate configuration. Idempotent.
    async fn initialize(&self) -> Result<()>;

    /// Retrieve the current measurements. Must return **explicit `Unavailable`** values for data
    /// the source did not supply, never zeros. Errors must use the correct [`UsageError`] variant
    /// (the engine's retry behaviour depends on it).
    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<Measurement>>;

    async fn health(&self) -> ProviderHealth;

    fn auth_state(&self) -> AuthState;

    /// Release resources. Called on stop/shutdown. Must not fail loudly.
    async fn disconnect(&self);
}

/// Provider registry. Providers are registered at startup, selected by id.
#[derive(Default, Clone)]
pub struct ProviderRegistry {
    providers: HashMap<String, Arc<dyn Provider>>,
    order: Vec<String>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, provider: Arc<dyn Provider>) -> Result<()> {
        let id = provider.descriptor().id.clone();
        if self.providers.contains_key(&id) {
            return Err(UsageError::Configuration(format!(
                "provider '{id}' registered twice"
            )));
        }
        self.order.push(id.clone());
        self.providers.insert(id, provider);
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Provider>> {
        self.providers.get(id).cloned()
    }

    pub fn descriptors(&self) -> Vec<ProviderDescriptor> {
        self.order
            .iter()
            .map(|id| self.providers[id].descriptor().clone())
            .collect()
    }

    pub fn ids(&self) -> &[String] {
        &self.order
    }
}

/// Check a provider's output against its declared contract. Returns an error if the provider
/// returned undeclared measurements, mislabelled categories, or real-looking simulated data.
pub fn validate_measurements(desc: &ProviderDescriptor, ms: &[Measurement]) -> Result<()> {
    for m in ms {
        if !desc.capabilities.contains(&m.id) {
            return Err(UsageError::InvalidResponse(format!(
                "provider '{}' returned undeclared measurement {:?}",
                desc.id, m.id
            )));
        }
        if m.provider_id != desc.id {
            return Err(UsageError::InvalidResponse(format!(
                "measurement provider_id '{}' does not match provider '{}'",
                m.provider_id, desc.id
            )));
        }
        if m.category != m.id.category() || m.unit != m.id.unit() {
            return Err(UsageError::InvalidResponse(format!(
                "measurement {:?} has inconsistent category/unit",
                m.id
            )));
        }
        if desc.simulated && !m.simulated {
            return Err(UsageError::InvalidResponse(format!(
                "simulated provider '{}' produced a measurement not flagged simulated",
                desc.id
            )));
        }
        if let Some(v) = m.value.value() {
            if !v.is_finite() {
                return Err(UsageError::InvalidResponse(format!(
                    "measurement {:?} is not a finite number",
                    m.id
                )));
            }
            if m.unit == crate::model::Unit::Percent && !(0.0..=1000.0).contains(&v) {
                return Err(UsageError::InvalidResponse(format!(
                    "percentage {:?} out of range: {v}",
                    m.id
                )));
            }
            if v < 0.0 {
                return Err(UsageError::InvalidResponse(format!(
                    "measurement {:?} is negative: {v}",
                    m.id
                )));
            }
        }
    }
    Ok(())
}
