# Module 01 hand-off

## Status — two separate completion statuses

| Status | Result |
|---|---|
| **Infrastructure complete** | **Yes** for what was verifiable on Linux (see "Evidence"). Windows validation is **defined but not yet run**. |
| **Real subscription monitoring complete** | **NO.** A supported source exists (Claude Code's status-line `rate_limits`), a provider is implemented against its documented schema, but it has **not been verified against an actual Pro/Max subscription account**. Mock/simulated data never counts. Unavailable regardless: absolute remaining quota, plan limits, usage-credit balance, data while Claude Code is idle, API consumption (needs an Admin key + org). |

## What Module 01 delivers

* Tauri 2 shell: window, tray (Show / Refresh / Monitoring toggle / Quit), hide-to-tray on close, intentional Quit with
  graceful shutdown, single instance, autostart plugin, reloadable logging.
* `usage-core`: data model with three never-mixed categories and explicit `Unavailable`; `Provider` abstraction;
  `MonitoringEngine` (timeouts, no duplicate syncs, backoff+jitter, `Retry-After`, stale retention, auth/rate-limit/unsupported
  states); SQLite persistence + migrations + retention; config; credential-store abstraction; diagnostics; typed events.
* Providers: `claude-code-statusline` (real, partial), `simulated`; `MockProvider` for tests.
* `usage-monitor-statusline` bridge binary.
* React/TS UI showing categories, coverage, stale/simulated/estimate/restored badges, settings, "Connect Claude Code", diagnostics.
* Docs (this folder) and CI (`.github/workflows/ci.yml`).

## How the next modules integrate

**Rust-side consumers (preferred, no UI):** depend on `usage-core`.
```rust
let snap: MonitoringSnapshot = engine.snapshot();                        // monitoring/state.rs
let hist = repo.measurement_history("claude-code-statusline",
              MeasurementId::SubscriptionFiveHourUtilization, None, 500).await?;   // storage/mod.rs: Repository
```
Add tables with a new migration `crates/usage-core/migrations/0002_*.sql` and new `Repository` methods (trait in
`storage/mod.rs`, impl in `storage/sqlite.rs`). Add providers per PROVIDER_CONTRACTS.md. Subscribe to engine output by
implementing `events::EventSink` (the Tauri app's implementation: `src-tauri/src/events.rs`).

**Frontend consumers:** use `src/services/tauri.ts` (`api.*`, `subscribe`), the Zustand store
`src/stores/monitoringStore.ts` (`useMonitoringStore(s => s.snapshot)`), and generated types from `src/types`.
Do not poll; follow `monitoring://updated`.

**Example payload** (`MonitoringSnapshot`, abridged):
```json
{
  "runState": "running", "activeProvider": "claude-code-statusline", "connection": "current", "simulated": false,
  "measurements": [
    { "id": "subscription_five_hour_utilization", "category": "subscription_usage",
      "value": { "state": "available", "value": 23.5 }, "unit": "percent", "providerId": "claude-code-statusline",
      "scope": null, "collectedAt": "…", "sourceAt": "…", "verification": "official_local_interface",
      "simulated": false, "estimated": false, "stale": false, "restored": false,
      "resetsAt": "…", "window": "five_hour" },
    { "id": "subscription_weekly_utilization", "category": "subscription_usage",
      "value": { "state": "unavailable", "reason": "no_data_yet" }, "…": "…" }
  ],
  "coverage": [ { "category": "subscription_usage", "state": "live" },
                { "category": "api_consumption", "state": "not_connected" },
                { "category": "local_claude_code", "state": "live" } ],
  "lastAttemptAt": "…", "lastSuccessAt": "…", "nextAttemptAt": "…", "rateLimitedUntil": null,
  "consecutiveFailures": 0, "latestError": null
}
```

## Rules later modules must keep

1. Never display an `unavailable` value as 0; render `describeMeasurement()` (src/utils/format.ts) or equivalent.
2. Use `coverage`, `simulated`, `stale`, `restored` for any claim about what is connected or current.
3. Do not present `api_*` or `local_context_window_*` as subscription usage; categories come from the backend.
4. A feature that needs a credential must complete the SECURITY.md checklist first.

## Evidence (what actually passed, in this environment)

| Check | Result |
|---|---|
| `cargo test -p usage-core` | 89 passed + 1 ignored (56 unit, 22 engine scenarios, 10 storage, 1 statusline e2e; the ignored one is the OS-keyring test, run separately) |
| `cargo test -p usage-monitor-statusline` | 2 passed |
| `cargo test -p usage-monitor-app` | 6 passed (lifecycle decisions, TS export) |
| keyring conformance, **real gnome-keyring Secret Service (Linux)** | passed (`--ignored keyring` under `dbus-run-session`) |
| `npm run typecheck`, `npm test` (Vitest + RTL) | clean / 13 passed |
| `npm run test:e2e` (Playwright, Chromium, **mocked IPC**) | 3 passed |
| `cargo clippy -D warnings` (core, bridge, app) | clean |
| `scripts/smoke-linux.sh` (real app, Xvfb): window visible → React rendered → close hides → engine still syncing → autostart enable/disable → quit exits 0; plus real bridge binary → provider → engine → React showed 23.5% / 41.2% | passed |

## Not verified / open (do not claim otherwise)

* **Windows:** Credential Manager, tray, autostart registry entry, hide/quit behaviour — CI job + `smoke-windows.ps1` exist, not run.
  macOS Keychain likewise. Tray *visibility* in a real desktop (Linux smoke ran under Xvfb without a notification host).
* Real-subscription verification of `rate_limits` (research §8 #1), multi-session agreement, chaining with an existing status line.
* The bridge is not yet bundled/installed by the app bundle (`get_statusline_setup` finds it only next to the executable). Bundling
  per-target (sidecar / `bundle.resources`) and an installer step are for the packaging module.
* Release builds/installers, code signing, auto-update; a bundled `tauri build` was not run (only `cargo build --features custom-protocol`).
* `reqwest` and an API-consumption provider (Admin key; org-only) are not implemented; OpenTelemetry local-activity provider not implemented.
* Log rotation by size; DB backup/export.

## Must happen before the next module starts

1. Run CI on the pushed branch; fix anything Windows/macOS turns up. 2. Someone with a **Pro/Max/Ultra** account configures
the bridge and confirms the 5h/weekly percentages match claude.ai → Settings → Usage (then update DATA_SOURCE_RESEARCH.md §6).
3. Decide the packaging approach for the bridge executable. 4. Decide whether to build the Admin-API provider (needs a security
review of storing an admin-privileged key).
