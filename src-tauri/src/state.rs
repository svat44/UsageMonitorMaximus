use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use usage_core::monitoring::MonitoringEngine;

use crate::logging::LogHandle;

pub struct AppState {
    pub engine: MonitoringEngine,
    pub data_dir: PathBuf,
    /// Set once the user (or the OS) has asked the app to quit; disables hide-to-tray.
    pub quitting: AtomicBool,
    /// False if the tray icon could not be created: closing the window must then quit,
    /// otherwise the app would become unreachable.
    pub tray_available: AtomicBool,
    pub log: LogHandle,
}
