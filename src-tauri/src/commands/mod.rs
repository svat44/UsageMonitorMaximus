//! Typed Tauri commands. Thin wrappers: all logic lives in `usage-core`.

use chrono::Utc;
use serde::Serialize;
use tauri::{AppHandle, State};
use ts_rs::TS;
use usage_core::config::AppConfig;
use usage_core::diagnostics::DiagnosticsReport;
use usage_core::errors::{ErrorInfo, UsageError};
use usage_core::monitoring::{MonitoringSnapshot, SyncResult};
use usage_core::providers::ProviderDescriptor;

use crate::state::AppState;

type CmdResult<T> = Result<T, ErrorInfo>;

fn err(e: UsageError, op: &str) -> ErrorInfo {
    e.to_info(None, op, Utc::now())
}

#[tauri::command]
pub fn get_monitoring_snapshot(state: State<'_, AppState>) -> MonitoringSnapshot {
    state.engine.snapshot()
}

#[tauri::command]
pub async fn get_diagnostics(state: State<'_, AppState>) -> CmdResult<DiagnosticsReport> {
    Ok(state.engine.diagnostics().await)
}

#[tauri::command]
pub fn list_providers(state: State<'_, AppState>) -> Vec<ProviderDescriptor> {
    state.engine.providers()
}

#[tauri::command]
pub fn get_config(state: State<'_, AppState>) -> AppConfig {
    state.engine.config()
}

#[tauri::command]
pub async fn update_config(
    app: AppHandle,
    state: State<'_, AppState>,
    config: AppConfig,
) -> CmdResult<AppConfig> {
    let applied = state
        .engine
        .apply_config(config)
        .await
        .map_err(|e| err(e, "update_config"))?;
    crate::apply_side_effects(&app, &applied);
    Ok(applied)
}

#[tauri::command]
pub async fn set_monitoring_enabled(
    app: AppHandle,
    state: State<'_, AppState>,
    enabled: bool,
) -> CmdResult<AppConfig> {
    let mut c = state.engine.config();
    c.monitoring_enabled = enabled;
    update_config(app, state, c).await
}

#[tauri::command]
pub async fn select_provider(
    app: AppHandle,
    state: State<'_, AppState>,
    provider_id: String,
) -> CmdResult<AppConfig> {
    let mut c = state.engine.config();
    c.selected_provider = provider_id;
    // A different provider has different interval limits: fall back to its recommended interval.
    c.refresh_interval_secs = None;
    update_config(app, state, c).await
}

#[tauri::command]
pub async fn refresh_now(state: State<'_, AppState>) -> CmdResult<SyncResult> {
    state
        .engine
        .refresh_now()
        .await
        .map_err(|e| err(e, "refresh_now"))
}

#[tauri::command]
pub fn hide_window(app: AppHandle) {
    crate::lifecycle::hide_main_window(&app);
}

#[tauri::command]
pub fn quit_app(app: AppHandle) {
    crate::lifecycle::quit(&app);
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StatuslineSetup {
    /// Absolute path of the bridge executable next to the app, when it exists.
    pub bridge_path: Option<String>,
    pub data_dir: String,
    /// JSON fragment for the user's own `~/.claude/settings.json`. We never edit that file.
    pub settings_snippet: String,
}

#[tauri::command]
pub fn get_statusline_setup(state: State<'_, AppState>) -> StatuslineSetup {
    let exe = if cfg!(windows) {
        "usage-monitor-statusline.exe"
    } else {
        "usage-monitor-statusline"
    };
    let path = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(exe)))
        .filter(|p| p.exists());
    let shown = path
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| format!("<path to {exe}>"));
    // No `refreshInterval`: a timer re-run would re-deliver cached data and make stale
    // readings look fresh. Event-driven updates keep `received_at` truthful.
    let snippet = serde_json::json!({ "statusLine": { "type": "command", "command": shown } });
    StatuslineSetup {
        bridge_path: path.map(|p| p.display().to_string()),
        data_dir: state.data_dir.display().to_string(),
        settings_snippet: serde_json::to_string_pretty(&snippet).unwrap_or_default(),
    }
}

/// Smoke-test hook: lets the webview report what it rendered. Does nothing unless
/// `USAGE_MONITOR_SMOKE_TEST=1` is set in the process environment.
#[tauri::command]
pub fn smoke_report(state: State<'_, AppState>, rendered: bool, text: String) {
    if std::env::var("USAGE_MONITOR_SMOKE_TEST").ok().as_deref() == Some("1") {
        let v =
            serde_json::json!({ "rendered": rendered, "text": text.chars().take(4000).collect::<String>() });
        let _ = std::fs::write(state.data_dir.join("smoke-ui.json"), v.to_string());
    }
}
