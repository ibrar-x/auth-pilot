//! Tray manager - system tray icon and popup window

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
    let menu = build_fallback_menu(app)?;

    let tray_state = state.clone();
    let tray_icon_bytes = include_bytes!("../../icons/tray-icon-white.png");
    let tray_icon = tauri::image::Image::from_bytes(tray_icon_bytes)
        .unwrap_or_else(|_| app.default_window_icon().unwrap().clone());

    let _tray = TrayIconBuilder::with_id("main")
        .icon(tray_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .icon_as_template(true)
        .on_menu_event(move |app, event| {
            let id = event.id().as_ref();
            if id == "quit" {
                app.exit(0);
            } else if id == "show_dashboard" {
                let _ = show_dashboard(app);
            }
        })
        .on_tray_icon_event(move |tray, event| {
            if let tauri::tray::TrayIconEvent::Click { button, .. } = event {
                match button {
                    tauri::tray::MouseButton::Left => {
                        let app = tray.app_handle();
                        toggle_tray_popup(app, &tray_state);
                    }
                    tauri::tray::MouseButton::Right => {}
                    _ => {}
                }
            }
        })
        .build(app)?;

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

fn toggle_tray_popup(app: &AppHandle, _state: &Arc<RwLock<MonitorState>>) {
    if let Some(popup) = app.get_webview_window("tray-popup") {
        let is_visible = popup.is_visible().unwrap_or(false);
        if is_visible {
            let _ = popup.hide();
        } else {
            position_popup_near_tray(app, &popup);
            let _ = popup.show();
            let _ = popup.set_focus();
        }
        return;
    }

    let popup_result = tauri::webview::WebviewWindowBuilder::new(
        app,
        "tray-popup",
        tauri::WebviewUrl::App("index.html".into()),
    )
    .title("AuthPilot")
    .inner_size(272.0, 420.0)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .resizable(false)
    .visible(false)
    .build();

    match popup_result {
        Ok(popup) => {
            let popup_clone = popup.clone();
            popup.on_window_event(move |event| {
                if let tauri::WindowEvent::Focused(is_focused) = event {
                    if !is_focused {
                        let _ = popup_clone.hide();
                    }
                }
            });

            position_popup_near_tray(app, &popup);
            let _ = popup.show();
            let _ = popup.set_focus();
        }
        Err(e) => {
            tracing::error!("Failed to create tray popup: {}", e);
        }
    }
}

#[cfg(target_os = "macos")]
fn position_popup_near_tray(app: &AppHandle, popup: &tauri::WebviewWindow) {
    use tauri::Monitor;

    if let Some(tray) = app.tray_by_id("main") {
        if let Ok(Some(rect)) = tray.rect() {
            let (tray_x, tray_y) = match rect.position {
                tauri::Position::Physical(p) => (p.x as f64, p.y as f64),
                tauri::Position::Logical(p) => (p.x, p.y),
            };
            let (tray_w, tray_h) = match rect.size {
                tauri::Size::Physical(s) => (s.width as f64, s.height as f64),
                tauri::Size::Logical(s) => (s.width, s.height),
            };

            let popup_w = 272.0;
            let popup_h = 420.0;

            let mut x = tray_x + (tray_w / 2.0) - (popup_w / 2.0);
            let mut y = tray_y + tray_h + 4.0;

            if let Ok(Some(monitor)) = popup.current_monitor() {
                let m_size = monitor.size();
                let m_pos = monitor.position();
                let m_w = m_size.width as f64;
                let m_h = m_size.height as f64;
                let m_x = m_pos.x as f64;
                let m_y = m_pos.y as f64;

                if x + popup_w > m_x + m_w {
                    x = m_x + m_w - popup_w - 8.0;
                }
                if x < m_x + 4.0 {
                    x = m_x + 4.0;
                }
                if y + popup_h > m_y + m_h {
                    y = tray_y - popup_h - 4.0;
                }
            }

            let _ = popup.set_position(tauri::PhysicalPosition::new(x as i32, y as i32));
            return;
        }
    }

    let _ = popup.center();
}

#[cfg(not(target_os = "macos"))]
fn position_popup_near_tray(_app: &AppHandle, popup: &tauri::WebviewWindow) {
    let _ = popup.center();
}

fn build_fallback_menu(app: &AppHandle) -> Result<Menu<Wry>, Box<dyn std::error::Error>> {
    let menu = Menu::new(app)?;
    let dashboard = MenuItem::with_id(app, "show_dashboard", "Open Dashboard...", true, None::<&str>)?;
    menu.append(&dashboard)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let quit = MenuItem::with_id(app, "quit", "Quit AuthPilot", true, None::<&str>)?;
    menu.append(&quit)?;
    Ok(menu)
}

fn build_tray_menu(
    app: &AppHandle,
    usages: &[UsageInfo],
    store: Option<&crate::types::AccountsStore>,
) -> Result<Menu<Wry>, Box<dyn std::error::Error>> {
    let menu = Menu::new(app)?;
    let active_label = if let Some(store) = store {
        store.active_account_id.as_ref()
            .and_then(|id| store.accounts.iter().find(|a| a.id == *id).map(|a| format!("Active: {}", a.name)))
            .unwrap_or_else(|| "AuthPilot".to_string())
    } else {
        "AuthPilot".to_string()
    };
    let active_item = MenuItem::with_id(app, "active", active_label, false, None::<&str>)?;
    menu.append(&active_item)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;

    if let Some(store) = store {
        for account in &store.accounts {
            if store.active_account_id.as_deref() == Some(&account.id) {
                continue;
            }
            let usage = usages.iter().find(|u| u.account_id == account.id);
            let label = if let Some(u) = usage {
                let primary = u.primary_used_percent.map(|p| format!("{:.0}%", p)).unwrap_or_else(|| "?".to_string());
                let secondary = u.secondary_used_percent.map(|p| format!("{:.0}%", p)).unwrap_or_else(|| "?".to_string());
                format!("{} — 5h window: {} | 7-day: {}", account.name, primary, secondary)
            } else {
                format!("{} — loading...", account.name)
            };
            let item = MenuItem::with_id(app, format!("switch_{}", account.id), label, true, None::<&str>)?;
            menu.append(&item)?;
        }
    }

    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let settings = MenuItem::with_id(app, "show_settings", "Open Dashboard...", true, None::<&str>)?;
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
        (state_guard.latest_usages.clone(), state_guard.cached_accounts.clone())
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
