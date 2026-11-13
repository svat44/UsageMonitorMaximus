# Database

SQLite via sqlx 0.8 (runtime queries, no compile-time DB needed). File: `<data dir>/usage-monitor.sqlite`
(`paths::database_path`). Opened with WAL, `synchronous=NORMAL`, `foreign_keys=ON`, 5 s busy timeout, pool of 4.
All SQL lives in `crates/usage-core/src/storage/sqlite.rs`; the engine only knows the `Repository` trait.
Timestamps are **UTC, RFC 3339 TEXT**. Migrations: `crates/usage-core/migrations/` (embedded with `sqlx::migrate!`, applied at
open; sqlx records them in `_sqlx_migrations`). Current schema version: **1** (`0001_initial.sql`).

## Tables

| Table | Purpose | Key columns |
|---|---|---|
| `app_preferences` | key/value (JSON text). Holds `app_config` (the whole `AppConfig`) | `key` PK, `value`, `updated_at` |
| `providers` | one row per registered provider; status metadata | `id` PK, `display_name`, `simulated`, `first_seen_at`, `last_success_at`, `last_error_category`, `last_error_at` |
| `sync_runs` | one row per sync **attempt** (success or failure) | `id` PK, `provider_id` FK→providers (CASCADE), `started_at`, `finished_at`, `outcome` ('success'/'failure'), `error_category`, `measurement_count` |
| `measurements` | measurement history | `id` PK, `sync_run_id` FK→sync_runs (**SET NULL**), `provider_id` FK→providers (CASCADE), `measurement_id`, `category`, `scope`, `availability`, `value`, `unavailable_reason`, `unit`, `collected_at`, `source_at`, `verification`, `simulated`, `estimated`, `resets_at`, `usage_window` |
| `diagnostic_events` | recent errors for support | `occurred_at`, `level`, `provider_id`, `operation`, `category`, `message` |

`measurements` has `CHECK ((availability='available' AND value IS NOT NULL) OR (availability='unavailable' AND value IS NULL))`
— an unavailable measurement is stored as `NULL` and can never be a stand-in zero (tested). Stale/restored flags are **not**
stored: they describe a live view; history is only ever written from fresh observations.

Indexes: `sync_runs(provider_id, started_at)`, `measurements(provider_id, measurement_id, scope, collected_at)`,
`measurements(collected_at)`, `diagnostic_events(occurred_at)`.

## Write conventions

* `record_sync` inserts the sync run, its measurements and the provider status update **in one transaction** (tested:
  a failing row rolls everything back).
* **Change-based history.** The engine persists a measurement only when its value or `resets_at` changed since the last
  persisted row for the same (provider, id, scope), or when the last row is ≥ 5 minutes old (heartbeat). **Stale
  measurements are never persisted.** So a missing row between two points means "unchanged", and `sync_runs` is the record
  of when attempts happened. Later modules that need dense series should read both.
* Storage failures never stop monitoring: they are logged, emitted as `monitoring://error` and shown in diagnostics.

## Read conventions

* `latest_measurements(provider, newer_than)` → newest row per (id, scope), returned with `restored=true, stale=true`
  (used at startup so history is never mistaken for a fresh measurement; window: 7 days).
* `measurement_history(provider, id, scope, limit)` → newest first, `restored=false` (records of the past).

## Retention (`RetentionPolicy::default`, applied at startup and every 2000 syncs)

| Data | Kept |
|---|---|
| `measurements` | 90 days |
| `sync_runs` | 30 days |
| `diagnostic_events` | 30 days |

`app_preferences` and `providers` are never pruned. Measurements outlive their `sync_run` (FK is `SET NULL`) so the two
windows are independent.

## Extending

New modules add numbered migrations (`0002_*.sql`, …) with their own tables; do not edit `0001`. Nothing for Time Machine,
Replay, Snapshots, Calendar, Reset History or Session Graveyard exists yet.
