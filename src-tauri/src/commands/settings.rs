//! Settings commands

use crate::settings;
use crate::types::AppSettings;

#[tauri::command]
pub async fn get_settings() -> Result<AppSettings, String> {
    settings::load_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_settings(new_settings: AppSettings) -> Result<(), String> {
    settings::save_settings(&new_settings).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn export_settings() -> Result<String, String> {
    let settings = settings::load_settings().map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())
}
