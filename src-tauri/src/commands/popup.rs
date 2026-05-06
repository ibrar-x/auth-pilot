//! Tray popup Tauri commands

use crate::auth::storage::{get_active_account, load_accounts_with_current_active};
use crate::switch_executor;
use crate::tray::PopupInteractionState;
use crate::types::{AccountInfo, SwitchReason, UsageInfo};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

use crate::types::MonitorState;

#[derive(Debug, serde::Serialize)]
pub struct TrayPopupData {
    pub active_account: Option<AccountInfo>,
    pub accounts: Vec<AccountInfo>,
    pub usages: Vec<UsageInfo>,
}

#[tauri::command]
pub async fn get_tray_popup_data(
    state: tauri::State<'_, Arc<RwLock<MonitorState>>>,
) -> Result<TrayPopupData, String> {
    let store = load_accounts_with_current_active().map_err(|e| e.to_string())?;
    let active_id = store.active_account_id.as_deref();

    let active_account = get_active_account()
        .map_err(|e| e.to_string())?
        .map(|a| AccountInfo::from_stored(&a, active_id));

    let accounts: Vec<AccountInfo> = store
        .accounts
        .iter()
        .map(|a| AccountInfo::from_stored(a, active_id))
        .collect();

    let usages = {
        let state_guard = state.read().await;
        state_guard.latest_usages.clone()
    };

    Ok(TrayPopupData {
        active_account,
        accounts,
        usages,
    })
}

#[tauri::command]
pub async fn popup_switch_account(
    account_id: String,
    app: AppHandle,
    state: tauri::State<'_, Arc<RwLock<MonitorState>>>,
) -> Result<(), String> {
    switch_executor::execute_switch(&account_id, SwitchReason::Manual, &app)
        .await
        .map_err(|e| e.to_string())?;

    if let Ok(new_store) = load_accounts_with_current_active() {
        let mut state_guard = state.write().await;
        state_guard.cached_accounts = Some(new_store);
    }

    Ok(())
}

#[tauri::command]
pub fn tray_popup_interaction(
    interaction: tauri::State<'_, PopupInteractionState>,
) -> Result<(), String> {
    interaction.touch();
    Ok(())
}

#[tauri::command]
pub async fn show_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(popup) = app.get_webview_window("tray-popup") {
        let _ = popup.hide();
    }
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<(), String> {
    if let Some(popup) = app.get_webview_window("tray-popup") {
        let _ = popup.hide();
    }
    if let Some(window) = app.get_webview_window("main") {
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())?;
        let _ = window.emit("open-settings", ());
    }
    Ok(())
}
