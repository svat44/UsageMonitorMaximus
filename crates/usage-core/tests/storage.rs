use chrono::{Duration, TimeZone, Utc};
use usage_core::errors::ErrorCategory;
use usage_core::model::*;
use usage_core::providers::{simulated::SimulatedProvider, Provider};
use usage_core::storage::sqlite::SqliteRepository;
use usage_core::storage::*;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap()
}

async fn repo() -> (SqliteRepository, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let r = SqliteRepository::open(&d.path().join("db.sqlite")).await.unwrap();
    let sim = SimulatedProvider::new();
    r.upsert_provider(sim.descriptor(), now()).await.unwrap();
    (r, d)
}

fn m(id: MeasurementId, v: f64, at: chrono::DateTime<Utc>) -> Measurement {
    Measurement::available(id, "simulated", v, at, Verification::Simulated)
}

fn rec(ms: Vec<Measurement>, at: chrono::DateTime<Utc>) -> SyncRecord {
    SyncRecord {
        provider_id: "simulated".into(),
        started_at: at,
        finished_at: at,
        outcome: SyncOutcome::Success,
        error_category: None,
        measurement_count: ms.len() as u32,
        measurements: ms,
    }
}

#[tokio::test]
async fn migrations_apply_and_schema_version_reported() {
    let (r, _d) = repo().await;
    assert_eq!(r.schema_version().await.unwrap(), 1);
}

#[tokio::test]
async fn reopening_an_existing_database_is_idempotent() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("x.sqlite");
    let r = SqliteRepository::open(&p).await.unwrap();
    r.set_preference("k", "\"v\"", now()).await.unwrap();
    r.close().await;
    let r2 = SqliteRepository::open(&p).await.unwrap();
    assert_eq!(r2.get_preference("k").await.unwrap().as_deref(), Some("\"v\""));
    assert_eq!(r2.schema_version().await.unwrap(), 1);
}

#[tokio::test]
async fn preferences_upsert() {
    let (r, _d) = repo().await;
    r.set_preference("a", "1", now()).await.unwrap();
    r.set_preference("a", "2", now()).await.unwrap();
    assert_eq!(r.get_preference("a").await.unwrap().as_deref(), Some("2"));
    assert_eq!(r.get_preference("missing").await.unwrap(), None);
}

#[tokio::test]
async fn unavailable_round_trips_as_null_not_zero() {
    let (r, _d) = repo().await;
    let un = Measurement::unavailable(
        MeasurementId::SubscriptionFiveHourUtilization,
        "simulated",
        UnavailableReason::WindowExpired,
        now(),
        Verification::Simulated,
    );
    r.record_sync(&rec(vec![un], now())).await.unwrap();
    let raw: Option<f64> = sqlx_value(&r).await;
    assert_eq!(raw, None, "stored as NULL");
    let h = r
        .measurement_history(
            "simulated",
            MeasurementId::SubscriptionFiveHourUtilization,
            None,
            5,
        )
        .await
        .unwrap();
    assert_eq!(
        h[0].value,
        MeasurementValue::Unavailable {
            reason: UnavailableReason::WindowExpired
        }
    );
}

async fn sqlx_value(r: &SqliteRepository) -> Option<f64> {
    sqlx_row(r).await
}
async fn sqlx_row(r: &SqliteRepository) -> Option<f64> {
    // direct SQL through the public pool accessor
    use sqlx::Row;
    sqlx::query("SELECT value FROM measurements LIMIT 1")
        .fetch_one(r.pool())
        .await
        .unwrap()
        .try_get::<Option<f64>, _>(0)
        .unwrap()
}

#[tokio::test]
async fn check_constraint_rejects_unavailable_row_with_a_value() {
    let (r, _d) = repo().await;
    let res = sqlx::query(
        "INSERT INTO measurements (provider_id, measurement_id, category, availability, value, unit, collected_at, verification, simulated, estimated)
         VALUES ('simulated','api_input_tokens','api_consumption','unavailable',0.0,'tokens','2026-06-01T12:00:00Z','simulated',1,0)",
    )
    .execute(r.pool())
    .await;
    assert!(res.is_err(), "a zero cannot stand in for 'unavailable'");
}

#[tokio::test]
async fn sync_record_is_transactional() {
    let (r, _d) = repo().await;
    // second measurement violates the FK (unknown provider) → whole record must roll back
    let good = m(MeasurementId::ApiInputTokens, 10.0, now());
    let mut bad = m(MeasurementId::ApiOutputTokens, 5.0, now());
    bad.provider_id = "does-not-exist".into();
    let err = r.record_sync(&rec(vec![good, bad], now())).await.unwrap_err();
    assert_eq!(err.category(), ErrorCategory::Database);
    let runs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_runs")
        .fetch_one(r.pool())
        .await
        .unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM measurements")
        .fetch_one(r.pool())
        .await
        .unwrap();
    assert_eq!((runs, rows), (0, 0));
}

#[tokio::test]
async fn latest_measurements_returns_newest_per_key_marked_restored_and_stale() {
    let (r, _d) = repo().await;
    r.record_sync(&rec(vec![m(MeasurementId::ApiInputTokens, 1.0, now())], now()))
        .await
        .unwrap();
    let later = now() + Duration::minutes(10);
    r.record_sync(&rec(
        vec![
            m(MeasurementId::ApiInputTokens, 2.0, later),
            m(MeasurementId::LocalActiveSessions, 3.0, later).with_scope("a"),
        ],
        later,
    ))
    .await
    .unwrap();
    let latest = r
        .latest_measurements("simulated", now() - Duration::days(1))
        .await
        .unwrap();
    assert_eq!(latest.len(), 2);
    let tokens = latest
        .iter()
        .find(|x| x.id == MeasurementId::ApiInputTokens)
        .unwrap();
    assert_eq!(tokens.value.value(), Some(2.0));
    assert!(latest.iter().all(|x| x.restored && x.stale));
    // too old → not restored
    assert!(r
        .latest_measurements("simulated", later + Duration::days(1))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn history_is_newest_first_and_not_flagged_restored() {
    let (r, _d) = repo().await;
    for i in 0..3 {
        let t = now() + Duration::minutes(i);
        r.record_sync(&rec(vec![m(MeasurementId::ApiInputTokens, i as f64, t)], t))
            .await
            .unwrap();
    }
    let h = r
        .measurement_history("simulated", MeasurementId::ApiInputTokens, None, 2)
        .await
        .unwrap();
    assert_eq!(
        h.iter().map(|x| x.value.value().unwrap()).collect::<Vec<_>>(),
        vec![2.0, 1.0]
    );
    assert!(h.iter().all(|x| !x.restored));
}

#[tokio::test]
async fn retention_prunes_each_table_by_its_own_window() {
    let (r, _d) = repo().await;
    let old = now() - Duration::days(100);
    let mid = now() - Duration::days(40);
    r.record_sync(&rec(vec![m(MeasurementId::ApiInputTokens, 1.0, old)], old))
        .await
        .unwrap();
    r.record_sync(&rec(vec![m(MeasurementId::ApiInputTokens, 2.0, mid)], mid))
        .await
        .unwrap();
    r.record_sync(&rec(vec![m(MeasurementId::ApiInputTokens, 3.0, now())], now()))
        .await
        .unwrap();
    r.record_diagnostic(&DiagnosticEvent {
        occurred_at: mid,
        level: "warn".into(),
        provider_id: None,
        operation: "x".into(),
        category: None,
        message: "old".into(),
    })
    .await
    .unwrap();
    let rep = r.prune(&RetentionPolicy::default(), now()).await.unwrap();
    assert_eq!(
        rep,
        PruneReport {
            measurements: 1,
            sync_runs: 2,
            diagnostics: 1
        }
    );
    // the 40-day-old measurement outlives its (30-day) sync run
    let h = r
        .measurement_history("simulated", MeasurementId::ApiInputTokens, None, 10)
        .await
        .unwrap();
    assert_eq!(h.len(), 2);
}

#[tokio::test]
async fn failure_records_update_provider_error_columns() {
    let (r, _d) = repo().await;
    let mut f = rec(vec![], now());
    f.outcome = SyncOutcome::Failure;
    f.error_category = Some(ErrorCategory::Network);
    r.record_sync(&f).await.unwrap();
    let cat: Option<String> =
        sqlx::query_scalar("SELECT last_error_category FROM providers WHERE id='simulated'")
            .fetch_one(r.pool())
            .await
            .unwrap();
    assert_eq!(cat.as_deref(), Some("network"));
}
