//! Switch log commands

use crate::switch_log;
use crate::types::SwitchEvent;

#[tauri::command]
pub async fn get_switch_log() -> Result<Vec<SwitchEvent>, String> {
    switch_log::get_switch_events().map_err(|e| e.to_string())
}
