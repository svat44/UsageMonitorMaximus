//! Clearly labelled simulated provider for UI/architecture development.
//! Every measurement it emits is `simulated = true`. It is NOT subscription monitoring.

use std::sync::atomic::{AtomicU32, Ordering};

use async_trait::async_trait;
use chrono::Duration;

use super::*;
use crate::model::{MeasurementId as Id, Verification};

pub const SIMULATED_PROVIDER_ID: &str = "simulated";

pub struct SimulatedProvider {
    desc: ProviderDescriptor,
    tick: AtomicU32,
}

impl Default for SimulatedProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulatedProvider {
    pub fn new() -> Self {
        Self {
            desc: ProviderDescriptor {
                id: SIMULATED_PROVIDER_ID.into(),
                display_name: "Simulated data (development only)".into(),
                description: "Deterministic fake values for all three categories. \
                              Not connected to Claude. Never real usage."
                    .into(),
                simulated: true,
                capabilities: Id::ALL.to_vec(),
                refresh: RefreshPolicy {
                    min_interval_secs: 1,
                    recommended_interval_secs: 5,
                    polling_supported: true,
                    event_driven: false,
                    request_timeout_secs: 5,
                    stale_after_secs: 60,
                    documented_rate_limit: None,
                },
            },
            tick: AtomicU32::new(0),
        }
    }
}

#[async_trait]
impl Provider for SimulatedProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.desc
    }
    async fn check_availability(&self) -> Availability {
        Availability::Available
    }
    async fn initialize(&self) -> Result<()> {
        Ok(())
    }

    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<Measurement>> {
        let t = self.tick.fetch_add(1, Ordering::Relaxed);
        let now = ctx.now;
        let v = Verification::Simulated;
        let m = |id, value: f64| {
            Measurement::available(id, SIMULATED_PROVIDER_ID, value, now, v).with_source_at(now)
        };
        Ok(vec![
            m(Id::SubscriptionFiveHourUtilization, ((t * 7) % 100) as f64)
                .with_resets_at(now + Duration::hours(3)),
            m(Id::SubscriptionWeeklyUtilization, (20 + t % 60) as f64)
                .with_resets_at(now + Duration::days(4)),
            m(Id::ApiInputTokens, 120_000.0 + t as f64 * 1_000.0),
            m(Id::ApiOutputTokens, 30_000.0 + t as f64 * 250.0),
            m(Id::LocalContextWindowUtilization, ((t * 3) % 100) as f64).with_scope("sim-session"),
            m(Id::LocalSessionCostEstimate, 0.25 + t as f64 * 0.01)
                .with_scope("sim-session")
                .with_estimated(true),
            m(Id::LocalActiveSessions, 1.0),
        ])
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth {
            state: HealthState::Healthy,
            detail: Some("simulated".into()),
            last_success_at: None,
        }
    }
    fn auth_state(&self) -> AuthState {
        AuthState::NotRequired
    }
    async fn disconnect(&self) {}
}
