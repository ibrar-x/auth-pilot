//! Auto-switch engine - decides when and where to switch

use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::{Context, Result};
use chrono::Utc;
use tauri::{AppHandle, Emitter};

use crate::auth::storage::load_accounts_with_current_active;
use crate::process;
use crate::session;
use crate::settings;
use crate::switch_executor;
use crate::switch_log;
use crate::types::{AccountsStore, AppSettings, SwitchEvent, SwitchReason, UsageInfo};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerOutcome {
    Switched,
    Deferred,
}

const CRITICAL_REMAINING_PERCENT: f64 = 5.0;

pub fn should_auto_switch(
    usage: &UsageInfo,
    settings: &AppSettings,
    _accounts: &AccountsStore,
) -> bool {
    if !settings.auto_switch_enabled {
        return false;
    }

    let threshold = settings
        .account_settings
        .get(&usage.account_id)
        .map(|s| s.switch_threshold)
        .unwrap_or(95.0);

    if !usage_windows_complete(usage) {
        tracing::info!(
            "Account {} missing 5h or weekly usage, skipping auto-switch decision: 5h={:?}, weekly={:?}",
            usage.account_id,
            usage.primary_used_percent,
            usage.secondary_used_percent
        );
        return false;
    }

    let critical_usage = usage_is_critical(usage);
    if !critical_usage {
        // Check global cooldown for ordinary threshold crossings. Critical
        // accounts keep retrying so Codex can be restarted as soon as it is idle.
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
    }

    let over_threshold = usage_window_over_threshold(usage, threshold);

    if over_threshold || critical_usage {
        tracing::info!(
            "Account {} crossed usage threshold: 5h={:?}, weekly={:?}, threshold={:.1}%, critical={}",
            usage.account_id,
            usage.primary_used_percent,
            usage.secondary_used_percent,
            threshold,
            critical_usage
        );
    }

    over_threshold || critical_usage
}

pub fn usage_is_critical(usage: &UsageInfo) -> bool {
    usage_window_remaining_at_or_below(usage, CRITICAL_REMAINING_PERCENT)
}

pub async fn trigger(
    active_account_id: String,
    app_handle: &AppHandle,
    state: &Arc<RwLock<crate::types::MonitorState>>,
) -> Result<TriggerOutcome> {
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

    let codex_busy = process::is_codex_desktop_busy().unwrap_or_else(|err| {
        tracing::warn!("Failed to inspect Codex activity before auto-switch: {err}");
        false
    });

    if should_defer_auto_switch(reason, codex_busy) {
        tracing::info!(
            "Auto-switch deferred for account {} because Codex still has active work",
            active_account_id
        );
        let _ = app_handle.emit("auto-switch-deferred", &target);
        return Ok(TriggerOutcome::Deferred);
    }

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

    Ok(TriggerOutcome::Switched)
}

fn select_target_account(
    active_account_id: &str,
    store: &AccountsStore,
    usages: &[UsageInfo],
) -> Result<String> {
    let mut candidates: Vec<(String, f64, Option<i64>)> = Vec::new();
    let threshold = 95.0;

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

        let Some(usage) = usage else {
            continue;
        };

        if !usage_has_capacity(usage, threshold) {
            continue;
        }

        let remaining = usage_remaining_capacity(usage);
        let resets_at = usage_next_reset(usage);

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
                let resets_at = usage.map(usage_next_reset).unwrap_or(None);
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
        if usage.is_some_and(|usage| usage_has_capacity(usage, threshold)) {
            return false;
        }
    }

    true
}

fn usage_window_over_threshold(usage: &UsageInfo, threshold: f64) -> bool {
    usage
        .primary_used_percent
        .is_some_and(|used| used >= threshold)
        || usage
            .secondary_used_percent
            .is_some_and(|used| used >= threshold)
}

fn usage_windows_complete(usage: &UsageInfo) -> bool {
    usage.primary_used_percent.is_some() && usage.secondary_used_percent.is_some()
}

fn usage_window_remaining_at_or_below(usage: &UsageInfo, remaining_threshold: f64) -> bool {
    usage.primary_used_percent.is_some_and(|used| {
        let remaining = (100.0 - used).clamp(0.0, 100.0);
        remaining <= remaining_threshold
    }) || usage.secondary_used_percent.is_some_and(|used| {
        let remaining = (100.0 - used).clamp(0.0, 100.0);
        remaining <= remaining_threshold
    })
}

fn usage_has_capacity(usage: &UsageInfo, threshold: f64) -> bool {
    if usage.error.is_some() {
        return false;
    }

    let primary_ok = usage
        .primary_used_percent
        .is_some_and(|used| used < threshold);
    let secondary_ok = usage
        .secondary_used_percent
        .is_some_and(|used| used < threshold);

    primary_ok && secondary_ok
}

fn usage_remaining_capacity(usage: &UsageInfo) -> f64 {
    let primary_remaining = usage
        .primary_used_percent
        .map(|used| 100.0 - used)
        .unwrap_or(0.0);
    let secondary_remaining = usage
        .secondary_used_percent
        .map(|used| 100.0 - used)
        .unwrap_or(0.0);

    primary_remaining.min(secondary_remaining).max(0.0)
}

fn usage_next_reset(usage: &UsageInfo) -> Option<i64> {
    match (usage.primary_resets_at, usage.secondary_resets_at) {
        (Some(primary), Some(secondary)) => Some(primary.min(secondary)),
        (Some(primary), None) => Some(primary),
        (None, Some(secondary)) => Some(secondary),
        (None, None) => None,
    }
}

fn should_defer_auto_switch(reason: SwitchReason, codex_busy: bool) -> bool {
    matches!(
        reason,
        SwitchReason::AutoLimitReached | SwitchReason::AutoDepleted
    ) && codex_busy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{AccountSettings, StoredAccount};
    use chrono::Utc;

    fn account(id: &str) -> StoredAccount {
        StoredAccount {
            id: id.to_string(),
            name: id.to_string(),
            email: None,
            plan_type: Some("plus".to_string()),
            subscription_expires_at: None,
            auth_mode: crate::types::AuthMode::ApiKey,
            auth_data: crate::types::AuthData::ApiKey {
                key: format!("sk-{id}"),
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    fn store(active_id: &str, ids: &[&str]) -> AccountsStore {
        AccountsStore {
            version: 1,
            accounts: ids.iter().map(|id| account(id)).collect(),
            active_account_id: Some(active_id.to_string()),
            masked_account_ids: Vec::new(),
        }
    }

    fn usage(account_id: &str, primary: f64, secondary: f64) -> UsageInfo {
        UsageInfo {
            account_id: account_id.to_string(),
            plan_type: Some("plus".to_string()),
            primary_used_percent: Some(primary),
            primary_window_minutes: Some(300),
            primary_resets_at: Some(1_700_000_000),
            secondary_used_percent: Some(secondary),
            secondary_window_minutes: Some(10_080),
            secondary_resets_at: Some(1_700_600_000),
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            error: None,
        }
    }

    fn settings() -> AppSettings {
        AppSettings {
            auto_switch_enabled: true,
            global_cooldown_seconds: 0,
            last_auto_switch: None,
            ..AppSettings::default()
        }
    }

    #[test]
    fn should_auto_switch_when_weekly_limit_crosses_threshold() {
        let usage = usage("active", 20.0, 100.0);

        assert!(should_auto_switch(
            &usage,
            &settings(),
            &store("active", &["active"])
        ));
    }

    #[test]
    fn target_selection_skips_account_with_exhausted_weekly_limit() {
        let store = store("active", &["active", "weekly-empty", "usable"]);
        let usages = vec![
            usage("weekly-empty", 0.0, 100.0),
            usage("usable", 40.0, 50.0),
        ];

        let selected = select_target_account("active", &store, &usages).unwrap();

        assert_eq!(selected, "usable");
    }

    #[test]
    fn all_accounts_depleted_when_only_weekly_limits_are_exhausted() {
        let store = store("active", &["active", "weekly-empty", "also-weekly-empty"]);
        let usages = vec![
            usage("weekly-empty", 0.0, 100.0),
            usage("also-weekly-empty", 20.0, 99.0),
        ];

        assert!(all_accounts_depleted("active", &usages, &store));
    }

    #[test]
    fn target_selection_ranks_by_tightest_remaining_window() {
        let store = store("active", &["active", "more_5h_less_weekly", "balanced"]);
        let usages = vec![
            usage("more_5h_less_weekly", 10.0, 90.0),
            usage("balanced", 40.0, 50.0),
        ];

        let selected = select_target_account("active", &store, &usages).unwrap();

        assert_eq!(selected, "balanced");
    }

    #[test]
    fn target_selection_skips_accounts_missing_weekly_usage() {
        let store = store("active", &["active", "missing-weekly", "usable"]);
        let mut missing_weekly = usage("missing-weekly", 10.0, 0.0);
        missing_weekly.secondary_used_percent = None;
        let usages = vec![missing_weekly, usage("usable", 60.0, 60.0)];

        let selected = select_target_account("active", &store, &usages).unwrap();

        assert_eq!(selected, "usable");
    }

    #[test]
    fn should_not_auto_switch_when_disabled_even_if_weekly_exhausted() {
        let mut settings = settings();
        settings.auto_switch_enabled = false;
        let usage = usage("active", 20.0, 100.0);

        assert!(!should_auto_switch(
            &usage,
            &settings,
            &store("active", &["active"])
        ));
    }

    #[test]
    fn should_not_auto_switch_when_active_weekly_usage_is_missing() {
        let mut active_usage = usage("active", 100.0, 0.0);
        active_usage.secondary_used_percent = None;

        assert!(!should_auto_switch(
            &active_usage,
            &settings(),
            &store("active", &["active"])
        ));
    }

    #[test]
    fn should_not_auto_switch_during_cooldown_for_non_critical_threshold_crossing() {
        let mut settings = settings();
        settings.global_cooldown_seconds = 300;
        settings.last_auto_switch = Some(Utc::now());
        settings
            .account_settings
            .insert("active".to_string(), AccountSettings { switch_threshold: 80.0 });
        let usage = usage("active", 85.0, 20.0);

        assert!(!should_auto_switch(
            &usage,
            &settings,
            &store("active", &["active"])
        ));
    }

    #[test]
    fn critical_remaining_usage_bypasses_cooldown() {
        let mut settings = settings();
        settings.global_cooldown_seconds = 300;
        settings.last_auto_switch = Some(Utc::now());
        let usage = usage("active", 95.0, 20.0);

        assert!(should_auto_switch(
            &usage,
            &settings,
            &store("active", &["active"])
        ));
    }

    #[test]
    fn auto_switch_defers_while_codex_is_busy() {
        assert!(should_defer_auto_switch(
            SwitchReason::AutoLimitReached,
            true
        ));
        assert!(should_defer_auto_switch(SwitchReason::AutoDepleted, true));
    }

    #[test]
    fn manual_switch_never_uses_auto_busy_deferral() {
        assert!(!should_defer_auto_switch(SwitchReason::Manual, true));
    }

    #[test]
    fn auto_switch_continues_when_codex_is_idle() {
        assert!(!should_defer_auto_switch(
            SwitchReason::AutoLimitReached,
            false
        ));
    }
}
