//! Structured logging: stderr + daily rolling file under `<data dir>/logs`.
//! Level is reloadable at runtime from the `logLevel` preference.
//!
//! Secret hygiene: nothing in this application passes credentials to `tracing`; the credential
//! types redact `Debug`/`Display`, and error messages are secret-free by construction.

use std::path::Path;

use tracing_subscriber::{prelude::*, reload, EnvFilter, Registry};
use usage_core::config::LogLevel;

pub type LogHandle = reload::Handle<EnvFilter, Registry>;

fn filter_for(level: LogLevel) -> EnvFilter {
    // RUST_LOG, when set, wins (developer override).
    EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level.as_filter()))
}

pub fn init(data_dir: &Path, level: LogLevel) -> LogHandle {
    let (filter, handle) = reload::Layer::new(filter_for(level));
    let file = tracing_appender::rolling::daily(data_dir.join("logs"), "app.log");
    let _ = Registry::default()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr).with_target(false))
        .with(tracing_subscriber::fmt::layer().with_writer(file).with_ansi(false))
        .try_init();
    handle
}

pub fn set_level(handle: &LogHandle, level: LogLevel) {
    let _ = handle.reload(filter_for(level));
}
