//! Background monitor - polls usage and triggers auto-switch

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

use chrono::{DateTime, Utc};
use tauri::{AppHandle, Emitter};

use crate::api::usage::refresh_all_usage;
use crate::auth::storage::load_accounts_with_current_active;
use crate::auto_switch;
use crate::settings;
use crate::types::MonitorState;

static MONITOR_CANCELLED: AtomicBool = AtomicBool::new(false);
const HARD_EXHAUSTED_RETRY_SECONDS: i64 = 30;

pub fn start_monitor(app_handle: AppHandle, state: Arc<RwLock<MonitorState>>) {
    tauri::async_runtime::spawn(async move {
        MONITOR_CANCELLED.store(false, Ordering::Relaxed);

        {
            let mut state_guard = state.write().await;
            state_guard.is_monitor_running = true;
        }

        tracing::info!("Background monitor started");

        loop {
            if MONITOR_CANCELLED.load(Ordering::Relaxed) {
                tracing::info!("Background monitor cancelled");
                break;
            }

            let poll_interval = {
                let state_guard = state.read().await;
                state_guard.settings.poll_interval_seconds
            };

            let mut critical_switch_pending = false;

            match load_accounts_with_current_active() {
                Ok(store) => {
                    {
                        let mut state_guard = state.write().await;
                        state_guard.cached_accounts = Some(store.clone());
                    }

                    let usages = refresh_all_usage(&store.accounts).await;

                    {
                        let mut state_guard = state.write().await;
                        state_guard.latest_usages = usages.clone();
                    }

                    let _ = app_handle.emit("usage-update", &usages);

                    let active_id = store.active_account_id.clone();
                    if let Some(active_id) = active_id {
                        if let Some(usage) = usages.iter().find(|u| u.account_id == active_id) {
                            let critical_usage = auto_switch::usage_is_critical(usage);
                            let hard_exhausted = auto_switch::usage_is_hard_exhausted(usage);
                            let should_switch = {
                                let state_guard = state.read().await;
                                !manual_switch_cooldown_active(&state_guard, Utc::now())
                                    && auto_switch::should_auto_switch(
                                        usage,
                                        &state_guard.settings,
                                        &store,
                                    )
                            };
                            critical_switch_pending = critical_usage && should_switch;

                            drop(store);

                            if should_switch {
                                let active_still_current = current_active_account_matches_decision(
                                    &active_id,
                                )
                                .unwrap_or_else(|err| {
                                    tracing::warn!(
                                        "Failed to re-check active account before auto-switch: {err}"
                                    );
                                    false
                                });

                                if !active_still_current {
                                    tracing::info!(
                                        "Auto-switch decision for account {} ignored because active account changed",
                                        active_id
                                    );
                                    critical_switch_pending = false;
                                    let mut state_guard = state.write().await;
                                    state_guard.last_hard_exhausted_auto_switch_attempt = None;
                                } else {
                                    let retry_allowed = if hard_exhausted {
                                        let should_retry = {
                                            let state_guard = state.read().await;
                                            hard_exhausted_retry_allowed(
                                                state_guard.last_hard_exhausted_auto_switch_attempt,
                                                Utc::now(),
                                            )
                                        };

                                        if !should_retry {
                                            tracing::info!(
                                            "Hard-exhausted auto-switch retry suppressed for account {}",
                                            active_id
                                        );
                                            critical_switch_pending = false;
                                            false
                                        } else {
                                            let mut state_guard = state.write().await;
                                            state_guard.last_hard_exhausted_auto_switch_attempt =
                                                Some(Utc::now());
                                            true
                                        }
                                    } else {
                                        true
                                    };

                                    if retry_allowed {
                                        match auto_switch::trigger(active_id, &app_handle, &state)
                                            .await
                                        {
                                            Ok(auto_switch::TriggerOutcome::Deferred) => {
                                                critical_switch_pending = critical_usage;
                                            }
                                            Ok(auto_switch::TriggerOutcome::Switched) => {
                                                critical_switch_pending = false;
                                                let mut state_guard = state.write().await;
                                                state_guard
                                                    .last_hard_exhausted_auto_switch_attempt = None;
                                            }
                                            Err(e) => {
                                                tracing::error!("Auto-switch failed: {}", e);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to load accounts: {}", e);
                }
            }

            if let Err(err) = crate::recovery::process_watch::check_cycle(&app_handle) {
                tracing::warn!("Recovery process check failed: {err}");
            }

            sleep(Duration::from_secs(next_poll_interval_seconds(
                poll_interval,
                critical_switch_pending,
                true,
            )))
            .await;
        }

        {
            let mut state_guard = state.write().await;
            state_guard.is_monitor_running = false;
        }
    });
}

fn hard_exhausted_retry_allowed(last_attempt: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    let Some(last_attempt) = last_attempt else {
        return true;
    };

    now.signed_duration_since(last_attempt).num_seconds() >= HARD_EXHAUSTED_RETRY_SECONDS
}

fn current_active_account_matches_decision(decision_account_id: &str) -> anyhow::Result<bool> {
    let store = load_accounts_with_current_active()?;
    Ok(active_account_matches_decision(
        decision_account_id,
        store.active_account_id.as_deref(),
    ))
}

fn active_account_matches_decision(
    decision_account_id: &str,
    current_account_id: Option<&str>,
) -> bool {
    current_account_id == Some(decision_account_id)
}

fn apply_manual_switch_auto_reset(state: &mut MonitorState, now: DateTime<Utc>) {
    state.settings.last_auto_switch = Some(now);
    state.last_hard_exhausted_auto_switch_attempt = None;
    state.last_manual_switch_at = Some(now);
}

pub fn manual_switch_cooldown_active(state: &MonitorState, now: DateTime<Utc>) -> bool {
    let Some(last_manual_switch_at) = state.last_manual_switch_at else {
        return false;
    };

    let cooldown_seconds = state.settings.global_cooldown_seconds as i64;
    cooldown_seconds > 0
        && now
            .signed_duration_since(last_manual_switch_at)
            .num_seconds()
            < cooldown_seconds
}

pub async fn reset_after_manual_switch(state: &Arc<RwLock<MonitorState>>) {
    let now = Utc::now();
    if let Err(err) = settings::update_last_auto_switch(now) {
        tracing::warn!("Failed to persist manual switch auto-switch cooldown reset: {err}");
    }

    let mut state_guard = state.write().await;
    apply_manual_switch_auto_reset(&mut state_guard, now);
    if let Ok(store) = load_accounts_with_current_active() {
        state_guard.cached_accounts = Some(store);
    }
}

pub fn stop_monitor() {
    MONITOR_CANCELLED.store(true, Ordering::Relaxed);
    tracing::info!("Monitor stop signal sent");
}

fn next_poll_interval_seconds(
    configured_interval_seconds: u64,
    critical_switch_pending: bool,
    fast_retry_enabled: bool,
) -> u64 {
    if critical_switch_pending && fast_retry_enabled {
        return 5;
    }

    configured_interval_seconds
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn critical_deferred_auto_switch_rechecks_after_five_seconds() {
        assert_eq!(next_poll_interval_seconds(60, true, true), 5);
    }

    #[test]
    fn hard_exhausted_retry_is_throttled_after_recent_attempt() {
        let now = Utc::now();

        assert!(!hard_exhausted_retry_allowed(
            Some(now - chrono::Duration::seconds(10)),
            now
        ));
    }

    #[test]
    fn hard_exhausted_retry_is_allowed_after_backoff() {
        let now = Utc::now();

        assert!(hard_exhausted_retry_allowed(
            Some(now - chrono::Duration::seconds(31)),
            now
        ));
    }

    #[test]
    fn stale_auto_switch_decision_is_invalid_after_active_account_changes() {
        assert!(!active_account_matches_decision(
            "old-account",
            Some("new-account")
        ));
    }

    #[test]
    fn manual_switch_reset_sets_cooldown_and_clears_hard_exhausted_retry() {
        let now = Utc::now();
        let mut state = MonitorState {
            last_hard_exhausted_auto_switch_attempt: Some(now - chrono::Duration::seconds(5)),
            ..MonitorState::default()
        };

        apply_manual_switch_auto_reset(&mut state, now);

        assert_eq!(state.settings.last_auto_switch, Some(now));
        assert_eq!(state.last_hard_exhausted_auto_switch_attempt, None);
        assert_eq!(state.last_manual_switch_at, Some(now));
        assert!(manual_switch_cooldown_active(&state, now));
    }
}
