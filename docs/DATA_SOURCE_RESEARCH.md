# Data-Source Research (Module 01, Phase 2)

Researched on **2026-11-13** against live official documentation. Nothing here is
from memory; every claim carries the page it came from. Where a page could not be
read, that is stated.

## 1. Sources investigated

| Source | Result |
|---|---|
| Claude Code — status line: <https://code.claude.com/docs/en/statusline> | Read in full. **Key finding** (see §3.1). |
| Claude Code — costs / `/usage`: <https://code.claude.com/docs/en/costs> | Read in full. |
| Claude Code — OpenTelemetry: <https://code.claude.com/docs/en/monitoring-usage> | First 100k of 175k characters read (metrics and events tables, enabling). |
| Claude Code — legal & compliance: <https://code.claude.com/docs/en/legal-and-compliance> | Read in full. Governs third-party credential use. |
| Usage and Cost Admin API: <https://platform.claude.com/docs/en/manage-claude/usage-cost-api> | Read in full. |
| Claude Code Analytics Admin API: <https://platform.claude.com/docs/en/manage-claude/claude-code-analytics-api> | Read in full. |
| API rate limits: <https://platform.claude.com/docs/en/api/rate-limits> | Read in full. |
| Support — usage and length limits: <https://support.claude.com/en/articles/11647753-understanding-usage-and-length-limits> | Read (summarised by the fetch tool). |
| Local CLI: `claude --help`, `claude doctor` (v2.1.323) | Run. No usage subcommand exists (§3.3). |

**Could not be reached from this environment** (network allow-list, DNS failure / proxy
refusal): `docs.anthropic.com` (the platform docs now live at `platform.claude.com`, which
was reachable and used instead), `docs.rs`. `https://claude.ai/settings/usage` requires a
logged-in browser session and was deliberately **not** fetched or scripted (see §3.4).
`support.anthropic.com` is now `support.claude.com`.

Secondary reporting (news articles on the February 2026 consumer-terms clarification) was
read only to locate the official wording; **only the official Claude Code legal page is
relied upon** below. I did not read the Consumer Terms text itself — see §8.

## 2. Candidate interfaces and classification

| # | Candidate | Class | Gives subscription usage? |
|---|---|---|---|
| 3.1 | Claude Code **status-line JSON** (`rate_limits`, `context_window`, `cost`) | **Official local interface** | **Yes — 5-hour and 7-day utilisation %, reset time** (subscribers only, while a session is running) |
| 3.2 | Claude Code **OpenTelemetry** metrics/events | Official local interface | No (tokens/cost/session counts only) |
| 3.3 | Claude Code `/usage` slash command and `claude` CLI | Official **manual** interface | Shows it, but interactive TUI; no documented machine-readable output |
| 3.4 | claude.ai **Settings → Usage** page | Official **manual** interface | Shows it; no documented programmatic interface |
| 3.5 | **Usage & Cost Admin API** (`/v1/organizations/usage_report/messages`, `/cost_report`) | Official API | **No** — API-organisation consumption only |
| 3.6 | **Claude Code Analytics Admin API** (`/v1/organizations/usage_report/claude_code`) | Official API | **No** — explicitly excludes subscription usage |
| 3.7 | Messages API **rate-limit response headers** (`anthropic-ratelimit-*`) | Official API | **No** — API org rate limits |
| 3.8 | Enterprise Analytics API | Official API | Enterprise only, org-level, separate Analytics key; not individual |
| 3.9 | Endpoint behind `/usage` ("the usage endpoint"), claude.ai web endpoints, OAuth token in the Claude Code credential store, browser cookies | **Unsupported implementation detail** | Would work technically; **rejected** (§5) |

### 3.1 Claude Code status line — *the one real subscription source*

Docs: <https://code.claude.com/docs/en/statusline>

- **What it is.** Claude Code runs a user-configured shell command (`statusLine` in
  `settings.json`, `type: "command"`) and writes JSON session data to its **stdin**.
- **Provides (documented field names):**
  - `rate_limits.five_hour.used_percentage` / `.resets_at` and
    `rate_limits.seven_day.used_percentage` / `.resets_at` — "Percentage of the 5-hour or
    7-day rate limit consumed, from 0 to 100"; `resets_at` is "Unix epoch seconds".
  - `context_window.used_percentage`, `.remaining_percentage`, `.context_window_size`,
    `.current_usage` (input-only formula: `input + cache_creation + cache_read`).
  - `cost.total_cost_usd` — "Estimated session cost in USD, computed client-side at list
    price… May differ from your actual bill."
  - `session_id`, `session_name`, `model`, `version`, `transcript_path`, `prompt_cache`,
    `rate_limits.spend_limit.*` (only behind a Claude apps gateway).
- **Does NOT provide:** absolute limits or "remaining messages/tokens", plan name/tier,
  usage-credit balance (on the status line), history, anything when Claude Code is not
  running, usage from claude.ai web / Desktop chat except as reflected in the same window
  percentage.
- **Conditions (verbatim from the docs):** `rate_limits` "appears only for claude.ai Pro,
  Max, and Ultra subscribers… Even then, it appears only after the first API response in
  the session. Each window … may be independently absent, and Claude Code drops a window
  once its `resets_at` time passes."
- **Update cadence:** runs on session start, new assistant message, `/compact`, mode
  changes, an optional `refreshInterval` timer, and when a rate-limit window reaches its
  `resets_at`. Debounced 300 ms; an in-flight script is cancelled by a newer update. "The
  event-driven triggers can go quiet when the main session is idle."
- **Authentication / permissions:** none for us. Claude Code authenticates itself; our
  process never sees a credential. It needs only permission to write a file in a directory
  we own.
- **Rate limits:** none documented (local process).
- **Continuous monitoring:** suitable **only while a Claude Code session is active**. Data
  is *push-on-activity*, so last-seen values go stale when the user stops working. This is
  inherent and must be surfaced to the user (we mark it `stale`).
- **Permitted for this application:** yes. It is documented functionality intended to let
  users pipe session data into their own tooling; the user installs the command in their
  own settings; no credential or token is read, stored or relayed.
- **Operates without exposing credentials:** yes.

### 3.2 OpenTelemetry

Docs: <https://code.claude.com/docs/en/monitoring-usage>. Metrics: `claude_code.session.count`,
`lines_of_code.count`, `pull_request.count`, `commit.count`, `cost.usage` (USD),
`token.usage` (input/output/cacheRead/cacheCreation), `code_edit_tool.decision`,
`active_time.total`; events include `api_request` (model, tokens, cost, duration),
`api_error`. **No quota/rate-limit/subscription metric** is listed in the portion read.
Enabled with `CLAUDE_CODE_ENABLE_TELEMETRY=1` plus OTLP exporter variables; needs an OTLP
receiver. **Not implemented in Module 01** (it would give *local token/cost activity*, a
legitimate future "local Claude Code" provider, but is not needed for the foundation and
requires running a receiver). Remaining ~75k characters of this page were not read; any
subscription metric there would change this row.

### 3.3 `/usage`, `claude` CLI

Docs: <https://code.claude.com/docs/en/costs>. `/usage` shows "plan usage bars, activity
stats, and a usage breakdown" for subscribers. It is an interactive screen. `claude --help`
and `claude doctor` on the installed v2.1.323 list no `usage` subcommand and no JSON usage
output. The costs page states that "the usage endpoint is rate limited" — i.e. `/usage`
calls a server endpoint that is **not documented as an integration surface**. Scraping the
TUI or replaying that endpoint is unsupported (§5). Class: manual.

### 3.4 claude.ai Settings → Usage

Support article: usage is checked at **Settings → Usage**; "The article doesn't mention any
API or other programmatic way to check usage." Authenticating to it requires Claude.ai
session cookies/tokens, which we must not collect (§5). Class: manual.

### 3.5 / 3.6 Admin APIs (API consumption, not subscription)

- Usage & Cost API: `GET /v1/organizations/usage_report/messages` (1m/1h/1d buckets),
  `GET /v1/organizations/cost_report` (1d only). Fields: uncached/cached/cache-creation
  input tokens, output tokens, model, workspace, API key, service tier, server-tool use;
  cost in USD. Data appears "within 5 minutes"; "The API supports polling once per minute
  for sustained use." Header `anthropic-version: 2026-10-21` as shown in the docs.
- Auth: **Admin API key** (`sk-ant-admin01-…`), an `org:admin` OAuth token, or an
  unscoped personal/service key; "workspace API keys don't work". Banner on the page:
  "**The Admin API isn't available to organizations on the claude.ai Team plan or to
  individual Claude Console accounts**" (an individual must first create a Console
  organisation). This is the legacy API; Anthropic recommends the newer *Usage and Cost
  Analytics API* for new integrations (not read in full; see §8).
- Claude Code Analytics API: daily per-actor metrics, `customer_type: api|subscription`
  but "records cover only usage authenticated against your Claude Console organization".
  FAQ, verbatim: "**Does this API include usage from Claude subscription plans? No.**"
  Daily granularity only ("Can this API return real-time metrics? No").
- Suitable for continuous monitoring of **API consumption** at ≤1 request/minute, by users
  who hold an Admin key. Permitted (documented API). Requires a credential that grants
  org-admin privileges — a significant secret; needs its own security review before use.
- **Not implemented in Module 01** (no Admin key available to verify against; would be the
  future `ApiConsumption` provider).

### 3.7 Messages API rate-limit headers

`anthropic-ratelimit-{requests,tokens,input-tokens,output-tokens}-{limit,remaining,reset}`
and `retry-after` (<https://platform.claude.com/docs/en/api/rate-limits>). These describe
**API organisation** limits for the key that made a request. They say nothing about a
claude.ai subscription. Polling would require making billable requests with an API key.
Relevant to our *own* backoff only: HTTP 429 + `retry-after`.

## 4. Does Anthropic restrict third-party apps from account data/auth?

Yes. <https://code.claude.com/docs/en/legal-and-compliance> ("Authentication and
credential use"), verbatim:

> "**OAuth authentication** is intended exclusively for purchasers of Claude Free, Pro,
> Max, Ultra, Team, and Enterprise subscription plans and is designed to support ordinary
> use of Claude Code and other native Anthropic applications."
>
> "Anthropic does not permit third-party developers to offer Claude.ai login into their own
> applications, or to route requests through Free, Pro, Max, or Ultra plan credentials on
> behalf of their users. Moreover, **developers may not collect, store, or intermediate
> Claude.ai credentials or session tokens** — sign-in to a Claude account must complete
> through Anthropic's own flow."
>
> "Anthropic reserves the right to take measures to enforce these restrictions and may do
> so without prior notice."

Consequence: reading the OAuth token that Claude Code stores, harvesting claude.ai
cookies, or calling `claude.ai`/`api.anthropic.com` endpoints with those tokens from our
application is **prohibited**, independent of whether it is technically possible.

## 5. Final classification of every source

| Source | Final class | Used? |
|---|---|---|
| Status-line JSON (`rate_limits`, `context_window`, `cost`) | Official local interface | **Yes** — `claude-code-statusline` provider |
| OpenTelemetry | Official local interface | No (future local-activity provider) |
| `/usage`, `claude` CLI | Official manual interface | No |
| claude.ai Settings → Usage | Official manual interface | No (link-out only, future) |
| Usage & Cost Admin API | Official API (API consumption) | No (future `ApiConsumption` provider; needs Admin key + org) |
| Claude Code Analytics Admin API | Official API (excludes subscriptions) | No |
| Messages API rate-limit headers | Official API (API org limits) | No |
| Enterprise Analytics API | Official API (Enterprise org) | No |
| `/usage` backend endpoint, claude.ai web endpoints, stored Claude Code OAuth token, browser cookies, scraping the TUI | **Unsupported implementation detail — rejected** | **Never** |
| Absolute plan limits / remaining quota / usage-credit balance | **Unavailable** | n/a |

## 6. Decision: **Outcome B — partial supported information**

- **Real subscription data we can legitimately show:** 5-hour and 7-day *utilisation
  percentage* and *reset time*, from the status-line interface, for Pro/Max/Ultra
  subscribers, only while Claude Code is running and only after its first API response in
  a session. Also real **local Claude Code** session data: context-window %, estimated
  session cost, count of recently active sessions.
- **Unavailable:** remaining quota in absolute units, plan limits, usage-credit balance,
  anything when Claude Code is idle/not running, claude.ai-only usage detail, Team/
  Enterprise seat allowances beyond what the same JSON reports, API consumption (needs an
  Admin key we do not have).
- **Mechanism ("bridge"):** a tiny executable we ship (`usage-monitor-statusline`) that the
  *user* configures as their Claude Code `statusLine.command`. It copies the JSON Claude
  Code hands it to a file in our data directory (atomic write) and prints a short status
  string. The desktop app reads those files. No network, no credentials, no modification of
  the user's settings by us (the user opts in by editing their own `settings.json`; the app
  shows the exact snippet).
- **A simulated provider** is provided for development; everything it emits is flagged
  `simulated` and the UI must say so. A simulated or mock provider can never count as real
  subscription monitoring.

### Completion statuses (must not be conflated)

- **Infrastructure complete:** yes, when the checks in `MODULE_01_HANDOFF.md` pass.
- **Real subscription monitoring complete:** **NO — not verified.** The status-line schema is
  documented and the provider is tested against fixtures that follow the documented schema,
  but it has **not** been verified against an actual Pro/Max subscription account in this
  environment (the sandbox Claude Code session is authenticated by a non-subscriber
  mechanism and no real `rate_limits` payload was observed here). Until someone runs the
  bridge with a real subscription and confirms the numbers match claude.ai Settings → Usage,
  this status stays "not verified".

## 7. Implementation strategy

1. `claude-code-statusline` provider (real, partial): reads bridge files; emits subscription
   5h/7d measurements when present, otherwise explicit `Unavailable` (never zero); emits
   local context/cost/active-session measurements; marks data stale per its refresh policy;
   treats a window past `resets_at` as *unavailable (window expired)* because Claude Code
   itself drops it.
2. `simulated` provider for development/UI work.
3. Provider-neutral engine, so an `anthropic-admin-api` provider can be added later without
   touching the engine.
4. No credentials are stored by Module 01. The credential abstraction exists and is tested
   with test secrets, ready for an Admin API key if/when that provider is approved
   (`SECURITY.md`).

## 8. Unresolved technical questions

1. **Real-account verification** of the status-line `rate_limits` values (the key open item).
2. Whether the newer **Usage and Cost Analytics API** (recommended for new integrations;
   <https://platform.claude.com/docs/en/manage-claude/usage-cost-analytics-api>) is
   preferable to the legacy Admin endpoints for the future API provider — not read.
3. Remaining portion of the OpenTelemetry page was not read; confirm it contains no
   quota-related metric.
4. The Consumer Terms text itself was not read (only the official Claude Code page that
   summarises the policy). A legal read is advisable before shipping a Claude-account
   integration of any other kind.
5. How the status line behaves for **multiple concurrent sessions**: every session reports
   account-wide window percentages; we take the most recently received value. Confirm they
   agree in practice.
6. Behaviour when the user's own `statusLine` is already configured (they must chain our
   bridge with their existing command; documented in `PROVIDER_CONTRACTS.md`, not verified
   with real setups).
7. The `rate_limits` field is documented for Pro/Max/Ultra only; Team/Enterprise seat
   behaviour via this field is undocumented (Team/Enterprise windows are described on the
   costs page as 5-hour and weekly, but the status-line page does not list them).
