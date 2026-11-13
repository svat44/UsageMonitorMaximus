//! Usage Monitor Maximus — Tauri shell. All monitoring logic is in `usage-core`.

mod commands;
mod events;
mod lifecycle;
mod logging;
mod smoke;
mod state;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::Manager;
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};
use usage_core::clock::SystemClock;
use usage_core::config::{self, AppConfig};
use usage_core::monitoring::{EngineDeps, MonitoringEngine};
use usage_core::paths;
use usage_core::providers::{
    simulated::SimulatedProvider, statusline::ClaudeCodeStatusLineProvider, Provider, ProviderRegistry,
};
use usage_core::storage::sqlite::SqliteRepository;
use usage_core::storage::RetentionPolicy;

use state::AppState;

/// Apply configuration changes that live outside the engine (OS autostart, log level).
pub(crate) fn apply_side_effects(app: &tauri::AppHandle, cfg: &AppConfig) {
    logging::set_level(&app.state::<AppState>().log, cfg.log_level);
    let autostart = app.autolaunch();
    let result = match autostart.is_enabled() {
        Ok(current) if current == cfg.launch_at_startup => Ok(()),
        _ if cfg.launch_at_startup => autostart.enable(),
        _ => autostart.disable(),
    };
    if let Err(e) = result {
        // Non-fatal: e.g. no autostart support in this environment.
        tracing::warn!(
            operation = "autostart",
            category = "configuration",
            "could not apply launch-at-startup: {e}"
        );
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            lifecycle::show_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .invoke_handler(tauri::generate_handler![
            commands::get_monitoring_snapshot,
            commands::get_diagnostics,
            commands::list_providers,
            commands::get_config,
            commands::update_config,
            commands::set_monitoring_enabled,
            commands::select_provider,
            commands::refresh_now,
            commands::get_statusline_setup,
            commands::hide_window,
            commands::quit_app,
            commands::smoke_report,
        ])
        .on_window_event(lifecycle::handle_window_event)
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = paths::default_data_dir();
            std::fs::create_dir_all(&data_dir)?;

            // Startup order: logging → database (+migrations) → config → providers → engine.
            let log = logging::init(&data_dir, AppConfig::default().log_level);
            tracing::info!(
                operation = "startup",
                version = env!("CARGO_PKG_VERSION"),
                "starting"
            );
            let repo = Arc::new(tauri::async_runtime::block_on(SqliteRepository::open(
                &paths::database_path(&data_dir),
            ))?);
            let cfg = tauri::async_runtime::block_on(config::load(repo.as_ref()))?;
            logging::set_level(&log, cfg.log_level);

            let mut registry = ProviderRegistry::new();
            registry.register(Arc::new(ClaudeCodeStatusLineProvider::new(paths::statusline_dir(
                &data_dir,
            ))) as Arc<dyn Provider>)?;
            registry.register(Arc::new(SimulatedProvider::new()) as Arc<dyn Provider>)?;
            // A persisted selection that no longer exists (provider removed) falls back to default.
            let cfg = if registry.get(&cfg.selected_provider).is_some() {
                cfg
            } else {
                AppConfig {
                    selected_provider: AppConfig::default().selected_provider,
                    ..cfg
                }
            };

            let engine = MonitoringEngine::new(EngineDeps {
                registry,
                repo,
                events: Arc::new(events::TauriEventSink(handle.clone())),
                clock: Arc::new(SystemClock),
                config: cfg.clone(),
                data_dir: data_dir.clone(),
                retention: RetentionPolicy::default(),
            });
            tauri::async_runtime::block_on(engine.initialize())?;

            app.manage(AppState {
                engine: engine.clone(),
                data_dir,
                quitting: AtomicBool::new(false),
                tray_available: AtomicBool::new(false),
                log,
            });

            let tray_ok = lifecycle::build_tray(&handle);
            app.state::<AppState>()
                .tray_available
                .store(tray_ok, Ordering::SeqCst);
            apply_side_effects(&handle, &cfg);

            let args: Vec<String> = std::env::args().collect();
            // Never start hidden if there is no tray to bring the window back from.
            if !(lifecycle::should_start_hidden(cfg.start_minimized, &args) && tray_ok) {
                lifecycle::show_main_window(&handle);
            }

            if cfg.monitoring_enabled {
                let e = engine.clone();
                tauri::async_runtime::spawn(async move {
                    if let Err(err) = e.start().await {
                        tracing::warn!(
                            operation = "start",
                            category = err.category().as_str(),
                            "monitoring did not start: {err}"
                        );
                    }
                });
            }
            smoke::maybe_run(&handle);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building the application");

    app.run(lifecycle::handle_run_event);
}
