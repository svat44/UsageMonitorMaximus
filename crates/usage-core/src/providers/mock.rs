//! Deterministic, scriptable provider for testing the engine.
//!
//! The mock is simulated by definition. `pretend_real()` exists only so engine tests can verify
//! the non-simulated code path; it is `#[doc(hidden)]` and must never be registered in the app.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::Duration;

use super::*;
use crate::model::{MeasurementId as Id, UnavailableReason, Verification};

#[derive(Debug, Clone)]
pub enum MockBehavior {
    /// Valid 5h + weekly + local measurements.
    Success,
    /// Success with zero measurements.
    Empty,
    /// Success but omit these ids entirely (missing fields).
    MissingFields(Vec<Id>),
    /// Success but `source_at` is this many seconds in the past.
    Stale(i64),
    /// Success with explicit `Unavailable` for the 5h window.
    ExplicitUnavailable,
    NetworkFailure,
    /// Never completes; the engine's timeout must fire.
    Hang,
    AuthExpired,
    PermissionDenied,
    RateLimited(Option<u64>),
    /// Returns a measurement whose id is not in the descriptor's capabilities.
    Malformed,
    /// Returns a negative percentage.
    OutOfRange,
    Unavailable,
    Unsupported,
}

pub struct MockProvider {
    desc: ProviderDescriptor,
    script: Mutex<VecDeque<MockBehavior>>,
    default: Mutex<MockBehavior>,
    calls: AtomicUsize,
    last_ok: Mutex<Option<DateTime<Utc>>>,
    auth: Mutex<AuthState>,
    pretend_real: bool,
    disconnects: AtomicUsize,
}

impl MockProvider {
    pub fn new(id: &str) -> Self {
        Self::with_policy(
            id,
            RefreshPolicy {
                min_interval_secs: 1,
                recommended_interval_secs: 60,
                polling_supported: true,
                event_driven: false,
                request_timeout_secs: 5,
                stale_after_secs: 300,
                documented_rate_limit: None,
            },
        )
    }

    pub fn with_policy(id: &str, refresh: RefreshPolicy) -> Self {
        Self {
            desc: ProviderDescriptor {
                id: id.into(),
                display_name: format!("Mock ({id})"),
                description: "Test double".into(),
                simulated: true,
                capabilities: vec![
                    Id::SubscriptionFiveHourUtilization,
                    Id::SubscriptionWeeklyUtilization,
                    Id::LocalContextWindowUtilization,
                    Id::LocalActiveSessions,
                ],
                refresh,
            },
            script: Mutex::new(VecDeque::new()),
            default: Mutex::new(MockBehavior::Success),
            calls: AtomicUsize::new(0),
            last_ok: Mutex::new(None),
            auth: Mutex::new(AuthState::NotRequired),
            pretend_real: false,
            disconnects: AtomicUsize::new(0),
        }
    }

    /// Test-only: emit measurements flagged as non-simulated to exercise the "real" engine paths.
    #[doc(hidden)]
    pub fn pretend_real(mut self) -> Self {
        self.desc.simulated = false;
        self.pretend_real = true;
        self
    }

    /// Queue behaviours consumed one per `fetch`; afterwards `default` repeats.
    pub fn script(self, steps: impl IntoIterator<Item = MockBehavior>) -> Self {
        *self.script.lock().unwrap() = steps.into_iter().collect();
        self
    }
    pub fn set_default(&self, b: MockBehavior) {
        *self.default.lock().unwrap() = b;
    }
    pub fn push(&self, b: MockBehavior) {
        self.script.lock().unwrap().push_back(b);
    }
    pub fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
    pub fn disconnects(&self) -> usize {
        self.disconnects.load(Ordering::SeqCst)
    }

    fn verification(&self) -> Verification {
        if self.pretend_real {
            Verification::OfficialLocalInterface
        } else {
            Verification::Simulated
        }
    }

    fn build(
        &self,
        now: DateTime<Utc>,
        source_age: i64,
        skip: &[Id],
        unavailable_5h: bool,
    ) -> Vec<Measurement> {
        let v = self.verification();
        let src = now - Duration::seconds(source_age);
        let id = self.desc.id.clone();
        let mut out = vec![];
        let mut push = |m: Measurement| {
            if !skip.contains(&m.id) {
                out.push(m.with_source_at(src));
            }
        };
        if unavailable_5h {
            push(Measurement::unavailable(
                Id::SubscriptionFiveHourUtilization,
                id.clone(),
                UnavailableReason::NotReportedBySource,
                now,
                v,
            ));
        } else {
            push(
                Measurement::available(Id::SubscriptionFiveHourUtilization, id.clone(), 42.0, now, v)
                    .with_resets_at(now + Duration::hours(3)),
            );
        }
        push(
            Measurement::available(Id::SubscriptionWeeklyUtilization, id.clone(), 17.5, now, v)
                .with_resets_at(now + Duration::days(3)),
        );
        push(
            Measurement::available(Id::LocalContextWindowUtilization, id.clone(), 33.0, now, v)
                .with_scope("s1"),
        );
        push(Measurement::available(Id::LocalActiveSessions, id, 1.0, now, v));
        out
    }
}

#[async_trait]
impl Provider for MockProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.desc
    }

    async fn check_availability(&self) -> Availability {
        match *self.default.lock().unwrap() {
            MockBehavior::Unsupported => Availability::Unsupported {
                reason: "mock".into(),
            },
            _ => Availability::Available,
        }
    }
    async fn initialize(&self) -> Result<()> {
        Ok(())
    }

    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<Measurement>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let behavior = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| self.default.lock().unwrap().clone());
        let now = ctx.now;
        let res = match behavior {
            MockBehavior::Success => Ok(self.build(now, 0, &[], false)),
            MockBehavior::Empty => Ok(vec![]),
            MockBehavior::MissingFields(ids) => Ok(self.build(now, 0, &ids, false)),
            MockBehavior::Stale(age) => Ok(self.build(now, age, &[], false)),
            MockBehavior::ExplicitUnavailable => Ok(self.build(now, 0, &[], true)),
            MockBehavior::NetworkFailure => Err(UsageError::Network("mock: connection reset".into())),
            MockBehavior::Hang => {
                tokio::time::sleep(StdDuration::from_secs(86_400)).await;
                Ok(vec![])
            }
            MockBehavior::AuthExpired => {
                *self.auth.lock().unwrap() = AuthState::Expired;
                Err(UsageError::Authentication("mock: token expired".into()))
            }
            MockBehavior::PermissionDenied => Err(UsageError::Permission("mock: forbidden".into())),
            MockBehavior::RateLimited(secs) => Err(UsageError::RateLimited {
                retry_after: secs.map(StdDuration::from_secs),
            }),
            MockBehavior::Malformed => Ok(vec![Measurement::available(
                Id::ApiInputTokens,
                self.desc.id.clone(),
                1.0,
                now,
                self.verification(),
            )]),
            MockBehavior::OutOfRange => Ok(vec![Measurement::available(
                Id::LocalContextWindowUtilization,
                self.desc.id.clone(),
                -5.0,
                now,
                self.verification(),
            )]),
            MockBehavior::Unavailable => Err(UsageError::ProviderUnavailable("mock: down".into())),
            MockBehavior::Unsupported => Err(UsageError::UnsupportedCapability("mock: unsupported".into())),
        };
        if res.is_ok() {
            *self.last_ok.lock().unwrap() = Some(now);
            if matches!(*self.auth.lock().unwrap(), AuthState::Expired) {
                *self.auth.lock().unwrap() = AuthState::NotRequired;
            }
        }
        res
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth {
            state: HealthState::Healthy,
            detail: None,
            last_success_at: *self.last_ok.lock().unwrap(),
        }
    }
    fn auth_state(&self) -> AuthState {
        *self.auth.lock().unwrap()
    }
    async fn disconnect(&self) {
        self.disconnects.fetch_add(1, Ordering::SeqCst);
    }
}
