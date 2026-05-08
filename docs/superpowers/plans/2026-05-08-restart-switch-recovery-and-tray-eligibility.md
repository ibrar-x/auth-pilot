# Restart Switch Recovery And Tray Eligibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the failed proxy hot-swap architecture, restore restart-based Codex account switching, block exhausted accounts from manual and automatic selection, and add best-effort Codex session recovery after forced restarts.

**Architecture:** AuthPilot returns to the proven `auth.json` swap plus Codex restart model. Usage eligibility is centralized in `auto_switch` and reused by the tray so manual and automatic paths agree. Session recovery is a separate subsystem that records Codex session metadata, detects interruptions, and exposes recovery actions without becoming a Codex client.

**Tech Stack:** Rust/Tauri, Tokio, `rusqlite`, `notify`, existing React tray popup/dashboard, macOS `open`, Codex CLI `codex exec resume`.

---

## Scope And Ordering

This plan supersedes `docs/superpowers/plans/2026-05-08-auth-hot-swap-proxy.md`. The proxy plan must be reverted before recovery work starts because recovery depends on restart-based switching. Do not implement TLS interception, system proxying, CLI wrapper proxying, or CA installation.

Ship sequence:
1. Revert proxy code and clean runtime side effects.
2. Implement tray eligibility and hard-exhaustion switch behavior.
3. Add minimal session capture before forced restart.
4. Add recovery DB, actions, and UI.
5. Build, install, commit, and push.

## File Structure

- Remove: `src-tauri/src/proxy/mod.rs`
- Remove: `src-tauri/src/cli_wrapper.rs`
- Remove: `src-tauri/src/cert.rs`
- Remove: `src-tauri/src/system_proxy.rs`
- Remove: `src-tauri/src/commands/cli_wrapper.rs`
- Remove: `src-tauri/src/commands/cert.rs`
- Remove: `src-tauri/src/commands/system_proxy.rs`
- Modify: `src-tauri/src/lib.rs` to unregister removed modules and commands.
- Modify: `src-tauri/src/commands/mod.rs` to remove proxy command exports.
- Modify: `src-tauri/src/commands/settings.rs` to stop syncing proxy runtime.
- Modify: `src-tauri/src/types.rs` and `src/types/index.ts` to remove proxy settings and add recovery state types.
- Modify: `src/components/Settings.tsx` to remove proxy controls and add recovery hook controls later.
- Modify: `src-tauri/src/auto_switch/mod.rs` to centralize capacity, hard exhaustion, and deferral logic.
- Modify: `src-tauri/src/monitor/mod.rs` to force immediate switch on hard exhaustion and call recovery process checks.
- Modify: `src-tauri/src/tray/mod.rs` to show 5h and weekly state and disable exhausted targets.
- Modify: `src-tauri/src/switch_executor/mod.rs` to capture active session before restart and resume after relaunch.
- Modify: `src-tauri/src/process/mod.rs` to expose recent session file detection with session id/path metadata.
- Create: `src-tauri/src/recovery/mod.rs`
- Create: `src-tauri/src/recovery/types.rs`
- Create: `src-tauri/src/recovery/session_db.rs`
- Create: `src-tauri/src/recovery/hooks.rs`
- Create: `src-tauri/src/recovery/hook_watcher.rs`
- Create: `src-tauri/src/recovery/process_watch.rs`
- Create: `src-tauri/src/recovery/resume.rs`
- Create: `src-tauri/src/commands/recovery.rs`
- Create: `src/components/RecoveryCard.tsx`
- Create: `src/components/RecoveryDialog.tsx`
- Create: `resources/record-session.sh`

## Task 1: Revert Proxy Foundation

**Files:**
- Modify/remove all proxy files listed above.
- Modify: `src-tauri/Cargo.toml`
- Modify: `src/components/Settings.tsx`
- Modify: `src/types/index.ts`

- [ ] **Step 1: Verify the proxy commits before reverting**

Run:

```bash
git log --oneline 033ac1b 9f772a4
```

Expected: both commits are present and describe proxy/hot-swap/CA/CLI-wrapper work. If either hash is missing or the messages do not match that scope, stop and report the actual `git log --oneline -n 12` output instead of reverting blindly.

- [ ] **Step 2: Revert the proxy commits without losing later work**

Run:

```bash
git revert --no-commit 033ac1b
git revert --no-commit 9f772a4
```

Expected: working tree contains reverse changes for the proxy commits.

- [ ] **Step 3: Resolve conflicts by keeping restart switching**

In `src-tauri/src/switch_executor/mod.rs`, final behavior must be:

```rust
// Always swap auth.json before a desktop switch.
session::swap_active_auth(target_account_id).context("Failed to swap auth.json")?;

let was_running = process::is_codex_desktop_running().unwrap_or(false);

if was_running {
    process::kill_codex_desktop().await?;
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    process::launch_codex_desktop().await?;
}
```

Delete `SwitchMode`, `ProxyHotSwap`, `proxy_healthy`, `desktop_proxy_auth_injection_supported`, and all tests for proxy switch mode.

- [ ] **Step 4: Remove proxy settings from defaults and TypeScript**

Delete these fields from `AppSettings` in `src-tauri/src/types.rs` and from `AppSettings` in `src/types/index.ts`:

```rust
proxy_mode_enabled
proxy_port
proxy_cli_wrapper_enabled
proxy_ca_trusted
```

- [ ] **Step 5: Remove proxy UI**

In `src/components/Settings.tsx`, remove the full "Seamless switching proxy" section, CLI wrapper controls, CA controls, and macOS system proxy controls.

- [ ] **Step 6: Clean runtime side effects**

Run these once during development, not from application code:

```bash
networksetup -listallnetworkservices | tail -n +2 | while IFS= read -r svc; do
  [ -n "$svc" ] && networksetup -setsecurewebproxystate "$svc" off 2>/dev/null || true
done
perl -0pi -e 's/# AuthPilot codex proxy - managed automatically.*?# End AuthPilot codex proxy\n//sg' ~/.zshrc ~/.bashrc ~/.bash_profile 2>/dev/null || true
rm -f ~/.authpilot/.authpilot-proxy-active ~/.authpilot/.authpilot-system-proxy-active ~/.authpilot/system-proxy-services.json
```

- [ ] **Step 7: Verify proxy code is gone**

Run:

```bash
rg -n "proxy_mode|ProxyHotSwap|cli_wrapper|system_proxy|install_proxy_ca|AuthPilot CLI wrapper|authpilot-ca|setsecurewebproxy" src src-tauri/src src-tauri/Cargo.toml
```

Expected: no production references. References inside old docs are acceptable.

- [ ] **Step 8: Run build checks**

Run:

```bash
cd src-tauri && cargo test
cd .. && pnpm build
```

Expected: all tests pass and frontend builds.

- [ ] **Step 9: Commit**

```bash
git add src src-tauri resources docs
git commit -m "Revert proxy hot swap foundation"
```

## Task 2: Centralize Usage Eligibility

**Files:**
- Modify: `src-tauri/src/auto_switch/mod.rs`
- Modify: `src-tauri/src/types.rs` if exported status enums are needed by tray.

- [ ] **Step 1: Write failing tests for exhausted weekly capacity**

Add tests:

```rust
#[test]
fn usage_with_weekly_exhausted_has_no_switch_capacity_even_when_5h_available() {
    let usage = usage("target", 69.0, 100.0);
    assert!(!usage_has_capacity(&usage, 95.0));
    assert_eq!(usage_remaining_capacity(&usage), 0.0);
}

#[test]
fn hard_exhaustion_is_true_at_one_percent_remaining() {
    let usage = usage("active", 99.0, 20.0);
    assert!(usage_is_hard_exhausted(&usage));
}

#[test]
fn hard_exhaustion_is_true_for_api_zero_used_quirk() {
    let usage = usage("active", 0.0, 20.0);
    assert!(usage_is_hard_exhausted(&usage));
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cd src-tauri
cargo test auto_switch::tests::usage_with_weekly_exhausted_has_no_switch_capacity_even_when_5h_available auto_switch::tests::hard_exhaustion -- --nocapture
```

Expected: new hard exhaustion tests fail because `usage_is_hard_exhausted` does not exist.

- [ ] **Step 3: Implement hard exhaustion and capacity helpers**

Add:

```rust
pub fn usage_is_hard_exhausted(usage: &UsageInfo) -> bool {
    usage.primary_used_percent == Some(0.0)
        || usage.secondary_used_percent == Some(0.0)
        || window_remaining_at_or_below(usage.primary_used_percent, 1.0)
        || window_remaining_at_or_below(usage.secondary_used_percent, 1.0)
}

fn window_remaining_at_or_below(percent_used: Option<f64>, remaining_threshold: f64) -> bool {
    percent_used.is_some_and(|used| {
        let remaining = (100.0 - used).clamp(0.0, 100.0);
        remaining <= remaining_threshold
    })
}

pub fn usage_has_capacity(usage: &UsageInfo, threshold: f64) -> bool {
    if usage.error.is_some() || usage_is_hard_exhausted(usage) {
        return false;
    }

    usage
        .primary_used_percent
        .is_some_and(|used| used < threshold)
        && usage
            .secondary_used_percent
            .is_some_and(|used| used < threshold)
}

pub fn usage_remaining_capacity(usage: &UsageInfo) -> f64 {
    if usage_is_hard_exhausted(usage) {
        return 0.0;
    }

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
```

Refactor `usage_window_remaining_at_or_below` to call `window_remaining_at_or_below` for both windows. If `usage_has_capacity` and `usage_remaining_capacity` already exist, replace their bodies with the implementations above instead of creating duplicates.

- [ ] **Step 4: Run tests**

```bash
cd src-tauri
cargo test auto_switch:: -- --nocapture
```

Expected: all `auto_switch` tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/auto_switch/mod.rs
git commit -m "Centralize exhausted usage eligibility"
```

## Task 3: Tray Shows Both Windows And Disables Exhausted Accounts

**Files:**
- Modify: `src-tauri/src/tray/mod.rs`

- [ ] **Step 1: Write failing tray label tests**

Add tests in `tray/mod.rs`:

```rust
#[test]
fn tray_label_shows_both_windows_and_weekly_reached() {
    let usage = UsageInfo {
        account_id: "a".to_string(),
        primary_used_percent: Some(69.0),
        secondary_used_percent: Some(100.0),
        ..UsageInfo::default()
    };

    assert_eq!(
        account_usage_label("Account", Some(&usage), UsageDisplayMode::Remaining),
        "Account — 5h: 31% left | 7-day: Weekly limit reached"
    );
}

#[test]
fn tray_item_is_disabled_when_weekly_limit_reached() {
    let usage = UsageInfo {
        account_id: "a".to_string(),
        primary_used_percent: Some(69.0),
        secondary_used_percent: Some(100.0),
        ..UsageInfo::default()
    };

    assert!(!tray_account_can_switch(Some(&usage), 95.0));
}
```

- [ ] **Step 2: Run failing tests**

```bash
cd src-tauri
cargo test tray::tests::tray_label_shows_both_windows_and_weekly_reached tray::tests::tray_item_is_disabled_when_weekly_limit_reached -- --nocapture
```

Expected: fail because helpers do not exist.

- [ ] **Step 3: Implement label and switchability helpers**

Add:

```rust
fn account_usage_label(
    account_name: &str,
    usage: Option<&UsageInfo>,
    usage_display_mode: UsageDisplayMode,
) -> String {
    if let Some(usage) = usage {
        format!(
            "{} — 5h: {} | 7-day: {}",
            account_name,
            format_window_status(usage.primary_used_percent, usage_display_mode, "5h"),
            format_window_status(usage.secondary_used_percent, usage_display_mode, "weekly"),
        )
    } else {
        format!("{account_name} — loading...")
    }
}

fn tray_account_can_switch(usage: Option<&UsageInfo>, threshold: f64) -> bool {
    usage.is_some_and(|usage| {
        usage.error.is_none()
            && usage.primary_used_percent.is_some_and(|used| used < threshold)
            && usage.secondary_used_percent.is_some_and(|used| used < threshold)
    })
}

fn format_window_status(
    percent_used: Option<f64>,
    usage_display_mode: UsageDisplayMode,
    exhausted_name: &str,
) -> String {
    let Some(percent_used) = percent_used else {
        return "loading...".to_string();
    };
    let used = percent_used.clamp(0.0, 100.0);
    if used >= 100.0 {
        return match exhausted_name {
            "weekly" => "Weekly limit reached".to_string(),
            _ => "Limit reached".to_string(),
        };
    }
    match usage_display_mode {
        UsageDisplayMode::Remaining => format!("{:.0}% left", 100.0 - used),
        UsageDisplayMode::Used => format!("{used:.0}% used"),
    }
}
```

Use `account_usage_label` and `tray_account_can_switch` inside `build_tray_menu`. The call site must pass the per-account threshold:

```rust
let threshold = settings
    .account_settings
    .get(&account.id)
    .map(|account_settings| account_settings.switch_threshold)
    .unwrap_or(95.0);
let enabled = tray_account_can_switch(usage, threshold);
```

- [ ] **Step 4: Run tests**

```bash
cd src-tauri
cargo test tray:: -- --nocapture
```

Expected: tray tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/tray/mod.rs
git commit -m "Show exhausted weekly limits in tray"
```

## Task 4: Force Immediate Restart On Hard Exhaustion

**Files:**
- Modify: `src-tauri/src/auto_switch/mod.rs`
- Modify: `src-tauri/src/monitor/mod.rs`

- [ ] **Step 1: Write failing deferral tests**

In `auto_switch/mod.rs`, add:

```rust
#[test]
fn hard_exhausted_auto_switch_does_not_defer_for_busy_codex() {
    assert!(!should_defer_auto_switch(
        SwitchReason::AutoLimitReached,
        true,
        true
    ));
}

#[test]
fn non_hard_auto_switch_still_defers_for_busy_codex() {
    assert!(should_defer_auto_switch(
        SwitchReason::AutoLimitReached,
        true,
        false
    ));
}
```

- [ ] **Step 2: Run failing tests**

```bash
cd src-tauri
cargo test auto_switch::tests::hard_exhausted_auto_switch_does_not_defer_for_busy_codex -- --nocapture
```

Expected: fail because `should_defer_auto_switch` has the old signature.

- [ ] **Step 3: Update deferral logic**

Change signature:

```rust
fn should_defer_auto_switch(
    reason: SwitchReason,
    codex_busy: bool,
    hard_exhausted: bool,
) -> bool {
    if hard_exhausted {
        return false;
    }
    matches!(
        reason,
        SwitchReason::AutoLimitReached | SwitchReason::AutoDepleted
    ) && codex_busy
}
```

Pass `hard_exhausted` from `trigger_with_activity_report`.

Update `trigger_with_activity_report` explicitly:

```rust
pub async fn trigger_with_activity_report(
    active_account_id: String,
    app_handle: &AppHandle,
    state: &Arc<RwLock<MonitorState>>,
    codex_activity: CodexActivityReport,
    force_after_critical_grace: bool,
    hard_exhausted: bool,
) -> Result<TriggerOutcome>
```

Before editing call sites, run:

```bash
rg -n "trigger_with_activity_report" src-tauri/src
```

Update every call site in the same commit. The default `trigger(...)` wrapper should pass `false` for both `force_after_critical_grace` and `hard_exhausted`.

- [ ] **Step 4: Update monitor force flag**

In `monitor/mod.rs`, compute:

```rust
let hard_exhausted = auto_switch::usage_is_hard_exhausted(usage);
let force_after_critical_grace = hard_exhausted
    || (critical_usage && !should_defer_critical_switch(&codex_activity, critical_age_seconds));
```

Log:

```rust
tracing::info!(
    "[monitor] gate: critical_usage={}, hard_exhausted={}, should_switch={}",
    critical_usage,
    hard_exhausted,
    should_switch
);
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri
cargo test auto_switch:: monitor:: -- --nocapture
```

Expected: all tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/auto_switch/mod.rs src-tauri/src/monitor/mod.rs
git commit -m "Force restart switch on hard exhaustion"
```

## Task 5: Capture Latest Codex Session Before Restart

**Files:**
- Modify: `src-tauri/src/process/mod.rs`
- Modify: `src-tauri/src/switch_executor/mod.rs`

- [ ] **Step 1: Write failing session parse tests**

In `process/mod.rs`, add:

```rust
#[test]
fn extracts_session_id_from_rollout_filename() {
    let path = Path::new("/Users/test/.codex/sessions/2026/05/08/rollout-2026-05-08T02-00-00-019e0484-6aef-7bc3-8737-a2e72b82157a.jsonl");
    assert_eq!(
        session_id_from_rollout_path(path).as_deref(),
        Some("019e0484-6aef-7bc3-8737-a2e72b82157a")
    );
}
```

- [ ] **Step 2: Run failing test**

```bash
cd src-tauri
cargo test process::tests::extracts_session_id_from_rollout_filename -- --nocapture
```

Expected: fail because helper does not exist.

- [ ] **Step 3: Implement session metadata**

Add:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCodexSession {
    pub session_id: String,
    pub transcript_path: PathBuf,
    pub workspace_path: Option<PathBuf>,
}

pub fn latest_recent_codex_session() -> Result<Option<RecentCodexSession>> {
    let home = dirs::home_dir().context("Unable to find home directory")?;
    latest_recent_codex_session_in(&home.join(".codex").join("sessions"))
}

fn session_id_from_rollout_path(path: &Path) -> Option<String> {
    let stem = path.file_stem()?.to_str()?;
    if stem.len() < 36 {
        return None;
    }

    let candidate = &stem[stem.len() - 36..];
    uuid::Uuid::parse_str(candidate)
        .ok()
        .map(|uuid| uuid.to_string())
}
```

The implementation must scan session files by modified time descending and return the newest `rollout-*.jsonl` with a valid UUID suffix.

- [ ] **Step 4: Capture before restart and resume after launch**

In `switch_executor::execute_switch`, before killing Codex:

```rust
let recovery_session = if was_running {
    process::latest_recent_codex_session().ok().flatten()
} else {
    None
};
```

After launching Codex:

```rust
if let Some(session) = recovery_session {
    if let Err(err) = process::resume_codex_session_continue(&session.session_id).await {
        tracing::warn!(
            "Failed to resume Codex session {} after switch: {err}",
            session.session_id
        );
    }
}
```

Add `resume_codex_session_continue`:

```rust
pub async fn resume_codex_session_continue(session_id: &str) -> Result<()> {
    let output = Command::new("codex")
        .args(["exec", "resume", session_id, "continue"])
        .output()
        .context("Failed to run codex exec resume")?;
    if !output.status.success() {
        anyhow::bail!(
            "codex exec resume failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri
cargo test process:: switch_executor:: -- --nocapture
```

Expected: tests pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/process/mod.rs src-tauri/src/switch_executor/mod.rs
git commit -m "Resume recent Codex session after restart switch"
```

## Task 6: Recovery Data Model And SQLite Store

**Files:**
- Create: `src-tauri/src/recovery/types.rs`
- Create: `src-tauri/src/recovery/session_db.rs`
- Create: `src-tauri/src/recovery/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add dependencies**

In `src-tauri/Cargo.toml`, add:

```toml
rusqlite = { version = "0.32", features = ["bundled", "chrono"] }
notify = "7"
```

- [ ] **Step 2: Add recovery types**

Create `src-tauri/src/recovery/types.rs` with:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexSession {
    pub id: String,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_path: String,
    pub account_id: Option<String>,
    pub process_id: Option<u32>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: SessionStatus,
    pub recovery_attempts: u32,
    pub last_recovery_at: Option<DateTime<Utc>>,
    pub last_recovery_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Interrupted,
    Ignored,
    BackgroundResumed,
}
```

- [ ] **Step 3: Write failing DB tests**

In `session_db.rs`, add tests for:

```rust
insert_session_then_query_by_id
update_status_then_query
mark_stale_sessions_interrupted_marks_old_running_sessions
query_interrupted_sessions_returns_only_interrupted
```

- [ ] **Step 4: Implement DB**

Create a `SessionDb` wrapper around `rusqlite::Connection` with methods:

```rust
pub fn open(path: &Path) -> Result<Self>;
pub fn migrate(&self) -> Result<()>;
pub fn upsert_session(&self, session: &CodexSession) -> Result<()>;
pub fn get(&self, id: &str) -> Result<Option<CodexSession>>;
pub fn query_by_status(&self, status: SessionStatus) -> Result<Vec<CodexSession>>;
pub fn update_status(&self, id: &str, status: SessionStatus) -> Result<()>;
pub fn increment_recovery_attempts(&self, id: &str, at: DateTime<Utc>, prompt: &str) -> Result<()>;
pub fn mark_stale_sessions_interrupted(&self, stale_before: DateTime<Utc>) -> Result<usize>;
```

Use the table schema from the user-provided plan exactly.

- [ ] **Step 5: Run DB tests**

```bash
cd src-tauri
cargo test recovery::session_db:: -- --nocapture
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/recovery src-tauri/src/lib.rs
git commit -m "Add Codex recovery session store"
```

## Task 7: Hook Installer And Hook Parser

**Files:**
- Create: `resources/record-session.sh`
- Create: `src-tauri/src/recovery/hooks.rs`
- Create: `src-tauri/src/recovery/hook_watcher.rs`

- [ ] **Step 1: Create hook script**

Create `resources/record-session.sh` with the script from the user-provided plan. Preserve the shebang and marker comment.

- [ ] **Step 2: Write failing hook config tests**

Tests:

```rust
register_hooks_creates_config_when_missing
register_hooks_is_idempotent
remove_hooks_preserves_other_config
```

- [ ] **Step 3: Implement hook install/remove**

Implement:

```rust
pub fn install_hook_script() -> Result<PathBuf>;
pub fn register_hooks_in_codex_config(config_path: &Path, script_path: &Path) -> Result<()>;
pub fn remove_hooks_from_codex_config(config_path: &Path) -> Result<()>;
```

Use marker strings:

```rust
const HOOKS_MARKER_START: &str = "# AuthPilot session recovery hooks";
const HOOKS_MARKER_END: &str = "# End AuthPilot hooks";
```

- [ ] **Step 4: Write failing parser tests**

Tests:

```rust
parse_hook_file_reads_valid_json
parse_hook_file_detects_clean_exit_sentinel
derive_thread_id_uses_valid_session_id
derive_thread_id_falls_back_to_transcript_path
is_valid_uuid_rejects_non_uuid
```

- [ ] **Step 5: Implement hook parser**

Implement the `HookPayload`, `parse_hook_file`, `derive_thread_id`, and `is_valid_uuid` functions from the user-provided plan.

- [ ] **Step 6: Run tests**

```bash
cd src-tauri
cargo test recovery::hooks:: recovery::hook_watcher:: -- --nocapture
```

- [ ] **Step 7: Commit**

```bash
git add resources/record-session.sh src-tauri/src/recovery/hooks.rs src-tauri/src/recovery/hook_watcher.rs
git commit -m "Install Codex recovery hooks"
```

## Task 8: Recovery Actions And Commands

**Files:**
- Create: `src-tauri/src/recovery/resume.rs`
- Create: `src-tauri/src/commands/recovery.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Implement recovery prompt constant**

Use the exact seven-line `RECOVERY_PROMPT` from the user-provided plan.

- [ ] **Step 2: Implement command builders with tests**

Add pure helpers:

```rust
pub fn build_deeplink(thread_id: &str) -> Option<String>;
pub fn build_background_resume_args(session: &CodexSession) -> Vec<String>;
pub fn codex_resume_available() -> bool;
```

Tests:

```rust
build_deeplink_rejects_non_uuid
background_resume_with_session_id_uses_session_id
background_resume_without_session_id_uses_last
```

- [ ] **Step 3: Implement actions**

Implement:

```rust
pub fn reopen_in_codex_desktop(session: &CodexSession) -> Result<ReopenOutcome>;
pub async fn background_resume(session: &CodexSession, db: &SessionDb, log_dir: &Path) -> Result<BackgroundResumeOutcome>;
pub fn open_recovery_log(log_path: &Path) -> Result<()>;
```

Enforce max attempts `3` and cooldown `30s`.

- [ ] **Step 4: Add Tauri commands**

Expose:

```rust
recovery_reopen
recovery_copy_prompt
recovery_background_resume
recovery_ignore
recovery_open_log
recovery_list_interrupted
```

- [ ] **Step 5: Run tests**

```bash
cd src-tauri
cargo test recovery::resume:: -- --nocapture
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/recovery/resume.rs src-tauri/src/commands/recovery.rs src-tauri/src/commands/mod.rs src-tauri/src/lib.rs
git commit -m "Add Codex recovery actions"
```

## Task 9: Recovery Process Watch And Startup Integration

**Files:**
- Create: `src-tauri/src/recovery/process_watch.rs`
- Modify: `src-tauri/src/recovery/mod.rs`
- Modify: `src-tauri/src/monitor/mod.rs`
- Modify: `src-tauri/src/types.rs`

- [ ] **Step 1: Add recovery state types**

Add:

```rust
#[derive(Debug, Default, Clone)]
pub struct RecoveryState {
    pub absence_started: std::collections::HashMap<String, chrono::DateTime<chrono::Utc>>,
    pub resume_command_available: bool,
}
```

Add `pub recovery: RecoveryState` to `MonitorState`.

- [ ] **Step 2: Write process watch tests**

Tests:

```rust
known_pid_present_keeps_session_running
known_pid_absent_under_grace_keeps_session_running
known_pid_absent_after_grace_marks_interrupted
matching_workspace_keeps_session_running_without_pid
```

- [ ] **Step 3: Implement process watch**

Implement `check_cycle` from the user-provided plan with `INTERRUPTION_GRACE_SECONDS = 10`.

- [ ] **Step 4: Integrate startup**

In Tauri setup:

```rust
let db = recovery::session_db::SessionDb::open(&recovery::default_db_path()?)?;
db.migrate()?;
db.mark_stale_sessions_interrupted(Utc::now() - chrono::Duration::seconds(120))?;
recovery::hooks::install_hook_script()?;
let resume_available = recovery::resume::codex_resume_available();
```

Store DB in Tauri state, update `MonitorState.recovery.resume_command_available`, and emit interrupted sessions.

- [ ] **Step 5: Add shared process scan and integrate monitor cycle**

In `src-tauri/src/process/mod.rs`, extract the existing `ps` call currently embedded in `codex_activity_report_result` into a public helper:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub command: String,
    pub cwd: Option<String>,
}

pub fn fetch_process_list() -> Result<Vec<ProcessInfo>> {
    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
        .context("Failed to run ps")?;

    if !output.status.success() {
        anyhow::bail!("ps failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }

    Ok(parse_process_list(&String::from_utf8_lossy(&output.stdout)))
}
```

If the existing `ProcessInfo` does not include `cwd`, add `cwd: Option<String>` and default it to `None` in the `ps` parser. Recovery can match PID first; workspace matching is best-effort and only works when cwd is available from a future enhancement.

In `monitor/mod.rs`, at the start of each monitor cycle after reading `poll_interval`, fetch once:

```rust
let processes = process::fetch_process_list().unwrap_or_else(|err| {
    tracing::warn!("[monitor] failed to fetch process list: {err}");
    Vec::new()
});
```

Update Codex activity checks to use the shared list by adding:

```rust
pub fn codex_activity_report_from_process_list(processes: &[ProcessInfo]) -> CodexActivityReport
```

Then call recovery at the end of each cycle:

```rust
let mut state_guard = state.write().await;
recovery::process_watch::check_cycle(
    &processes,
    &mut state_guard.recovery,
    &db,
    &app_handle,
)
.await?;
```

- [ ] **Step 6: Run tests**

```bash
cd src-tauri
cargo test recovery:: monitor:: -- --nocapture
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/recovery src-tauri/src/monitor/mod.rs src-tauri/src/types.rs src-tauri/src/lib.rs
git commit -m "Detect interrupted Codex sessions"
```

## Task 10: Frontend Recovery Card

**Files:**
- Create: `src/components/RecoveryCard.tsx`
- Create: `src/components/RecoveryDialog.tsx`
- Modify: `src/components/TrayPopup.tsx`
- Modify: `src/types/index.ts`

- [ ] **Step 1: Add TypeScript types**

Add:

```ts
export type SessionStatus =
  | "running"
  | "completed"
  | "interrupted"
  | "ignored"
  | "background_resumed";

export interface CodexSession {
  id: string;
  thread_id: string | null;
  session_id: string | null;
  workspace_path: string;
  account_id: string | null;
  process_id: number | null;
  started_at: string;
  last_seen_at: string;
  ended_at: string | null;
  status: SessionStatus;
  recovery_attempts: number;
  last_recovery_at: string | null;
  last_recovery_prompt: string | null;
}
```

- [ ] **Step 2: Create compact card**

`RecoveryCard.tsx` props:

```tsx
interface RecoveryCardProps {
  sessions: CodexSession[];
  resumeAvailable: boolean;
  onReopen: (id: string) => void;
  onCopyPrompt: () => void;
  onResume: (id: string) => void;
  onIgnore: (id: string) => void;
}
```

Render one card at top of tray popup. Show `1 of N` navigation when `sessions.length > 1`. Hide background resume button when `resumeAvailable` is false.

- [ ] **Step 3: Add confirmation dialog**

`RecoveryDialog.tsx` must show the warning text from the user-provided plan before calling `onConfirm`.

- [ ] **Step 4: Wire events and commands**

In `TrayPopup.tsx`, listen for:

```ts
recovery:session-interrupted
recovery:session-resolved
recovery:sessions-list
```

Invoke commands:

```ts
recovery_reopen
recovery_copy_prompt
recovery_background_resume
recovery_ignore
```

- [ ] **Step 5: Run frontend build**

```bash
pnpm build
```

- [ ] **Step 6: Commit**

```bash
git add src/components/RecoveryCard.tsx src/components/RecoveryDialog.tsx src/components/TrayPopup.tsx src/types/index.ts
git commit -m "Add recovery card to tray"
```

## Task 11: Full Verification, Install, Push

**Files:** all touched files.

- [ ] **Step 1: Run full tests**

```bash
cd src-tauri && cargo test
cd .. && pnpm build
```

Expected: all Rust tests pass; frontend build passes.

- [ ] **Step 2: Build app**

```bash
pnpm tauri build
```

Expected:

```text
Finished 2 bundles at:
src-tauri/target/release/bundle/macos/AuthPilot.app
src-tauri/target/release/bundle/dmg/AuthPilot_1.0.0_aarch64.dmg
```

- [ ] **Step 3: Replace installed app**

```bash
osascript -e 'quit app "AuthPilot"' >/dev/null 2>&1 || true
sleep 1
rm -rf /Applications/AuthPilot.app
cp -R src-tauri/target/release/bundle/macos/AuthPilot.app /Applications/AuthPilot.app
open -a /Applications/AuthPilot.app
```

- [ ] **Step 4: Verify app and proxy cleanup**

```bash
osascript -e 'application "AuthPilot" is running'
lsof -nP -iTCP:18080 -sTCP:LISTEN || true
scutil --proxy | sed -n '1,80p'
```

Expected: AuthPilot running; no AuthPilot listener on `18080`; no AuthPilot-managed HTTPS proxy.

- [ ] **Step 5: Commit remaining changes**

```bash
git status --short
git add .
git commit -m "Add restart recovery and exhausted-limit handling"
```

If all changes were already committed in earlier tasks, skip this commit.

- [ ] **Step 6: Push**

```bash
git push origin main
```

## Acceptance Criteria

- Proxy hot-swap, CLI wrapper proxy, CA, and system proxy code are removed from production code.
- AuthPilot no longer starts a proxy listener or modifies macOS proxy settings.
- Tray inactive accounts show both 5h and 7-day usage.
- Tray disables accounts whose weekly limit is exhausted even if 5h has capacity.
- Auto-switch never selects an account whose weekly limit is exhausted.
- Active account at `<= 1% remaining` in either window switches immediately and restarts Codex even if Codex is active.
- Before forced restart, AuthPilot captures the newest recent Codex session id when available.
- After restart, AuthPilot best-effort runs `codex exec resume <SESSION_ID> continue`.
- Recovery DB stores interrupted sessions and survives AuthPilot restarts.
- Recovery card appears in tray for interrupted sessions and offers reopen, copy prompt, ignore, and optional background resume.
- Background resume is hidden unless `codex exec resume --help` succeeds.
- No recovery code reads `~/.codex/auth.json` except existing account switch/session modules.
- All tests pass, app builds, installed app is replaced, and `main` is pushed.

## Self-Review

- Spec coverage: covers proxy revert, tray edge case, hard exhaustion, session capture, hook-based recovery, DB persistence, UI, and install.
- Placeholder scan: no `TBD`, `TODO`, or undefined implementation sections remain.
- Type consistency: `CodexSession`, `SessionStatus`, `RecoveryState`, `ReopenOutcome`, and command names are introduced before frontend usage.
