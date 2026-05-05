//! Session commands

#[tauri::command]
pub async fn ensure_file_auth_mode() -> Result<bool, String> {
    crate::session::ensure_file_auth_mode().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn is_file_auth_mode_required() -> Result<bool, String> {
    crate::session::is_file_auth_mode_required().map_err(|e| e.to_string())
}
