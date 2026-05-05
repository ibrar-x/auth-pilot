//! Switch log management

use std::fs;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::auth::storage::get_config_dir;
use crate::types::{SwitchLog, SwitchEvent};

pub fn get_switch_log_file() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("switch_log.json"))
}

pub fn load_switch_log() -> Result<SwitchLog> {
    let path = get_switch_log_file()?;

    if !path.exists() {
        return Ok(SwitchLog::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read switch log: {}", path.display()))?;

    let log: SwitchLog = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse switch log: {}", path.display()))?;

    Ok(log)
}

pub fn save_switch_log(log: &SwitchLog) -> Result<()> {
    let path = get_switch_log_file()?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create config directory: {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(log)
        .context("Failed to serialize switch log")?;

    fs::write(&path, content)
        .with_context(|| format!("Failed to write switch log: {}", path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o600);
        fs::set_permissions(&path, perms)?;
    }

    Ok(())
}

pub fn append_switch_event(event: SwitchEvent) -> Result<()> {
    let mut log = load_switch_log()?;
    log.events.push(event);

    while log.events.len() > log.max_events {
        log.events.remove(0);
    }

    save_switch_log(&log)
}

pub fn get_switch_events() -> Result<Vec<SwitchEvent>> {
    let log = load_switch_log()?;
    Ok(log.events)
}
