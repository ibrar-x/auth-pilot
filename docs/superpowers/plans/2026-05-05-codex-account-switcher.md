# Intelligent Codex Account Switcher Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a tray-only macOS Tauri app that manages multiple Codex accounts, monitors usage in real-time, and automatically switches accounts when limits are reached.

**Architecture:** Tauri v2 (Rust backend + React frontend). Background `tokio` task polls usage every 60s. Auto-switch engine selects best alternative account and executes kill+relaunch of Codex Desktop App. System tray shows live status and quick-switch menu.

**Tech Stack:** Tauri v2, React 19, Tailwind CSS, Rust, tokio, reqwest, tracing

---

## File Structure

```
codex-account-switcher/
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tsconfig.node.json
├── index.html
├── tailwind.config.js
├── src/
│   ├── main.tsx
│   ├── App.tsx
│   ├── App.css
│   ├── types/
│   │   └── index.ts
│   ├── lib/
│   │   └── platform.ts
│   ├── hooks/
│   │   └── useAccounts.ts
│   └── components/
│       ├── index.ts
│       ├── AccountCard.tsx
│       ├── UsageBar.tsx
│       ├── AddAccountModal.tsx
│       ├── Dashboard.tsx
│       ├── Settings.tsx
│       └── SwitchLog.tsx
├── src-tauri/
│   ├── Cargo.toml
│   ├── tauri.conf.json
│   ├── tauri.macos.conf.json
│   ├── build.rs
│   ├── capabilities/
│   │   └── default.json
│   └── src/
│       ├── main.rs
│       ├── lib.rs
│       ├── types.rs
│       ├── api/
│       │   └── mod.rs
│       ├── auth/
│       │   ├── mod.rs
│       │   ├── storage.rs
│       │   ├── switcher.rs
│       │   ├── token_refresh.rs
│       │   └── oauth_server.rs
│       ├── commands/
│       │   ├── mod.rs
│       │   ├── account.rs
│       │   ├── usage.rs
│       │   ├── oauth.rs
│       │   └── process.rs
│       ├── session/
│       │   └── mod.rs
│       ├── settings/
│       │   └── mod.rs
│       ├── monitor/
│       │   └── mod.rs
│       ├── auto_switch/
│       │   └── mod.rs
│       ├── switch_executor/
│       │   └── mod.rs
│       ├── process/
│       │   └── mod.rs
│       ├── tray/
│       │   └── mod.rs
│       └── switch_log/
│           └── mod.rs
```

---

## Phase 1: Project Scaffolding

### Task 1: Initialize Tauri v2 Project

**Files:**
- Create: `package.json`
- Create: `vite.config.ts`
- Create: `tsconfig.json`
- Create: `tsconfig.node.json`
- Create: `index.html`
- Create: `tailwind.config.js`
- Create: `src/main.tsx`
- Create: `src/App.css`

- [ ] **Step 1: Create package.json**

```json
{
  "name": "codex-account-switcher",
  "private": true,
  "version": "1.0.0",
  "description": "Intelligent multi-account manager for Codex CLI",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "preview": "vite preview",
    "tauri": "tauri"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.2.0",
    "@tauri-apps/plugin-dialog": "^2.2.0",
    "@tauri-apps/plugin-notification": "^2.2.0",
    "@tauri-apps/plugin-opener": "^2.2.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tailwindcss/vite": "^4.0.0",
    "@tauri-apps/cli": "^2.2.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.0",
    "tailwindcss": "^4.0.0",
    "typescript": "~5.7.0",
    "vite": "^6.0.0"
  }
}
```

- [ ] **Step 2: Create vite.config.ts**

```typescript
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

export default defineConfig(async () => ({
  plugins: [react(), tailwindcss()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: {
      ignored: ["**/src-tauri/**"],
    },
  },
}));
```

- [ ] **Step 3: Create tsconfig.json**

```json
{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "isolatedModules": true,
    "moduleDetection": "force",
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"]
}
```

- [ ] **Step 4: Create tsconfig.node.json**

```json
{
  "compilerOptions": {
    "strict": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true,
    "noEmit": true
  },
  "include": ["vite.config.ts"]
}
```

- [ ] **Step 5: Create index.html**

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/vite.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Codex Switcher</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

- [ ] **Step 6: Create tailwind.config.js**

```javascript
/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {},
  },
  plugins: [],
};
```

- [ ] **Step 7: Create src/main.tsx**

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import "./App.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

- [ ] **Step 8: Create src/App.css**

```css
@tailwind base;
@tailwind components;
@tailwind utilities;

@layer base {
  body {
    @apply bg-gray-50 text-gray-900;
  }
}
```

- [ ] **Step 9: Install frontend dependencies**

Run: `cd /Users/ibrar/Desktop/infinora.noworkspace/codex-account-switcher && pnpm install`
Expected: Dependencies installed successfully.

- [ ] **Step 10: Commit**

```bash
git add .
git commit -m "chore: scaffold frontend project"
```

---

## Phase 2: Rust Backend Core

### Task 2: Tauri Project Scaffolding

**Files:**
- Create: `src-tauri/Cargo.toml`
- Create: `src-tauri/tauri.conf.json`
- Create: `src-tauri/tauri.macos.conf.json`
- Create: `src-tauri/build.rs`
- Create: `src-tauri/capabilities/default.json`
- Create: `src-tauri/src/main.rs`

- [ ] **Step 1: Create src-tauri/Cargo.toml**

```toml
[package]
name = "codex-account-switcher"
version = "1.0.0"
description = "Intelligent multi-account manager for Codex CLI"
authors = ["user"]
edition = "2021"

[lib]
name = "codex_account_switcher_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-dialog = "2"
tauri-plugin-notification = "2"
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
dirs = "6"
base64 = "0.22"
sha2 = "0.10"
rand = "0.9"
thiserror = "2"
anyhow = "1"
urlencoding = "2"
futures = "0.3"
url = "2"
flate2 = "1"
chacha20poly1305 = "0.10"
pbkdf2 = "0.12"
toml = "0.8"
tracing = "0.1"
tracing-appender = "0.2"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
```

- [ ] **Step 2: Create src-tauri/tauri.conf.json**

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "Codex Switcher",
  "version": "1.0.0",
  "identifier": "com.user.codex-switcher",
  "build": {
    "beforeDevCommand": "pnpm dev",
    "devUrl": "http://localhost:1420",
    "beforeBuildCommand": "pnpm build",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "title": "Codex Switcher",
        "width": 900,
        "height": 700,
        "minWidth": 600,
        "minHeight": 500,
        "visible": false,
        "exitOnClose": false
      }
    ],
    "security": {
      "csp": null
    }
  },
  "bundle": {
    "active": true,
    "targets": "all",
    "icon": [
      "icons/32x32.png",
      "icons/128x128.png",
      "icons/128x128@2x.png",
      "icons/icon.icns",
      "icons/icon.ico"
    ]
  },
  "plugins": {}
}
```

- [ ] **Step 3: Create src-tauri/tauri.macos.conf.json**

```json
{
  "$schema": "../gen/schemas/macOS-config-schema.json",
  "bundle": {
    "macOS": {
      "frameworks": [],
      "minimumSystemVersion": "10.15",
      "signingIdentity": "-",
      "entitlements": null
    }
  }
}
```

- [ ] **Step 4: Create src-tauri/build.rs**

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 5: Create src-tauri/capabilities/default.json**

```json
{
  "$schema": "../gen/schemas/desktop-schema.json",
  "identifier": "default",
  "description": "Capability for the main window",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "notification:default",
    "opener:default"
  ]
}
```

- [ ] **Step 6: Create src-tauri/src/main.rs**

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    codex_account_switcher_lib::run()
}
```

- [ ] **Step 7: Commit**

```bash
git add src-tauri/
git commit -m "chore: scaffold tauri backend"
```

---

### Task 3: Core Types

**Files:**
- Create: `src-tauri/src/types.rs`

- [ ] **Step 1: Create src-tauri/src/types.rs**

```rust
use base64::Engine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountsStore {
    pub version: u32,
    pub accounts: Vec<StoredAccount>,
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub masked_account_ids: Vec<String>,
}

impl Default for AccountsStore {
    fn default() -> Self {
        Self {
            version: 1,
            accounts: Vec::new(),
            active_account_id: None,
            masked_account_ids: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAccount {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    #[serde(default)]
    pub subscription_expires_at: Option<DateTime<Utc>>,
    pub auth_mode: AuthMode,
    pub auth_data: AuthData,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl StoredAccount {
    pub fn new_api_key(name: String, api_key: String) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email: None,
            plan_type: None,
            subscription_expires_at: None,
            auth_mode: AuthMode::ApiKey,
            auth_data: AuthData::ApiKey { key: api_key },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }

    pub fn new_chatgpt(
        name: String,
        email: Option<String>,
        plan_type: Option<String>,
        subscription_expires_at: Option<DateTime<Utc>>,
        id_token: String,
        access_token: String,
        refresh_token: String,
        account_id: Option<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            email,
            plan_type,
            subscription_expires_at,
            auth_mode: AuthMode::ChatGPT,
            auth_data: AuthData::ChatGPT {
                id_token,
                access_token,
                refresh_token,
                account_id,
            },
            created_at: Utc::now(),
            last_used_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMode {
    ApiKey,
    ChatGPT,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthData {
    ApiKey { key: String },
    ChatGPT {
        id_token: String,
        access_token: String,
        refresh_token: String,
        account_id: Option<String>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ChatGptIdTokenClaims {
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub account_id: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
}

pub fn parse_chatgpt_id_token_claims(id_token: &str) -> ChatGptIdTokenClaims {
    let parts: Vec<&str> = id_token.split('.').collect();
    if parts.len() != 3 {
        return ChatGptIdTokenClaims::default();
    }

    let payload = match base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(parts[1]) {
        Ok(bytes) => bytes,
        Err(_) => return ChatGptIdTokenClaims::default(),
    };

    let json: serde_json::Value = match serde_json::from_slice(&payload) {
        Ok(value) => value,
        Err(_) => return ChatGptIdTokenClaims::default(),
    };

    let auth_claims = json.get("https://api.openai.com/auth");

    ChatGptIdTokenClaims {
        email: json.get("email").and_then(|v| v.as_str()).map(String::from),
        plan_type: auth_claims
            .and_then(|auth| auth.get("chatgpt_plan_type"))
            .and_then(|v| v.as_str())
            .map(String::from),
        account_id: auth_claims
            .and_then(|auth| auth.get("chatgpt_account_id"))
            .and_then(|v| v.as_str())
            .map(String::from),
        subscription_expires_at: auth_claims
            .and_then(|auth| auth.get("chatgpt_subscription_active_until"))
            .and_then(|v| v.as_str())
            .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
            .map(|value| value.with_timezone(&Utc)),
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub id: String,
    pub name: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub subscription_expires_at: Option<DateTime<Utc>>,
    pub auth_mode: AuthMode,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
}

impl AccountInfo {
    pub fn from_stored(account: &StoredAccount, active_id: Option<&str>) -> Self {
        let fallback_subscription = match &account.auth_data {
            AuthData::ChatGPT { id_token, .. } => {
                parse_chatgpt_id_token_claims(id_token).subscription_expires_at
            }
            AuthData::ApiKey { .. } => None,
        };

        Self {
            id: account.id.clone(),
            name: account.name.clone(),
            email: account.email.clone(),
            plan_type: account.plan_type.clone(),
            subscription_expires_at: account
                .subscription_expires_at
                .clone()
                .or(fallback_subscription),
            auth_mode: account.auth_mode,
            is_active: active_id == Some(&account.id),
            created_at: account.created_at,
            last_used_at: account.last_used_at,
        }
    }
}

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

impl UsageInfo {
    pub fn error(account_id: String, error: String) -> Self {
        Self {
            account_id,
            plan_type: None,
            primary_used_percent: None,
            primary_window_minutes: None,
            primary_resets_at: None,
            secondary_used_percent: None,
            secondary_window_minutes: None,
            secondary_resets_at: None,
            has_credits: None,
            unlimited_credits: None,
            credits_balance: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupSummary {
    pub total_accounts: usize,
    pub warmed_accounts: usize,
    pub failed_account_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportAccountsSummary {
    pub total_in_payload: usize,
    pub imported_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthLoginInfo {
    pub auth_url: String,
    pub callback_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitStatusPayload {
    pub plan_type: String,
    #[serde(default)]
    pub rate_limit: Option<RateLimitDetails>,
    #[serde(default)]
    pub credits: Option<CreditStatusDetails>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitDetails {
    pub primary_window: Option<RateLimitWindow>,
    pub secondary_window: Option<RateLimitWindow>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitWindow {
    pub used_percent: f64,
    pub limit_window_seconds: Option<i32>,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreditStatusDetails {
    pub has_credits: bool,
    pub unlimited: bool,
    #[serde(default)]
    pub balance: Option<String>,
}

// Settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub poll_interval_seconds: u64,
    pub notifications_enabled: bool,
    pub auto_switch_enabled: bool,
    pub global_cooldown_seconds: u64,
    pub last_auto_switch: Option<DateTime<Utc>>,
    pub account_settings: HashMap<String, AccountSettings>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            poll_interval_seconds: 60,
            notifications_enabled: true,
            auto_switch_enabled: true,
            global_cooldown_seconds: 300,
            last_auto_switch: None,
            account_settings: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSettings {
    pub switch_threshold: f64,
}

impl Default for AccountSettings {
    fn default() -> Self {
        Self {
            switch_threshold: 95.0,
        }
    }
}

// Switch log
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwitchLog {
    pub events: Vec<SwitchEvent>,
    #[serde(default = "default_max_events")]
    pub max_events: usize,
}

fn default_max_events() -> usize {
    20
}

impl Default for SwitchLog {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            max_events: 20,
        }
    }
}

// Monitor state
#[derive(Debug, Clone, Default)]
pub struct MonitorState {
    pub settings: AppSettings,
    pub latest_usages: Vec<UsageInfo>,
    pub is_monitor_running: bool,
}
```

- [ ] **Step 2: Commit**

```bash
git add src-tauri/src/types.rs
git commit -m "feat: add core types"
```

---

### Task 4: Auth Storage Module

**Files:**
- Create: `src-tauri/src/auth/mod.rs`
- Create: `src-tauri/src/auth/storage.rs`

- [ ] **Step 1: Create src-tauri/src/auth/mod.rs**

```rust
pub mod oauth_server;
pub mod storage;
pub mod switcher;
pub mod token_refresh;

pub use oauth_server::*;
pub use storage::*;
pub use switcher::*;
pub use token_refresh::*;
```

- [ ] **Step 2: Create src-tauri/src/auth/storage.rs**

```rust
use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::types::{AccountsStore, AuthData, StoredAccount};

pub fn get_config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not find home directory")?;
    Ok(home.join(".codex-switcher"))
}

pub fn get_accounts_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("accounts.json"))
}

pub fn load_accounts() -> Result<AccountsStore> {
    let path = get_accounts_file()?;

    if !path.exists() {
        return Ok(AccountsStore::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read accounts file: {}", path.display()))?;

    let store: AccountsStore = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse accounts file: {}", path.display()))?;

    Ok(store)
}

pub fn save_accounts(store: &AccountsStore) -> Result<()> {
    let path = get_accounts_file()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content =
        serde_json::to_string_pretty(store).context("Failed to serialize accounts store")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write accounts file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

pub fn add_account(account: StoredAccount) -> Result<StoredAccount> {
    let mut store = load_accounts()?;

    if store.accounts.iter().any(|a| a.name == account.name) {
        anyhow::bail!("An account with name '{}' already exists", account.name);
    }

    let account_clone = account.clone();
    store.accounts.push(account);

    if store.accounts.len() == 1 {
        store.active_account_id = Some(account_clone.id.clone());
    }

    save_accounts(&store)?;
    Ok(account_clone)
}

pub fn remove_account(account_id: &str) -> Result<()> {
    let mut store = load_accounts()?;

    let initial_len = store.accounts.len();
    store.accounts.retain(|a| a.id != account_id);

    if store.accounts.len() == initial_len {
        anyhow::bail!("Account not found: {account_id}");
    }

    if store.active_account_id.as_deref() == Some(account_id) {
        store.active_account_id = store.accounts.first().map(|a| a.id.clone());
    }

    save_accounts(&store)?;
    Ok(())
}

pub fn set_active_account(account_id: &str) -> Result<()> {
    let mut store = load_accounts()?;

    if !store.accounts.iter().any(|a| a.id == account_id) {
        anyhow::bail!("Account not found: {account_id}");
    }

    store.active_account_id = Some(account_id.to_string());
    save_accounts(&store)?;
    Ok(())
}

pub fn get_account(account_id: &str) -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    Ok(store.accounts.into_iter().find(|a| a.id == account_id))
}

pub fn get_active_account() -> Result<Option<StoredAccount>> {
    let store = load_accounts()?;
    let active_id = match &store.active_account_id {
        Some(id) => id,
        None => return Ok(None),
    };
    Ok(store.accounts.into_iter().find(|a| a.id == *active_id))
}

pub fn touch_account(account_id: &str) -> Result<()> {
    let mut store = load_accounts()?;

    if let Some(account) = store.accounts.iter_mut().find(|a| a.id == account_id) {
        account.last_used_at = Some(chrono::Utc::now());
        save_accounts(&store)?;
    }

    Ok(())
}

pub fn update_account_metadata(
    account_id: &str,
    name: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    subscription_expires_at: Option<Option<DateTime<Utc>>>,
) -> Result<StoredAccount> {
    let mut store = load_accounts()?;

    if let Some(ref new_name) = name {
        if store
            .accounts
            .iter()
            .any(|a| a.id != account_id && a.name == *new_name)
        {
            anyhow::bail!("An account with name '{new_name}' already exists");
        }
    }

    let account = store
        .accounts
        .iter_mut()
        .find(|a| a.id == account_id)
        .context("Account not found")?;

    if let Some(new_name) = name {
        account.name = new_name;
    }

    if email.is_some() {
        account.email = email;
    }

    if plan_type.is_some() {
        account.plan_type = plan_type;
    }

    if let Some(subscription_expires_at) = subscription_expires_at {
        account.subscription_expires_at = subscription_expires_at;
    }

    let updated = account.clone();
    save_accounts(&store)?;
    Ok(updated)
}

pub fn update_account_chatgpt_tokens(
    account_id: &str,
    id_token: String,
    access_token: String,
    refresh_token: String,
    chatgpt_account_id: Option<String>,
    email: Option<String>,
    plan_type: Option<String>,
    subscription_expires_at: Option<DateTime<Utc>>,
) -> Result<StoredAccount> {
    let mut store = load_accounts()?;

    let account = store
        .accounts
        .iter_mut()
        .find(|a| a.id == account_id)
        .context("Account not found")?;

    match &mut account.auth_data {
        AuthData::ChatGPT {
            id_token: stored_id_token,
            access_token: stored_access_token,
            refresh_token: stored_refresh_token,
            account_id: stored_account_id,
        } => {
            *stored_id_token = id_token;
            *stored_access_token = access_token;
            *stored_refresh_token = refresh_token;
            if let Some(new_account_id) = chatgpt_account_id {
                *stored_account_id = Some(new_account_id);
            }
        }
        AuthData::ApiKey { .. } => {
            anyhow::bail!("Cannot update OAuth tokens for an API key account");
        }
    }

    if let Some(new_email) = email {
        account.email = Some(new_email);
    }

    if let Some(new_plan_type) = plan_type {
        account.plan_type = Some(new_plan_type);
    }

    if let Some(subscription_expires_at) = subscription_expires_at {
        account.subscription_expires_at = Some(subscription_expires_at);
    }

    let updated = account.clone();
    save_accounts(&store)?;
    Ok(updated)
}

pub fn get_masked_account_ids() -> Result<Vec<String>> {
    let store = load_accounts()?;
    Ok(store.masked_account_ids.clone())
}

pub fn set_masked_account_ids(ids: Vec<String>) -> Result<()> {
    let mut store = load_accounts()?;
    store.masked_account_ids = ids;
    save_accounts(&store)?;
    Ok(())
}
```

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/auth/
git commit -m "feat: add auth storage module"
```

---

Given the massive scope, I will now proceed with sub-agent driven development to implement the remaining modules. Let me start dispatching sub-agents to build out the complete application.