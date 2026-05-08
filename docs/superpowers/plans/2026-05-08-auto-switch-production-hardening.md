# Auto Switch Production Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make auto-switch reliably switch away from a zero/critical active Codex account after the current chat turn has stopped, with immediate production diagnostics for every gate.

**Architecture:** Debug the live failure first by instrumenting the existing monitor path. Then fix the two most likely gates: stale Codex session/log activity causing permanent deferral, and incomplete/quirky usage data preventing critical detection. After the live issue is understood and fixed, harden with structured decision reports and full signal-level tests.

**Tech Stack:** Rust/Tauri backend, existing `cargo test` unit tests, existing React/Tauri frontend only for optional status surfacing.

---

## Current Failure Theory

For the observed scenario, the most likely root cause is:

1. Codex hits the limit and the chat stops.
2. Active account usage is now `0% remaining` or exhausted.
3. The monitor enters auto-switch evaluation.
4. `is_codex_desktop_busy()` returns true because `.codex/sessions` files or Codex logs were recently modified.
5. Auto-switch defers.
6. On the next 5-second retry, recent session/log activity is still inside the activity window.
7. Auto-switch defers again.
8. This repeats long enough that the app effectively never switches.

The close second root cause is incomplete usage data:

1. The usage API may return one active window as missing/null near the exact limit.
2. `should_auto_switch()` currently requires both windows to be present.
3. The active account can be effectively exhausted, but the decision exits early as `UsageIncomplete`.

The system currently has no production-grade visibility into which gate blocked the switch.

## File Structure

- Modify `src-tauri/src/monitor/mod.rs`: immediate gate logging, critical retry state, stale-file grace logic.
- Modify `src-tauri/src/auto_switch/mod.rs`: critical usage detection that does not require both windows when one known window is exhausted.
- Modify `src-tauri/src/process/mod.rs`: later structured Codex activity reports by signal.
- Modify `src-tauri/src/types.rs`: later serializable decision/activity report types.
- Optional later: modify `src/components/Dashboard.tsx` to display last auto-switch decision.

## Task 1: Add Immediate Gate Logging Before Changing Logic

**Files:**
- Modify: `src-tauri/src/monitor/mod.rs`
- Modify: `src-tauri/src/auto_switch/mod.rs`

- [ ] **Step 1: Add monitor-cycle logging around active account and usage gates**

In `src-tauri/src/monitor/mod.rs`, inside the monitor loop after loading the store and usages, log the key state:

```rust
tracing::info!(
    "[monitor] cycle: accounts={}, active_account={:?}, usages={}",
    store.accounts.len(),
    store.active_account_id,
    usages.len()
);
```

After `let active_id = store.active_account_id.clone();`, add:

```rust
if active_id.is_none() {
    tracing::info!("[monitor] gate: no_active_account");
}
```

Inside `if let Some(active_id) = active_id`, before looking up usage, add:

```rust
let active_usage = usages.iter().find(|u| u.account_id == active_id);
tracing::info!(
    "[monitor] gate: active_usage_present={}, active_account={}",
    active_usage.is_some(),
    active_id
);
```

Use `active_usage` in the following `if let Some(usage) = active_usage` block so lookup happens once.

- [ ] **Step 2: Add usage and settings gate logging**

Inside the active usage block in `src-tauri/src/monitor/mod.rs`, before calling `should_auto_switch`, log:

```rust
tracing::info!(
    "[monitor] gate: active_usage primary={:?}, secondary={:?}, error={:?}",
    usage.primary_used_percent,
    usage.secondary_used_percent,
    usage.error
);
```

Inside the `state_guard` block before calling `should_auto_switch`, log:

```rust
tracing::info!(
    "[monitor] gate: auto_switch_enabled={}, poll_interval={}s, cooldown={}s",
    state_guard.settings.auto_switch_enabled,
    state_guard.settings.poll_interval_seconds,
    state_guard.settings.global_cooldown_seconds
);
```

After `should_switch` is computed, log:

```rust
tracing::info!(
    "[monitor] gate: critical_usage={}, should_switch={}",
    critical_usage,
    should_switch
);
```

- [ ] **Step 3: Add busy-gate logging**

In `src-tauri/src/auto_switch/mod.rs`, inside `trigger`, replace the current `codex_busy` assignment with a version that logs the raw outcome:

```rust
let codex_busy_result = process::is_codex_desktop_busy();
tracing::info!("[monitor] gate: codex_busy_result={:?}", codex_busy_result);
let codex_busy = codex_busy_result.unwrap_or_else(|err| {
    tracing::warn!("Failed to inspect Codex activity before auto-switch: {err}");
    false
});
tracing::info!("[monitor] gate: codex_busy={}", codex_busy);
```

- [ ] **Step 4: Build and run with filtered logs**

Run:

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher
RUST_LOG=debug pnpm tauri dev 2>&1 | grep '\\[monitor\\]'
```

This app writes tracing to `~/.authpilot/authpilot.log.YYYY-MM-DD`, so in a second terminal tail the file instead of relying on stdout:

```bash
tail -F ~/.authpilot/authpilot.log.* | grep --line-buffered '\\[monitor\\]'
```

Expected: every monitor cycle prints active account, usage presence, usage values, settings gates, critical flag, switch decision, and Codex busy result in the tailed log file.

- [ ] **Step 5: Reproduce the live failure**

With the dev app running:

1. Use an account that reaches `0% remaining`.
2. Let the Codex chat stop because of the limit.
3. Watch the `[monitor]` logs for the first cycle after the stop.
4. Record which gate blocks switching.

Expected likely result: `should_switch=true`, followed by `codex_busy=true` and repeated deferrals.

## Task 2: Fix Stale Session/Log Activity Blocking Critical Switches

**Files:**
- Modify: `src-tauri/src/types.rs`
- Modify: `src-tauri/src/process/mod.rs`
- Modify: `src-tauri/src/monitor/mod.rs`
- Modify: `src-tauri/src/auto_switch/mod.rs`
- Test: `src-tauri/src/process/mod.rs`
- Test: `src-tauri/src/monitor/mod.rs`

- [ ] **Step 1: Add `CodexActivityReport`**

Add to `src-tauri/src/types.rs`:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodexActivityReport {
    pub busy: bool,
    pub desktop_running: bool,
    pub active_cli_process: bool,
    pub active_descendant_process: bool,
    pub recent_session_file_activity: bool,
    pub recent_desktop_log_activity: bool,
    pub inspection_error: Option<String>,
}
```

- [ ] **Step 2: Write failing tests for activity signal reports**

Add tests in `src-tauri/src/process/mod.rs`:

```rust
#[test]
fn activity_report_marks_busy_when_cli_process_is_active() {
    let snapshot = r#"
      200     1 /bin/zsh
      201   200 /opt/homebrew/bin/codex exec run tests
    "#;

    let processes = parse_process_snapshot(snapshot);
    let report = codex_activity_report_from_processes(&processes, false, false, true);

    assert!(report.busy);
    assert!(report.active_cli_process);
    assert!(!report.active_descendant_process);
    assert!(!report.recent_session_file_activity);
    assert!(!report.recent_desktop_log_activity);
}

#[test]
fn activity_report_marks_idle_when_only_background_processes_exist() {
    let snapshot = r#"
      100     1 /Applications/Codex.app/Contents/MacOS/Codex
      101   100 /Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled
      102   101 /Applications/Codex.app/Contents/Resources/node_repl
    "#;

    let processes = parse_process_snapshot(snapshot);
    let report = codex_activity_report_from_processes(&processes, false, false, true);

    assert!(!report.busy);
    assert!(report.desktop_running);
}

#[test]
fn activity_report_exposes_file_only_activity() {
    let processes = Vec::new();
    let report = codex_activity_report_from_processes(&processes, true, false, true);

    assert!(report.busy);
    assert!(report.recent_session_file_activity);
    assert!(!report.active_cli_process);
    assert!(!report.active_descendant_process);
}
```

- [ ] **Step 3: Run tests to verify failure**

Run:

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher/src-tauri
cargo test process::tests::activity_report -- --nocapture
```

Expected: fail because `codex_activity_report_from_processes` does not exist.

- [ ] **Step 4: Implement activity reports**

In `src-tauri/src/process/mod.rs`, import `CodexActivityReport`:

```rust
use crate::types::CodexActivityReport;
```

Add:

```rust
pub fn codex_activity_report() -> CodexActivityReport {
    match codex_activity_report_result() {
        Ok(report) => report,
        Err(err) => CodexActivityReport {
            inspection_error: Some(err.to_string()),
            ..CodexActivityReport::default()
        },
    }
}

fn codex_activity_report_result() -> Result<CodexActivityReport> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
        .context("Failed to inspect process tree")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to inspect process tree: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let processes = parse_process_snapshot(&stdout);
    let recent_session_file_activity =
        codex_session_files_have_recent_activity().unwrap_or(false);
    let desktop_running = is_codex_desktop_running().unwrap_or(false);
    let recent_desktop_log_activity =
        desktop_running && codex_logs_have_recent_activity().unwrap_or(false);

    Ok(codex_activity_report_from_processes(
        &processes,
        recent_session_file_activity,
        recent_desktop_log_activity,
        desktop_running,
    ))
}

fn codex_activity_report_from_processes(
    processes: &[ProcessInfo],
    recent_session_file_activity: bool,
    recent_desktop_log_activity: bool,
    desktop_running: bool,
) -> CodexActivityReport {
    let active_cli_process = has_active_codex_cli_process(processes);
    let active_descendant_process =
        desktop_running && codex_has_active_descendant_processes(processes);
    let busy = active_cli_process
        || active_descendant_process
        || recent_session_file_activity
        || recent_desktop_log_activity;

    CodexActivityReport {
        busy,
        desktop_running,
        active_cli_process,
        active_descendant_process,
        recent_session_file_activity,
        recent_desktop_log_activity,
        inspection_error: None,
    }
}
```

Change `is_codex_desktop_busy()` to:

```rust
pub fn is_codex_desktop_busy() -> Result<bool> {
    Ok(codex_activity_report_result()?.busy)
}
```

- [ ] **Step 5: Add critical deferral tests**

Add to `src-tauri/src/monitor/mod.rs` tests:

```rust
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
```

- [ ] **Step 6: Implement grace helper**

In `src-tauri/src/monitor/mod.rs`, add:

```rust
const CRITICAL_SESSION_FILE_GRACE_SECONDS: i64 = 60;

fn should_defer_critical_switch(
    report: &CodexActivityReport,
    critical_age_seconds: i64,
) -> bool {
    if report.active_cli_process || report.active_descendant_process {
        return true;
    }

    if report.recent_session_file_activity || report.recent_desktop_log_activity {
        return critical_age_seconds < CRITICAL_SESSION_FILE_GRACE_SECONDS;
    }

    false
}
```

- [ ] **Step 7: Track critical age in monitor state**

Add to `MonitorState` in `src-tauri/src/types.rs`:

```rust
pub critical_auto_switch_since: Option<DateTime<Utc>>,
```

In `src-tauri/src/monitor/mod.rs`, when `critical_usage && settings.auto_switch_enabled`, set `critical_auto_switch_since` if it is `None`. When usage is no longer critical or a switch succeeds, clear it.

Use an explicit age calculation in the monitor loop. Do not pass a hard-coded `0` into `should_defer_critical_switch`.

Add `Utc` to the monitor imports:

```rust
use chrono::Utc;
```

In the active usage block, after `critical_usage` is computed and while holding the settings/state guard, start or clear the clock:

```rust
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

let critical_age_seconds = state_guard
    .critical_auto_switch_since
    .map(|started_at| (Utc::now() - started_at).num_seconds())
    .unwrap_or(0);
```

After a successful switch, clear the clock:

```rust
if matches!(outcome, auto_switch::TriggerOutcome::Switched) {
    let mut state_guard = state.write().await;
    state_guard.critical_auto_switch_since = None;
}
```

Pass `critical_age_seconds` into `should_defer_critical_switch`. This is the value that lets the monitor exit the file/log grace window after 60 seconds.

Locking warning: check the existing `state.read().await` / `state.write().await` scopes in `src-tauri/src/monitor/mod.rs` before adding this code. Do not acquire `state.write().await` while another read or write guard is still live in the same async task. Either fold this clock logic into an existing guard scope, or explicitly end/drop the earlier guard before acquiring the write guard.

- [ ] **Step 8: Use activity report for critical trigger**

Update `auto_switch::trigger` to accept an optional `CodexActivityReport`, or add a new function. Prefer adding a new function to avoid changing manual-switch behavior:

```rust
pub async fn trigger_with_activity_report(
    active_account_id: String,
    app_handle: &AppHandle,
    state: &Arc<RwLock<crate::types::MonitorState>>,
    codex_activity: CodexActivityReport,
    force_after_critical_grace: bool,
) -> Result<TriggerOutcome>
```

Behavior:

- If `force_after_critical_grace` is false and `codex_activity.busy` is true, defer.
- If `force_after_critical_grace` is true, only defer for `active_cli_process` or `active_descendant_process`.
- If only session/log activity remains after grace, switch.

The deferral conditional inside `trigger_with_activity_report` must be exactly:

```rust
let should_defer = if force_after_critical_grace {
    // Grace expired: only real processes block the switch now.
    codex_activity.active_cli_process || codex_activity.active_descendant_process
} else {
    // Normal path: any busy signal defers.
    codex_activity.busy
};

if should_defer {
    tracing::info!(
        "[monitor] gate: deferred codex_busy, force={}",
        force_after_critical_grace
    );
    return Ok(TriggerOutcome::Deferred);
}
```

Everything else in `trigger_with_activity_report` stays identical to the existing `trigger` body: load accounts, select target, determine reason, execute credential swap, restart Codex if it was running, update cooldown/state, append log entry, and emit events.

Replace the existing call site in `src-tauri/src/monitor/mod.rs`. Search first:

```bash
rg -n "auto_switch::trigger\\(" src-tauri/src
```

Expected current call site:

```rust
match auto_switch::trigger(active_id, &app_handle, &state).await {
```

Replace it with:

```rust
let codex_activity = process::codex_activity_report();
let force_after_critical_grace =
    critical_usage && !should_defer_critical_switch(&codex_activity, critical_age_seconds);

match auto_switch::trigger_with_activity_report(
    active_id,
    &app_handle,
    &state,
    codex_activity,
    force_after_critical_grace,
)
.await
{
```

If `rg` finds any additional `auto_switch::trigger(...)` call sites, update them in the same commit or keep a thin compatibility wrapper:

```rust
pub async fn trigger(
    active_account_id: String,
    app_handle: &AppHandle,
    state: &Arc<RwLock<crate::types::MonitorState>>,
) -> Result<TriggerOutcome> {
    let codex_activity = process::codex_activity_report();
    trigger_with_activity_report(
        active_account_id,
        app_handle,
        state,
        codex_activity,
        false,
    )
    .await
}
```

- [ ] **Step 9: Run focused tests**

Run:

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher/src-tauri
cargo test process::tests monitor::tests -- --nocapture
```

Expected: process and monitor tests pass.

## Task 3: Fix Incomplete/Critical Usage Gate

**Files:**
- Modify: `src-tauri/src/auto_switch/mod.rs`
- Test: `src-tauri/src/auto_switch/mod.rs`

- [ ] **Step 1: Add tests for critical detection with incomplete windows**

Add to `src-tauri/src/auto_switch/mod.rs` tests:

```rust
#[test]
fn critical_primary_usage_does_not_require_secondary_window() {
    let mut active_usage = usage("active", 100.0, 20.0);
    active_usage.secondary_used_percent = None;

    assert!(usage_is_critical(&active_usage));
    assert!(should_auto_switch(
        &active_usage,
        &settings(),
        &store("active", &["active", "target"])
    ));
}

#[test]
fn zero_primary_usage_with_missing_secondary_is_treated_as_api_exhausted_quirk() {
    let mut active_usage = usage("active", 0.0, 20.0);
    active_usage.secondary_used_percent = None;

    assert!(usage_is_critical(&active_usage));
    assert!(should_auto_switch(
        &active_usage,
        &settings(),
        &store("active", &["active", "target"])
    ));
}

#[test]
fn missing_secondary_still_blocks_non_critical_usage() {
    let mut active_usage = usage("active", 20.0, 20.0);
    active_usage.secondary_used_percent = None;

    assert!(!usage_is_critical(&active_usage));
    assert!(!should_auto_switch(
        &active_usage,
        &settings(),
        &store("active", &["active", "target"])
    ));
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher/src-tauri
cargo test auto_switch::tests::critical_primary_usage_does_not_require_secondary_window auto_switch::tests::zero_primary_usage_with_missing_secondary_is_treated_as_api_exhausted_quirk auto_switch::tests::missing_secondary_still_blocks_non_critical_usage -- --nocapture
```

If cargo rejects multiple test names, run:

```bash
cargo test auto_switch::tests -- --nocapture
```

Expected: the new critical incomplete-window tests fail.

- [ ] **Step 3: Implement critical usage detection before completeness gate**

Update `usage_is_critical` in `src-tauri/src/auto_switch/mod.rs` to treat explicit exhaustion values as critical:

```rust
pub fn usage_is_critical(usage: &UsageInfo) -> bool {
    usage_window_remaining_at_or_below(usage, CRITICAL_REMAINING_PERCENT)
        || usage.primary_used_percent == Some(0.0)
        || usage.secondary_used_percent == Some(0.0)
}
```

The exact `Some(0.0)` comparison is intentional here because the value is JSON-deserialized API output and is either exactly zero or absent. Do not use exact float equality for non-zero thresholds.

Update `should_auto_switch` so critical usage bypasses the completeness check:

```rust
let critical_usage = usage_is_critical(usage);

if !critical_usage && !usage_windows_complete(usage) {
    tracing::info!(
        "Account {} missing 5h or weekly usage, skipping non-critical auto-switch decision: 5h={:?}, weekly={:?}",
        usage.account_id,
        usage.primary_used_percent,
        usage.secondary_used_percent
    );
    return false;
}
```

Keep cooldown bypass limited to critical usage.

- [ ] **Step 4: Run auto-switch tests**

Run:

```bash
cargo test auto_switch::tests -- --nocapture
```

Expected: all auto-switch tests pass.

## Task 4: Add Structured Decision Reports After the Live Fix

**Files:**
- Modify: `src-tauri/src/types.rs`
- Modify: `src-tauri/src/auto_switch/mod.rs`
- Modify: `src-tauri/src/monitor/mod.rs`
- Test: `src-tauri/src/auto_switch/mod.rs`

- [ ] **Step 1: Add decision report types**

Add to `src-tauri/src/types.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutoSwitchDecisionKind {
    Switched,
    DeferredCodexBusy,
    NoActiveAccount,
    NoActiveUsage,
    AutoSwitchDisabled,
    UsageIncomplete,
    BelowThreshold,
    NoEligibleTarget,
    SwitchFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoSwitchDecisionReport {
    pub timestamp: DateTime<Utc>,
    pub kind: AutoSwitchDecisionKind,
    pub active_account_id: Option<String>,
    pub target_account_id: Option<String>,
    pub primary_used_percent: Option<f64>,
    pub secondary_used_percent: Option<f64>,
    pub critical_usage: bool,
    pub threshold: Option<f64>,
    pub message: String,
    pub codex_activity: Option<CodexActivityReport>,
}
```

- [ ] **Step 2: Emit decision event every monitor cycle**

In `src-tauri/src/monitor/mod.rs`, add:

```rust
fn emit_auto_switch_decision(app_handle: &AppHandle, report: &AutoSwitchDecisionReport) {
    tracing::info!(
        "Auto-switch decision: {:?}, active={:?}, target={:?}, critical={}, message={}",
        report.kind,
        report.active_account_id,
        report.target_account_id,
        report.critical_usage,
        report.message
    );
    let _ = app_handle.emit("auto-switch-decision", report);
}
```

Emit reports for:

- no active account,
- no active usage,
- disabled auto-switch,
- incomplete non-critical usage,
- below threshold,
- Codex busy deferral,
- no eligible target,
- switch failed,
- switched.

- [ ] **Step 3: Save last decision in monitor state**

Extend `MonitorState`:

```rust
pub last_auto_switch_decision: Option<AutoSwitchDecisionReport>,
```

Whenever a report is emitted, write it into state.

- [ ] **Step 4: Run tests**

Run:

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher/src-tauri
cargo test
```

Expected: all tests pass.

## Task 5: Production Verification

**Files:**
- No source changes unless verification exposes failures.

- [ ] **Step 1: Run backend tests**

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher/src-tauri
cargo test
```

Expected: all tests pass.

- [ ] **Step 2: Run frontend build**

```bash
cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher
pnpm build
```

Expected: TypeScript and Vite build exit 0.

- [ ] **Step 3: Run release build**

```bash
pnpm tauri build
```

Expected: `src-tauri/target/release/bundle/macos/AuthPilot.app` is produced.

- [ ] **Step 4: Manual production scenario**

1. Install the release build into `/Applications/AuthPilot.app`.
2. Enable auto-switch.
3. Use an active account that is at `0% remaining`.
4. Start a Codex chat that stops due to the limit.
5. Observe `[monitor]` logs and `auto-switch-decision`.
6. Confirm AuthPilot retries every 5 seconds while critical.
7. Confirm it defers while an actual Codex process is active.
8. Confirm it switches and relaunches Codex after the grace period when only stale file/log activity remains.

## Self-Review

- Spec coverage: covers immediate diagnosis, stale session/log deferral, incomplete usage critical detection, structured reports, and production verification.
- Placeholder scan: no TBD/TODO placeholders remain.
- Type consistency: `CodexActivityReport`, `AutoSwitchDecisionReport`, and `AutoSwitchDecisionKind` are defined before use.
