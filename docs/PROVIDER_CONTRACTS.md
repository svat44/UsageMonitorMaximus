# Provider contracts

Source: `crates/usage-core/src/providers/mod.rs`. The engine only sees `dyn Provider`.

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn descriptor(&self) -> &ProviderDescriptor;          // id, name, simulated, capabilities, refresh policy
    async fn check_availability(&self) -> Availability;   // Available | Unavailable{reason} | Unsupported{reason}
    async fn initialize(&self) -> Result<()>;             // idempotent; engine calls it lazily before first fetch
    async fn fetch(&self, ctx: &FetchContext) -> Result<Vec<Measurement>>;
    async fn health(&self) -> ProviderHealth;             // state, detail, last_success_at
    fn auth_state(&self) -> AuthState;                    // NotRequired | Authenticated | Required | Expired
    async fn disconnect(&self);
}
```

## Rules a provider must follow

1. **Declare capabilities.** `descriptor().capabilities` lists every `MeasurementId` it can ever return. The engine
   rejects (`InvalidResponse`) anything else, any `provider_id` mismatch, wrong category/unit, non-finite or negative
   values, percentages outside 0–1000, and a *simulated* provider emitting a measurement not flagged `simulated`.
2. **Never fabricate zero.** If the source didn't supply a value return `Measurement::unavailable(id, …, reason, …)`.
   Capabilities you omit entirely are filled by the engine as `Unavailable(NotReported)`.
3. **Stamp time from `ctx.now`** (`collected_at`) and set `source_at` when the source says when it produced the value —
   the engine derives `stale` from `now - source_at > refresh.stale_after_secs`.
4. **Use the right error variant** — behaviour depends on it:

| Return | Engine reaction |
|---|---|
| `Network`, `Timeout`, `InvalidResponse`, `ProviderUnavailable`, `Internal`, `Database` | `TemporarilyUnavailable`; exponential backoff + jitter |
| `RateLimited { retry_after }` | `RateLimited`; wait ≥ `retry_after` (else backoff); no calls meanwhile |
| `Authentication`, `Permission` | `AuthenticationRequired`; **polling stops** |
| `UnsupportedCapability` | `PermanentlyUnsupported`; polling stops |

5. **Honour the timeout** — the engine wraps `initialize+fetch` in `request_timeout_secs`; don't swallow cancellation.
6. **No secrets in error messages or logs.** Messages are logged and sent to the UI.
7. **Simulated providers set `descriptor.simulated = true`** and build measurements with `Verification::Simulated`.

## Measurement (TS: `src/types/generated/Measurement.ts`)

```jsonc
{
  "id": "subscription_five_hour_utilization",
  "category": "subscription_usage",              // derived from id; never set independently
  "value": { "state": "available", "value": 23.5 },   // or { "state": "unavailable", "reason": "no_data_yet" }
  "unit": "percent",                             // percent | usd | tokens | count
  "providerId": "claude-code-statusline",
  "scope": null,                                 // e.g. Claude Code session id for per-session measurements
  "collectedAt": "2026-06-01T12:00:00Z",
  "sourceAt": "2026-06-01T11:59:55Z",
  "verification": "official_local_interface",    // | official_api | simulated
  "simulated": false, "estimated": false,
  "stale": false,                                // old or could not be refreshed
  "restored": false,                             // loaded from DB at startup, not observed this run
  "resetsAt": "2026-06-01T14:00:00Z",
  "window": "five_hour"                          // | seven_day
}
```

Implemented ids (`MeasurementId`): `subscription_five_hour_utilization`, `subscription_weekly_utilization`
(SubscriptionUsage); `api_input_tokens`, `api_output_tokens` (ApiConsumption — **no real provider exists yet**, only
the simulator emits them); `local_context_window_utilization`, `local_session_cost_estimate` (`estimated`),
`local_active_sessions` (LocalClaudeCode). Unavailable reasons: `not_supported_by_provider`, `no_data_yet`,
`not_reported_by_source`, `window_expired`, `not_reported`.

## Refresh policy (`RefreshPolicy`)

`min_interval_secs`, `recommended_interval_secs`, `polling_supported`, `event_driven`, `request_timeout_secs`,
`stale_after_secs`, `documented_rate_limit`. Providers without a documented limit use conservative values.

| Provider | min | recommended | timeout | stale after | notes |
|---|---|---|---|---|---|
| `claude-code-statusline` | 5 s | 30 s | 10 s | 600 s | local file reads; Claude Code pushes to the bridge on activity (event-driven); no documented rate limit |
| `simulated` | 1 s | 5 s | 5 s | 60 s | development only |

## Provider health vs connection state

`ProviderHealth` is what the provider says about itself (`healthy|degraded|unhealthy`, last success). `ConnectionState`
is what the *engine* concludes from outcomes. The UI shows the latter and the `coverage` derived from measurements.

## Implemented providers

### `claude-code-statusline` (real, **partial**) — `providers/statusline.rs`
Reads files written by the `usage-monitor-statusline` bridge (`<data dir>/statusline/*.json`, format in
`statusline_protocol.rs`, only these fields are kept: `session_id`, `version`, `model`, `context_window`,
`cost.total_cost_usd`, `rate_limits.{five_hour,seven_day}.{used_percentage,resets_at}`).
* 5h/weekly: **newest snapshot that carries the window wins** (account-wide values). Past `resets_at` →
  `Unavailable(window_expired)` (Claude Code itself drops the window). No snapshots → `no_data_yet`; snapshots without
  `rate_limits` (API-key users, or before the first response) → `not_reported_by_source`.
* Per-session context % and cost for sessions seen in the last 10 minutes (`scope` = sanitised session id);
  `local_active_sessions` = their count. Files older than 7 days are deleted at initialise.
* Only unparsable files present → `InvalidResponse`. Unreadable directory → `Permission` / `ProviderUnavailable`.
* **Limits:** data exists only while Claude Code is being used; values are percentages of an unknown absolute limit;
  not verified against a real subscription (see DATA_SOURCE_RESEARCH.md §6).

**Connecting it:** the user adds `{"statusLine": {"type": "command", "command": "<path>/usage-monitor-statusline"}}` to
their own `~/.claude/settings.json` (the app's "Connect Claude Code" card shows the snippet). To keep an existing status
line, wrap it: have your script write the stdin to both (`tee >(usage-monitor-statusline >/dev/null) | your-script`);
this was not tested with real setups. Do **not** set `refreshInterval` for the bridge: a timer re-run re-delivers cached
data and would make old readings look fresh.

### `simulated` — `providers/simulated.rs`
Deterministic values in all three categories, every one `simulated: true`. Its connection state is `simulated`, never `current`.

### `MockProvider` — `providers/mock.rs` (tests only; never registered by the app)
Scriptable: `Success`, `Empty`, `MissingFields`, `Stale(age)`, `ExplicitUnavailable`, `NetworkFailure`, `Hang`
(timeout), `AuthExpired`, `PermissionDenied`, `RateLimited(Option<secs>)`, `Malformed`, `OutOfRange`, `Unavailable`,
`Unsupported`. `pretend_real()` is `#[doc(hidden)]` and exists solely to exercise non-simulated engine paths.

## Adding a provider

1. Implement `Provider` in `providers/<name>.rs`; declare capabilities honestly. 2. Add any new `MeasurementId` to
`model.rs` (category/unit/window/`as_str`/`ALL`) — regenerate TS with `cargo test -p usage-core --lib export_bindings`
and add labels in `src/utils/labels.ts`. 3. Register it in `src-tauri/src/lib.rs`. 4. Test it against fixtures that follow
the source's documented schema, and add the `MockProvider`-style engine tests only if it needs new engine behaviour.
5. If it needs a credential: stop and follow SECURITY.md ("Before enabling a new authentication mechanism") first.
