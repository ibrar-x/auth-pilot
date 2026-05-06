//! Background monitor - polls usage and triggers auto-switch

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};

use tauri::{AppHandle, Emitter};

use crate::api::usage::refresh_all_usage;
use crate::auth::storage::load_accounts;
use crate::auto_switch;
use crate::types::MonitorState;

static MONITOR_CANCELLED: AtomicBool = AtomicBool::new(false);

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

            match load_accounts() {
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
                            let should_switch = {
                                let state_guard = state.read().await;
                                auto_switch::should_auto_switch(
                                    usage,
                                    &state_guard.settings,
                                    &store,
                                )
                            };

                            drop(store);

                            if should_switch {
                                if let Err(e) =
                                    auto_switch::trigger(active_id, &app_handle, &state).await
                                {
                                    tracing::error!("Auto-switch failed: {}", e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to load accounts: {}", e);
                }
            }

            sleep(Duration::from_secs(poll_interval)).await;
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
