//! macOS system proxy commands

use tauri::{AppHandle, Manager};

use crate::system_proxy::{self, SystemProxyStatus};

#[tauri::command]
pub async fn get_system_proxy_status() -> Result<SystemProxyStatus, String> {
    system_proxy::status().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn enable_system_proxy(app: AppHandle) -> Result<SystemProxyStatus, String> {
    let settings = crate::settings::load_settings().map_err(|e| e.to_string())?;
    if let Some(runtime) = app.try_state::<crate::proxy::SharedProxyRuntime>() {
        crate::proxy::sync_runtime(runtime.inner().clone(), &settings, Some(app.clone()))
            .await
            .map_err(|e| e.to_string())?;
    }

    let status = system_proxy::enable(settings.proxy_port).map_err(|e| e.to_string())?;
    system_proxy::watch_network_changes(settings.proxy_port);
    Ok(status)
}

#[tauri::command]
pub async fn disable_system_proxy() -> Result<SystemProxyStatus, String> {
    system_proxy::disable().map_err(|e| e.to_string())
}
