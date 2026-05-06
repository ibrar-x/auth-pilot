//! App settings management

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::auth::storage::get_config_dir;
use crate::types::{AccountSettings, AppSettings};

pub fn get_settings_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("settings.json"))
}

pub fn load_settings() -> Result<AppSettings> {
    let path = get_settings_file()?;

    if !path.exists() {
        return Ok(AppSettings::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read settings file: {}", path.display()))?;

    let settings: AppSettings = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse settings file: {}", path.display()))?;

    Ok(settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    let path = get_settings_file()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(settings).context("Failed to serialize settings")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write settings file: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

pub fn get_account_settings(account_id: &str) -> Result<AccountSettings> {
    let settings = load_settings()?;
    Ok(settings
        .account_settings
        .get(account_id)
        .cloned()
        .unwrap_or_default())
}

pub fn update_account_settings(account_id: &str, account_settings: AccountSettings) -> Result<()> {
    let mut settings = load_settings()?;
    settings
        .account_settings
        .insert(account_id.to_string(), account_settings);
    save_settings(&settings)
}

pub fn update_last_auto_switch(timestamp: DateTime<Utc>) -> Result<()> {
    let mut settings = load_settings()?;
    settings.last_auto_switch = Some(timestamp);
    save_settings(&settings)
}
