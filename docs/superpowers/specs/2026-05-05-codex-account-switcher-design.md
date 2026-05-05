# Intelligent Codex Account Switcher — Design Specification

> **Version:** 1.1 (Revised)  
> **Date:** 2026-05-05  
> **Status:** Pending Approval

---

## 1. Overview

### 1.1 Purpose
Build an **intelligent, tray-only desktop app** for macOS that manages multiple OpenAI Codex accounts, monitors usage limits in real-time, and **automatically switches accounts** when limits are reached.

### 1.2 Key Differentiators vs. Original
| Feature | Original | This Version |
|---------|----------|--------------|
| UI Mode | Windowed app | Tray-only (dashboard on click) |
| Auto-switch | Manual only | Automatic + manual |
| Background monitoring | None | 60-second polling loop |
| Codex restart on switch | Kill background helpers only | Full kill + relaunch |
| Usage threshold | N/A | Configurable per account (default 95%) |
| First-run config | None | Forces `file` auth mode |
| Logging | `println!` only | Structured `tracing` with file appender |

### 1.3 Target Platform
- **Primary:** macOS (Apple Silicon + Intel)
- **Future:** Windows (taskkill / start.exe swap)

---

## 2. Architecture

### 2.1 High-Level Diagram

```
┌─────────────────────────────────────────────────────────┐
│                      macOS Menu Bar                     │
│  ┌──────────────┐                                       │
│  │ Tray Icon    │  ← Green/Amber/Red indicator           │
│  │ (Tauri Tray) │                                       │
│  └──────┬───────┘                                       │
│         │ click                                          │
│         ▼                                                │
│  ┌──────────────────────────────────────┐               │
│  │ Tray Menu                            │               │
│  │ • Active: Work (87%)                 │               │
│  │ ─────────────────────                │               │
│  │ • Personal (12%)                     │               │
│  │ • Team B (45%)                       │               │
│  │ ─────────────────────                │               │
│  │ Open Dashboard    Quit               │               │
│  └──────────┬───────────────────────────┘               │
│             │ click "Open Dashboard"                    │
│             ▼                                            │
│  ┌──────────────────────────────────────────────┐       │
│  │ Tauri Webview (React Dashboard)              │       │
│  │ • All accounts with usage bars               │       │
│  │ • Settings: threshold, poll interval         │       │
│  │ • Switch log (last 20 entries)               │       │
│  │ • Add / remove / rename accounts             │       │
│  │ • First-run consent modal                    │       │
│  └──────────────────────────────────────────────┘       │
└─────────────────────────────────────────────────────────┘
                           │
                           │ Tauri Commands
                           ▼
┌─────────────────────────────────────────────────────────┐
│              Rust Backend (Tauri Process)               │
│  ┌─────────────────┐  ┌──────────────────────────────┐  │
│  │ Background      │  │ Session Manager              │  │
│  │ Monitor         │  │ • Snapshot per-account       │  │
│  │ (tokio task)    │  │   auth.json                  │  │
│  │ • Poll every 60s│  │ • Atomic write to            │  │
│  │ • Check limits  │  │   ~/.codex/auth.json         │  │
│  │ • Emit events   │  │ • First-run config patch     │  │
│  └────────┬────────┘  └──────────────┬───────────────┘  │
│           │ threshold hit            │                  │
│           ▼                          │ switch command   │
│  ┌─────────────────┐                 │                  │
│  │ Auto-Switch     │─────────────────┘                  │
│  │ Engine          │                                    │
│  │ • Pick target   │                                    │
│  │ • Cooldown 5m   │                                    │
│  │ • Depleted →    │                                    │
│  │   soonest reset │                                    │
│  └────────┬────────┘                                    │
│           │                                              │
│           ▼                                              │
│  ┌─────────────────┐  ┌──────────────────────────────┐  │
│  │ Switch Executor │  │ Tray Manager                 │  │
│  │ • fs::write     │  │ • Build menu dynamically     │  │
│  │ • notify user   │  │ • Update icon colour         │  │
│  │ • kill Codex    │  │ • Handle clicks              │  │
│  │ • sleep 500ms   │  │ • Show native notifications  │  │
│  │ • open Codex    │  └──────────────────────────────┘  │
│  └─────────────────┘                                     │
│  ┌─────────────────────────────────────────────────────┐│
│  │ Shared State (Arc<RwLock<MonitorState>>)            ││
│  │ • active_account_id                                 ││
│  │ • last_auto_switch (persisted)                      ││
│  │ • settings                                          ││
│  │ • latest_usages                                     ││
│  └─────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────┘
```

### 2.2 File System Layout

```
~/.codex-switcher/
├── accounts.json                 # Account registry (original format)
├── settings.json                 # App settings + last_auto_switch
│   {
│     "poll_interval_seconds": 60,
│     "notification_enabled": true,
│     "auto_switch_enabled": true,
│     "global_cooldown_seconds": 300,
│     "last_auto_switch": "2026-05-05T12:00:00Z",  // NEW: persisted cooldown
│     "account_settings": {
│       "<account_id>": { "switch_threshold": 95.0 },
│       ...
│     }
│   }
├── switch_log.json               # Last 20 switch events
├── switcher.log                  # Structured tracing log (rotated daily)
└── accounts/
    ├── <uuid_1>/
    │   └── auth.json             # Snapshot for account 1
    ├── <uuid_2>/
    │   └── auth.json             # Snapshot for account 2
    └── ...

~/.codex/
├── config.toml                   # Patched on first run
│   cli_auth_credentials_store = "file"
└── auth.json                     # Active account (swapped atomically)
```

---

## 3. Usage API Specification

### 3.1 Endpoint

For **ChatGPT OAuth accounts**, the usage endpoint is:

```
GET https://chatgpt.com/backend-api/wham/usage
```

**Headers:**
```
User-Agent: codex-cli/1.0.0
Authorization: Bearer <access_token>
chatgpt-account-id: <account_id>    (optional, for multi-account users)
```

**Note:** API key accounts do **not** support usage queries through this endpoint. They return `UsageInfo::error("Usage info not available for API key accounts")`.

### 3.2 Response Shape

```json
{
  "plan_type": "plus",
  "rate_limit": {
    "primary_window": {
      "used_percent": 87.5,
      "limit_window_seconds": 18000,
      "reset_at": 1714915200
    },
    "secondary_window": {
      "used_percent": 34.2,
      "limit_window_seconds": 604800,
      "reset_at": 1715347200
    }
  },
  "credits": {
    "has_credits": false,
    "unlimited": false,
    "balance": null
  }
}
```

### 3.3 Rate Limit Types

| Window | `limit_window_seconds` | Description |
|--------|------------------------|-------------|
| Primary | 18,000 (5h) | Rolling 5-hour window. Most important for auto-switch. |
| Secondary | 604,800 (7d) | Weekly rolling window. Secondary consideration. |

### 3.4 Computing `used_percent`

The API returns `used_percent` directly as a `f64` (0.0 – 100.0). No computation needed.

**Auto-switch threshold comparison:**
```rust
fn is_over_threshold(usage: &UsageInfo, threshold: f64) -> bool {
    usage.primary_used_percent.map_or(false, |p| p >= threshold)
}
```

### 3.5 Error Handling

| Response | Behavior |
|----------|----------|
| `401 Unauthorized` | Attempt token refresh once. If still 401, mark usage as error and skip this account for auto-switch. |
| `429 Too Many Requests` | Back off for one poll cycle (skip this account), log warning. |
| `5xx` | Retry once after 1s delay. If still failing, mark as error. |
| Network timeout (>10s) | Mark as error, do not block other accounts. |

---

## 4. Token Refresh Specification

### 4.1 Endpoint

```
POST https://auth.openai.com/oauth/token
Content-Type: application/x-www-form-urlencoded
```

**Body:**
```
grant_type=refresh_token&refresh_token=<refresh_token>&client_id=app_EMoamEEZ73f0CkXaXp7hrann
```

### 4.2 Response

```json
{
  "id_token": "eyJ...",
  "access_token": "eyJ...",
  "refresh_token": "def502..."
}
```

**Note:** `id_token` and `refresh_token` may be omitted on rotation. The existing values must be retained as fallbacks.

### 4.3 Refresh Flow

1. Before querying usage for a ChatGPT account, check if `access_token` is expired (JWT `exp` claim ≤ now + 60s skew).
2. If expired or near expiry, call the refresh endpoint.
3. On success, update the account's tokens in `accounts.json` AND update the active `~/.codex/auth.json` if this is the currently active account.
4. On failure (e.g., refresh token revoked), mark the account as error. Do **not** attempt to auto-switch to an account whose token cannot be refreshed.

### 4.4 Failure Mode

If token refresh fails for an account:
- Usage is marked as error: `"Token refresh failed — account may need re-authentication"`.
- The account is **excluded** from auto-switch target selection.
- A notification is sent: "Account {name} needs re-authentication."

---

## 5. Data Models

### 5.1 AuthDotJson

The exact structure of `~/.codex/auth.json`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthDotJson {
    #[serde(rename = "OPENAI_API_KEY", skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenData {
    pub id_token: String,
    pub access_token: String,
    pub refresh_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}
```

### 5.2 UsageInfo

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub account_id: String,
    pub plan_type: Option<String>,
    pub primary_used_percent: Option<f64>,
    pub primary_window_minutes: Option<i64>,
    pub primary_resets_at: Option<i64>,
    pub secondary_used_percent: Option<f64>,
    pub secondary_window_minutes: Option<i64>,
    pub secondary_resets_at: Option<i64>,
    pub has_credits: Option<bool>,
    pub unlimited_credits: Option<bool>,
    pub credits_balance: Option<String>,
    pub error: Option<String>,
}
```

### 5.3 AppSettings (with persisted cooldown)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub poll_interval_seconds: u64,          // default 60
    pub notifications_enabled: bool,         // default true
    pub auto_switch_enabled: bool,           // default true
    pub global_cooldown_seconds: u64,        // default 300
    pub last_auto_switch: Option<DateTime<Utc>>,  // PERSISTED cooldown
    pub account_settings: HashMap<String, AccountSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSettings {
    pub switch_threshold: f64,               // default 95.0
}
```

### 5.4 Switch Event

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchEvent {
    pub timestamp: DateTime<Utc>,
    pub from_account_id: Option<String>,
    pub to_account_id: String,
    pub reason: SwitchReason,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SwitchReason {
    AutoLimitReached,
    AutoDepleted,
    Manual,
}
```

### 5.5 MonitorState (Shared State)

```rust
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MonitorState {
    pub settings: AppSettings,
    pub latest_usages: Vec<UsageInfo>,
    pub is_monitor_running: bool,
}

// Stored in Tauri managed state:
// app.manage(Arc::new(RwLock::new(MonitorState::default())));
```

**Concurrency rules:**
- Background monitor holds a **read lock** while polling.
- Command handlers (`save_settings`, `manual_switch_account`) acquire a **write lock** when mutating settings or triggering switches.
- `last_auto_switch` is read from `settings` (which is behind the RwLock), so it is naturally synchronized.

---

## 6. Component Specifications

### 6.1 Session Manager (`src-tauri/src/session/mod.rs`)

**Responsibilities:**
- Store per-account `auth.json` snapshots in `~/.codex-switcher/accounts/<id>/auth.json`
- Atomically write a snapshot to `~/.codex/auth.json` on switch
- Validate that a stored token is not expired before switching
- Own the first-run config patch

**Key Behaviors:**
- `snapshot_account(account_id: &str, auth_json: &AuthDotJson) -> Result<()>`  
  Writes the current `auth.json` content into the account's snapshot directory.
- `restore_account(account_id: &str) -> Result<AuthDotJson>`  
  Reads the snapshot and returns it. Fails if the snapshot is missing.
- `swap_active_auth(account_id: &str) -> Result<()>`  
  Atomically writes the account snapshot to `~/.codex/auth.json`:
  ```rust
  let tmp = codex_auth_path.with_extension("tmp");
  fs::write(&tmp, content)?;
  fs::rename(&tmp, &codex_auth_path)?;  // atomic on same filesystem
  ```
  **Assumption:** `~/.codex-switcher/` and `~/.codex/` are on the same APFS volume. This is true for 99.9% of macOS home directories.
- `ensure_file_auth_mode() -> Result<bool>`  
  On first run, checks `~/.codex/config.toml` for `cli_auth_credentials_store = "file"`. If missing or different, writes it. Returns `true` if a change was made.
- `is_token_expired(auth_json: &AuthDotJson) -> bool`  
  For ChatGPT tokens, parses the `access_token` JWT `exp` claim. Returns `true` if expired (with 60s skew).

**Security:**
- All snapshot files created with `0o600` permissions.
- Parent directories created with `0o700`.

### 6.2 Background Monitor (`src-tauri/src/monitor/mod.rs`)

**Responsibilities:**
- Spawn a long-lived `tokio` task that polls usage every N seconds (configurable, default 60).
- Fetch usage for **all** accounts concurrently (max 10 concurrent).
- Emit `usage-update` events to the frontend via `tauri::Emitter`.
- Trigger the auto-switch engine when the active account crosses its threshold.

**Key Behaviors:**
- `start_monitor(app_handle: AppHandle, state: Arc<RwLock<MonitorState>>)`  
  Spawns the background task. Called from `.setup()` in `lib.rs`.
- `stop_monitor()`  
  Signals cancellation via an `AtomicBool`.
- Polling loop:
  ```rust
  loop {
      if cancelled.load(Relaxed) { break; }
      
      let state = state.read().await;
      let poll_interval = state.settings.poll_interval_seconds;
      let accounts = load_accounts()?;
      drop(state);  // Release read lock during network I/O
      
      let usages = refresh_all_usage(&accounts).await;
      
      {
          let mut state = state.write().await;
          state.latest_usages = usages.clone();
      }
      
      // Emit to frontend
      app_handle.emit("usage-update", &usages)?;
      
      // Check active account threshold
      if let Some(active) = accounts.active_account_id {
          if let Some(usage) = usages.iter().find(|u| u.account_id == active) {
              let state = state.read().await;
              if should_auto_switch(usage, &state.settings, &accounts) {
                  drop(state);
                  auto_switch_engine::trigger(active, &accounts, &usages, &app_handle, &state).await?;
              }
          }
      }
      
      sleep(Duration::from_secs(poll_interval)).await;
  }
  ```

**Event Emission:**
- `usage-update` — `Vec<UsageInfo>` sent every poll cycle.
- `auto-switch-triggered` — `{ from_account_id, to_account_id, reason }` sent when auto-switch fires.
- `account-switched` — `SwitchEvent` sent on any switch.

### 6.3 Auto-Switch Engine (`src-tauri/src/auto_switch/mod.rs`)

**Responsibilities:**
- Decide **when** to switch.
- Decide **which** account to switch **to**.
- Enforce cooldowns to prevent thrashing.

**Threshold Detection:**
```rust
fn should_auto_switch(
    usage: &UsageInfo,
    settings: &AppSettings,
    accounts: &AccountsStore,
) -> bool {
    if !settings.auto_switch_enabled {
        return false;
    }
    
    // Check global cooldown
    if let Some(last) = settings.last_auto_switch {
        if Utc::now().signed_duration_since(last).num_seconds() < settings.global_cooldown_seconds as i64 {
            return false;
        }
    }
    
    let threshold = settings.account_settings
        .get(&usage.account_id)
        .and_then(|s| Some(s.switch_threshold))
        .unwrap_or(95.0);
    
    usage.primary_used_percent.map_or(false, |p| p >= threshold)
}
```

**Target Selection Algorithm:**
1. Filter out the current active account.
2. Filter out accounts with `usage.error` set.
3. Filter out accounts whose token is expired (call `session::is_token_expired()`).
4. Sort by: **highest remaining quota** (`100 - used_percent`) descending.
5. Tie-breaker: **earliest reset time** (`resets_at` ascending).
6. If no account has quota left → select account with **soonest reset time** and send a "all depleted" notification.

**Cooldown:**
- Global cooldown of **5 minutes** between auto-switches.
- Stored in `settings.last_auto_switch` and persisted to `settings.json`.
- When a switch happens, write `last_auto_switch = Some(Utc::now())` and save settings.
- If cooldown is active, log a skip at `INFO` level and do nothing.

**Depleted State:**
- If all accounts are at or above threshold:
  - Switch to the account with the soonest `resets_at`.
  - Emit a notification: "All accounts depleted. Switched to {name}. Limit resets in {eta}."

### 6.4 Switch Executor (`src-tauri/src/switch_executor/mod.rs`)

**Responsibilities:**
- Execute the confirmed switch-and-relaunch sequence.
- Handle both manual and auto-initiated switches.
- Skip kill/relaunch if Codex was not running.

**Sequence:**
```rust
pub async fn execute_switch(
    target_account_id: &str,
    reason: SwitchReason,
    app_handle: &AppHandle,
) -> Result<()> {
    let previous_active = storage::get_active_account()?.map(|a| a.id);
    
    // 1. Write auth.json
    session_manager::swap_active_auth(target_account_id)?;
    
    // 2. Detect if Codex is running
    let was_running = process::is_codex_desktop_running()?;
    
    // 3. Notify user
    if was_running {
        let target_name = get_account_name(target_account_id)?;
        notification::send(
            "Switching Codex Account",
            &format!("Switching to {} — Codex will restart", target_name)
        )?;
    }
    
    // 4. Kill Codex if running
    if was_running {
        process::kill_codex_desktop().await?;
        tokio::time::sleep(Duration::from_millis(500)).await;
        process::launch_codex_desktop().await?;
    }
    
    // 5. Log the switch
    switch_log::append(SwitchEvent {
        timestamp: Utc::now(),
        from_account_id: previous_active,
        to_account_id: target_account_id.to_string(),
        reason,
    })?;
    
    // 6. Update active account in store
    storage::set_active_account(target_account_id)?;
    
    // 7. Update cooldown
    if matches!(reason, SwitchReason::AutoLimitReached | SwitchReason::AutoDepleted) {
        settings::update_last_auto_switch(Utc::now())?;
    }
    
    // 8. Emit event to frontend
    app_handle.emit("account-switched", &SwitchEvent { ... })?;
    
    Ok(())
}
```

**Process Management (macOS):**
- `is_codex_desktop_running() -> Result<bool>`: Uses `osascript -e 'application "Codex" is running'`. Returns `true` if exit code is 0 and output is `"true"`.
- `kill_codex_desktop() -> Result<()>`: Uses `osascript -e 'quit application "Codex"'`. Waits up to 3s, then falls back to `pkill -x "Codex"` if still running.
- `launch_codex_desktop() -> Result<()>`: Uses `open -a "Codex"`.

**Why osascript over killall:**
- `osascript` addresses the app by **bundle ID**, not by process name string matching.
- This avoids false positives on anything with "Codex" in its path or command line.
- It sends a graceful quit message through the macOS app lifecycle.

### 6.5 Tray Manager (`src-tauri/src/tray/mod.rs`)

**Responsibilities:**
- Build and update the system tray menu dynamically.
- Show native macOS notifications.
- Handle tray icon clicks (left = show dashboard, right = show menu).
- Update tray icon colour based on active account usage.

**Tray Icon States:**
| State | Asset File | Condition |
|-------|------------|-----------|
| Green | `tray-green.png` | Active account < 70% used |
| Amber | `tray-amber.png` | Active account 70–94% used |
| Red   | `tray-red.png` | Active account ≥ 95% used |

Tauri v2 requires pre-built icon assets for tray icons. We generate 3 32×32 PNGs with transparent backgrounds and solid colour circles.

**Menu Structure:**
```
Active: Work (87%)           ← disabled, info only
━━━━━━━━━━━━━━━━━━━━━━━━━━
Personal          12% left   ← clickable → manual switch
Team B            45% left   ← clickable → manual switch
━━━━━━━━━━━━━━━━━━━━━━━━━━
Open Dashboard               ← opens main window
Settings...                  ← opens settings panel
━━━━━━━━━━━━━━━━━━━━━━━━━━
Quit                         ← exits app + kills monitor
```

**Menu Rebuild Debounce:**
- Track the last built menu state as a hash of `(active_account_id, active_primary_percent, account_count)`.
- Only call `tray.set_menu()` when the hash changes.
- If the user has the menu open, Tauri replaces it atomically — there may be a brief dismiss. This is acceptable for a 60s update cycle.

**Window Configuration:**
```json
{
  "app": {
    "windows": [
      {
        "title": "Codex Switcher",
        "width": 900,
        "height": 700,
        "visible": false,
        "decorations": true,
        "exitOnClose": false
      }
    ]
  }
}
```

**Critical:** `exitOnClose: false` on the **window** config (not global app config) ensures the Tauri process stays alive when the dashboard window is closed.

### 6.6 Enhanced Frontend

**First-Run Consent Flow:**
1. On app startup, backend calls `session::is_file_auth_mode_required()`.
2. If `true`, backend emits `first-run-required` event to frontend BEFORE showing the window.
3. Frontend opens the dashboard with a modal overlay:
   - Title: "First-Time Setup"
   - Body: "To enable automatic account switching, Codex Switcher needs to store credentials in a file instead of the macOS Keychain. This is required for the switcher to read and swap accounts."
   - Button: "I Understand — Enable File Mode"
4. On button click, frontend calls `ensure_file_auth_mode()` Tauri command.
5. Backend writes `cli_auth_credentials_store = "file"` to `~/.codex/config.toml`.
6. Modal closes, normal dashboard appears.

**Dashboard (`src/components/Dashboard.tsx`):**
- Opens when tray "Open Dashboard" is clicked or on first-run.
- Shows all accounts in a scrollable list.
- Each account card shows: name, email (masked), plan badge, usage bars, last refresh time, switch button.
- Active account highlighted with a green border.
- Top bar: global poll interval, last poll timestamp, active account status.
- Receives `usage-update` events via `listen('usage-update', ...)`.

**Settings (`src/components/Settings.tsx`):**
- Poll interval: slider 10s – 300s (default 60s).
- Global notification toggle.
- Per-account threshold: slider 50% – 100% (default 95%).
- Auto-switch master toggle.

**Switch Log (`src/components/SwitchLog.tsx`):**
- Table showing last 20 switches.
- Columns: Time, From → To, Reason, Duration since last switch.

---

## 7. API & Commands

### 7.1 Tauri Commands

| Command | Input | Output | Description |
|---------|-------|--------|-------------|
| `get_settings` | — | `AppSettings` | Load current settings |
| `save_settings` | `AppSettings` | `()` | Save settings to disk |
| `get_switch_log` | — | `Vec<SwitchEvent>` | Return last 20 events |
| `manual_switch_account` | `account_id: String` | `()` | Trigger a manual switch |
| `show_dashboard` | — | `()` | Show the dashboard window |
| `ensure_file_auth_mode` | — | `bool` | Patch config.toml, return if changed |
| `is_file_auth_mode_required` | — | `bool` | Check if first-run patch is needed |

### 7.2 Frontend Events (Rust → JS)

| Event | Payload | Frequency |
|-------|---------|-----------|
| `usage-update` | `Vec<UsageInfo>` | Every poll cycle |
| `auto-switch-triggered` | `{ from, to, reason }` | On auto-switch |
| `account-switched` | `SwitchEvent` | On any switch |
| `first-run-required` | `()` | Once, on first launch |

---

## 8. Security & Privacy

### 8.1 Credential Storage
- Per-account snapshots stored at `~/.codex-switcher/accounts/<id>/auth.json`.
- File permissions: `0o600` (owner read/write only).
- Directory permissions: `0o700`.
- No encryption at rest — same threat model as `~/.codex/auth.json`.

### 8.2 First-Run Config Patch
- Forces `cli_auth_credentials_store = "file"` in `~/.codex/config.toml`.
- **Warning to user:** Credentials will be stored in a plaintext file instead of the macOS Keychain. This is required for the switcher to function.
- Show a one-time consent dialog in the frontend before applying the patch.
- If user declines, the app runs in "manual mode" (no auto-switch, no auth.json swapping).

### 8.3 Process Termination
- `osascript -e 'quit application "Codex"'` is used to terminate Codex.
- If graceful quit fails after 3s, `pkill -x "Codex"` is used as fallback.
- Never sends signals to unrelated processes.
- If Codex is not running, the kill step is skipped entirely.

### 8.4 App Signing Note
- The app should be code-signed and notarized for macOS distribution.
- Unsigned apps may be blocked by Gatekeeper when executing `osascript` or `pkill`.
- For development, right-click → Open bypasses Gatekeeper.

---

## 9. Error Handling

| Error Scenario | Behavior |
|----------------|----------|
| Usage API fails for one account | Log error, mark usage as `error`, continue polling others |
| Usage API fails for ALL accounts | Log error, retry next cycle, do NOT auto-switch |
| Token expired for target account | Refresh token first. If refresh fails, skip account and pick next best. Notify user. |
| Token refresh fails | Mark account as error, exclude from auto-switch, notify user |
| Codex kill fails | Log error, still attempt `open -a "Codex"`. Notify user. |
| Codex launch fails | Log error, notify user: "Failed to relaunch Codex. Please open it manually." |
| auth.json write fails (disk full) | Atomic temp-file rename prevents corruption. Notify user. |
| Settings file corrupt | Reset to defaults, log warning. |
| Tray menu rebuild during open menu | Brief dismiss is acceptable. Debounce prevents unnecessary rebuilds. |

---

## 10. Logging

### 10.1 Structured Logging with `tracing`

**Crate:** `tracing` + `tracing-appender` + `tracing-subscriber`

**Log file:** `~/.codex-switcher/switcher.log` (rotated daily, 7-day retention)

**Log levels:**
- `ERROR` — Failures that require user attention (token refresh failed, Codex launch failed)
- `WARN` — Recoverable issues (usage API 429, cooldown active)
- `INFO` — Normal operations (poll cycle completed, switch executed, threshold crossed)
- `DEBUG` — Detailed state (usage response body, menu rebuild hash comparison)

**Example log entries:**
```
2026-05-05T14:32:01.234Z  INFO  monitor: Poll cycle started, 3 accounts
2026-05-05T14:32:02.456Z  INFO  monitor: Account "Work" usage: 87.5% primary, 34.2% secondary
2026-05-05T14:32:02.457Z  WARN  auto_switch: Account "Work" crossed threshold (95%), but cooldown active (last switch 2m ago)
2026-05-05T14:37:02.891Z  INFO  auto_switch: Threshold crossed for "Work" (96.2%). Selecting target...
2026-05-05T14:37:02.892Z  INFO  auto_switch: Selected "Personal" (12.3% used, resets in 2h)
2026-05-05T14:37:02.893Z  INFO  switch_executor: Writing auth.json for account "Personal"
2026-05-05T14:37:02.895Z  INFO  switch_executor: Codex is running. Sending quit signal.
2026-05-05T14:37:03.412Z  INFO  switch_executor: Codex quit confirmed. Relaunching...
2026-05-05T14:37:04.123Z  INFO  switch_executor: Switch complete. Work → Personal (AutoLimitReached)
```

**Configuration:**
```rust
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

let file_appender = RollingFileAppender::new(
    Rotation::DAILY,
    config_dir,
    "switcher.log",
);
let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

tracing_subscriber::registry()
    .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
    .with(tracing_subscriber::EnvFilter::new("info"))
    .init();
```

---

## 11. Testing Strategy

### 11.1 Unit Tests (Rust)
- **Auto-switch engine:** Mock usage data, verify target selection algorithm.
- **Session manager:** Temp directory tests for snapshot/restore/atomic write.
- **Cooldown logic:** Verify 5-minute cooldown enforcement with persisted state.
- **Token expiry parsing:** JWT `exp` claim parsing with skew.
- **Menu hash comparison:** Verify debounce only rebuilds on meaningful changes.

### 11.2 Integration Tests
- **Mock usage API:** Spin up a local HTTP server that returns fake rate limits. Verify auto-switch fires at 95%.
- **Process lifecycle:** Verify `osascript quit` + `open` sequence works with a dummy app.

### 11.3 Manual Tests
- Add 3 accounts, set low thresholds, verify auto-switch sequence.
- Close dashboard window, verify monitor continues (exitOnClose: false).
- Click tray account → verify manual switch.
- Deplete all accounts → verify "soonest reset" fallback.
- Restart app mid-cooldown → verify cooldown is preserved.

---

## 12. Dependencies

### Rust (Tauri)
| Crate | Purpose |
|-------|---------|
| `tauri` (v2) | Core framework |
| `tauri-plugin-notification` | Native macOS notifications |
| `tokio` | Async runtime, background task |
| `serde`, `serde_json` | Serialization |
| `chrono` | Timestamps |
| `toml` | Config file parsing/editing |
| `reqwest` | HTTP client (already present) |
| `tracing` | Structured logging |
| `tracing-appender` | File-based log appender |
| `tracing-subscriber` | Log subscriber configuration |

### Frontend
| Package | Purpose |
|---------|---------|
| `react`, `react-dom` | UI (already present) |
| `tailwindcss` | Styling (already present) |
| `@tauri-apps/api` | Tauri JS API (already present) |

---

## 13. Migration from Original

The original `codex-switcher` stores accounts in `~/.codex-switcher/accounts.json`. This new version:
1. Reads the **same** `accounts.json` format (backward compatible).
2. Ignores the old `auth.json` switching logic (replaced by Session Manager).
3. Adds new `settings.json`, `switch_log.json`, and `switcher.log` files.
4. Removes the web server (`codex-web` binary) — not needed for a tray app.

---

## 14. Decisions (Formerly Open Questions)

1. **Windows support:** Will be added in a future release. The process management module is designed with a trait boundary so macOS and Windows implementations can coexist.
2. **Keychain reversion:** No automatic reversion on uninstall. The user can manually remove `cli_auth_credentials_store = "file"` from `~/.codex/config.toml` if desired.
3. **CLI mode:** Not in scope. The app is tray-only. A `--headless` flag could be added later if needed.
4. **Notification plugin:** `tauri-plugin-notification` is the chosen plugin. `notify-rust` is not used.
5. **Process detection:** `osascript` is the primary method; `pkill -x` is the fallback.

---

*End of Design Specification*
