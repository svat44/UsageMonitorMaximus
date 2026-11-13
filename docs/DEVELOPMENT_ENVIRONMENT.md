# Development environment (as found and as used)

Recorded 2026-11-13 from commands run in the session, not assumed.

## Host

| Item | Value |
|---|---|
| OS | Ubuntu 24.04.5 LTS, Linux 6.18.44 x86_64 (cloud container, headless) |
| Intended release platform | **Windows** (not available here; see "Windows validation") |
| Rust | rustc 1.97.0 / cargo 1.97.0 |
| Node / npm | v22.23.4 / 10.9.9 |
| Claude Code (installed) | 2.1.323 (`claude --help`, `claude doctor` inspected) |
| Chromium | pre-installed at `/opt/pw-browsers/chromium` (used by Playwright) |
| Git | repository existed with **no commits and no files**; branch `claude/core-infrastructure-usage-monitoring-unnely` |

Because the repository was empty, the project was initialised from scratch with the prescribed stack
(there was no existing architecture to preserve).

## System libraries installed for the Tauri Linux build

`libwebkit2gtk-4.1-dev` (2.52.6), `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`,
`libsoup-3.0-dev`, `libssl-dev`, `libsecret-1-dev`, `libdbus-1-dev`, `pkg-config`, `build-essential`;
for headless testing: `xvfb`, `dbus-x11`, `gnome-keyring`.

## Network restrictions that affected research

`docs.anthropic.com` and `docs.rs` were not reachable (sandbox allow-list). Anthropic's platform docs are
served from `platform.claude.com` and Claude Code docs from `code.claude.com`, both reachable and used.

## Locked versions (key dependencies)

Rust: tauri 2.12.5 (crates.io also lists a 3.0 alpha; **2.x chosen as specified**), tauri-plugin-autostart 2.7.0,
tauri-plugin-single-instance 2.5.5, sqlx 0.8.6 (0.9.0 had just been published; 0.8 is the mature line), keyring 3.6.3
(4.x exists but its API could not be checked offline; 3.6 is well established), tokio 1.54, tokio-util 0.7,
chrono 0.4.45, thiserror 2, async-trait 0.1, ts-rs 12.0.1, tracing 0.1.44, tracing-subscriber 0.3.23,
tracing-appender 0.2.5, fastrand 2, dirs 6.

JS: react 19.3.1, vite 8.3.8, typescript 7.0.2, vitest 5.1.0, zustand 5.1.0, @tanstack/react-query 5.107.1,
tailwindcss 4.3.5, @radix-ui/react-switch 1.4.1, lucide-react 1.72.0, @playwright/test 1.65.0,
@tauri-apps/cli and @tauri-apps/api 2.12.x.

### Dependency rationale (only what is used)

| Dependency | Why |
|---|---|
| tokio, tokio-util | async scheduling, timeouts, cancellation (`CancellationToken`) for clean shutdown |
| sqlx (sqlite, migrate) | persistence + migrations behind `Repository` |
| keyring | OS credential store behind `CredentialStore` (no credentials are stored yet) |
| chrono | UTC timestamps |
| thiserror | error enum with categories |
| async-trait | `dyn Provider` / `dyn Repository` |
| fastrand | backoff jitter (tiny, no `rand` tree) |
| dirs | default data dir shared by app and bridge |
| ts-rs | generates the TypeScript types from the Rust types |
| tracing(+subscriber, appender) | structured logs, reloadable level, daily file |
| tauri-plugin-autostart | the supported Tauri autostart integration |
| tauri-plugin-single-instance | a tray app must not run twice (second launch focuses the first) |

**`reqwest` was deliberately not added.** The recommended stack lists it for "documented external APIs", but no
selected provider performs HTTP (research §6). Add it with the first provider that does (the future Admin API provider).

## How to build and test

```bash
# Rust core + bridge (no GUI libs needed)
cargo test -p usage-core -p usage-monitor-statusline

# Frontend
npm ci
npm run typecheck && npm test && npm run test:e2e

# Desktop shell (Linux needs the system libraries above)
npm run build
cargo test -p usage-monitor-app
cargo build -p usage-monitor-app -p usage-monitor-statusline --features usage-monitor-app/custom-protocol
./scripts/smoke-linux.sh        # real window/tray/quit/autostart/React check under Xvfb

# Real OS keyring conformance (needs Credential Manager / Keychain / running Secret Service)
cargo test -p usage-core --lib -- --ignored keyring
```

Developing the UI: `npm run tauri dev`. UI-only in a browser with a mocked backend:
`VITE_MOCK_TAURI=1 npx vite` (what Playwright uses).

## Windows validation path

Nothing Windows-specific was run in this session. `.github/workflows/ci.yml` job **`windows-native`** (a `windows-latest`
runner) runs: the ignored keyring test against real **Windows Credential Manager**; a debug build; and
`scripts/smoke-windows.ps1`, which launches the real app and verifies window-close→hide, engine continues, autostart
enable/disable via the real registry integration, and intentional quit exit code 0. Job `macos-keychain` does the
Keychain check. **These jobs have not been run yet** (the repository has no remote CI history); until they are green,
Windows behaviour is unverified. Also manual: confirm the tray icon is visible in the Windows notification area and that
tray left-click / menu work (cannot be asserted from CI).
