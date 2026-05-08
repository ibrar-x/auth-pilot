//! Managed Codex CLI wrapper support.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

pub const WRAPPER_MARKER: &str = "AuthPilot CLI wrapper";
const SHELL_MARKER_START: &str = "# AuthPilot codex proxy - managed automatically";
const SHELL_MARKER_END: &str = "# End AuthPilot codex proxy";
const DEFAULT_WRAPPER_PATH: &str = "/usr/local/bin/codex";

#[derive(Debug, Clone, Serialize)]
pub struct CliWrapperStatus {
    pub wrapper_path: String,
    pub backup_path: String,
    pub installed: bool,
    pub binary_installed: bool,
    pub shell_installed: bool,
    pub backup_exists: bool,
    pub real_codex_path: Option<String>,
}

pub fn install(proxy_port: u16) -> Result<CliWrapperStatus> {
    let wrapper_path = default_wrapper_path();
    let real_codex = find_real_codex_binary(Some(&wrapper_path))?;
    if let Err(err) = install_at_path(proxy_port, &wrapper_path, &real_codex) {
        tracing::warn!(
            "[cli] binary wrapper install failed ({}), falling back to shell snippet",
            err
        );
        install_shell_function(proxy_port, &real_codex)?;
    }
    status_with_real_codex(Some(real_codex))
}

pub fn remove() -> Result<CliWrapperStatus> {
    let wrapper_path = default_wrapper_path();
    let backup_path = backup_path_for(&wrapper_path);

    if wrapper_is_installed(&wrapper_path) {
        if backup_path.exists() {
            fs::copy(&backup_path, &wrapper_path).with_context(|| {
                format!(
                    "failed to restore Codex CLI backup from {}",
                    backup_path.display()
                )
            })?;
            fs::remove_file(&backup_path).with_context(|| {
                format!(
                    "failed to remove Codex CLI backup {}",
                    backup_path.display()
                )
            })?;
        } else {
            fs::remove_file(&wrapper_path).with_context(|| {
                format!(
                    "failed to remove AuthPilot CLI wrapper {}",
                    wrapper_path.display()
                )
            })?;
        }
    }
    remove_shell_function()?;

    status()
}

pub fn status() -> Result<CliWrapperStatus> {
    status_with_real_codex(find_real_codex_binary(Some(&default_wrapper_path())).ok())
}

fn status_with_real_codex(real_codex: Option<PathBuf>) -> Result<CliWrapperStatus> {
    let wrapper_path = default_wrapper_path();
    let backup_path = backup_path_for(&wrapper_path);
    let binary_installed = wrapper_is_installed(&wrapper_path);
    let shell_installed = shell_function_is_installed();
    Ok(CliWrapperStatus {
        installed: binary_installed || shell_installed,
        binary_installed,
        shell_installed,
        backup_exists: backup_path.exists(),
        wrapper_path: wrapper_path.display().to_string(),
        backup_path: backup_path.display().to_string(),
        real_codex_path: real_codex.map(|path| path.display().to_string()),
    })
}

fn install_at_path(proxy_port: u16, wrapper_path: &Path, real_codex: &Path) -> Result<()> {
    if let Some(parent) = wrapper_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create wrapper directory {}", parent.display()))?;
    }

    if wrapper_path.exists() && !wrapper_is_installed(wrapper_path) {
        let backup_path = backup_path_for(wrapper_path);
        if !backup_path.exists() {
            fs::copy(wrapper_path, &backup_path).with_context(|| {
                format!(
                    "failed to back up existing Codex CLI from {} to {}",
                    wrapper_path.display(),
                    backup_path.display()
                )
            })?;
        }
    }

    fs::write(wrapper_path, wrapper_script(proxy_port, real_codex)).with_context(|| {
        format!(
            "failed to write AuthPilot CLI wrapper to {}",
            wrapper_path.display()
        )
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(wrapper_path, fs::Permissions::from_mode(0o755)).with_context(
            || {
                format!(
                    "failed to make wrapper executable {}",
                    wrapper_path.display()
                )
            },
        )?;
    }

    tracing::info!("[cli] wrapper installed at {}", wrapper_path.display());
    Ok(())
}

fn wrapper_script(proxy_port: u16, real_codex: &Path) -> String {
    format!(
        r#"#!/bin/sh
# {WRAPPER_MARKER} - managed automatically.
# Safe to delete: AuthPilot can reinstall it.

AUTHPILOT_PORT="{proxy_port}"

if lsof -iTCP:"$AUTHPILOT_PORT" -sTCP:LISTEN -t >/dev/null 2>&1; then
    export OPENAI_BASE_URL="http://127.0.0.1:$AUTHPILOT_PORT"
    export HTTP_PROXY="http://127.0.0.1:$AUTHPILOT_PORT"
    export HTTPS_PROXY="http://127.0.0.1:$AUTHPILOT_PORT"
fi

exec "{real_codex}" "$@"
"#,
        real_codex = shell_double_quoted_path(real_codex)
    )
}

fn install_shell_function(proxy_port: u16, real_codex: &Path) -> Result<()> {
    let snippet = shell_function_snippet(proxy_port, real_codex);
    let mut installed_any = false;
    for rc in shell_rc_files(true) {
        let existing = fs::read_to_string(&rc).unwrap_or_default();
        let next = replace_or_append_shell_snippet(&existing, &snippet);
        fs::write(&rc, next).with_context(|| {
            format!(
                "failed to write AuthPilot shell wrapper to {}",
                rc.display()
            )
        })?;
        installed_any = true;
    }

    if !installed_any {
        anyhow::bail!("no writable shell rc file found for AuthPilot CLI wrapper fallback");
    }

    tracing::info!("[cli] shell wrapper installed");
    Ok(())
}

fn remove_shell_function() -> Result<()> {
    for rc in shell_rc_files(false) {
        let Ok(existing) = fs::read_to_string(&rc) else {
            continue;
        };
        let cleaned = remove_shell_snippet(&existing);
        if cleaned != existing {
            fs::write(&rc, cleaned).with_context(|| {
                format!(
                    "failed to remove AuthPilot shell wrapper from {}",
                    rc.display()
                )
            })?;
        }
    }
    Ok(())
}

fn shell_function_is_installed() -> bool {
    shell_rc_files(false).iter().any(|rc| {
        fs::read_to_string(rc)
            .map(|contents| contents.contains(SHELL_MARKER_START))
            .unwrap_or(false)
    })
}

fn shell_function_snippet(proxy_port: u16, real_codex: &Path) -> String {
    format!(
        r#"{SHELL_MARKER_START}
codex() {{
    if lsof -iTCP:{proxy_port} -sTCP:LISTEN -t >/dev/null 2>&1; then
        OPENAI_BASE_URL="http://127.0.0.1:{proxy_port}" \
        HTTP_PROXY="http://127.0.0.1:{proxy_port}" \
        HTTPS_PROXY="http://127.0.0.1:{proxy_port}" \
        "{real_codex}" "$@"
    else
        "{real_codex}" "$@"
    fi
}}
{SHELL_MARKER_END}
"#,
        real_codex = shell_double_quoted_path(real_codex)
    )
}

fn replace_or_append_shell_snippet(existing: &str, snippet: &str) -> String {
    let cleaned = remove_shell_snippet(existing);
    let separator = if cleaned.ends_with('\n') || cleaned.is_empty() {
        ""
    } else {
        "\n"
    };
    format!("{cleaned}{separator}{snippet}")
}

fn remove_shell_snippet(existing: &str) -> String {
    let Some(start) = existing.find(SHELL_MARKER_START) else {
        return existing.to_string();
    };
    let Some(relative_end) = existing[start..].find(SHELL_MARKER_END) else {
        return existing.to_string();
    };
    let end = start + relative_end + SHELL_MARKER_END.len();
    let mut next = format!("{}{}", &existing[..start], &existing[end..]);
    while next.contains("\n\n\n") {
        next = next.replace("\n\n\n", "\n\n");
    }
    next
}

fn shell_rc_files(create_default: bool) -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let candidates = [".zshrc", ".bashrc", ".bash_profile"];
    let mut files: Vec<PathBuf> = candidates
        .iter()
        .map(|name| home.join(name))
        .filter(|path| path.exists())
        .collect();

    if files.is_empty() && create_default {
        let default = home.join(".zshrc");
        let _ = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&default);
        files.push(default);
    }

    files
}

fn find_real_codex_binary(exclude: Option<&Path>) -> Result<PathBuf> {
    for candidate in [
        "/opt/homebrew/bin/codex",
        "/usr/local/bin/codex",
        "/usr/bin/codex",
    ] {
        let path = PathBuf::from(candidate);
        if usable_real_codex_path(&path, exclude) {
            return Ok(path);
        }
    }

    let output = Command::new("which")
        .arg("codex")
        .output()
        .context("failed to locate Codex CLI with which")?;
    if output.status.success() {
        let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
        if usable_real_codex_path(&path, exclude) {
            return Ok(path);
        }
    }

    anyhow::bail!("Codex CLI not found. Install with: npm install -g @openai/codex")
}

fn usable_real_codex_path(path: &Path, exclude: Option<&Path>) -> bool {
    path.exists()
        && exclude != Some(path)
        && !wrapper_is_installed(path)
        && path.metadata().map(|meta| meta.is_file()).unwrap_or(false)
}

fn wrapper_is_installed(path: &Path) -> bool {
    fs::read_to_string(path)
        .map(|contents| contents.contains(WRAPPER_MARKER))
        .unwrap_or(false)
}

fn backup_path_for(wrapper_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.authpilot-backup", wrapper_path.display()))
}

fn default_wrapper_path() -> PathBuf {
    PathBuf::from(DEFAULT_WRAPPER_PATH)
}

fn shell_double_quoted_path(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_wrapper_contains_marker_and_proxy_environment() {
        let script = wrapper_script(18080, Path::new("/opt/homebrew/bin/codex"));

        assert!(script.contains(WRAPPER_MARKER));
        assert!(script.contains("AUTHPILOT_PORT=\"18080\""));
        assert!(script.contains("OPENAI_BASE_URL=\"http://127.0.0.1:$AUTHPILOT_PORT\""));
        assert!(script.contains("exec \"/opt/homebrew/bin/codex\" \"$@\""));
    }

    #[test]
    fn generated_wrapper_escapes_double_quotes_in_real_path() {
        let script = wrapper_script(18080, Path::new("/tmp/codex \"test\"/codex"));
        assert!(script.contains("exec \"/tmp/codex \\\"test\\\"/codex\" \"$@\""));
    }

    #[test]
    fn backup_path_uses_authpilot_suffix_without_changing_extension() {
        assert_eq!(
            backup_path_for(Path::new("/usr/local/bin/codex")),
            PathBuf::from("/usr/local/bin/codex.authpilot-backup")
        );
    }

    #[test]
    fn shell_snippet_can_be_replaced_and_removed() {
        let first = shell_function_snippet(18080, Path::new("/opt/homebrew/bin/codex"));
        let second = shell_function_snippet(18081, Path::new("/opt/homebrew/bin/codex"));
        let existing = format!("export PATH=/test\n{first}\n# user config\n");

        let replaced = replace_or_append_shell_snippet(&existing, &second);

        assert!(replaced.contains("export PATH=/test"));
        assert!(replaced.contains("127.0.0.1:18081"));
        assert!(!replaced.contains("127.0.0.1:18080"));
        assert!(replaced.contains("# user config"));

        let removed = remove_shell_snippet(&replaced);
        assert!(!removed.contains(SHELL_MARKER_START));
        assert!(removed.contains("# user config"));
    }
}
