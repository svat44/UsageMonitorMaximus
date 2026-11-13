//! Locations shared by the desktop app and the statusline bridge.
//!
//! Both processes must agree on the directory, so the logic lives here.
//! `USAGE_MONITOR_DATA_DIR` overrides the default (used by tests and portable setups).
//! The default equals Tauri's `app_data_dir()` for the identifier below
//! (`dirs::data_dir()/<identifier>`).

use std::path::PathBuf;

pub const APP_IDENTIFIER: &str = "com.usagemonitormaximus.app";
pub const DATA_DIR_ENV: &str = "USAGE_MONITOR_DATA_DIR";

pub fn default_data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os(DATA_DIR_ENV).filter(|v| !v.is_empty()) {
        return PathBuf::from(p);
    }
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join(APP_IDENTIFIER)
}

/// Directory the bridge writes one JSON file per Claude Code session into.
pub fn statusline_dir(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("statusline")
}

pub fn database_path(data_dir: &std::path::Path) -> PathBuf {
    data_dir.join("usage-monitor.sqlite")
}
