//! launchd commands

use crate::launchd::{self, LaunchdStatus};

#[tauri::command]
pub async fn get_launchd_status() -> Result<LaunchdStatus, String> {
    launchd::status().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_launchd_agent() -> Result<LaunchdStatus, String> {
    launchd::install().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_launchd_agent() -> Result<LaunchdStatus, String> {
    launchd::remove().map_err(|e| e.to_string())
}
