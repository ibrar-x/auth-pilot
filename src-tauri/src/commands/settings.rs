//! Settings commands

use std::sync::Arc;

use crate::settings;
use crate::types::{AppSettings, MonitorState};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::RwLock;

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    settings::load_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_settings(new_settings: AppSettings, app: AppHandle) -> Result<(), String> {
    crate::app_behavior::apply_settings(&new_settings).map_err(|e| e.to_string())?;
    settings::save_settings(&new_settings).map_err(|e| e.to_string())?;

    if let Some(state) = app.try_state::<Arc<RwLock<MonitorState>>>() {
        let mut state_guard = state.write().await;
        state_guard.settings = new_settings.clone();
    }

    let _ = crate::tray::refresh_accounts_and_tray_menu(&app).await;
    let _ = app.emit("settings-updated", &new_settings);

    Ok(())
}

#[tauri::command]
pub async fn export_settings() -> Result<String, String> {
    let settings = settings::load_settings().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())
}
