//! Session manager - snapshots per-account auth.json files

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::auth::storage::{get_account, get_config_dir};
use crate::auth::switcher::{get_codex_auth_file, get_codex_home};
use crate::types::{AuthData, AuthDotJson, StoredAccount, TokenData};
use base64::Engine;

pub fn get_account_snapshot_dir(account_id: &str) -> Result<PathBuf> {
    Ok(get_config_dir()?.join("accounts").join(account_id))
}

pub fn snapshot_account(account_id: &str, auth_json: &AuthDotJson) -> Result<()> {
    let dir = get_account_snapshot_dir(account_id)?;
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create snapshot dir: {}", dir.display()))?;

    let path = dir.join("auth.json");
    let content =
        serde_json::to_string_pretty(auth_json).context("Failed to serialize auth snapshot")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write auth snapshot: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
        let dir_perms = fs::Permissions::from_mode(0o700);
        fs::set_permissions(&dir, dir_perms)?;
    }

    Ok(())
}

pub fn restore_account(account_id: &str) -> Result<AuthDotJson> {
    let path = get_account_snapshot_dir(account_id)?.join("auth.json");

    if !path.exists() {
        // Fallback: try to create snapshot from account data
        if let Ok(Some(account)) = get_account(account_id) {
            tracing::info!(
                "Creating missing snapshot for account {} from stored auth_data",
                account_id
            );
            snapshot_account_from_data(&account)?;
            return restore_account(account_id);
        }
        anyhow::bail!("No auth.json snapshot found for account {}", account_id);
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read auth snapshot: {}", path.display()))?;

    let auth: AuthDotJson = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse auth snapshot: {}", path.display()))?;

    Ok(auth)
}

pub fn snapshot_account_from_data(account: &StoredAccount) -> Result<()> {
    let auth_json = match &account.auth_data {
        AuthData::ApiKey { key } => AuthDotJson {
            openai_api_key: Some(key.clone()),
            tokens: None,
            last_refresh: None,
        },
        AuthData::ChatGPT {
            id_token,
            access_token,
            refresh_token,
            account_id,
        } => AuthDotJson {
            openai_api_key: None,
            tokens: Some(TokenData {
                id_token: id_token.clone(),
                access_token: access_token.clone(),
                refresh_token: refresh_token.clone(),
                account_id: account_id.clone(),
            }),
            last_refresh: Some(chrono::Utc::now()),
        },
    };

    snapshot_account(&account.id, &auth_json)
        .with_context(|| format!("Failed to snapshot account {}", account.id))?;

    tracing::info!("Created auth.json snapshot for account {}", account.id);
    Ok(())
}

pub fn swap_active_auth(account_id: &str) -> Result<()> {
    let snapshot = restore_account(account_id)?;
    let auth_path = get_codex_auth_file()?;
    let codex_home = get_codex_home()?;

    fs::create_dir_all(&codex_home)
        .with_context(|| format!("Failed to create codex home: {}", codex_home.display()))?;

    let content =
        serde_json::to_string_pretty(&snapshot).context("Failed to serialize auth.json")?;

    // Atomic write using temp file + rename
    let tmp_path = auth_path.with_extension("tmp");
    fs::write(&tmp_path, content)
        .with_context(|| format!("Failed to write temp auth.json: {}", tmp_path.display()))?;
    fs::rename(&tmp_path, &auth_path)
        .with_context(|| format!("Failed to rename auth.json: {}", auth_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&auth_path, perms)?;
    }

    tracing::info!("Swapped active auth.json to account {}", account_id);
    Ok(())
}

pub fn ensure_file_auth_mode() -> Result<bool> {
    let codex_home = get_codex_home()?;
    let config_path = codex_home.join("config.toml");

    let mut config = if config_path.exists() {
        let content = fs::read_to_string(&config_path)
            .with_context(|| format!("Failed to read config.toml: {}", config_path.display()))?;
        content
            .parse::<toml::Table>()
            .unwrap_or_else(|_| toml::Table::new())
    } else {
        toml::Table::new()
    };

    let current = config
        .get("cli_auth_credentials_store")
        .and_then(|v| v.as_str());

    if current == Some("file") {
        return Ok(false);
    }

    config.insert(
        "cli_auth_credentials_store".to_string(),
        toml::Value::String("file".to_string()),
    );

    fs::create_dir_all(&codex_home)
        .with_context(|| format!("Failed to create codex home: {}", codex_home.display()))?;

    let content = toml::to_string_pretty(&config).context("Failed to serialize config.toml")?;

    fs::write(&config_path, content)
        .with_context(|| format!("Failed to write config.toml: {}", config_path.display()))?;

    tracing::info!("Patched config.toml to use file auth mode");
    Ok(true)
}

pub fn is_file_auth_mode_required() -> Result<bool> {
    let codex_home = get_codex_home()?;
    let config_path = codex_home.join("config.toml");

    if !config_path.exists() {
        return Ok(true);
    }

    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("Failed to read config.toml: {}", config_path.display()))?;

    let config = content
        .parse::<toml::Table>()
        .unwrap_or_else(|_| toml::Table::new());

    let current = config
        .get("cli_auth_credentials_store")
        .and_then(|v| v.as_str());

    Ok(current != Some("file"))
}

pub fn is_token_expired(auth_json: &AuthDotJson) -> bool {
    match &auth_json.tokens {
        Some(tokens) => match parse_jwt_exp(&tokens.access_token) {
            Some(exp) => exp <= chrono::Utc::now().timestamp() + 60,
            None => false,
        },
        None => false,
    }
}

fn parse_jwt_exp(token: &str) -> Option<i64> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }

    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let json: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    json.get("exp").and_then(|v| v.as_i64())
}
