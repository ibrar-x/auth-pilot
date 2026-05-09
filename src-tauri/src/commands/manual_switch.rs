//! Manual switch command

use crate::switch_executor;
use crate::types::{MonitorState, SwitchReason};
use std::sync::Arc;
use tokio::sync::RwLock;

#[tauri::command]
pub async fn manual_switch_account(
    account_id: String,
    app_handle: tauri::AppHandle,
    state: tauri::State<'_, Arc<RwLock<MonitorState>>>,
) -> Result<(), String> {
    switch_executor::execute_switch(&account_id, SwitchReason::Manual, &app_handle)
        .await
        .map_err(|e| e.to_string())?;

    crate::monitor::reset_after_manual_switch(&state).await;

    Ok(())
}
