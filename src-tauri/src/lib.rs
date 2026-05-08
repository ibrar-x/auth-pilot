//! AuthPilot — Intelligent Codex Account Switcher

#![allow(unexpected_cfgs)]

pub mod api;
pub mod app_behavior;
pub mod auth;
pub mod auto_switch;
pub mod commands;
pub mod crypto;
pub mod monitor;
pub mod process;
pub mod recovery;
pub mod session;
pub mod settings;
pub mod switch_executor;
pub mod switch_log;
pub mod tray;
pub mod types;

use std::sync::Arc;
use tauri::Manager;
use tokio::sync::RwLock;

use commands::{
    add_account_from_file, cancel_login, complete_login, delete_account, ensure_file_auth_mode,
    export_accounts_full_encrypted_file, export_accounts_slim_text, export_settings,
    get_active_account_info, get_masked_account_ids, get_settings, get_switch_log,
    get_tray_popup_data, get_usage, import_accounts_full_encrypted_file, import_accounts_slim_text,
    is_file_auth_mode_required, list_accounts, manual_switch_account, open_settings,
    popup_switch_account, quit_app, refresh_account_metadata, refresh_all_accounts_usage,
    rename_account, save_settings, set_masked_account_ids, show_main_window, start_login,
    switch_account, tray_popup_interaction, warmup_account, warmup_all_accounts,
};
use types::MonitorState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Setup tracing
    setup_logging();

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init());

    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_global_shortcut::Builder::new().build());

    builder
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // Prevent window from closing, hide it instead
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .setup(|app| {
            // Migrate legacy accounts from ~/.codex-switcher/ if needed
            if let Err(e) = crate::auth::storage::migrate_legacy_accounts() {
                tracing::warn!("Legacy account migration failed or skipped: {}", e);
            }

            match crate::recovery::default_db_path()
                .and_then(|path| crate::recovery::session_db::SessionDb::open(&path))
            {
                Ok(db) => {
                    if let Err(err) = db.mark_stale_sessions_interrupted(
                        chrono::Utc::now() - chrono::Duration::seconds(120),
                    ) {
                        tracing::warn!("Failed to mark stale recovery sessions: {err}");
                    }
                }
                Err(err) => tracing::warn!("Failed to initialize recovery database: {err}"),
            }

            if let Err(err) = crate::recovery::hooks::install_hook_script() {
                tracing::warn!("Failed to install Codex recovery hooks: {err}");
            }

            let app_handle = app.handle().clone();

            // Initialize monitor state
            let settings = settings::load_settings().unwrap_or_default();
            if let Err(e) = app_behavior::apply_settings(&settings) {
                tracing::warn!("Failed to apply app behavior settings: {e}");
            }
            let cached_accounts = crate::auth::storage::load_accounts_with_current_active().ok();
            let state = Arc::new(RwLock::new(MonitorState {
                settings,
                latest_usages: Vec::new(),
                is_monitor_running: false,
                cached_accounts,
            }));

            app.manage(state.clone());

            // Setup tray
            if let Err(e) = tray::setup_tray(&app_handle, state.clone()) {
                tracing::error!("Failed to setup tray: {}", e);
            }

            // Start background monitor
            monitor::start_monitor(app_handle.clone(), state);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Account management
            list_accounts,
            get_active_account_info,
            add_account_from_file,
            switch_account,
            delete_account,
            rename_account,
            export_accounts_slim_text,
            import_accounts_slim_text,
            export_accounts_full_encrypted_file,
            import_accounts_full_encrypted_file,
            get_masked_account_ids,
            set_masked_account_ids,
            // OAuth
            start_login,
            complete_login,
            cancel_login,
            // Usage
            get_usage,
            refresh_account_metadata,
            refresh_all_accounts_usage,
            warmup_account,
            warmup_all_accounts,
            // Settings
            get_settings,
            save_settings,
            export_settings,
            // Session
            ensure_file_auth_mode,
            is_file_auth_mode_required,
            // Switch log
            get_switch_log,
            // Manual switch
            manual_switch_account,
            // Tray popup
            get_tray_popup_data,
            popup_switch_account,
            tray_popup_interaction,
            show_main_window,
            open_settings,
            quit_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| {
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                if let Err(err) = crate::app_behavior::show_main_window(app_handle) {
                    tracing::warn!("Failed to show dashboard after Dock reopen: {err}");
                }
            }
        });
}

fn setup_logging() {
    use tracing_appender::rolling::{RollingFileAppender, Rotation};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

    let config_dir = dirs::home_dir()
        .map(|h| h.join(".authpilot"))
        .unwrap_or_else(|| std::path::PathBuf::from("."));

    let file_appender = RollingFileAppender::new(Rotation::DAILY, config_dir, "authpilot.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Leak the guard so the background writer thread stays alive for the program lifetime
    let _ = Box::leak(Box::new(guard));

    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .with(tracing_subscriber::EnvFilter::new("info"))
        .init();
}

// Commands are defined in the commands module
