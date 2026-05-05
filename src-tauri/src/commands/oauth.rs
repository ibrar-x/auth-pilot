//! OAuth login Tauri commands

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::auth::oauth_server::{start_oauth_login, wait_for_oauth_login, OAuthLoginResult};
use crate::auth::{
    add_account, load_accounts, set_active_account, switch_to_account, touch_account,
};
use crate::types::{AccountInfo, OAuthLoginInfo};

struct PendingOAuth {
    rx: oneshot::Receiver<anyhow::Result<OAuthLoginResult>>,
    cancelled: Arc<AtomicBool>,
}

static PENDING_OAUTH: Mutex<Option<PendingOAuth>> = Mutex::new(None);

#[tauri::command]
pub async fn start_login(account_name: String) -> Result<OAuthLoginInfo, String> {
    if let Some(previous) = {
        let mut pending = PENDING_OAUTH.lock().unwrap();
        pending.take()
    } {
        previous.cancelled.store(true, Ordering::Relaxed);
    }

    let (info, rx, cancelled) = start_oauth_login(account_name)
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut pending = PENDING_OAUTH.lock().unwrap();
        *pending = Some(PendingOAuth { rx, cancelled });
    }

    Ok(info)
}

#[tauri::command]
pub async fn complete_login() -> Result<AccountInfo, String> {
    let pending = {
        let mut pending = PENDING_OAUTH.lock().unwrap();
        pending
            .take()
            .ok_or_else(|| "No pending OAuth login".to_string())?
    };

    let account = wait_for_oauth_login(pending.rx)
        .await
        .map_err(|e| e.to_string())?;

    let stored = add_account(account).map_err(|e| e.to_string())?;

    set_active_account(&stored.id).map_err(|e| e.to_string())?;
    switch_to_account(&stored).map_err(|e| e.to_string())?;
    touch_account(&stored.id).map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    let active_id = store.active_account_id.as_deref();

    Ok(AccountInfo::from_stored(&stored, active_id))
}

#[tauri::command]
pub async fn cancel_login() -> Result<(), String> {
    let mut pending = PENDING_OAUTH.lock().unwrap();
    if let Some(pending_oauth) = pending.take() {
        pending_oauth.cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}
