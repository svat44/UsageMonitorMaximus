//! Window / tray / quit lifecycle.
//!
//! Decision: closing the dashboard **hides it to the tray** (monitoring keeps running). The app
//! only terminates through an intentional Quit (tray menu or `quit_app` command) or an
//! OS-initiated exit. If the tray icon is unavailable, closing the window quits instead, so the
//! app can never become invisible and unreachable.

use std::sync::atomic::Ordering;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, RunEvent, Window, WindowEvent};

use crate::state::AppState;

pub const MAIN_WINDOW: &str = "main";

#[derive(Debug, PartialEq, Eq)]
pub enum CloseAction {
    HideToTray,
    QuitGracefully,
    /// Already quitting: let the window close.
    Allow,
}

/// Pure decision logic (unit-tested; no Tauri types).
pub fn on_close_requested(minimize_to_tray: bool, tray_available: bool, quitting: bool) -> CloseAction {
    if quitting {
        CloseAction::Allow
    } else if minimize_to_tray && tray_available {
        CloseAction::HideToTray
    } else {
        CloseAction::QuitGracefully
    }
}

/// `code == None` means "last window closed" (implicit); `Some` means an explicit `exit()` call.
pub fn should_prevent_exit(code: Option<i32>, quitting: bool) -> bool {
    code.is_none() && !quitting
}

pub fn should_start_hidden(start_minimized_pref: bool, args: &[String]) -> bool {
    start_minimized_pref || args.iter().any(|a| a == "--hidden" || a == "--minimized")
}

pub fn show_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

pub fn hide_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
        let _ = w.hide();
    }
}

/// Intentional quit: stop the engine (cancels tasks, disconnects providers), close the database,
/// then exit the process. Idempotent.
pub fn quit(app: &AppHandle) {
    let state = app.state::<AppState>();
    if state.quitting.swap(true, Ordering::SeqCst) {
        return;
    }
    tracing::info!(operation = "quit", "quit requested");
    let engine = state.engine.clone();
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        engine.shutdown().await;
        app.exit(0);
    });
}

pub fn handle_window_event(window: &Window, event: &WindowEvent) {
    if window.label() != MAIN_WINDOW {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        let app = window.app_handle();
        let state = app.state::<AppState>();
        let minimize = state.engine.config().minimize_to_tray;
        let action = on_close_requested(
            minimize,
            state.tray_available.load(Ordering::SeqCst),
            state.quitting.load(Ordering::SeqCst),
        );
        match action {
            CloseAction::Allow => {}
            CloseAction::HideToTray => {
                api.prevent_close();
                let _ = window.hide();
                tracing::info!(operation = "window_close", "dashboard hidden to tray; monitoring continues");
            }
            CloseAction::QuitGracefully => {
                api.prevent_close();
                quit(app);
            }
        }
    }
}

pub fn handle_run_event(app: &AppHandle, event: RunEvent) {
    match event {
        RunEvent::ExitRequested { api, code, .. } => {
            let quitting = app.state::<AppState>().quitting.load(Ordering::SeqCst);
            if should_prevent_exit(code, quitting) {
                api.prevent_exit();
            }
        }
        RunEvent::Exit => {
            // Safety net for OS-initiated exits that bypass `quit()`: shutdown is idempotent.
            let engine = app.state::<AppState>().engine.clone();
            tauri::async_runtime::block_on(engine.shutdown());
        }
        _ => {}
    }
}

/// Create the tray icon + menu. Returns false if the platform could not provide a tray.
pub fn build_tray(app: &AppHandle) -> bool {
    match try_build_tray(app) {
        Ok(()) => true,
        Err(e) => {
            tracing::warn!(operation = "tray", category = "internal", "tray unavailable: {e}");
            false
        }
    }
}

fn try_build_tray(app: &AppHandle) -> tauri::Result<()> {
    let enabled = app.state::<AppState>().engine.config().monitoring_enabled;
    let show = MenuItem::with_id(app, "show", "Show dashboard", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let toggle = CheckMenuItem::with_id(app, "toggle", "Monitoring enabled", true, enabled, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &refresh, &toggle, &PredefinedMenuItem::separator(app)?, &quit_i])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Usage Monitor Maximus")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, ev| match ev.id().as_ref() {
            "show" => show_main_window(app),
            "refresh" => {
                let engine = app.state::<AppState>().engine.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = engine.refresh_now().await;
                });
            }
            "toggle" => {
                let engine = app.state::<AppState>().engine.clone();
                tauri::async_runtime::spawn(async move {
                    let mut c = engine.config();
                    c.monitoring_enabled = !c.monitoring_enabled;
                    if let Err(e) = engine.apply_config(c).await {
                        tracing::warn!(operation = "tray_toggle", category = e.category().as_str(), "{e}");
                    }
                });
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_hides_to_tray_by_default() {
        assert_eq!(on_close_requested(true, true, false), CloseAction::HideToTray);
    }

    #[test]
    fn close_quits_when_tray_disabled_or_unavailable() {
        assert_eq!(on_close_requested(false, true, false), CloseAction::QuitGracefully);
        assert_eq!(on_close_requested(true, false, false), CloseAction::QuitGracefully);
    }

    #[test]
    fn close_is_allowed_once_quitting() {
        assert_eq!(on_close_requested(true, true, true), CloseAction::Allow);
    }

    #[test]
    fn implicit_exit_is_prevented_unless_quitting() {
        assert!(should_prevent_exit(None, false));
        assert!(!should_prevent_exit(None, true));
        assert!(!should_prevent_exit(Some(0), false), "explicit exit() must always work");
    }

    #[test]
    fn hidden_start_from_pref_or_autostart_arg() {
        assert!(should_start_hidden(true, &[]));
        assert!(should_start_hidden(false, &["app".into(), "--hidden".into()]));
        assert!(!should_start_hidden(false, &["app".into()]));
    }
}
