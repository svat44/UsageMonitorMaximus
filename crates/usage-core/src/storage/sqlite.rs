//! SQLite implementation (sqlx). All SQL lives here.

use std::path::Path;
use std::time::Duration as StdDuration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteRow, SqliteSynchronous};
use sqlx::{Row, SqlitePool};

use super::*;
use crate::errors::UsageError;
use crate::model::{MeasurementValue, UnavailableReason, Unit, UsageWindow, Verification};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

pub struct SqliteRepository {
    pool: SqlitePool,
}

impl SqliteRepository {
    /// Open (creating if needed) the database at `path` and apply migrations.
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| UsageError::Database(format!("cannot create data dir: {e}")))?;
        }
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Normal)
            .foreign_keys(true)
            .busy_timeout(StdDuration::from_secs(5));
        let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn row_to_measurement(r: &SqliteRow, restored: bool) -> Result<Measurement> {
    let bad = |what: &str, v: &str| UsageError::Database(format!("corrupt {what} in database: {v}"));
    let id_s: String = r.try_get("measurement_id")?;
    let id = MeasurementId::parse(&id_s).ok_or_else(|| bad("measurement_id", &id_s))?;
    let availability: String = r.try_get("availability")?;
    let value = if availability == "available" {
        MeasurementValue::Available { value: r.try_get::<f64, _>("value")? }
    } else {
        let rs: String = r.try_get::<Option<String>, _>("unavailable_reason")?.unwrap_or_default();
        MeasurementValue::Unavailable {
            reason: UnavailableReason::parse(&rs).ok_or_else(|| bad("unavailable_reason", &rs))?,
        }
    };
    let unit_s: String = r.try_get("unit")?;
    let ver_s: String = r.try_get("verification")?;
    let win: Option<String> = r.try_get("usage_window")?;
    let mut m = Measurement::new(
        id,
        r.try_get::<String, _>("provider_id")?,
        value,
        r.try_get("collected_at")?,
        Verification::parse(&ver_s).ok_or_else(|| bad("verification", &ver_s))?,
    );
    m.unit = Unit::parse(&unit_s).ok_or_else(|| bad("unit", &unit_s))?;
    m.scope = r.try_get("scope")?;
    m.source_at = r.try_get("source_at")?;
    m.simulated = r.try_get::<i64, _>("simulated")? != 0;
    m.estimated = r.try_get::<i64, _>("estimated")? != 0;
    m.resets_at = r.try_get("resets_at")?;
    m.window = win.as_deref().and_then(UsageWindow::parse);
    m.restored = restored;
    m.stale = restored; // history is never a fresh observation
    Ok(m)
}

#[async_trait]
impl Repository for SqliteRepository {
    async fn schema_version(&self) -> Result<i64> {
        let v: Option<i64> = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok(v.unwrap_or(0))
    }

    async fn get_preference(&self, key: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT value FROM app_preferences WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?)
    }

    async fn set_preference(&self, key: &str, value: &str, now: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "INSERT INTO app_preferences (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(value)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn upsert_provider(&self, d: &ProviderDescriptor, now: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "INSERT INTO providers (id, display_name, simulated, first_seen_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET display_name = excluded.display_name, simulated = excluded.simulated",
        )
        .bind(&d.id)
        .bind(&d.display_name)
        .bind(d.simulated as i64)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn record_sync(&self, rec: &SyncRecord) -> Result<i64> {
        let mut tx = self.pool.begin().await?;
        let outcome = match rec.outcome {
            SyncOutcome::Success => "success",
            SyncOutcome::Failure => "failure",
        };
        let run_id = sqlx::query(
            "INSERT INTO sync_runs (provider_id, started_at, finished_at, outcome, error_category, measurement_count)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&rec.provider_id)
        .bind(rec.started_at)
        .bind(rec.finished_at)
        .bind(outcome)
        .bind(rec.error_category.map(|c| c.as_str()))
        .bind(rec.measurement_count as i64)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();

        for m in &rec.measurements {
            let (avail, value, reason) = match m.value {
                MeasurementValue::Available { value } => ("available", Some(value), None),
                MeasurementValue::Unavailable { reason } => ("unavailable", None, Some(reason.as_str())),
            };
            sqlx::query(
                "INSERT INTO measurements (sync_run_id, provider_id, measurement_id, category, scope, availability,
                    value, unavailable_reason, unit, collected_at, source_at, verification, simulated, estimated,
                    resets_at, usage_window)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(run_id)
            .bind(&m.provider_id)
            .bind(m.id.as_str())
            .bind(m.category.as_str())
            .bind(&m.scope)
            .bind(avail)
            .bind(value)
            .bind(reason)
            .bind(m.unit.as_str())
            .bind(m.collected_at)
            .bind(m.source_at)
            .bind(m.verification.as_str())
            .bind(m.simulated as i64)
            .bind(m.estimated as i64)
            .bind(m.resets_at)
            .bind(m.window.map(|w| w.as_str()))
            .execute(&mut *tx)
            .await?;
        }

        match rec.outcome {
            SyncOutcome::Success => {
                sqlx::query("UPDATE providers SET last_success_at = ? WHERE id = ?")
                    .bind(rec.finished_at)
                    .bind(&rec.provider_id)
                    .execute(&mut *tx)
                    .await?;
            }
            SyncOutcome::Failure => {
                sqlx::query("UPDATE providers SET last_error_category = ?, last_error_at = ? WHERE id = ?")
                    .bind(rec.error_category.map(|c| c.as_str()))
                    .bind(rec.finished_at)
                    .bind(&rec.provider_id)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await?;
        Ok(run_id)
    }

    async fn latest_measurements(&self, provider_id: &str, newer_than: DateTime<Utc>) -> Result<Vec<Measurement>> {
        let rows = sqlx::query(
            "SELECT m.* FROM measurements m
             JOIN (SELECT MAX(id) AS mid FROM measurements
                   WHERE provider_id = ? AND collected_at >= ?
                   GROUP BY measurement_id, COALESCE(scope, '')) x ON m.id = x.mid
             ORDER BY m.id",
        )
        .bind(provider_id)
        .bind(newer_than)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(|r| row_to_measurement(r, true)).collect()
    }

    async fn measurement_history(
        &self,
        provider_id: &str,
        id: MeasurementId,
        scope: Option<&str>,
        limit: u32,
    ) -> Result<Vec<Measurement>> {
        let rows = sqlx::query(
            "SELECT * FROM measurements
             WHERE provider_id = ? AND measurement_id = ? AND COALESCE(scope, '') = COALESCE(?, '')
             ORDER BY collected_at DESC, id DESC LIMIT ?",
        )
        .bind(provider_id)
        .bind(id.as_str())
        .bind(scope)
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;
        // History rows are records of the past, not live observations.
        rows.iter().map(|r| row_to_measurement(r, false)).collect()
    }

    async fn record_diagnostic(&self, ev: &DiagnosticEvent) -> Result<()> {
        sqlx::query(
            "INSERT INTO diagnostic_events (occurred_at, level, provider_id, operation, category, message)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(ev.occurred_at)
        .bind(&ev.level)
        .bind(&ev.provider_id)
        .bind(&ev.operation)
        .bind(ev.category.map(|c| c.as_str()))
        .bind(&ev.message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn recent_diagnostics(&self, limit: u32) -> Result<Vec<DiagnosticEvent>> {
        let rows = sqlx::query("SELECT * FROM diagnostic_events ORDER BY occurred_at DESC, id DESC LIMIT ?")
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;
        rows.iter()
            .map(|r| {
                Ok(DiagnosticEvent {
                    occurred_at: r.try_get("occurred_at")?,
                    level: r.try_get("level")?,
                    provider_id: r.try_get("provider_id")?,
                    operation: r.try_get("operation")?,
                    category: r
                        .try_get::<Option<String>, _>("category")?
                        .and_then(|s| ErrorCategory::parse(&s)),
                    message: r.try_get("message")?,
                })
            })
            .collect()
    }

    async fn prune(&self, p: &RetentionPolicy, now: DateTime<Utc>) -> Result<PruneReport> {
        let mut tx = self.pool.begin().await?;
        let m = sqlx::query("DELETE FROM measurements WHERE collected_at < ?")
            .bind(now - p.measurements)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        // Measurements outlive their sync run (FK is ON DELETE SET NULL); they have their own retention.
        let s = sqlx::query("DELETE FROM sync_runs WHERE finished_at < ?")
            .bind(now - p.sync_runs)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        let d = sqlx::query("DELETE FROM diagnostic_events WHERE occurred_at < ?")
            .bind(now - p.diagnostics)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        tx.commit().await?;
        Ok(PruneReport { measurements: m, sync_runs: s, diagnostics: d })
    }

    async fn close(&self) {
        // Checkpoint the WAL so the .sqlite file is self-contained, then close.
        let _ = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)").execute(&self.pool).await;
        self.pool.close().await;
    }
}

