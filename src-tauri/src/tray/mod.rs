//! Tray manager - system tray icon and menu

use std::sync::Arc;
use tokio::sync::RwLock;

use tauri::{
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, Wry,
};
use tauri_plugin_notification::NotificationExt;

use crate::switch_executor;
use crate::types::{MonitorState, SwitchReason, UsageInfo};

pub fn setup_tray(
    app: &AppHandle,
    state: Arc<RwLock<MonitorState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (usages, cached_accounts) = tauri::async_runtime::block_on(async {
        let state_guard = state.read().await;
        (
            state_guard.latest_usages.clone(),
            state_guard.cached_accounts.clone(),
        )
    });
    let menu = build_tray_menu(app, &usages, cached_accounts.as_ref())?;

    let menu_state = state.clone();
    let tray_icon_bytes = include_bytes!("../../icons/tray-icon-white.png");
    let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes)
        .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());

    let _tray = TrayIconBuilder::with_id("main")
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(true)
        .icon_as_template(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();

            if id == "quit" {
                app.exit(0);
            } else if id == "show_dashboard" {
                let _ = show_dashboard(app);
            } else if id == "show_settings" {
                let _ = show_dashboard(app);
            } else if id.starts_with("switch_") {
                let account_id = id.strip_prefix("switch_").unwrap_or("");
                tracing::info!("Tray switch triggered for account: {}", account_id);
                if !account_id.is_empty() {
                    let app_handle = app.clone();
                    let account_id = account_id.to_string();
                    let state = menu_state.clone();
                    tauri::async_runtime::spawn(async move {
                        tracing::info!("Starting switch execution for: {}", account_id);
                        match switch_executor::execute_switch(
                            &account_id,
                            SwitchReason::Manual,
                            &app_handle,
                        )
                        .await
                        {
                            Ok(_) => {
                                tracing::info!("Tray switch succeeded for: {}", account_id);
                                // Refresh tray menu to update active account
                                if let Some(tray) = app_handle.tray_by_id("main") {
                                    let (usages, cached_accounts) = {
                                        let state_guard = state.read().await;
                                        (
                                            state_guard.latest_usages.clone(),
                                            state_guard.cached_accounts.clone(),
                                        )
                                    };
                                    if let Ok(new_menu) = build_tray_menu(
                                        &app_handle,
                                        &usages,
                                        cached_accounts.as_ref(),
                                    ) {
                                        let _ = tray.set_menu(Some(new_menu));
                                    }
                                }
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = app_handle
                                        .notification()
                                        .builder()
                                        .title("AuthPilot")
                                        .body("Account switched successfully")
                                        .show();
                                }
                            }
                            Err(e) => {
                                tracing::error!("Tray switch failed for {}: {}", account_id, e);
                                #[cfg(target_os = "macos")]
                                {
                                    let _ = app_handle
                                        .notification()
                                        .builder()
                                        .title("AuthPilot")
                                        .body(format!("Switch failed: {}", e))
                                        .show();
                                }
                            }
                        }
                    });
                }
            }
        })
        .build(app)?;

    // Update tray periodically
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(60)).await;
            if let Err(e) = update_tray_menu(&app_handle, &state).await {
                tracing::error!("Failed to update tray menu: {}", e);
            }
        }
    });

    Ok(())
}

fn build_tray_menu(
    app: &AppHandle,
    usages: &[UsageInfo],
    store: Option<&crate::types::AccountsStore>,
) -> Result<Menu<Wry>, Box<dyn std::error::Error>> {
    let menu = Menu::new(app)?;

    // Active account info
    let active_label = if let Some(store) = store {
        store
            .active_account_id
            .as_ref()
            .and_then(|id| {
                store
                    .accounts
                    .iter()
                    .find(|a| a.id == *id)
                    .map(|a| format!("Active: {}", a.name))
            })
            .unwrap_or_else(|| "AuthPilot".to_string())
    } else {
        "AuthPilot".to_string()
    };
    let active_item = MenuItem::with_id(app, "active", active_label, false, None::<&str>)?;
    menu.append(&active_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    // Account switches (skip active account)
    if let Some(store) = store {
        for account in &store.accounts {
            // Skip the currently active account
            if store.active_account_id.as_deref() == Some(&account.id) {
                continue;
            }

            let usage = usages.iter().find(|u| u.account_id == account.id);
            let label = if let Some(u) = usage {
                let primary = u
                    .primary_used_percent
                    .map(|p| format!("{:.0}%", p))
                    .unwrap_or_else(|| "?".to_string());
                let secondary = u
                    .secondary_used_percent
                    .map(|p| format!("{:.0}%", p))
                    .unwrap_or_else(|| "?".to_string());
                format!(
                    "{} — 5h window: {} | 7-day: {}",
                    account.name, primary, secondary
                )
            } else {
                format!("{} — loading...", account.name)
            };

            let item = MenuItem::with_id(
                app,
                format!("switch_{}", account.id),
                label,
                true,
                None::<&str>,
            )?;
            menu.append(&item)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;

    let settings = MenuItem::with_id(
        app,
        "show_settings",
        "Open Dashboard...",
        true,
        None::<&str>,
    )?;
    menu.append(&settings)?;

    let quit = MenuItem::with_id(app, "quit", "Quit AuthPilot", true, None::<&str>)?;
    menu.append(&quit)?;

    Ok(menu)
}

async fn update_tray_menu(
    app: &AppHandle,
    state: &Arc<RwLock<MonitorState>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let (usages, cached_accounts) = {
        let state_guard = state.read().await;
        (
            state_guard.latest_usages.clone(),
            state_guard.cached_accounts.clone(),
        )
    };

    if let Some(tray) = app.tray_by_id("main") {
        let menu = build_tray_menu(app, &usages, cached_accounts.as_ref())?;
        tray.set_menu(Some(menu))?;
    }

    Ok(())
}

fn show_dashboard(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.set_focus()?;
    }
    Ok(())
}
