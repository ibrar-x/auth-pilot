//! Certificate commands

use crate::cert::{self, CaStatus};

#[tauri::command]
pub async fn get_proxy_ca_status() -> Result<CaStatus, String> {
    cert::ca_status().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn generate_proxy_ca() -> Result<CaStatus, String> {
    cert::generate_or_load_ca().map_err(|e| e.to_string())?;
    cert::ca_status().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn regenerate_proxy_ca() -> Result<CaStatus, String> {
    cert::regenerate_ca().map_err(|e| e.to_string())?;
    cert::ca_status().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn install_proxy_ca_trust() -> Result<CaStatus, String> {
    cert::install_ca_trust().map_err(|e| e.to_string())
}
