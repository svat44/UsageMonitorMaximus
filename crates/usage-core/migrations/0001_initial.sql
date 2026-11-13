-- Module 01 schema. All timestamps are UTC, stored as RFC 3339 TEXT.

CREATE TABLE app_preferences (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL,          -- JSON
    updated_at TEXT NOT NULL
);

CREATE TABLE providers (
    id                  TEXT PRIMARY KEY NOT NULL,
    display_name        TEXT NOT NULL,
    simulated           INTEGER NOT NULL,
    first_seen_at       TEXT NOT NULL,
    last_success_at     TEXT,
    last_error_category TEXT,
    last_error_at       TEXT
);

-- One row per synchronisation attempt (success or failure).
CREATE TABLE sync_runs (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    provider_id       TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    started_at        TEXT NOT NULL,
    finished_at       TEXT NOT NULL,
    outcome           TEXT NOT NULL CHECK (outcome IN ('success','failure')),
    error_category    TEXT,
    measurement_count INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sync_runs_provider_started ON sync_runs(provider_id, started_at);

-- Measurement history. Written only when a value changes (or on a heartbeat), see DATABASE.md.
CREATE TABLE measurements (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    sync_run_id    INTEGER REFERENCES sync_runs(id) ON DELETE SET NULL,
    provider_id    TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    measurement_id TEXT NOT NULL,
    category       TEXT NOT NULL,
    scope          TEXT,
    availability   TEXT NOT NULL CHECK (availability IN ('available','unavailable')),
    value          REAL,               -- NULL when unavailable (never 0 as a stand-in)
    unavailable_reason TEXT,
    unit           TEXT NOT NULL,
    collected_at   TEXT NOT NULL,
    source_at      TEXT,
    verification   TEXT NOT NULL,
    simulated      INTEGER NOT NULL,
    estimated      INTEGER NOT NULL,
    resets_at      TEXT,
    usage_window   TEXT,
    CHECK ((availability = 'available' AND value IS NOT NULL) OR
           (availability = 'unavailable' AND value IS NULL))
);
CREATE INDEX idx_measurements_lookup
    ON measurements(provider_id, measurement_id, scope, collected_at);
CREATE INDEX idx_measurements_collected ON measurements(collected_at);

CREATE TABLE diagnostic_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    occurred_at TEXT NOT NULL,
    level       TEXT NOT NULL,
    provider_id TEXT,
    operation   TEXT NOT NULL,
    category    TEXT,
    message     TEXT NOT NULL
);
CREATE INDEX idx_diagnostic_events_time ON diagnostic_events(occurred_at);
