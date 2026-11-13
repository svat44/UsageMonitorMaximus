//! The monitoring engine: provider-agnostic scheduler + state owner.
//!
//! Invariants (each covered by tests in `tests/engine.rs`):
//! * At most one sync runs at a time per engine (`sync_lock`).
//! * Every provider call is bounded by the provider's `request_timeout_secs`.
//! * A failed refresh never discards data: measurements are kept and marked `stale`.
//! * Auth/permission/unsupported failures stop polling (no retry loop); other failures back off
//!   exponentially with jitter; a rate-limit `Retry-After` is honoured as a floor.
//! * Simulated providers are reported as `Simulated`, never `Current`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::clock::Clock;
use crate::config::{AppConfig, PREF_KEY_CONFIG};
use crate::diagnostics::DiagnosticsReport;
use crate::errors::{ErrorInfo, Result, UsageError};
use crate::events::*;
use crate::model::{compute_coverage, Measurement, MeasurementKey, UnavailableReason};
use crate::providers::{validate_measurements, FetchContext, Provider, ProviderDescriptor, ProviderRegistry};
use crate::storage::{DiagnosticEvent, Repository, RetentionPolicy, SyncOutcome, SyncRecord};

use super::backoff::BackoffPolicy;
use super::state::{ConnectionState, MonitoringSnapshot, RunState};

/// Unchanged values are re-persisted at most this often (history heartbeat).
const PERSIST_HEARTBEAT: ChronoDuration = ChronoDuration::minutes(5);
/// Restored history older than this is not shown at startup.
const RESTORE_WINDOW: ChronoDuration = ChronoDuration::days(7);
const PRUNE_EVERY_N_SYNCS: u64 = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Scheduled,
    Manual,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(tag = "result", rename_all = "snake_case", rename_all_fields = "camelCase")]
#[ts(export)]
pub enum SyncResult {
    Succeeded,
    Failed,
    /// Another sync was in progress; nothing was done.
    AlreadyRunning,
    /// Provider is rate limited; no request was made.
    SkippedRateLimited { resume_at: DateTime<Utc> },
}

pub struct EngineDeps {
    pub registry: ProviderRegistry,
    pub repo: Arc<dyn Repository>,
    pub events: Arc<dyn EventSink>,
    pub clock: Arc<dyn Clock>,
    pub config: AppConfig,
    pub data_dir: PathBuf,
    pub retention: RetentionPolicy,
}

struct Runner {
    cancel: CancellationToken,
    handle: JoinHandle<()>,
}

struct Inner {
    registry: ProviderRegistry,
    repo: Arc<dyn Repository>,
    events: Arc<dyn EventSink>,
    clock: Arc<dyn Clock>,
    data_dir: PathBuf,
    retention: RetentionPolicy,
    config: RwLock<AppConfig>,
    state: RwLock<MonitoringSnapshot>,
    sync_lock: tokio::sync::Mutex<()>,
    runner: tokio::sync::Mutex<Option<Runner>>,
    wake: Notify,
    /// last persisted (value, time) per measurement, for change-based de-duplication
    persisted: Mutex<HashMap<MeasurementKey, (Measurement, DateTime<Utc>)>>,
    initialized_provider: Mutex<Option<String>>,
    activated_provider: Mutex<Option<String>>,
    last_error_ever: Mutex<Option<ErrorInfo>>,
    sync_counter: std::sync::atomic::AtomicU64,
}

#[derive(Clone)]
pub struct MonitoringEngine {
    inner: Arc<Inner>,
}

impl MonitoringEngine {
    pub fn new(deps: EngineDeps) -> Self {
        let mut st = MonitoringSnapshot::empty();
        st.active_provider = Some(deps.config.selected_provider.clone());
        if let Some(p) = deps.registry.get(&deps.config.selected_provider) {
            st.simulated = p.descriptor().simulated;
        }
        Self {
            inner: Arc::new(Inner {
                registry: deps.registry,
                repo: deps.repo,
                events: deps.events,
                clock: deps.clock,
                data_dir: deps.data_dir,
                retention: deps.retention,
                config: RwLock::new(deps.config),
                state: RwLock::new(st),
                sync_lock: tokio::sync::Mutex::new(()),
                runner: tokio::sync::Mutex::new(None),
                wake: Notify::new(),
                persisted: Mutex::new(HashMap::new()),
                initialized_provider: Mutex::new(None),
                activated_provider: Mutex::new(None),
                last_error_ever: Mutex::new(None),
                sync_counter: Default::default(),
            }),
        }
    }

    // ---------------------------------------------------------------- accessors

    pub fn snapshot(&self) -> MonitoringSnapshot {
        self.inner.state.read().unwrap().clone()
    }
    pub fn config(&self) -> AppConfig {
        self.inner.config.read().unwrap().clone()
    }
    pub fn providers(&self) -> Vec<ProviderDescriptor> {
        self.inner.registry.descriptors()
    }
    pub fn repository(&self) -> Arc<dyn Repository> {
        self.inner.repo.clone()
    }

    fn active_provider(&self) -> Result<Arc<dyn Provider>> {
        let id = self.inner.config.read().unwrap().selected_provider.clone();
        self.inner
            .registry
            .get(&id)
            .ok_or_else(|| UsageError::Configuration(format!("unknown provider '{id}'")))
    }

    fn update<R>(&self, f: impl FnOnce(&mut MonitoringSnapshot) -> R) -> R {
        let mut g = self.inner.state.write().unwrap();
        f(&mut g)
    }

    fn emit(&self, e: MonitoringEvent) {
        self.inner.events.emit(&e);
    }

    fn effective_interval(&self, desc: &ProviderDescriptor) -> Duration {
        let secs = self.inner.config.read().unwrap().effective_interval_secs(&desc.refresh);
        Duration::from_secs(secs as u64)
    }

    // ---------------------------------------------------------------- lifecycle

    /// Register providers in the DB, prune old history, and load persisted history for the
    /// selected provider (shown as `restored`/`stale`). Safe to call repeatedly.
    pub async fn initialize(&self) -> Result<()> {
        let now = self.inner.clock.now();
        for d in self.inner.registry.descriptors() {
            self.inner.repo.upsert_provider(&d, now).await?;
        }
        if let Err(e) = self.inner.repo.prune(&self.inner.retention, now).await {
            tracing::warn!(operation = "prune", category = e.category().as_str(), "retention prune failed: {e}");
        }
        self.activate_provider().await
    }

    async fn activate_provider(&self) -> Result<()> {
        let provider = self.active_provider()?;
        let desc = provider.descriptor().clone();
        {
            let mut a = self.inner.activated_provider.lock().unwrap();
            if a.as_deref() == Some(desc.id.as_str()) {
                return Ok(());
            }
            *a = Some(desc.id.clone());
        }
        *self.inner.initialized_provider.lock().unwrap() = None;
        let now = self.inner.clock.now();
        let restored = match self.inner.repo.latest_measurements(&desc.id, now - RESTORE_WINDOW).await {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(provider = %desc.id, operation = "restore", category = e.category().as_str(), "{e}");
                vec![]
            }
        };
        {
            let mut p = self.inner.persisted.lock().unwrap();
            p.clear();
            for m in &restored {
                p.insert(m.key(), (m.clone(), m.collected_at));
            }
        }
        self.update(|s| {
            let was = s.run_state;
            *s = MonitoringSnapshot::empty();
            s.run_state = was;
            s.active_provider = Some(desc.id.clone());
            s.simulated = desc.simulated;
            s.coverage = compute_coverage(&restored);
            s.last_success_at = restored.iter().map(|m| m.collected_at).max();
            s.measurements = restored;
        });
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        let mut runner = self.inner.runner.lock().await;
        if runner.is_some() {
            return Ok(());
        }
        self.activate_provider().await?;
        let provider = self.active_provider()?;
        let desc = provider.descriptor().clone();

        if let crate::providers::Availability::Unsupported { reason } = provider.check_availability().await {
            let err = UsageError::UnsupportedCapability(reason);
            let info = err.to_info(Some(&desc.id), "start", self.inner.clock.now());
            self.set_connection(ConnectionState::PermanentlyUnsupported);
            self.update(|s| s.latest_error = Some(info.clone()));
            self.emit(MonitoringEvent::Error(info));
            self.emit(MonitoringEvent::Updated(self.snapshot()));
            return Err(err);
        }

        let cancel = CancellationToken::new();
        self.update(|s| s.run_state = RunState::Running);
        let this = self.clone();
        let token = cancel.clone();
        let handle = tokio::spawn(async move { this.run_loop(token).await });
        *runner = Some(Runner { cancel, handle });
        tracing::info!(provider = %desc.id, operation = "start", "monitoring started");
        self.emit(MonitoringEvent::Started(LifecyclePayload {
            at: self.inner.clock.now(),
            provider_id: Some(desc.id),
        }));
        self.emit(MonitoringEvent::Updated(self.snapshot()));
        Ok(())
    }

    pub async fn stop(&self) {
        let runner = self.inner.runner.lock().await.take();
        let Some(r) = runner else { return };
        r.cancel.cancel();
        let _ = r.handle.await;
        // Wait for an in-flight manual refresh (bounded by the provider timeout).
        let _g = self.inner.sync_lock.lock().await;
        let pid = self.snapshot().active_provider;
        if let Some(p) = pid.as_deref().and_then(|id| self.inner.registry.get(id)) {
            p.disconnect().await;
        }
        *self.inner.initialized_provider.lock().unwrap() = None;
        self.update(|s| {
            s.run_state = RunState::Stopped;
            s.next_attempt_at = None;
            for m in &mut s.measurements {
                m.stale = true;
            }
            s.coverage = compute_coverage(&s.measurements);
            s.connection = ConnectionState::NotStarted;
        });
        tracing::info!(operation = "stop", "monitoring stopped");
        self.emit(MonitoringEvent::Stopped(LifecyclePayload { at: self.inner.clock.now(), provider_id: pid }));
        self.emit(MonitoringEvent::Updated(self.snapshot()));
    }

    /// Stop everything and close the database. Call once on application exit.
    pub async fn shutdown(&self) {
        self.stop().await;
        self.inner.repo.close().await;
        tracing::info!(operation = "shutdown", "engine shut down");
    }

    pub async fn is_running(&self) -> bool {
        self.inner.runner.lock().await.is_some()
    }

    // ---------------------------------------------------------------- configuration

    /// Validate, persist and apply a new configuration, restarting the loop if needed.
    pub async fn apply_config(&self, new: AppConfig) -> Result<AppConfig> {
        let provider = self
            .inner
            .registry
            .get(&new.selected_provider)
            .ok_or_else(|| UsageError::Configuration(format!("unknown provider '{}'", new.selected_provider)))?;
        new.validate(&provider.descriptor().refresh)?;

        let json = serde_json::to_string(&new).map_err(|e| UsageError::Configuration(e.to_string()))?;
        self.inner.repo.set_preference(PREF_KEY_CONFIG, &json, self.inner.clock.now()).await?;

        let old = std::mem::replace(&mut *self.inner.config.write().unwrap(), new.clone());
        let was_running = self.is_running().await;
        let provider_changed = old.selected_provider != new.selected_provider;

        if was_running && (provider_changed || !new.monitoring_enabled) {
            self.stop().await;
        }
        if provider_changed {
            self.activate_provider().await?;
            self.emit(MonitoringEvent::Updated(self.snapshot()));
        }
        if new.monitoring_enabled && !self.is_running().await {
            // A provider that is unsupported reports through events; config is still saved.
            if let Err(e) = self.start().await {
                tracing::warn!(operation = "apply_config", category = e.category().as_str(), "start failed: {e}");
            }
        } else if old.refresh_interval_secs != new.refresh_interval_secs {
            let interval = self.effective_interval(provider.descriptor());
            self.update(|s| {
                if matches!(
                    s.connection,
                    ConnectionState::Current | ConnectionState::Stale | ConnectionState::Simulated
                ) {
                    if let Some(last) = s.last_attempt_at {
                        s.next_attempt_at = Some(last + ChronoDuration::from_std(interval).unwrap_or_default());
                    }
                }
            });
            self.inner.wake.notify_one();
        }
        Ok(new)
    }

    // ---------------------------------------------------------------- scheduling loop

    async fn run_loop(self, cancel: CancellationToken) {
        loop {
            let res = tokio::select! {
                _ = cancel.cancelled() => return,
                r = self.sync_once(Trigger::Scheduled) => r,
            };
            if matches!(res, Ok(SyncResult::AlreadyRunning)) || res.is_err() {
                // Don't spin: something else is syncing, or the provider is misconfigured.
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = tokio::time::sleep(Duration::from_secs(1)) => {}
                }
                if res.is_err() {
                    tokio::select! {
                        _ = cancel.cancelled() => return,
                        _ = tokio::time::sleep(Duration::from_secs(30)) => {}
                    }
                }
                continue;
            }
            loop {
                let wait = {
                    let st = self.inner.state.read().unwrap();
                    st.next_attempt_at.map(|t| (t - self.inner.clock.now()).to_std().unwrap_or(Duration::ZERO))
                };
                tokio::select! {
                    _ = cancel.cancelled() => return,
                    _ = async {
                        match wait {
                            Some(d) => tokio::time::sleep(d).await,
                            None => std::future::pending::<()>().await,
                        }
                    } => break,
                    _ = self.inner.wake.notified() => continue,
                }
            }
        }
    }

    /// User-initiated refresh (also usable to retry after `AuthenticationRequired`).
    /// Never bypasses an active rate-limit backoff.
    pub async fn refresh_now(&self) -> Result<SyncResult> {
        let r = self.sync_once(Trigger::Manual).await;
        self.inner.wake.notify_one();
        r
    }

    /// Run exactly one synchronisation. Public so tests (and `refresh_now`) can drive it directly.
    pub async fn sync_once(&self, trigger: Trigger) -> Result<SyncResult> {
        let Ok(_guard) = self.inner.sync_lock.try_lock() else {
            tracing::debug!(operation = "sync", "skipped: sync already in progress");
            return Ok(SyncResult::AlreadyRunning);
        };
        let provider = self.active_provider()?;
        let desc = provider.descriptor().clone();
        let started = self.inner.clock.now();

        if let Some(until) = self.snapshot().rate_limited_until.filter(|u| *u > started) {
            tracing::debug!(provider = %desc.id, operation = "sync", "skipped: rate limited");
            return Ok(SyncResult::SkippedRateLimited { resume_at: until });
        }

        self.update(|s| s.last_attempt_at = Some(started));
        let timeout = Duration::from_secs(desc.refresh.request_timeout_secs.max(1) as u64);
        let ctx = FetchContext { now: started };
        let outcome: Result<Vec<Measurement>> = match tokio::time::timeout(timeout, async {
            self.ensure_initialized(&provider).await?;
            let ms = provider.fetch(&ctx).await?;
            validate_measurements(&desc, &ms)?;
            Ok(ms)
        })
        .await
        {
            Ok(r) => r,
            Err(_) => Err(UsageError::Timeout(timeout)),
        };
        let finished = self.inner.clock.now();

        let ok = match outcome {
            Ok(ms) => {
                self.on_success(&desc, ms, started, finished).await;
                true
            }
            Err(e) => {
                self.on_failure(&desc, e, started, finished, trigger).await;
                false
            }
        };

        let n = self.inner.sync_counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n % PRUNE_EVERY_N_SYNCS == 0 {
            let _ = self.inner.repo.prune(&self.inner.retention, finished).await;
        }
        Ok(if ok { SyncResult::Succeeded } else { SyncResult::Failed })
    }

    async fn ensure_initialized(&self, p: &Arc<dyn Provider>) -> Result<()> {
        let id = p.descriptor().id.clone();
        if self.inner.initialized_provider.lock().unwrap().as_deref() == Some(id.as_str()) {
            return Ok(());
        }
        p.initialize().await?;
        *self.inner.initialized_provider.lock().unwrap() = Some(id);
        Ok(())
    }

    fn set_connection(&self, c: ConnectionState) -> Option<ConnectionState> {
        self.update(|s| {
            let prev = s.connection;
            s.connection = c;
            (prev != c).then_some(prev)
        })
    }

    // ---------------------------------------------------------------- outcomes

    async fn on_success(&self, desc: &ProviderDescriptor, mut ms: Vec<Measurement>, started: DateTime<Utc>, now: DateTime<Utc>) {
        let stale_after = ChronoDuration::seconds(desc.refresh.stale_after_secs as i64);
        for m in &mut ms {
            m.stale = m.source_at.is_some_and(|src| now - src > stale_after);
        }
        // Explicit "unavailable" for every declared capability the provider did not mention.
        for cap in &desc.capabilities {
            if !ms.iter().any(|m| m.id == *cap) {
                let v = if desc.simulated {
                    crate::model::Verification::Simulated
                } else {
                    crate::model::Verification::OfficialLocalInterface
                };
                let mut m = Measurement::unavailable(*cap, desc.id.clone(), UnavailableReason::NotReported, now, v);
                m.simulated = desc.simulated;
                ms.push(m);
            }
        }
        let order = |m: &Measurement| crate::model::MeasurementId::ALL.iter().position(|x| *x == m.id).unwrap_or(99);
        ms.sort_by(|a, b| order(a).cmp(&order(b)).then_with(|| a.scope.cmp(&b.scope)));

        let any_available = ms.iter().any(|m| m.value.value().is_some());
        let all_available_stale = any_available && ms.iter().filter(|m| m.value.value().is_some()).all(|m| m.stale);
        let conn = if desc.simulated {
            ConnectionState::Simulated
        } else if all_available_stale {
            ConnectionState::Stale
        } else {
            ConnectionState::Current
        };
        let interval = self.effective_interval(desc);
        let next = now + ChronoDuration::from_std(interval).unwrap_or_default();

        // Persist (best effort: a storage problem must not stop monitoring).
        let to_persist = self.select_for_persistence(&ms, now);
        let rec = SyncRecord {
            provider_id: desc.id.clone(),
            started_at: started,
            finished_at: now,
            outcome: SyncOutcome::Success,
            error_category: None,
            measurements: to_persist.clone(),
            measurement_count: ms.len() as u32,
        };
        match self.inner.repo.record_sync(&rec).await {
            Ok(_) => {
                let mut p = self.inner.persisted.lock().unwrap();
                for m in to_persist {
                    p.insert(m.key(), (m, now));
                }
            }
            Err(e) => self.persistence_failed(&desc.id, "record_sync", e).await,
        }

        let prev = self.update(|s| {
            let prev = s.connection;
            s.measurements = ms;
            s.coverage = compute_coverage(&s.measurements);
            s.connection = conn;
            s.simulated = desc.simulated;
            s.last_success_at = Some(now);
            s.next_attempt_at = Some(next);
            s.rate_limited_until = None;
            s.consecutive_failures = 0;
            s.latest_error = None;
            prev
        });
        tracing::debug!(provider = %desc.id, operation = "sync", "sync succeeded");
        if prev != conn {
            self.emit(MonitoringEvent::ProviderStatusChanged(ProviderStatusChanged {
                provider_id: desc.id.clone(),
                previous: prev,
                current: conn,
            }));
        }
        self.emit(MonitoringEvent::Updated(self.snapshot()));
    }

    fn select_for_persistence(&self, ms: &[Measurement], now: DateTime<Utc>) -> Vec<Measurement> {
        let p = self.inner.persisted.lock().unwrap();
        ms.iter()
            .filter(|m| !m.stale) // never record a stale value as a fresh reading
            .filter(|m| match p.get(&m.key()) {
                None => true,
                Some((prev, at)) => {
                    prev.value != m.value || prev.resets_at != m.resets_at || now - *at >= PERSIST_HEARTBEAT
                }
            })
            .cloned()
            .collect()
    }

    async fn on_failure(&self, desc: &ProviderDescriptor, err: UsageError, started: DateTime<Utc>, now: DateTime<Utc>, _t: Trigger) {
        use crate::errors::ErrorCategory as C;
        let info = err.to_info(Some(&desc.id), "fetch", now);
        // Log category, never the raw payload; messages are provider-authored and secret-free.
        tracing::warn!(provider = %desc.id, operation = "fetch", category = info.category.as_str(), "sync failed: {err}");

        let failures = self.update(|s| {
            s.consecutive_failures = s.consecutive_failures.saturating_add(1);
            s.consecutive_failures
        });
        let interval = self.effective_interval(desc);
        let backoff = BackoffPolicy::for_interval(interval);
        let jitter = fastrand::f64();

        let (conn, next, rate_until) = match err.category() {
            C::RateLimited => {
                let d = backoff.rate_limit_delay(err.retry_after(), failures, jitter);
                let until = now + ChronoDuration::from_std(d).unwrap_or_default();
                (ConnectionState::RateLimited, Some(until), Some(until))
            }
            C::Authentication | C::Permission => (ConnectionState::AuthenticationRequired, None, None),
            C::UnsupportedCapability => (ConnectionState::PermanentlyUnsupported, None, None),
            _ => {
                let d = backoff.delay(failures, jitter);
                (ConnectionState::TemporarilyUnavailable, Some(now + ChronoDuration::from_std(d).unwrap_or_default()), None)
            }
        };

        let rec = SyncRecord {
            provider_id: desc.id.clone(),
            started_at: started,
            finished_at: now,
            outcome: SyncOutcome::Failure,
            error_category: Some(info.category),
            measurements: vec![],
            measurement_count: 0,
        };
        if let Err(e) = self.inner.repo.record_sync(&rec).await {
            self.persistence_failed(&desc.id, "record_sync", e).await;
        }
        let _ = self
            .inner
            .repo
            .record_diagnostic(&DiagnosticEvent {
                occurred_at: now,
                level: "warn".into(),
                provider_id: Some(desc.id.clone()),
                operation: "fetch".into(),
                category: Some(info.category),
                message: info.message.clone(),
            })
            .await;
        *self.inner.last_error_ever.lock().unwrap() = Some(info.clone());

        let prev = self.update(|s| {
            let prev = s.connection;
            for m in &mut s.measurements {
                m.stale = true; // retain last valid reading, but say it is old
            }
            s.coverage = compute_coverage(&s.measurements);
            s.connection = conn;
            s.next_attempt_at = next;
            s.rate_limited_until = rate_until;
            s.latest_error = Some(info.clone());
            prev
        });

        match conn {
            ConnectionState::RateLimited => self.emit(MonitoringEvent::RateLimited(RateLimitedPayload {
                provider_id: desc.id.clone(),
                retry_after_secs: info.retry_after_secs,
                resume_at: rate_until.unwrap_or(now),
            })),
            ConnectionState::AuthenticationRequired => {
                self.emit(MonitoringEvent::AuthenticationRequired(AuthenticationRequiredPayload {
                    provider_id: desc.id.clone(),
                    error: info.clone(),
                }))
            }
            _ => {}
        }
        self.emit(MonitoringEvent::Error(info));
        if prev != conn {
            self.emit(MonitoringEvent::ProviderStatusChanged(ProviderStatusChanged {
                provider_id: desc.id.clone(),
                previous: prev,
                current: conn,
            }));
        }
        self.emit(MonitoringEvent::Updated(self.snapshot()));
    }

    async fn persistence_failed(&self, provider_id: &str, op: &str, e: UsageError) {
        tracing::error!(provider = provider_id, operation = op, category = e.category().as_str(), "persistence failed: {e}");
        let info = e.to_info(Some(provider_id), op, self.inner.clock.now());
        *self.inner.last_error_ever.lock().unwrap() = Some(info.clone());
        self.emit(MonitoringEvent::Error(info));
    }

    // ---------------------------------------------------------------- diagnostics

    pub async fn diagnostics(&self) -> DiagnosticsReport {
        let snap = self.snapshot();
        let (schema, db_ok) = match self.inner.repo.schema_version().await {
            Ok(v) => (Some(v as i32), v > 0),
            Err(_) => (None, false),
        };
        let health = match snap.active_provider.as_deref().and_then(|id| self.inner.registry.get(id)) {
            Some(p) => Some(p.health().await),
            None => None,
        };
        DiagnosticsReport {
            app_version: env!("CARGO_PKG_VERSION").into(),
            database_initialized: db_ok,
            schema_version: schema,
            active_provider: snap.active_provider,
            provider_health: health,
            run_state: snap.run_state,
            connection: snap.connection,
            last_success_at: snap.last_success_at,
            latest_error_category: self.inner.last_error_ever.lock().unwrap().as_ref().map(|e| e.category),
            data_dir: self.inner.data_dir.display().to_string(),
        }
    }
}
