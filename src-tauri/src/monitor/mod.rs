//! Background monitor - polls usage and triggers auto-switch

use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

use tauri::{AppHandle, Emitter};

use crate::api::usage::refresh_all_usage;
use crate::auth::storage::load_accounts_with_current_active;
use crate::auto_switch;
use crate::process;
use crate::types::{
    AutoSwitchDecisionKind, AutoSwitchDecisionReport, CodexActivityReport, MonitorState, UsageInfo,
};

static MONITOR_CANCELLED: AtomicBool = AtomicBool::new(false);
const CRITICAL_SESSION_FILE_GRACE_SECONDS: i64 = 60;

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
                    tracing::info!(
                        "[monitor] cycle: accounts={}, active_account={:?}, usages={}",
                        store.accounts.len(),
                        store.active_account_id,
                        usages.len()
                    );

                    {
                        let mut state_guard = state.write().await;
                        state_guard.latest_usages = usages.clone();
                    }

                    let _ = app_handle.emit("usage-update", &usages);

                    let active_id = store.active_account_id.clone();
                    if active_id.is_none() {
                        tracing::info!("[monitor] gate: no_active_account");
                        record_auto_switch_decision(
                            &app_handle,
                            &state,
                            auto_switch_decision_report(
                                AutoSwitchDecisionKind::NoActiveAccount,
                                None,
                                None,
                                false,
                                None,
                                None,
                                "No active account is configured",
                                None,
                            ),
                        )
                        .await;
                    }

                    if let Some(active_id) = active_id {
                        let active_usage = usages.iter().find(|u| u.account_id == active_id);
                        tracing::info!(
                            "[monitor] gate: active_usage_present={}, active_account={}",
                            active_usage.is_some(),
                            active_id
                        );

                        if let Some(usage) = active_usage {
                            tracing::info!(
                                "[monitor] gate: active_usage primary={:?}, secondary={:?}, error={:?}",
                                usage.primary_used_percent,
                                usage.secondary_used_percent,
                                usage.error
                            );
                            let critical_usage = auto_switch::usage_is_critical(usage);
                            let critical_age_seconds = {
                                let mut state_guard = state.write().await;
                                let auto_switch_enabled = state_guard.settings.auto_switch_enabled;

                                if critical_usage && auto_switch_enabled {
                                    if state_guard.critical_auto_switch_since.is_none() {
                                        state_guard.critical_auto_switch_since = Some(Utc::now());
                                        tracing::info!("[monitor] critical clock started");
                                    }
                                } else {
                                    state_guard.critical_auto_switch_since = None;
                                }

                                state_guard
                                    .critical_auto_switch_since
                                    .map(|started_at| (Utc::now() - started_at).num_seconds())
                                    .unwrap_or(0)
                            };
                            let (should_switch, auto_switch_enabled, usage_complete, threshold) = {
                                let state_guard = state.read().await;
                                tracing::info!(
                                    "[monitor] gate: auto_switch_enabled={}, poll_interval={}s, cooldown={}s",
                                    state_guard.settings.auto_switch_enabled,
                                    state_guard.settings.poll_interval_seconds,
                                    state_guard.settings.global_cooldown_seconds
                                );
                                critical_switch_pending =
                                    critical_usage && state_guard.settings.auto_switch_enabled;
                                let threshold = state_guard
                                    .settings
                                    .account_settings
                                    .get(&usage.account_id)
                                    .map(|settings| settings.switch_threshold)
                                    .unwrap_or(95.0);
                                (
                                    auto_switch::should_auto_switch(
                                        usage,
                                        &state_guard.settings,
                                        &store,
                                    ),
                                    state_guard.settings.auto_switch_enabled,
                                    usage.primary_used_percent.is_some()
                                        && usage.secondary_used_percent.is_some(),
                                    threshold,
                                )
                            };
                            tracing::info!(
                                "[monitor] gate: critical_usage={}, should_switch={}",
                                critical_usage,
                                should_switch
                            );

                            drop(store);

                            if should_switch {
                                let codex_activity = process::codex_activity_report();
                                let force_after_critical_grace = critical_usage
                                    && !should_defer_critical_switch(
                                        &codex_activity,
                                        critical_age_seconds,
                                    );

                                match auto_switch::trigger_with_activity_report(
                                    active_id.clone(),
                                    &app_handle,
                                    &state,
                                    codex_activity.clone(),
                                    force_after_critical_grace,
                                )
                                .await
                                {
                                    Ok(auto_switch::TriggerOutcome::Deferred) => {
                                        critical_switch_pending = critical_usage;
                                        record_auto_switch_decision(
                                            &app_handle,
                                            &state,
                                            auto_switch_decision_report(
                                                AutoSwitchDecisionKind::DeferredCodexBusy,
                                                Some(active_id.clone()),
                                                Some(usage),
                                                critical_usage,
                                                Some(threshold),
                                                None,
                                                "Auto-switch deferred because Codex still appears active",
                                                Some(codex_activity),
                                            ),
                                        )
                                        .await;
                                    }
                                    Ok(auto_switch::TriggerOutcome::Switched) => {
                                        critical_switch_pending = false;
                                        let mut state_guard = state.write().await;
                                        state_guard.critical_auto_switch_since = None;
                                        drop(state_guard);
                                        record_auto_switch_decision(
                                            &app_handle,
                                            &state,
                                            auto_switch_decision_report(
                                                AutoSwitchDecisionKind::Switched,
                                                Some(active_id.clone()),
                                                Some(usage),
                                                critical_usage,
                                                Some(threshold),
                                                None,
                                                "Auto-switch completed",
                                                None,
                                            ),
                                        )
                                        .await;
                                    }
                                    Err(e) => {
                                        tracing::error!("Auto-switch failed: {}", e);
                                        let kind = if e
                                            .to_string()
                                            .contains("No alternative accounts available")
                                        {
                                            AutoSwitchDecisionKind::NoEligibleTarget
                                        } else {
                                            AutoSwitchDecisionKind::SwitchFailed
                                        };
                                        record_auto_switch_decision(
                                            &app_handle,
                                            &state,
                                            auto_switch_decision_report(
                                                kind,
                                                Some(active_id.clone()),
                                                Some(usage),
                                                critical_usage,
                                                Some(threshold),
                                                None,
                                                format!("Auto-switch failed: {e}"),
                                                None,
                                            ),
                                        )
                                        .await;
                                    }
                                }
                            } else {
                                let (kind, message) = if !auto_switch_enabled {
                                    (
                                        AutoSwitchDecisionKind::AutoSwitchDisabled,
                                        "Auto-switch is disabled",
                                    )
                                } else if !critical_usage && !usage_complete {
                                    (
                                        AutoSwitchDecisionKind::UsageIncomplete,
                                        "Active usage is missing one or more non-critical windows",
                                    )
                                } else {
                                    (
                                        AutoSwitchDecisionKind::BelowThreshold,
                                        "Active account is below auto-switch threshold",
                                    )
                                };
                                record_auto_switch_decision(
                                    &app_handle,
                                    &state,
                                    auto_switch_decision_report(
                                        kind,
                                        Some(active_id.clone()),
                                        Some(usage),
                                        critical_usage,
                                        Some(threshold),
                                        None,
                                        message,
                                        None,
                                    ),
                                )
                                .await;
                            }
                        } else {
                            record_auto_switch_decision(
                                &app_handle,
                                &state,
                                auto_switch_decision_report(
                                    AutoSwitchDecisionKind::NoActiveUsage,
                                    Some(active_id.clone()),
                                    None,
                                    false,
                                    None,
                                    None,
                                    "No usage result was found for the active account",
                                    None,
                                ),
                            )
                            .await;
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to load accounts: {}", e);
                }
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

pub fn stop_monitor() {
    MONITOR_CANCELLED.store(true, Ordering::Relaxed);
    tracing::info!("Monitor stop signal sent");
}

async fn record_auto_switch_decision(
    app_handle: &AppHandle,
    state: &Arc<RwLock<MonitorState>>,
    report: AutoSwitchDecisionReport,
) {
    tracing::info!(
        "Auto-switch decision: {:?}, active={:?}, target={:?}, critical={}, message={}",
        report.kind,
        report.active_account_id,
        report.target_account_id,
        report.critical_usage,
        report.message
    );

    {
        let mut state_guard = state.write().await;
        state_guard.last_auto_switch_decision = Some(report.clone());
    }

    let _ = app_handle.emit("auto-switch-decision", &report);
}

fn auto_switch_decision_report(
    kind: AutoSwitchDecisionKind,
    active_account_id: Option<String>,
    usage: Option<&UsageInfo>,
    critical_usage: bool,
    threshold: Option<f64>,
    target_account_id: Option<String>,
    message: impl Into<String>,
    codex_activity: Option<CodexActivityReport>,
) -> AutoSwitchDecisionReport {
    AutoSwitchDecisionReport {
        timestamp: Utc::now(),
        kind,
        active_account_id,
        target_account_id,
        primary_used_percent: usage.and_then(|usage| usage.primary_used_percent),
        secondary_used_percent: usage.and_then(|usage| usage.secondary_used_percent),
        critical_usage,
        threshold,
        message: message.into(),
        codex_activity,
    }
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

fn should_defer_critical_switch(report: &CodexActivityReport, critical_age_seconds: i64) -> bool {
    if report.active_cli_process || report.active_descendant_process {
        return true;
    }

    if report.recent_session_file_activity || report.recent_desktop_log_activity {
        return critical_age_seconds < CRITICAL_SESSION_FILE_GRACE_SECONDS;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CodexActivityReport;

    #[test]
    fn critical_deferred_auto_switch_rechecks_after_five_seconds() {
        assert_eq!(next_poll_interval_seconds(60, true, true), 5);
    }

    #[test]
    fn critical_retry_still_defers_for_active_processes() {
        let report = CodexActivityReport {
            busy: true,
            active_cli_process: true,
            ..CodexActivityReport::default()
        };

        assert!(should_defer_critical_switch(&report, 90));
    }

    #[test]
    fn critical_retry_defers_file_only_activity_inside_grace_period() {
        let report = CodexActivityReport {
            busy: true,
            recent_session_file_activity: true,
            ..CodexActivityReport::default()
        };

        assert!(should_defer_critical_switch(&report, 30));
    }

    #[test]
    fn critical_retry_stops_deferring_after_stale_file_only_grace_period() {
        let report = CodexActivityReport {
            busy: true,
            recent_session_file_activity: true,
            ..CodexActivityReport::default()
        };

        assert!(!should_defer_critical_switch(&report, 90));
    }
}
