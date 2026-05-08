//! Switch executor - handles account switches and optional Codex relaunch.

use anyhow::{Context, Result};
use chrono::Utc;
use tauri::{AppHandle, Emitter};
#[cfg(target_os = "macos")]
use tauri_plugin_notification::NotificationExt;

use crate::auth::storage::{get_active_account, set_active_account, touch_account};
use crate::process;
use crate::proxy;
use crate::session;
use crate::settings;
use crate::switch_log;
use crate::types::{AppSettings, SwitchEvent, SwitchReason};

pub async fn execute_switch(
    target_account_id: &str,
    reason: SwitchReason,
    app_handle: &AppHandle,
) -> Result<()> {
    let previous_active = get_active_account()?.map(|a| a.id);
    let settings = settings::load_settings().unwrap_or_default();
    let proxy_healthy = if settings.proxy_mode_enabled {
        proxy::is_local_proxy_listening(settings.proxy_port).await
    } else {
        false
    };
    let switch_mode = switch_mode_for_settings(&settings, proxy_healthy);

    // 1. Keep auth.json in sync unless the active local proxy can rotate tokens directly.
    if switch_mode == SwitchMode::RestartCodex {
        session::swap_active_auth(target_account_id).context("Failed to swap auth.json")?;
    }

    // 2. Detect if Codex is running
    let was_running = process::is_codex_desktop_running().unwrap_or(false);

    // 3. Notify user
    if was_running {
        if let Ok(_account) = crate::auth::storage::get_account(target_account_id) {
            #[cfg(target_os = "macos")]
            if let Some(name) = _account.map(|a| a.name) {
                let body = match switch_mode {
                    SwitchMode::ProxyHotSwap => {
                        format!("Switching to {} — proxy token updated", name)
                    }
                    SwitchMode::RestartCodex => {
                        format!("Switching to {} — Codex will restart", name)
                    }
                };
                let _ = app_handle
                    .notification()
                    .builder()
                    .title("Switching Codex Account")
                    .body(body)
                    .show();
            }
        }
    }

    // 4. Kill and relaunch Codex if running
    if was_running && switch_mode == SwitchMode::RestartCodex {
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
    touch_account(target_account_id).context("Failed to update account last-used timestamp")?;

    // 6. Log the switch
    let event = SwitchEvent {
        timestamp: Utc::now(),
        from_account_id: previous_active,
        to_account_id: target_account_id.to_string(),
        reason,
    };

    switch_log::append_switch_event(event.clone())?;

    // 7. Refresh tray state before notifying windows to reload.
    if let Err(err) = crate::tray::refresh_accounts_and_tray_menu(app_handle).await {
        tracing::warn!("Failed to refresh tray after switch: {err}");
    }

    // 8. Emit events
    let _ = app_handle.emit("account-switched", &event);

    tracing::info!(
        "Switch complete: {} -> {} ({:?}, mode={:?})",
        event.from_account_id.as_deref().unwrap_or("none"),
        event.to_account_id,
        reason,
        switch_mode
    );

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SwitchMode {
    RestartCodex,
    ProxyHotSwap,
}

fn switch_mode_for_settings(settings: &AppSettings, proxy_healthy: bool) -> SwitchMode {
    if desktop_proxy_auth_injection_supported() && settings.proxy_mode_enabled && proxy_healthy {
        SwitchMode::ProxyHotSwap
    } else {
        SwitchMode::RestartCodex
    }
}

fn desktop_proxy_auth_injection_supported() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_mode_restarts_codex_until_desktop_proxy_can_inject_auth() {
        let settings = AppSettings {
            proxy_mode_enabled: true,
            ..AppSettings::default()
        };

        assert_eq!(
            switch_mode_for_settings(&settings, true),
            SwitchMode::RestartCodex
        );
    }

    #[test]
    fn proxy_mode_falls_back_to_restart_when_listener_is_down() {
        let settings = AppSettings {
            proxy_mode_enabled: true,
            ..AppSettings::default()
        };

        assert_eq!(
            switch_mode_for_settings(&settings, false),
            SwitchMode::RestartCodex
        );
    }

    #[test]
    fn restart_mode_remains_default_even_if_port_has_listener() {
        let settings = AppSettings::default();

        assert_eq!(
            switch_mode_for_settings(&settings, true),
            SwitchMode::RestartCodex
        );
    }
}
