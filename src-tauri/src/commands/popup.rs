//! Tray popup Tauri commands

use crate::api::usage::refresh_all_usage;
use crate::auth::storage::{get_active_account, load_accounts_with_current_active};
use crate::auto_switch;
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
    app: AppHandle,
    state: tauri::State<'_, Arc<RwLock<MonitorState>>>,
) -> Result<TrayPopupData, String> {
    refresh_usage_and_switch_if_needed(&app, &state).await;

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

async fn refresh_usage_and_switch_if_needed(app: &AppHandle, state: &Arc<RwLock<MonitorState>>) {
    let Ok(store) = load_accounts_with_current_active() else {
        return;
    };

    let usages = refresh_all_usage(&store.accounts).await;
    {
        let mut state_guard = state.write().await;
        state_guard.cached_accounts = Some(store.clone());
        state_guard.latest_usages = usages.clone();
    }
    let _ = app.emit("usage-update", &usages);

    let Some(active_id) = store.active_account_id.clone() else {
        return;
    };
    let Some(usage) = usages.iter().find(|usage| usage.account_id == active_id) else {
        return;
    };

    let should_switch = {
        let state_guard = state.read().await;
        !crate::monitor::manual_switch_cooldown_active(&state_guard, chrono::Utc::now())
            && auto_switch::should_auto_switch(usage, &state_guard.settings, &store)
    };

    if !should_switch {
        return;
    }

    match auto_switch::trigger(active_id, app, state).await {
        Ok(auto_switch::TriggerOutcome::Switched) => {
            tracing::info!("Tray-open threshold check switched account");
        }
        Ok(auto_switch::TriggerOutcome::Deferred) => {
            tracing::info!("Tray-open threshold check deferred because Codex is busy");
        }
        Err(err) => {
            tracing::warn!("Tray-open threshold check failed: {err}");
        }
    }
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

    crate::monitor::reset_after_manual_switch(&state).await;

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
    crate::app_behavior::show_main_window(&app).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn quit_app(app: AppHandle) -> Result<(), String> {
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn open_settings(app: AppHandle) -> Result<(), String> {
    crate::app_behavior::show_main_window(&app).map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.emit("open-settings", ());
    }
    Ok(())
}
