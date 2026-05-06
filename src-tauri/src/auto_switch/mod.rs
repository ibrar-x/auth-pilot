//! Auto-switch engine - decides when and where to switch

use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{Context, Result};
use chrono::Utc;
use tauri::{AppHandle, Emitter};

use crate::auth::storage::load_accounts_with_current_active;
use crate::session;
use crate::settings;
use crate::switch_executor;
use crate::switch_log;
use crate::types::{AccountsStore, AppSettings, SwitchEvent, SwitchReason, UsageInfo};

pub fn should_auto_switch(
    usage: &UsageInfo,
    settings: &AppSettings,
    _accounts: &AccountsStore,
) -> bool {
    if !settings.auto_switch_enabled {
        return false;
    }

    // Check global cooldown
    if let Some(last) = settings.last_auto_switch {
        let elapsed = Utc::now().signed_duration_since(last).num_seconds();
        if elapsed < settings.global_cooldown_seconds as i64 {
            tracing::info!(
                "Cooldown active: {}s remaining",
                settings.global_cooldown_seconds as i64 - elapsed
            );
            return false;
        }
    }

    let threshold = settings
        .account_settings
        .get(&usage.account_id)
        .map(|s| s.switch_threshold)
        .unwrap_or(95.0);

    let over_threshold = usage.primary_used_percent.is_some_and(|p| p >= threshold);

    if over_threshold {
        tracing::info!(
            "Account {} crossed threshold: {:.1}% >= {:.1}%",
            usage.account_id,
            usage.primary_used_percent.unwrap_or(0.0),
            threshold
        );
    }

    over_threshold
}

pub async fn trigger(
    active_account_id: String,
    app_handle: &AppHandle,
    state: &Arc<RwLock<crate::types::MonitorState>>,
) -> Result<()> {
    tracing::info!("Auto-switch triggered for account {}", active_account_id);

    let store = load_accounts_with_current_active()?;
    let usages = {
        let state_guard = state.read().await;
        state_guard.latest_usages.clone()
    };

    let target = select_target_account(&active_account_id, &store, &usages)?;

    let reason = if all_accounts_depleted(&active_account_id, &usages, &store) {
        SwitchReason::AutoDepleted
    } else {
        SwitchReason::AutoLimitReached
    };

    tracing::info!("Selected target account: {}", target);

    // Execute the switch
    switch_executor::execute_switch(&target, reason, app_handle).await?;

    // Update cooldown
    settings::update_last_auto_switch(Utc::now())?;

    // Update state
    {
        let mut state_guard = state.write().await;
        state_guard.settings.last_auto_switch = Some(Utc::now());
    }

    let event = SwitchEvent {
        timestamp: Utc::now(),
        from_account_id: Some(active_account_id),
        to_account_id: target,
        reason,
    };

    switch_log::append_switch_event(event.clone())?;
    let _ = app_handle.emit("auto-switch-triggered", &event);
    let _ = app_handle.emit("account-switched", &event);

    Ok(())
}

fn select_target_account(
    active_account_id: &str,
    store: &AccountsStore,
    usages: &[UsageInfo],
) -> Result<String> {
    let mut candidates: Vec<(String, f64, Option<i64>)> = Vec::new();

    for account in &store.accounts {
        if account.id == active_account_id {
            continue;
        }

        // Skip accounts with errors
        let usage = usages.iter().find(|u| u.account_id == account.id);
        if usage.map(|u| u.error.is_some()).unwrap_or(true) {
            continue;
        }

        // Skip expired tokens
        if let Ok(auth) = session::restore_account(&account.id) {
            if session::is_token_expired(&auth) {
                tracing::warn!("Account {} has expired token, skipping", account.name);
                continue;
            }
        }

        let used_percent = usage.and_then(|u| u.primary_used_percent).unwrap_or(100.0);
        let remaining = 100.0 - used_percent;
        let resets_at = usage.and_then(|u| u.primary_resets_at);

        candidates.push((account.id.clone(), remaining, resets_at));
    }

    if candidates.is_empty() {
        // All accounts depleted - pick soonest reset
        let mut all_accounts: Vec<(String, Option<i64>)> = store
            .accounts
            .iter()
            .filter(|a| a.id != active_account_id)
            .map(|a| {
                let usage = usages.iter().find(|u| u.account_id == a.id);
                let resets_at = usage.and_then(|u| u.primary_resets_at);
                (a.id.clone(), resets_at)
            })
            .collect();

        all_accounts.sort_by_key(|(_, resets_at)| *resets_at);

        return all_accounts
            .first()
            .map(|(id, _)| id.clone())
            .context("No alternative accounts available");
    }

    // Sort by highest remaining, then earliest reset
    candidates.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.2.cmp(&b.2))
    });

    Ok(candidates[0].0.clone())
}

fn all_accounts_depleted(
    active_account_id: &str,
    usages: &[UsageInfo],
    store: &AccountsStore,
) -> bool {
    let threshold = 95.0;

    for account in &store.accounts {
        if account.id == active_account_id {
            continue;
        }

        let usage = usages.iter().find(|u| u.account_id == account.id);
        let used = usage.and_then(|u| u.primary_used_percent).unwrap_or(100.0);

        if used < threshold {
            return false;
        }
    }

    true
}
