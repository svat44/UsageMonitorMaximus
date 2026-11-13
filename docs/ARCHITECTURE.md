# Architecture

```
 Claude Code ──(documented statusLine JSON on stdin)──▶ usage-monitor-statusline (bridge, user-configured)
                                                        │ atomic write
                                                        ▼
                                       <data dir>/statusline/<session>.json
                                                        │ read
┌───────────────────────── Tauri app (src-tauri) ───────┼──────────────────────────────────────┐
│  lifecycle (tray, hide-to-tray, quit)                 ▼                                      │
│  commands/ (typed)      ┌──────────────────── usage-core (no UI deps) ───────────────────┐   │
│  events.rs ◀────────────┤ MonitoringEngine ──▶ dyn Provider (statusline | simulated)      │   │
│      │ emit             │   │ state (authoritative)         ▲ registry                   │   │
│      ▼                  │   ├──▶ dyn Repository ──▶ SQLite (sqlx, migrations)            │   │
│  webview (React/TS) ◀───┘   └──▶ dyn EventSink                                           │   │
│   zustand store ◀ events;  TanStack Query ◀ commands                                      │   │
└──────────────────────────────────────────────────────────────────────────────────────────────┘
```

## Crates and directories (actual)

| Path | Responsibility |
|---|---|
| `crates/usage-core/` | Everything that is not UI: model, providers, engine, storage, credentials, config, events, errors, diagnostics. Builds and tests without Tauri. |
| `crates/usage-core/src/model.rs` | Measurement model, categories, `Unavailable`, coverage |
| `crates/usage-core/src/providers/` | `Provider` trait + registry (`mod.rs`), `statusline.rs` (real, partial), `simulated.rs`, `mock.rs` (tests) |
| `crates/usage-core/src/monitoring/` | `engine.rs` (scheduler/state owner), `backoff.rs`, `state.rs` |
| `crates/usage-core/src/storage/` | `Repository` trait (`mod.rs`), `sqlite.rs` |
| `crates/usage-core/src/auth/` | `CredentialStore` trait, in-memory + OS keyring implementations |
| `crates/usage-core/src/{config,events,errors,diagnostics,paths,clock,statusline_protocol}.rs` | as named |
| `crates/statusline-bridge/` | `usage-monitor-statusline` binary: Claude Code's `statusLine.command` |
| `src-tauri/src/` | `lib.rs` (startup), `commands/`, `events.rs`, `lifecycle.rs`, `logging.rs`, `state.rs`, `smoke.rs` |
| `src/` | React UI: `components/`, `features/{dashboard,settings}/`, `hooks/`, `services/` (typed IPC), `stores/`, `types/` (generated + index), `utils/`, `mocks/` (browser-only fake backend) |
| `src/types/generated/` | **Generated** from Rust by `ts-rs` during `cargo test`. Never hand-edited; CI fails on drift. |

The spec's suggested `src-tauri/src/{core,providers,monitoring,storage,auth,config,errors}` live in `usage-core`
instead, so they can be tested on any platform without GUI libraries. `src-tauri/src/commands` and `events` follow the
spec. Business logic is not duplicated in React: the UI renders `MonitoringSnapshot` and calls commands.

## The three data categories never mix

`MeasurementId::category()` is the only place a category is assigned; `Measurement.category` is always derived from the id,
and `validate_measurements` rejects any provider output where it isn't. `compute_coverage` (a pure function of the
measurements) yields per-category `live | stale | simulated | not_connected`, which the UI uses for its claims. Simulated
values never count as real.

## Startup order (`src-tauri/src/lib.rs`)

1. data dir (`USAGE_MONITOR_DATA_DIR` or `dirs::data_dir()/com.usagemonitormaximus.app`) 2. logging 3. SQLite open +
migrations 4. load config (corrupt/missing → defaults) 5. register providers 6. `MonitoringEngine::initialize()`
(upsert providers, prune retention, load persisted history as `restored`+`stale`) 7. tray 8. apply autostart/log level
9. show window unless starting hidden (and only if a tray exists) 10. `engine.start()` if enabled.

## Monitoring engine (`monitoring/engine.rs`)

* One loop task per run, cancelled via `CancellationToken`; an in-flight provider call is dropped on cancel.
* `sync_once()`: `try_lock` (**no duplicate concurrent syncs**) → rate-limit guard → `timeout(provider.request_timeout)`
  around *initialize-if-needed + fetch + validate* → success or failure handler.
* **Success:** stamp `stale` from `source_at` vs provider `stale_after`; add explicit `Unavailable(NotReported)` for
  every declared capability the provider omitted; connection = `Simulated` | `Stale` | `Current`; schedule next at
  `effective interval`; persist (change-based, see DATABASE.md); emit `updated` (+ `provider-status-changed`).
* **Failure:** keep all measurements, mark `stale`; by error category —
  `RateLimited` → `RateLimited` state, wait = `Retry-After` (floor, ≤10% added jitter) else exponential;
  `Authentication`/`Permission` → `AuthenticationRequired`, **polling stops** (manual refresh retries);
  `UnsupportedCapability` → `PermanentlyUnsupported`, polling stops;
  everything else → `TemporarilyUnavailable`, exponential backoff with jitter (`base=interval`, ×2, cap 15 min, −≤25% jitter).
  Counters reset on success. While rate limited, scheduled **and manual** refreshes make no provider call.
* Refresh interval: `AppConfig.refresh_interval_secs` (None → provider's `recommended`), validated against the
  provider's `min_interval_secs`.
* `stop()` cancels the loop, waits for an in-flight manual sync (bounded by the provider timeout), calls
  `provider.disconnect()`, marks values stale. `shutdown()` = `stop()` + close the DB (WAL checkpoint).
* `apply_config()` validates, persists, then starts/stops/restarts as needed (enable flag, provider change).

### Connection states (`ConnectionState`)

`not_started`, `current`, `stale`, `authentication_required`, `rate_limited`, `temporarily_unavailable`,
`permanently_unsupported`, `simulated`.

## Window / tray lifecycle (`src-tauri/src/lifecycle.rs`)

* Decision: **closing the dashboard hides it to the tray; the engine is untouched.** Pure function `on_close_requested`
  (unit-tested). If the tray could not be created, closing quits instead (the app must never become invisible and
  unreachable), and "start minimised" is ignored.
* Quit is explicit: tray menu "Quit" or `quit_app` → `engine.shutdown()` (cancel tasks, disconnect, close DB) →
  `app.exit(0)`. Implicit exit (last window closed) is prevented unless quitting; `RunEvent::Exit` runs an idempotent
  `shutdown()` as a safety net for OS-initiated exits.
* Single instance: a second launch focuses the first. Autostart passes `--hidden`.
* Tray menu: Show dashboard · Refresh now · Monitoring enabled (check) · Quit. Left click shows the dashboard.

## Commands (all in `src-tauri/src/commands/mod.rs`; wrappers in `src/services/tauri.ts`)

`get_monitoring_snapshot`, `get_diagnostics`, `list_providers`, `get_config`, `update_config(config)`,
`set_monitoring_enabled(enabled)`, `select_provider(providerId)`, `refresh_now`, `get_statusline_setup`, `hide_window`,
`quit_app`, and the smoke-only `smoke_report` (no-op unless `USAGE_MONITOR_SMOKE_TEST=1`). Errors reject with an
`ErrorInfo` object (`category`, `message`, `providerId`, `operation`, `occurredAt`, `retryAfterSecs`).

## Event contract (Tauri events; names in `crates/usage-core/src/events.rs`, mirrored in `src/services/events.ts`, drift-tested)

| Event | Payload (TS type) | When |
|---|---|---|
| `monitoring://started` | `LifecyclePayload` | engine loop started |
| `monitoring://updated` | `MonitoringSnapshot` | after every sync attempt, start, stop, provider switch. **Authoritative full state**; the UI never needs to poll |
| `monitoring://provider-status-changed` | `ProviderStatusChanged {providerId, previous, current}` | `ConnectionState` changed |
| `monitoring://authentication-required` | `AuthenticationRequiredPayload` | auth/permission failure |
| `monitoring://rate-limited` | `RateLimitedPayload {retryAfterSecs, resumeAt}` | rate-limit response |
| `monitoring://error` | `ErrorInfo` | any sync/persistence error |
| `monitoring://stopped` | `LifecyclePayload` | engine stopped |

Frontend rule: subscribe first, then fetch the snapshot once (`useMonitoringEvents`), so no update is lost.
