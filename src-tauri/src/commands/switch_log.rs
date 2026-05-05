//! Switch log commands

use crate::types::SwitchEvent;
use crate::switch_log;

#[tauri::command]
pub async fn get_switch_log() -> Result<Vec<SwitchEvent>, String> {
    switch_log::get_switch_events().map_err(|e| e.to_string())
}
