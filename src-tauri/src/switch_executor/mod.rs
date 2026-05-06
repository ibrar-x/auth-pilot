//! Switch executor - handles the kill + relaunch sequence

use anyhow::{Context, Result};
use chrono::Utc;
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

use crate::auth::storage::{get_active_account, set_active_account};
use crate::process;
use crate::session;
use crate::switch_log;
use crate::types::{SwitchEvent, SwitchReason};

pub async fn execute_switch(
    target_account_id: &str,
    reason: SwitchReason,
    app_handle: &AppHandle,
) -> Result<()> {
    let previous_active = get_active_account()?.map(|a| a.id);

    // 1. Write auth.json
    session::swap_active_auth(target_account_id).context("Failed to swap auth.json")?;

    // 2. Detect if Codex is running
    let was_running = process::is_codex_desktop_running().unwrap_or(false);

    // 3. Notify user
    if was_running {
        if let Ok(account) = crate::auth::storage::get_account(target_account_id) {
            if let Some(name) = account.map(|a| a.name) {
                #[cfg(target_os = "macos")]
                {
                    let _ = app_handle
                        .notification()
                        .builder()
                        .title("Switching Codex Account")
                        .body(format!("Switching to {} — Codex will restart", name))
                        .show();
                }
            }
        }
    }

    // 4. Kill and relaunch Codex if running
    if was_running {
        if let Err(e) = process::kill_codex_desktop().await {
            tracing::error!("Failed to kill Codex: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if let Err(e) = process::launch_codex_desktop().await {
            tracing::error!("Failed to launch Codex: {}", e);
        }
    }

    // 5. Update active account
    set_active_account(target_account_id).context("Failed to update active account")?;

    // 6. Log the switch
    let event = SwitchEvent {
        timestamp: Utc::now(),
        from_account_id: previous_active,
        to_account_id: target_account_id.to_string(),
        reason,
    };

    switch_log::append_switch_event(event.clone())?;

    // 7. Emit events
    let _ = app_handle.emit("account-switched", &event);

    tracing::info!(
        "Switch complete: {} -> {} ({:?})",
        event.from_account_id.as_deref().unwrap_or("none"),
        event.to_account_id,
        reason
    );

    Ok(())
}
