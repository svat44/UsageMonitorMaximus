//! Headless native smoke test, enabled only by `USAGE_MONITOR_SMOKE_TEST=1`
//! (used by `scripts/smoke-linux.sh` under Xvfb and by CI).
//!
//! It drives the *real* window-event path: closes the dashboard (must hide, not exit), checks the
//! monitoring engine is still running, then performs an intentional quit (must exit cleanly).
//! Results are written to `<data dir>/smoke-result.json`.

use std::sync::atomic::Ordering;
use std::time::Duration;

use serde_json::json;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

use crate::lifecycle::MAIN_WINDOW;
use crate::state::AppState;

pub fn maybe_run(app: &AppHandle) {
    if std::env::var("USAGE_MONITOR_SMOKE_TEST").ok().as_deref() != Some("1") {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        tokio::time::sleep(Duration::from_secs(3)).await;
        let Some(win) = app.get_webview_window(MAIN_WINDOW) else {
            return;
        };
        let visible_before = win.is_visible().unwrap_or(false);
        let tray = state.tray_available.load(Ordering::SeqCst);
        let running_before = state.engine.is_running().await;
        // Ask the real webview (React) to report what it rendered, via a real invoke() to Rust.
        let _ = win.eval(
            "const ok = !!document.querySelector('[data-testid=\"cat-subscription_usage\"]');\
             window.__TAURI_INTERNALS__.invoke('smoke_report', { rendered: ok, text: document.body.innerText });",
        );
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Autostart: the real OS integration (Windows: HKCU Run key; Linux: autostart .desktop).
        let auto = app.autolaunch();
        let enable_err = auto.enable().err().map(|e| e.to_string());
        let enabled = auto.is_enabled().unwrap_or(false);
        let _ = auto.disable();
        let disabled = !auto.is_enabled().unwrap_or(true);

        let _ = win.close(); // → CloseRequested → hide-to-tray (or quit if no tray)
        tokio::time::sleep(Duration::from_secs(2)).await;

        let hidden_after_close = !win.is_visible().unwrap_or(true);
        let running_after_close = state.engine.is_running().await;
        let snapshot_calls_ok = state.engine.snapshot().last_attempt_at.is_some();

        let result = json!({
            "trayAvailable": tray,
            "visibleBeforeClose": visible_before,
            "engineRunningBeforeClose": running_before,
            "windowHiddenAfterClose": hidden_after_close,
            "engineRunningAfterClose": running_after_close,
            "engineHasSyncedWhileHidden": snapshot_calls_ok,
            "autostartEnabledAfterEnable": enabled,
            "autostartDisabledAfterDisable": disabled,
            "autostartEnableError": enable_err,
        });
        let _ = std::fs::write(state.data_dir.join("smoke-result.json"), result.to_string());
        if tray {
            crate::lifecycle::quit(&app); // intentional quit path
        }
        // without a tray the close itself already initiated a graceful quit
    });
}
