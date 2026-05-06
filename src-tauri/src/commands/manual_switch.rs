//! Manual switch command

use crate::switch_executor;
use crate::types::SwitchReason;

#[tauri::command]
pub async fn manual_switch_account(
    account_id: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    switch_executor::execute_switch(&account_id, SwitchReason::Manual, &app_handle)
        .await
        .map_err(|e| e.to_string())
}
