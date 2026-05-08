//! Switch executor - handles the kill + relaunch sequence

use anyhow::{Context, Result};
use chrono::Utc;
use tauri::{AppHandle, Emitter};
#[cfg(target_os = "macos")]
use tauri_plugin_notification::NotificationExt;

use crate::auth::storage::{get_active_account, set_active_account, touch_account};
use crate::process;
use crate::session;
use crate::settings;
use crate::switch_log;
use crate::types::{SwitchEvent, SwitchReason};

pub async fn execute_switch(
    target_account_id: &str,
    reason: SwitchReason,
    app_handle: &AppHandle,
) -> Result<()> {
    let previous_active = get_active_account()?.map(|a| a.id);
    let auto_resume_after_restart = settings::load_settings()
        .map(|settings| settings.auto_resume_after_restart)
        .unwrap_or(true);
    let mut codex_stopped_at = None;
    let mut codex_restarted_at = None;
    let mut auto_resume_attempted = false;
    let mut auto_resume_started = false;

    // 1. Write auth.json
    session::swap_active_auth(target_account_id).context("Failed to swap auth.json")?;

    // 2. Detect if Codex is running
    let was_running = process::is_codex_desktop_running().unwrap_or(false);
    let recent_codex_session = if was_running {
        match process::latest_recent_codex_session() {
            Ok(session) => session,
            Err(err) => {
                tracing::warn!("Failed to capture recent Codex session before switch: {err}");
                None
            }
        }
    } else {
        None
    };

    // 3. Notify user
    if was_running {
        if let Ok(_account) = crate::auth::storage::get_account(target_account_id) {
            #[cfg(target_os = "macos")]
            if let Some(name) = _account.map(|a| a.name) {
                let _ = app_handle
                    .notification()
                    .builder()
                    .title("Switching Codex Account")
                    .body(format!("Switching to {} — Codex will restart", name))
                    .show();
            }
        }
    }

    // 4. Kill and relaunch Codex if running
    if was_running {
        codex_stopped_at = Some(Utc::now());
        if let Err(e) = process::kill_codex_desktop().await {
            tracing::error!("Failed to kill Codex: {}", e);
        }

        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        if let Err(e) = process::launch_codex_desktop().await {
            tracing::error!("Failed to launch Codex: {}", e);
        } else {
            codex_restarted_at = Some(Utc::now());
            if auto_resume_after_restart {
                if let Some(session) = &recent_codex_session {
                    auto_resume_attempted = true;
                    if let Err(err) = process::resume_codex_session_continue(session) {
                        tracing::warn!(
                            "Failed to resume Codex session {} after switch: {err}",
                            session.session_id
                        );
                    } else {
                        auto_resume_started = true;
                    }
                }
            }
        }
    }

    // 5. Update active account
    set_active_account(target_account_id).context("Failed to update active account")?;
    touch_account(target_account_id).context("Failed to update account last-used timestamp")?;

    // 6. Log the switch
    let event = SwitchEvent {
        timestamp: Utc::now(),
        from_account_id: previous_active,
        to_account_id: target_account_id.to_string(),
        reason,
        codex_was_running: was_running,
        codex_stopped_at,
        codex_restarted_at,
        recovery_session_id: recent_codex_session.map(|session| session.session_id.to_string()),
        auto_resume_attempted,
        auto_resume_started,
    };

    switch_log::append_switch_event(event.clone())?;

    // 7. Refresh tray state before notifying windows to reload.
    if let Err(err) = crate::tray::refresh_accounts_and_tray_menu(app_handle).await {
        tracing::warn!("Failed to refresh tray after switch: {err}");
    }

    // 8. Emit events
    let _ = app_handle.emit("account-switched", &event);

    tracing::info!(
        "Switch complete: {} -> {} ({:?})",
        event.from_account_id.as_deref().unwrap_or("none"),
        event.to_account_id,
        reason
    );

    Ok(())
}
