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
    let previous_settings = settings::load_settings().unwrap_or_default();
    crate::app_behavior::apply_settings(&new_settings).map_err(|e| e.to_string())?;
    sync_start_at_login_if_changed(&previous_settings, &new_settings)?;
    settings::save_settings(&new_settings).map_err(|e| e.to_string())?;

    if let Some(state) = app.try_state::<Arc<RwLock<MonitorState>>>() {
        let mut state_guard = state.write().await;
        state_guard.settings = new_settings.clone();
    }

    if let Some(runtime) = app.try_state::<crate::proxy::SharedProxyRuntime>() {
        crate::proxy::sync_runtime(runtime.inner().clone(), &new_settings, Some(app.clone()))
            .await
            .map_err(|e| e.to_string())?;
    }

    let _ = crate::tray::refresh_accounts_and_tray_menu(&app).await;
    let _ = app.emit("settings-updated", &new_settings);

    Ok(())
}

fn sync_start_at_login_if_changed(
    previous_settings: &AppSettings,
    new_settings: &AppSettings,
) -> Result<(), String> {
    if previous_settings.start_at_login == new_settings.start_at_login {
        return Ok(());
    }

    if new_settings.start_at_login {
        crate::launchd::install().map_err(|e| e.to_string())?;
    } else {
        crate::launchd::remove().map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn export_settings() -> Result<String, String> {
    let settings = settings::load_settings().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())
}
