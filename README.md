# Usage Monitor Maximus

Desktop app (Tauri 2 + Rust + React/TypeScript) for monitoring Claude usage. **Module 01 — core infrastructure.**

> **Honest status:** the infrastructure works and is tested. *Real subscription monitoring is not verified.* The only
> supported source of subscription numbers is Claude Code's status-line interface (5-hour / weekly **percentages**, subscribers
> only, only while Claude Code is active). It is implemented but has not been checked against a real subscription account.
> See [`docs/DATA_SOURCE_RESEARCH.md`](docs/DATA_SOURCE_RESEARCH.md) and [`docs/MODULE_01_HANDOFF.md`](docs/MODULE_01_HANDOFF.md).

## Docs
[Data-source research](docs/DATA_SOURCE_RESEARCH.md) · [Development environment](docs/DEVELOPMENT_ENVIRONMENT.md) ·
[Architecture](docs/ARCHITECTURE.md) · [Provider contracts](docs/PROVIDER_CONTRACTS.md) · [Database](docs/DATABASE.md) ·
[Security](docs/SECURITY.md) · [Module 01 hand-off](docs/MODULE_01_HANDOFF.md)

## Quick start
See "How to build and test" in [DEVELOPMENT_ENVIRONMENT.md](docs/DEVELOPMENT_ENVIRONMENT.md). `npm run tauri dev` runs the app.
