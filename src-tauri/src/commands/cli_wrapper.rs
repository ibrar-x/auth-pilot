//! CLI wrapper commands

use crate::cli_wrapper::{self, CliWrapperStatus};
use crate::proxy::DEFAULT_PROXY_PORT;
use crate::settings;

#[tauri::command]
pub async fn install_cli_wrapper() -> Result<CliWrapperStatus, String> {
    let proxy_port = settings::load_settings()
        .map(|settings| settings.proxy_port)
        .unwrap_or(DEFAULT_PROXY_PORT);
    cli_wrapper::install(proxy_port).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn remove_cli_wrapper() -> Result<CliWrapperStatus, String> {
    cli_wrapper::remove().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_cli_wrapper_status() -> Result<CliWrapperStatus, String> {
    cli_wrapper::status().map_err(|e| e.to_string())
}
