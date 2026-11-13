//! Real (partial) provider: reads what Claude Code's documented status-line interface exposes,
//! via the `usage-monitor-statusline` bridge. See docs/DATA_SOURCE_RESEARCH.md §3.1.
//!
//! Supplies: subscription 5h/7d utilisation % + reset time (subscribers only), per-session
//! context-window %, per-session estimated cost, active-session count.
//! Does NOT supply: absolute remaining quota, plan limits, anything while Claude Code is idle.

use std::path::PathBuf;
use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Duration, TimeZone, Utc};

use super::*;
use crate::model::{MeasurementId as Id, UnavailableReason, Verification};
use crate::statusline_protocol::{self as proto, RateWindow, SnapshotEnvelope};

pub const STATUSLINE_PROVIDER_ID: &str = "claude-code-statusline";
/// A session is "active" if its bridge file was updated within this window.
pub const ACTIVE_SESSION_WINDOW_SECS: i64 = 600;
const PRUNE_AFTER: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

pub struct ClaudeCodeStatusLineProvider {
    desc: ProviderDescriptor,
    dir: PathBuf,
    last_ok: Mutex<Option<DateTime<Utc>>>,
    last_detail: Mutex<Option<String>>,
}

impl ClaudeCodeStatusLineProvider {
    /// `dir` is the bridge output directory (see [`crate::paths::statusline_dir`]).
    pub fn new(dir: PathBuf) -> Self {
        Self {
            desc: ProviderDescriptor {
                id: STATUSLINE_PROVIDER_ID.into(),
                display_name: "Claude Code (status line bridge)".into(),
                description: "Reads Claude Code's documented status-line data. Provides 5-hour and \
                              weekly subscription utilisation % for Pro/Max/Ultra subscribers while a \
                              Claude Code session is active, plus local session information. \
                              No credentials involved."
                    .into(),
                simulated: false,
                capabilities: vec![
                    Id::SubscriptionFiveHourUtilization,
                    Id::SubscriptionWeeklyUtilization,
                    Id::LocalContextWindowUtilization,
                    Id::LocalSessionCostEstimate,
                    Id::LocalActiveSessions,
                ],
                refresh: RefreshPolicy {
                    min_interval_secs: 5,
                    recommended_interval_secs: 30,
                    polling_supported: true,
                    // Claude Code pushes to the bridge on activity; we poll the resulting files.
                    event_driven: true,
                    request_timeout_secs: 10,
                    // Status line only fires on activity: older than this means "last seen".
                    stale_after_secs: 600,
                    documented_rate_limit: None,
                },
            },
            dir,
            last_ok: Mutex::new(None),
            last_detail: Mutex::new(None),
        }
    }

    fn window_measurement(
        id: Id,
        win: Option<&RateWindow>,
        newest: Option<&SnapshotEnvelope>,
        any_snapshot: bool,
        now: DateTime<Utc>,
    ) -> Measurement {
        let v = Verification::OfficialLocalInterface;
        let pid = STATUSLINE_PROVIDER_ID;
        let (Some(win), Some(snap)) = (win, newest) else {
            let reason = if any_snapshot {
                UnavailableReason::NotReportedBySource
            } else {
                UnavailableReason::NoDataYet
            };
            return Measurement::unavailable(id, pid, reason, now, v);
        };
        let Some(pct) = win.used_percentage else {
            return Measurement::unavailable(id, pid, UnavailableReason::NotReportedBySource, now, v);
        };
        let resets_at = win
            .resets_at
            .and_then(|s| Utc.timestamp_opt(s as i64, 0).single());
        // Claude Code drops a window once it resets; after that we know nothing about the new one.
        if resets_at.is_some_and(|r| r <= now) {
            return Measurement::unavailable(id, pid, UnavailableReason::WindowExpired, now, v)
                .with_source_at(snap.received_at);
        }
        let mut m = Measurement::available(id, pid, pct, now, v).with_source_at(snap.received_at);
        if let Some(r) = resets_at {
            m = m.with_resets_at(r);
        }
        m
    }
}

#[async_trait]
impl Provider for ClaudeCodeStatusLineProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.desc
    }

    async fn check_availability(&self) -> Availability {
        Availability::Available
    }

    async fn initialize(&self) -> Result<()> {
        let dir = self.dir.clone();
        tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&dir)
                .map_err(|e| UsageError::Permission(format!("cannot create {}: {e}", dir.display())))?;
            let n = proto::prune_old(&dir, PRUNE_AFTER);
            if n > 0 {
                tracing::debug!(
                    provider = STATUSLINE_PROVIDER_ID,
                    removed = n,
                    "pruned old bridge files"
                );
            }
            Ok(())
        })
        .await
        .map_err(|e| UsageError::Internal(e.to_string()))?
    }

    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<Measurement>> {
        let dir = self.dir.clone();
        let outcome = tokio::task::spawn_blocking(move || proto::read_snapshots(&dir))
            .await
            .map_err(|e| UsageError::Internal(e.to_string()))?
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => {
                    UsageError::Permission(format!("cannot read bridge directory: {e}"))
                }
                _ => UsageError::ProviderUnavailable(format!("cannot read bridge directory: {e}")),
            })?;

        if outcome.snapshots.is_empty() && outcome.malformed > 0 {
            return Err(UsageError::InvalidResponse(format!(
                "{} bridge file(s) present but none could be parsed",
                outcome.malformed
            )));
        }

        let now = ctx.now;
        let v = Verification::OfficialLocalInterface;
        let pid = STATUSLINE_PROVIDER_ID;
        let any = !outcome.snapshots.is_empty();
        let mut snaps = outcome.snapshots;
        snaps.sort_by_key(|s| std::cmp::Reverse(s.received_at));

        // Rate-limit windows are account-wide; the most recent report that carries each one wins.
        let newest_with = |pick: fn(&proto::RateLimits) -> Option<&RateWindow>| {
            snaps.iter().find(|s| {
                s.payload
                    .rate_limits
                    .as_ref()
                    .and_then(pick)
                    .is_some_and(|w| w.used_percentage.is_some())
            })
        };
        let five_snap = newest_with(|r| r.five_hour.as_ref());
        let week_snap = newest_with(|r| r.seven_day.as_ref());
        // If no snapshot carries the window, still pass the newest snapshot's absence info.
        let five = five_snap
            .and_then(|s| s.payload.rate_limits.as_ref())
            .and_then(|r| r.five_hour.as_ref());
        let week = week_snap
            .and_then(|s| s.payload.rate_limits.as_ref())
            .and_then(|r| r.seven_day.as_ref());

        let mut out = vec![
            Self::window_measurement(Id::SubscriptionFiveHourUtilization, five, five_snap, any, now),
            Self::window_measurement(Id::SubscriptionWeeklyUtilization, week, week_snap, any, now),
        ];

        // Local, per-session data for recently active sessions.
        let cutoff = now - Duration::seconds(ACTIVE_SESSION_WINDOW_SECS);
        let active: Vec<&SnapshotEnvelope> = snaps.iter().filter(|s| s.received_at >= cutoff).collect();
        if any {
            out.push(
                Measurement::available(Id::LocalActiveSessions, pid, active.len() as f64, now, v)
                    .with_source_at(snaps[0].received_at),
            );
        } else {
            out.push(Measurement::unavailable(
                Id::LocalActiveSessions,
                pid,
                UnavailableReason::NoDataYet,
                now,
                v,
            ));
        }
        for s in active {
            let scope = proto::sanitize_session_id(s.payload.session_id.as_deref().unwrap_or("unknown"));
            let ctxw = s.payload.context_window.as_ref().and_then(|c| c.used_percentage);
            out.push(
                match ctxw {
                    Some(p) => Measurement::available(Id::LocalContextWindowUtilization, pid, p, now, v),
                    None => Measurement::unavailable(
                        Id::LocalContextWindowUtilization,
                        pid,
                        UnavailableReason::NotReportedBySource,
                        now,
                        v,
                    ),
                }
                .with_scope(scope.clone())
                .with_source_at(s.received_at),
            );
            if let Some(c) = s.payload.cost.as_ref().and_then(|c| c.total_cost_usd) {
                out.push(
                    Measurement::available(Id::LocalSessionCostEstimate, pid, c, now, v)
                        .with_scope(scope)
                        .with_estimated(true)
                        .with_source_at(s.received_at),
                );
            }
        }

        *self.last_ok.lock().unwrap() = Some(now);
        *self.last_detail.lock().unwrap() = Some(if any {
            "receiving status-line data".into()
        } else {
            "waiting for Claude Code: configure the statusLine command (see docs) and send a prompt".into()
        });
        Ok(out)
    }

    async fn health(&self) -> ProviderHealth {
        let last = *self.last_ok.lock().unwrap();
        ProviderHealth {
            state: if last.is_some() {
                HealthState::Healthy
            } else {
                HealthState::Degraded
            },
            detail: self.last_detail.lock().unwrap().clone(),
            last_success_at: last,
        }
    }
    fn auth_state(&self) -> AuthState {
        AuthState::NotRequired
    }
    async fn disconnect(&self) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::MeasurementValue;
    use crate::statusline_protocol::{
        write_snapshot, ContextWindow, Cost, RateLimits, StatusLinePayload, SCHEMA_VERSION,
    };

    fn env(session: &str, at: DateTime<Utc>, rl: Option<RateLimits>) -> SnapshotEnvelope {
        SnapshotEnvelope {
            schema_version: SCHEMA_VERSION,
            received_at: at,
            payload: StatusLinePayload {
                session_id: Some(session.into()),
                context_window: Some(ContextWindow {
                    used_percentage: Some(12.0),
                    context_window_size: Some(200000.0),
                }),
                cost: Some(Cost {
                    total_cost_usd: Some(0.5),
                }),
                rate_limits: rl,
                ..Default::default()
            },
        }
    }
    fn rl(five: f64, week: f64, reset5: DateTime<Utc>, reset7: DateTime<Utc>) -> RateLimits {
        RateLimits {
            five_hour: Some(RateWindow {
                used_percentage: Some(five),
                resets_at: Some(reset5.timestamp() as f64),
            }),
            seven_day: Some(RateWindow {
                used_percentage: Some(week),
                resets_at: Some(reset7.timestamp() as f64),
            }),
        }
    }
    fn find(ms: &[Measurement], id: Id) -> Vec<&Measurement> {
        ms.iter().filter(|m| m.id == id).collect()
    }

    #[tokio::test]
    async fn no_data_is_unavailable_not_zero() {
        let d = tempfile::tempdir().unwrap();
        let p = ClaudeCodeStatusLineProvider::new(d.path().to_path_buf());
        let now = Utc::now();
        let ms = p.fetch(&FetchContext { now }).await.unwrap();
        for m in &ms {
            assert!(
                matches!(
                    m.value,
                    MeasurementValue::Unavailable {
                        reason: UnavailableReason::NoDataYet
                    }
                ),
                "{m:?}"
            );
        }
        crate::providers::validate_measurements(p.descriptor(), &ms).unwrap();
    }

    #[tokio::test]
    async fn reports_documented_fields_with_provenance() {
        let d = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let r5 = now + Duration::hours(2);
        let r7 = now + Duration::days(2);
        write_snapshot(
            d.path(),
            &env("s1", now - Duration::seconds(5), Some(rl(23.5, 41.2, r5, r7))),
        )
        .unwrap();
        let p = ClaudeCodeStatusLineProvider::new(d.path().to_path_buf());
        let ms = p.fetch(&FetchContext { now }).await.unwrap();
        crate::providers::validate_measurements(p.descriptor(), &ms).unwrap();

        let five = &find(&ms, Id::SubscriptionFiveHourUtilization)[0];
        assert_eq!(five.value.value(), Some(23.5));
        assert_eq!(five.resets_at.unwrap().timestamp(), r5.timestamp());
        assert_eq!(five.verification, Verification::OfficialLocalInterface);
        assert!(!five.simulated && !five.estimated);
        assert_eq!(
            five.category,
            crate::model::MeasurementCategory::SubscriptionUsage
        );

        let ctx = &find(&ms, Id::LocalContextWindowUtilization)[0];
        assert_eq!(ctx.category, crate::model::MeasurementCategory::LocalClaudeCode);
        assert_eq!(ctx.scope.as_deref(), Some("s1"));
        let cost = &find(&ms, Id::LocalSessionCostEstimate)[0];
        assert!(cost.estimated);
        assert_eq!(find(&ms, Id::LocalActiveSessions)[0].value.value(), Some(1.0));
    }

    #[tokio::test]
    async fn api_key_user_without_rate_limits_gets_unavailable_subscription() {
        let d = tempfile::tempdir().unwrap();
        let now = Utc::now();
        write_snapshot(d.path(), &env("s1", now, None)).unwrap();
        let p = ClaudeCodeStatusLineProvider::new(d.path().to_path_buf());
        let ms = p.fetch(&FetchContext { now }).await.unwrap();
        let five = &find(&ms, Id::SubscriptionFiveHourUtilization)[0];
        assert_eq!(
            five.value,
            MeasurementValue::Unavailable {
                reason: UnavailableReason::NotReportedBySource
            }
        );
        // local data still flows
        assert!(find(&ms, Id::LocalContextWindowUtilization)[0]
            .value
            .value()
            .is_some());
    }

    #[tokio::test]
    async fn expired_window_is_unavailable_not_stale_percentage() {
        let d = tempfile::tempdir().unwrap();
        let now = Utc::now();
        write_snapshot(
            d.path(),
            &env(
                "s1",
                now - Duration::hours(6),
                Some(rl(99.0, 50.0, now - Duration::hours(1), now + Duration::days(1))),
            ),
        )
        .unwrap();
        let p = ClaudeCodeStatusLineProvider::new(d.path().to_path_buf());
        let ms = p.fetch(&FetchContext { now }).await.unwrap();
        assert_eq!(
            find(&ms, Id::SubscriptionFiveHourUtilization)[0].value,
            MeasurementValue::Unavailable {
                reason: UnavailableReason::WindowExpired
            }
        );
        assert_eq!(
            find(&ms, Id::SubscriptionWeeklyUtilization)[0].value.value(),
            Some(50.0)
        );
        // 6h-old session is not "active"
        assert_eq!(find(&ms, Id::LocalActiveSessions)[0].value.value(), Some(0.0));
        assert!(find(&ms, Id::LocalContextWindowUtilization).is_empty());
    }

    #[tokio::test]
    async fn newest_snapshot_wins_for_account_wide_windows() {
        let d = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let (r5, r7) = (now + Duration::hours(1), now + Duration::days(1));
        write_snapshot(
            d.path(),
            &env("old", now - Duration::minutes(5), Some(rl(10.0, 10.0, r5, r7))),
        )
        .unwrap();
        write_snapshot(
            d.path(),
            &env("new", now - Duration::seconds(10), Some(rl(30.0, 20.0, r5, r7))),
        )
        .unwrap();
        let p = ClaudeCodeStatusLineProvider::new(d.path().to_path_buf());
        let ms = p.fetch(&FetchContext { now }).await.unwrap();
        assert_eq!(
            find(&ms, Id::SubscriptionFiveHourUtilization)[0].value.value(),
            Some(30.0)
        );
        assert_eq!(find(&ms, Id::LocalContextWindowUtilization).len(), 2);
    }

    #[tokio::test]
    async fn only_garbage_files_is_invalid_response() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("x.json"), b"nope").unwrap();
        let p = ClaudeCodeStatusLineProvider::new(d.path().to_path_buf());
        let err = p.fetch(&FetchContext { now: Utc::now() }).await.unwrap_err();
        assert_eq!(err.category(), crate::errors::ErrorCategory::InvalidResponse);
    }
}
