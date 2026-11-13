mod common;
use common::*;

use std::time::Duration as Std;

use usage_core::clock::Clock;
use usage_core::providers::Provider;
use usage_core::errors::ErrorCategory;
use usage_core::events::*;
use usage_core::model::*;
use usage_core::monitoring::{ConnectionState, RunState, SyncResult, Trigger};
use usage_core::providers::mock::{MockBehavior, MockProvider};
use usage_core::providers::RefreshPolicy;
use usage_core::storage::Repository;

fn real(id: &str) -> MockProvider {
    MockProvider::new(id).pretend_real()
}
fn find(snap: &usage_core::monitoring::MonitoringSnapshot, id: MeasurementId) -> Vec<Measurement> {
    snap.measurements.iter().filter(|m| m.id == id).cloned().collect()
}

#[tokio::test]
async fn normal_sync_updates_state_persists_and_emits() {
    let h = harness(real("p")).await;
    assert_eq!(h.engine.sync_once(Trigger::Manual).await.unwrap(), SyncResult::Succeeded);
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::Current);
    assert_eq!(s.last_success_at, Some(t0()));
    assert_eq!(find(&s, MeasurementId::SubscriptionFiveHourUtilization)[0].value.value(), Some(42.0));
    assert!(s.measurements.iter().all(|m| !m.stale && !m.restored));
    // persisted
    let hist = h
        .repo
        .measurement_history("p", MeasurementId::SubscriptionFiveHourUtilization, None, 10)
        .await
        .unwrap();
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].value.value(), Some(42.0));
    // events
    let names = h.sink.names();
    assert!(names.contains(&EVENT_UPDATED) && names.contains(&EVENT_PROVIDER_STATUS_CHANGED));
    assert!(h.sink.events().iter().any(|e| matches!(e, MonitoringEvent::ProviderStatusChanged(p)
        if p.previous == ConnectionState::NotStarted && p.current == ConnectionState::Current)));
}

#[tokio::test]
async fn simulated_provider_is_never_reported_as_current_or_live() {
    let h = harness(MockProvider::new("sim")).await; // mock is simulated by default
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let s = h.engine.snapshot();
    assert!(s.simulated);
    assert_eq!(s.connection, ConnectionState::Simulated);
    let sub = s.coverage.iter().find(|c| c.category == MeasurementCategory::SubscriptionUsage).unwrap();
    assert_eq!(sub.state, CoverageState::Simulated);
    assert!(s.measurements.iter().all(|m| m.simulated));
}

#[tokio::test]
async fn empty_and_missing_fields_become_explicit_unavailable_never_zero() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::Empty);
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.measurements.len(), 4); // one placeholder per declared capability
    for m in &s.measurements {
        assert_eq!(m.value, MeasurementValue::Unavailable { reason: UnavailableReason::NotReported });
    }
    let sub = s.coverage.iter().find(|c| c.category == MeasurementCategory::SubscriptionUsage).unwrap();
    assert_eq!(sub.state, CoverageState::NotConnected);

    h.provider.set_default(MockBehavior::MissingFields(vec![MeasurementId::SubscriptionWeeklyUtilization]));
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let weekly = find(&h.engine.snapshot(), MeasurementId::SubscriptionWeeklyUtilization);
    assert_eq!(weekly.len(), 1);
    assert!(weekly[0].value.value().is_none());
}

#[tokio::test]
async fn old_source_data_is_flagged_stale_and_connection_becomes_stale() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::Stale(3600)); // stale_after is 300s
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::Stale);
    assert!(find(&s, MeasurementId::SubscriptionFiveHourUtilization)[0].stale);
    // stale values must not be written to history as fresh readings
    let hist = h.repo.measurement_history("p", MeasurementId::SubscriptionFiveHourUtilization, None, 10).await.unwrap();
    assert!(hist.is_empty());
}

#[tokio::test]
async fn network_failure_keeps_last_value_marked_stale_and_backs_off() {
    let h = harness(real("p")).await;
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    h.provider.set_default(MockBehavior::NetworkFailure);

    let mut delays = vec![];
    for _ in 0..4 {
        h.clock.advance(secs(3600));
        assert_eq!(h.engine.sync_once(Trigger::Scheduled).await.unwrap(), SyncResult::Failed);
        let s = h.engine.snapshot();
        delays.push((s.next_attempt_at.unwrap() - h.clock.now()).num_seconds());
        assert_eq!(s.connection, ConnectionState::TemporarilyUnavailable);
        let five = find(&s, MeasurementId::SubscriptionFiveHourUtilization);
        assert_eq!(five[0].value.value(), Some(42.0), "last valid reading retained, not zeroed");
        assert!(five[0].stale);
        assert_eq!(s.latest_error.as_ref().unwrap().category, ErrorCategory::Network);
    }
    // 60s base: 60,120,240,480 minus ≤25% jitter → strictly increasing envelope
    assert!(delays[0] >= 45 && delays[0] <= 60, "{delays:?}");
    assert!(delays[3] >= 360 && delays[3] <= 480, "{delays:?}");
    assert!(delays.windows(2).all(|w| w[1] > w[0]));

    // Recovery
    h.provider.set_default(MockBehavior::Success);
    h.engine.sync_once(Trigger::Scheduled).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::Current);
    assert_eq!(s.consecutive_failures, 0);
    assert!(s.latest_error.is_none());
    assert!(!find(&s, MeasurementId::SubscriptionFiveHourUtilization)[0].stale);
}

#[tokio::test]
async fn provider_timeout_is_enforced() {
    let p = MockProvider::with_policy(
        "slow",
        RefreshPolicy {
            min_interval_secs: 1,
            recommended_interval_secs: 60,
            polling_supported: true,
            event_driven: false,
            request_timeout_secs: 1,
            stale_after_secs: 300,
            documented_rate_limit: None,
        },
    )
    .pretend_real()
    .script([MockBehavior::Hang]);
    let h = harness(p).await;
    let t = std::time::Instant::now();
    assert_eq!(h.engine.sync_once(Trigger::Manual).await.unwrap(), SyncResult::Failed);
    assert!(t.elapsed() < Std::from_secs(5));
    let s = h.engine.snapshot();
    assert_eq!(s.latest_error.unwrap().category, ErrorCategory::Timeout);
    assert_eq!(s.connection, ConnectionState::TemporarilyUnavailable);
}

#[tokio::test]
async fn concurrent_syncs_are_deduplicated() {
    let p = MockProvider::with_policy(
        "slow",
        RefreshPolicy {
            min_interval_secs: 1,
            recommended_interval_secs: 60,
            polling_supported: true,
            event_driven: false,
            request_timeout_secs: 1,
            stale_after_secs: 300,
            documented_rate_limit: None,
        },
    )
    .script([MockBehavior::Hang]);
    let h = harness(p).await;
    let e1 = h.engine.clone();
    let first = tokio::spawn(async move { e1.sync_once(Trigger::Scheduled).await });
    tokio::time::sleep(Std::from_millis(150)).await;
    assert_eq!(h.engine.sync_once(Trigger::Manual).await.unwrap(), SyncResult::AlreadyRunning);
    first.await.unwrap().unwrap();
    assert_eq!(h.provider.calls(), 1);
}

#[tokio::test]
async fn auth_expiry_requires_authentication_without_retry_loop_or_crash() {
    let h = harness(real("p")).await;
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    h.provider.set_default(MockBehavior::AuthExpired);
    h.clock.advance(secs(120));
    h.engine.sync_once(Trigger::Scheduled).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::AuthenticationRequired);
    assert_eq!(s.next_attempt_at, None, "no automatic retries after an auth failure");
    assert!(find(&s, MeasurementId::SubscriptionFiveHourUtilization)[0].stale);
    assert!(h.sink.names().contains(&EVENT_AUTHENTICATION_REQUIRED));
    assert_eq!(h.provider.auth_state(), usage_core::providers::AuthState::Expired);

    // user fixes credentials → manual refresh recovers
    h.provider.set_default(MockBehavior::Success);
    h.engine.refresh_now().await.unwrap();
    assert_eq!(h.engine.snapshot().connection, ConnectionState::Current);
}

#[tokio::test]
async fn permission_failure_also_stops_polling() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::PermissionDenied);
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::AuthenticationRequired);
    assert_eq!(s.latest_error.unwrap().category, ErrorCategory::Permission);
    assert!(s.next_attempt_at.is_none());
}

#[tokio::test]
async fn rate_limit_honours_retry_after_and_suspends_requests() {
    let h = harness(real("p")).await;
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    h.provider.set_default(MockBehavior::RateLimited(Some(120)));
    h.clock.advance(secs(60));
    h.engine.sync_once(Trigger::Scheduled).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::RateLimited);
    let until = s.rate_limited_until.unwrap();
    let wait = (until - h.clock.now()).num_seconds();
    assert!((120..=132).contains(&wait), "retry-after is a floor with ≤10% jitter: {wait}");
    assert_eq!(s.next_attempt_at, Some(until));
    assert!(h.sink.events().iter().any(|e| matches!(e, MonitoringEvent::RateLimited(p) if p.retry_after_secs == Some(120))));

    // While backing off: scheduled AND manual refreshes make no request.
    let calls = h.provider.calls();
    h.clock.advance(secs(60));
    assert!(matches!(h.engine.sync_once(Trigger::Scheduled).await.unwrap(), SyncResult::SkippedRateLimited { .. }));
    assert!(matches!(h.engine.refresh_now().await.unwrap(), SyncResult::SkippedRateLimited { .. }));
    assert_eq!(h.provider.calls(), calls);

    // After the retry window it resumes.
    h.provider.set_default(MockBehavior::Success);
    h.clock.advance(secs(120));
    assert_eq!(h.engine.sync_once(Trigger::Scheduled).await.unwrap(), SyncResult::Succeeded);
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::Current);
    assert!(s.rate_limited_until.is_none());
}

#[tokio::test]
async fn rate_limit_without_retry_after_uses_exponential_backoff() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::RateLimited(None));
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let first = (h.engine.snapshot().rate_limited_until.unwrap() - h.clock.now()).num_seconds();
    h.clock.advance(secs(7200));
    h.engine.sync_once(Trigger::Scheduled).await.unwrap();
    let second = (h.engine.snapshot().rate_limited_until.unwrap() - h.clock.now()).num_seconds();
    assert!(first >= 60 && second > first, "{first} {second}");
}

#[tokio::test]
async fn malformed_and_out_of_range_responses_are_rejected_without_panicking() {
    let h = harness(real("p")).await;
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    for b in [MockBehavior::Malformed, MockBehavior::OutOfRange] {
        h.provider.set_default(b);
        h.clock.advance(secs(3600));
        assert_eq!(h.engine.sync_once(Trigger::Scheduled).await.unwrap(), SyncResult::Failed);
        let s = h.engine.snapshot();
        assert_eq!(s.latest_error.as_ref().unwrap().category, ErrorCategory::InvalidResponse);
        // last good data survives
        assert_eq!(find(&s, MeasurementId::SubscriptionFiveHourUtilization)[0].value.value(), Some(42.0));
    }
}

#[tokio::test]
async fn provider_unavailable_and_unsupported_are_distinguished() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::Unavailable);
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::TemporarilyUnavailable);
    assert!(s.next_attempt_at.is_some());

    h.provider.set_default(MockBehavior::Unsupported);
    h.clock.advance(secs(3600));
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let s = h.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::PermanentlyUnsupported);
    assert!(s.next_attempt_at.is_none());
}

#[tokio::test]
async fn unchanged_values_are_not_re_persisted_until_heartbeat() {
    let h = harness(real("p")).await;
    for _ in 0..3 {
        h.engine.sync_once(Trigger::Manual).await.unwrap();
        h.clock.advance(secs(30));
    }
    let id = MeasurementId::SubscriptionWeeklyUtilization;
    // resets_at moves with the clock in the mock, so use a field that is constant:
    let ctx = h.repo.measurement_history("p", MeasurementId::LocalContextWindowUtilization, Some("s1"), 10).await.unwrap();
    assert_eq!(ctx.len(), 1, "constant value written once within the heartbeat");
    h.clock.advance(secs(400)); // > 5 min heartbeat
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let ctx = h.repo.measurement_history("p", MeasurementId::LocalContextWindowUtilization, Some("s1"), 10).await.unwrap();
    assert_eq!(ctx.len(), 2);
    let _ = id;
}

#[tokio::test]
async fn restart_restores_history_as_restored_and_stale_not_fresh() {
    let dir = tempfile::tempdir().unwrap();
    let h = harness_in(dir, real("p")).await;
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let cfg = usage_core::config::AppConfig { refresh_interval_secs: Some(45), minimize_to_tray: false, ..h.engine.config() };
    h.engine.apply_config(cfg).await.unwrap();
    h.engine.shutdown().await;
    let dir = h.dir; // keep the files
    drop(h.engine);

    // "restart": new repo + engine over the same file
    let h2 = harness_in(dir, real("p")).await;
    let loaded = usage_core::config::load(h2.repo.as_ref()).await.unwrap();
    assert_eq!(loaded.refresh_interval_secs, Some(45));
    assert!(!loaded.minimize_to_tray);

    let s = h2.engine.snapshot();
    assert_eq!(s.connection, ConnectionState::NotStarted);
    assert_eq!(s.run_state, RunState::Stopped);
    let five = find(&s, MeasurementId::SubscriptionFiveHourUtilization);
    assert_eq!(five[0].value.value(), Some(42.0));
    assert!(five[0].restored && five[0].stale, "history must not look like a fresh measurement");
    let sub = s.coverage.iter().find(|c| c.category == MeasurementCategory::SubscriptionUsage).unwrap();
    assert_eq!(sub.state, CoverageState::Stale);
}

#[tokio::test]
async fn config_is_validated_and_provider_must_exist() {
    let h = harness(real("p")).await;
    let mut bad = h.engine.config();
    bad.refresh_interval_secs = Some(0);
    assert_eq!(h.engine.apply_config(bad).await.unwrap_err().category(), ErrorCategory::Configuration);
    let mut unknown = h.engine.config();
    unknown.selected_provider = "nope".into();
    assert_eq!(h.engine.apply_config(unknown).await.unwrap_err().category(), ErrorCategory::Configuration);
    // rejected config is not persisted
    assert_eq!(usage_core::config::load(h.repo.as_ref()).await.unwrap().refresh_interval_secs, None);
}

#[tokio::test]
async fn diagnostics_report_reflects_engine_state() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::NetworkFailure);
    h.engine.sync_once(Trigger::Manual).await.unwrap();
    let d = h.engine.diagnostics().await;
    assert!(d.database_initialized);
    assert_eq!(d.schema_version, Some(1));
    assert_eq!(d.active_provider.as_deref(), Some("p"));
    assert_eq!(d.latest_error_category, Some(ErrorCategory::Network));
    assert_eq!(d.connection, ConnectionState::TemporarilyUnavailable);
    let diag = h.repo.recent_diagnostics(5).await.unwrap();
    assert_eq!(diag[0].category, Some(ErrorCategory::Network));
}

// ----- background loop: start/stop/shutdown, independent of any UI -----

fn fast_policy() -> RefreshPolicy {
    RefreshPolicy {
        min_interval_secs: 1,
        recommended_interval_secs: 1,
        polling_supported: true,
        event_driven: false,
        request_timeout_secs: 2,
        stale_after_secs: 300,
        documented_rate_limit: None,
    }
}

async fn loop_harness(p: MockProvider) -> Harness {
    // Real clock so the loop's sleeps and next_attempt_at agree.
    let h = harness(p).await;
    h.clock.set(chrono::Utc::now());
    h
}

#[tokio::test]
async fn loop_polls_without_any_observer_and_stops_cleanly_on_shutdown() {
    let h = loop_harness(MockProvider::with_policy("p", fast_policy()).pretend_real()).await;
    // no frontend/window exists in this test: the engine must keep going by itself
    h.engine.start().await.unwrap();
    assert!(h.engine.is_running().await);
    tokio::time::sleep(Std::from_millis(2600)).await;
    // clock is manual, so advance it as real time passes for next_attempt_at; calls come from the loop
    assert!(h.provider.calls() >= 2, "calls={}", h.provider.calls());
    assert_eq!(h.engine.snapshot().run_state, RunState::Running);

    h.engine.shutdown().await;
    assert!(!h.engine.is_running().await);
    assert_eq!(h.provider.disconnects(), 1, "provider resources released");
    let calls = h.provider.calls();
    tokio::time::sleep(Std::from_millis(1500)).await;
    assert_eq!(h.provider.calls(), calls, "no polling after shutdown");
    let s = h.engine.snapshot();
    assert_eq!(s.run_state, RunState::Stopped);
    assert!(s.measurements.iter().all(|m| m.stale));
    // database closed: further queries fail instead of silently working
    assert!(h.repo.schema_version().await.is_err());
    let names = h.sink.names();
    assert!(names.contains(&EVENT_STARTED) && names.contains(&EVENT_STOPPED));
}

#[tokio::test]
async fn loop_does_not_hammer_a_failing_provider() {
    let h = loop_harness(
        MockProvider::with_policy("p", fast_policy()).pretend_real().script([]),
    )
    .await;
    h.provider.set_default(MockBehavior::NetworkFailure);
    h.engine.start().await.unwrap();
    tokio::time::sleep(Std::from_millis(3500)).await;
    h.engine.stop().await;
    // 1s base with exponential backoff: attempts at ~0, 1, 3(+) → never more than a handful
    assert!(h.provider.calls() <= 4, "calls={}", h.provider.calls());
}

#[tokio::test]
async fn disabling_monitoring_stops_the_loop_and_enabling_restarts_it() {
    let h = loop_harness(MockProvider::with_policy("p", fast_policy()).pretend_real()).await;
    h.engine.start().await.unwrap();
    let mut c = h.engine.config();
    c.monitoring_enabled = false;
    h.engine.apply_config(c.clone()).await.unwrap();
    assert!(!h.engine.is_running().await);
    c.monitoring_enabled = true;
    h.engine.apply_config(c).await.unwrap();
    assert!(h.engine.is_running().await);
    h.engine.shutdown().await;
}

#[tokio::test]
async fn unsupported_provider_refuses_to_start() {
    let h = harness(real("p")).await;
    h.provider.set_default(MockBehavior::Unsupported);
    let err = h.engine.start().await.unwrap_err();
    assert_eq!(err.category(), ErrorCategory::UnsupportedCapability);
    assert_eq!(h.engine.snapshot().connection, ConnectionState::PermanentlyUnsupported);
    assert!(!h.engine.is_running().await);
}

#[test]
fn event_names_are_the_documented_contract() {
    assert_eq!(EVENT_STARTED, "monitoring://started");
    assert_eq!(EVENT_UPDATED, "monitoring://updated");
    assert_eq!(EVENT_PROVIDER_STATUS_CHANGED, "monitoring://provider-status-changed");
    assert_eq!(EVENT_AUTHENTICATION_REQUIRED, "monitoring://authentication-required");
    assert_eq!(EVENT_RATE_LIMITED, "monitoring://rate-limited");
    assert_eq!(EVENT_ERROR, "monitoring://error");
    assert_eq!(EVENT_STOPPED, "monitoring://stopped");
}

