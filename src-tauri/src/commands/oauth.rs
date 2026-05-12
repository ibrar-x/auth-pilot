//! OAuth login Tauri commands

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::sync::oneshot;

use crate::auth::oauth_server::{start_oauth_login, wait_for_oauth_login, OAuthLoginResult};
use crate::auth::{
    add_account, get_account, load_accounts, save_accounts, set_active_account, switch_to_account,
    touch_account,
};
use crate::types::{AccountInfo, OAuthLoginInfo};

struct PendingOAuth {
    rx: oneshot::Receiver<anyhow::Result<OAuthLoginResult>>,
    cancelled: Arc<AtomicBool>,
    replace_account_id: Option<String>,
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
        *pending = Some(PendingOAuth {
            rx,
            cancelled,
            replace_account_id: None,
        });
    }

    Ok(info)
}

#[tauri::command]
pub async fn start_relogin(account_id: String) -> Result<OAuthLoginInfo, String> {
    let account = get_account(&account_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Account not found: {account_id}"))?;

    if account.auth_mode != crate::types::AuthMode::ChatGPT {
        return Err("Only ChatGPT accounts can be reconnected with browser login.".to_string());
    }

    if let Some(previous) = {
        let mut pending = PENDING_OAUTH.lock().unwrap();
        pending.take()
    } {
        previous.cancelled.store(true, Ordering::Relaxed);
    }

    tracing::info!(
        account_id = %account.id,
        account_name = %account.name,
        "Starting browser re-login for existing account"
    );

    let (info, rx, cancelled) = start_oauth_login(account.name)
        .await
        .map_err(|e| e.to_string())?;

    {
        let mut pending = PENDING_OAUTH.lock().unwrap();
        *pending = Some(PendingOAuth {
            rx,
            cancelled,
            replace_account_id: Some(account_id),
        });
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

    let stored = if let Some(account_id) = pending.replace_account_id {
        replace_existing_chatgpt_account(account_id, account).map_err(|e| e.to_string())?
    } else {
        add_account(account).map_err(|e| e.to_string())?
    };

    set_active_account(&stored.id).map_err(|e| e.to_string())?;
    switch_to_account(&stored).map_err(|e| e.to_string())?;
    touch_account(&stored.id).map_err(|e| e.to_string())?;

    let store = load_accounts().map_err(|e| e.to_string())?;
    let active_id = store.active_account_id.as_deref();

    Ok(AccountInfo::from_stored(&stored, active_id))
}

fn replace_existing_chatgpt_account(
    account_id: String,
    fresh_account: crate::types::StoredAccount,
) -> anyhow::Result<crate::types::StoredAccount> {
    let mut store = load_accounts()?;
    let existing = store
        .accounts
        .iter_mut()
        .find(|account| account.id == account_id)
        .ok_or_else(|| anyhow::anyhow!("Account not found: {account_id}"))?;

    if existing.auth_mode != crate::types::AuthMode::ChatGPT {
        anyhow::bail!("Only ChatGPT accounts can be reconnected with browser login");
    }

    existing.email = fresh_account.email;
    existing.plan_type = fresh_account.plan_type;
    existing.subscription_expires_at = fresh_account.subscription_expires_at;
    existing.auth_data = fresh_account.auth_data;

    let updated = existing.clone();
    save_accounts(&store)?;

    if let Err(err) = crate::session::snapshot_account_from_data(&updated) {
        tracing::warn!(
            account_id = %updated.id,
            "Failed to update auth snapshot after browser re-login: {err}"
        );
    }

    tracing::info!(
        account_id = %updated.id,
        account_name = %updated.name,
        "Browser re-login refreshed existing account tokens"
    );

    Ok(updated)
}

#[tauri::command]
pub async fn cancel_login() -> Result<(), String> {
    let mut pending = PENDING_OAUTH.lock().unwrap();
    if let Some(pending_oauth) = pending.take() {
        pending_oauth.cancelled.store(true, Ordering::Relaxed);
    }
    Ok(())
}
