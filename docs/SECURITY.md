# Security

## Authentication boundary (what this app will and will not touch)

Anthropic's Claude Code legal page (<https://code.claude.com/docs/en/legal-and-compliance>) says developers "may not collect,
store, or intermediate Claude.ai credentials or session tokens" and may not "route requests through Free, Pro, Max, or Ultra
plan credentials". Therefore this application:

* **never** asks for a Claude.ai password, has no login form, and does not embed `claude.ai` pages;
* **never** reads, copies or stores Claude.ai cookies, Claude Code's own OAuth/credential files, or session tokens;
* **never** calls undocumented Anthropic/claude.ai endpoints (including whatever endpoint backs `/usage`);
* receives subscription data only through the **documented status-line interface**: Claude Code (running as the user,
  authenticated by itself) hands JSON to a command the user configured. Our process never sees a credential.

## Credentials currently stored: **none**

No selected provider needs one. The foundation exists (`crates/usage-core/src/auth/`) and is tested with fake secrets.

### Before enabling a new authentication mechanism (required checklist)

Document, in this file, before merging code that stores a credential: **what** credential; **why** it is needed;
**which component** can read it; **where** it is stored; **how** it is removed; **what happens on expiry**. Example, for
the likely first case (not implemented):

| | Anthropic Admin API key (`sk-ant-admin01-…`) for the future `ApiConsumption` provider |
|---|---|
| Why | Usage & Cost Admin API requires it; there is no lower-privilege option for org usage reports |
| Risk | Admin keys carry org-administration privileges — treat as highly sensitive; explicit user opt-in and a security review first |
| Reader | only that provider, via `CredentialStore::get` |
| Storage | OS credential store through `KeyringCredentialStore`, service `com.usagemonitormaximus.app`, account `anthropic-admin-api/admin-key` |
| Removal | `CredentialStore::delete` on disconnect/uninstall; never written to the DB, config, logs or events |
| Expiry | provider returns `UsageError::Authentication` → `AuthenticationRequired`, polling stops, user re-enters key |

## Credential storage (`CredentialStore`)

* `Secret` redacts `Debug`/`Display` and exposes its value only via `expose()` (audit surface). Tested.
* `KeyringCredentialStore` uses the `keyring` crate: Windows Credential Manager, macOS Keychain, Linux Secret Service.
  A shared `conformance_suite` runs against every implementation. Verified on Linux against a real gnome-keyring Secret
  Service in this session; **Windows Credential Manager and macOS Keychain are unverified** (CI jobs exist, not yet run).
* `InMemoryCredentialStore` is for tests only.

## Logging protections

* `tracing` logs carry provider id, operation and error category. No credentials flow through the logging path: secrets
  are only ever held in `Secret`; error messages are authored by providers and must be secret-free (provider contract #6).
* Logs go to stderr and `<data dir>/logs/app.log.<date>` (local only, not rotated by size/age yet — known limitation).
* The bridge persists only the whitelisted fields (no `transcript_path`, `cwd`, workspace/PR info). Tested.

## Local attack surface

* The webview has only `core:default` permissions; no filesystem/shell/HTTP plugins. CSP: `default-src 'self'; connect-src
  ipc: http://ipc.localhost; img-src 'self' data:; style-src 'self' 'unsafe-inline'`.
* Bridge files: written atomically; session ids are sanitised to `[A-Za-z0-9_-]{1,64}` (no path traversal, tested);
  reads reject files > 256 KiB and unknown schema versions. They live in the user's own app-data dir; any process running as
  that user could forge them. That is inherent to a local file interface and not a privilege boundary — displayed numbers
  are informational. Forged data cannot execute anything.
* The bridge never fails Claude Code (always exit 0, errors swallowed) and has no network code.
* We never modify `~/.claude/settings.json`; the user opts in by pasting the snippet.
* `smoke_report` command: inert unless `USAGE_MONITOR_SMOKE_TEST=1` is set in the environment.
* Single-instance plugin prevents duplicate engines writing one database.

## Known limitations

1. Windows/macOS credential stores and Windows tray/autostart behaviour not yet exercised (see DEVELOPMENT_ENVIRONMENT.md).
2. No code signing, auto-update, or installer hardening yet.
3. Log files are not size-capped.
4. Same-user processes can write bridge files (see above).
5. The Consumer Terms were not read in full (only the official Claude Code summary); a legal review is advisable before any
   Claude-account integration beyond the status line.
