use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const HOOKS_MARKER_START: &str = "# AuthPilot session recovery hooks";
const HOOKS_MARKER_END: &str = "# End AuthPilot hooks";

pub fn install_hook_script() -> Result<PathBuf> {
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("Unable to find repository root")?
        .join("resources")
        .join("record-session.sh");
    let home = dirs::home_dir().context("Unable to find home directory")?;
    let target_dir = home.join(".authpilot").join("codex-hooks");
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("Failed to create {}", target_dir.display()))?;
    let target = target_dir.join("record-session.sh");
    fs::copy(&source, &target).with_context(|| {
        format!(
            "Failed to install hook script from {} to {}",
            source.display(),
            target.display()
        )
    })?;
    let mut permissions = fs::metadata(&target)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&target, permissions)?;
    register_hooks_in_codex_config(&codex_config_path()?, &target)?;
    Ok(target)
}

fn codex_config_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Unable to find home directory")?;
    Ok(home.join(".codex").join("config.toml"))
}

pub fn register_hooks_in_codex_config(config_path: &Path, script_path: &Path) -> Result<()> {
    let existing = fs::read_to_string(config_path).unwrap_or_default();
    let cleaned = remove_marker_block(&existing);
    let block = hook_block(script_path);
    let mut next = cleaned.trim_end().to_string();
    if !next.is_empty() {
        next.push_str("\n\n");
    }
    next.push_str(&block);
    next.push('\n');

    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    fs::write(config_path, next)
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

pub fn remove_hooks_from_codex_config(config_path: &Path) -> Result<()> {
    let existing = match fs::read_to_string(config_path) {
        Ok(value) => value,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => {
            return Err(err).with_context(|| format!("Failed to read {}", config_path.display()))
        }
    };
    let next = remove_marker_block(&existing);
    fs::write(config_path, next.trim_end_matches('\n').to_string() + "\n")
        .with_context(|| format!("Failed to write {}", config_path.display()))?;
    Ok(())
}

fn hook_block(script_path: &Path) -> String {
    let script = script_path
        .display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!(
        r#"{HOOKS_MARKER_START}
[hooks]
session_started    = ["{script}"]
user_prompt_submit = ["{script}"]
stop               = ["{script}"]
{HOOKS_MARKER_END}"#
    )
}

fn remove_marker_block(input: &str) -> String {
    let mut output = String::new();
    let mut skipping = false;

    for line in input.lines() {
        if line.trim() == HOOKS_MARKER_START {
            skipping = true;
            continue;
        }
        if skipping {
            if line.trim() == HOOKS_MARKER_END {
                skipping = false;
            }
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }

    output
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("authpilot-hooks-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn register_hooks_creates_config_when_missing() {
        let dir = temp_dir();
        let config_path = dir.join("config.toml");
        let script_path = dir.join("record-session.sh");

        register_hooks_in_codex_config(&config_path, &script_path).unwrap();

        let config = fs::read_to_string(config_path).unwrap();
        assert!(config.contains(HOOKS_MARKER_START));
        assert!(config.contains(script_path.to_str().unwrap()));
    }

    #[test]
    fn register_hooks_is_idempotent() {
        let dir = temp_dir();
        let config_path = dir.join("config.toml");
        let script_path = dir.join("record-session.sh");

        register_hooks_in_codex_config(&config_path, &script_path).unwrap();
        register_hooks_in_codex_config(&config_path, &script_path).unwrap();

        let config = fs::read_to_string(config_path).unwrap();
        assert_eq!(config.matches(HOOKS_MARKER_START).count(), 1);
        assert_eq!(config.matches(HOOKS_MARKER_END).count(), 1);
        assert_eq!(config.matches("[hooks]").count(), 1);
    }

    #[test]
    fn remove_hooks_preserves_other_config() {
        let dir = temp_dir();
        let config_path = dir.join("config.toml");
        let script_path = dir.join("record-session.sh");
        fs::write(&config_path, "model = \"gpt-5\"\n").unwrap();
        register_hooks_in_codex_config(&config_path, &script_path).unwrap();

        remove_hooks_from_codex_config(&config_path).unwrap();

        let config = fs::read_to_string(config_path).unwrap();
        assert!(config.contains("model = \"gpt-5\""));
        assert!(!config.contains(HOOKS_MARKER_START));
        assert!(!config.contains(script_path.to_str().unwrap()));
    }
}
