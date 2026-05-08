//! macOS launchd login-agent support for AuthPilot background startup.

use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::process::Command;

use anyhow::{Context, Result};
use serde::Serialize;

pub const LOGIN_AGENT_LABEL: &str = "com.authpilot.agent";
const PLIST_FILE_NAME: &str = "com.authpilot.agent.plist";
const CACHE_DIR_NAME: &str = "AuthPilot";
const STDOUT_LOG_FILE: &str = "launchd.stdout.log";
const STDERR_LOG_FILE: &str = "launchd.stderr.log";

#[derive(Debug, Clone, Serialize)]
pub struct LaunchdStatus {
    pub supported: bool,
    pub installed: bool,
    pub loaded: bool,
    pub plist_path: String,
    pub executable_path: Option<String>,
    pub stdout_log_path: String,
    pub stderr_log_path: String,
}

#[derive(Debug, Clone)]
pub struct LaunchdLogPaths {
    pub stdout: PathBuf,
    pub stderr: PathBuf,
}

pub fn install() -> Result<LaunchdStatus> {
    install_with_launchctl(true)
}

pub fn remove() -> Result<LaunchdStatus> {
    remove_with_launchctl(true)
}

pub fn status() -> Result<LaunchdStatus> {
    status_with_launchctl(true)
}

#[cfg(target_os = "macos")]
fn install_with_launchctl(run_launchctl: bool) -> Result<LaunchdStatus> {
    let plist_path = login_agent_path()?;
    let executable_path =
        std::env::current_exe().context("failed to resolve current executable path")?;
    let log_paths = log_paths()?;

    if let Some(parent) = plist_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    if let Some(parent) = log_paths.stdout.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    fs::write(
        &plist_path,
        build_login_agent_plist(&executable_path, &log_paths),
    )
    .with_context(|| format!("failed to write {}", plist_path.display()))?;

    if run_launchctl {
        let _ = launchctl_bootout(&plist_path);
        launchctl_bootstrap(&plist_path)?;
    }

    status_with_launchctl(run_launchctl)
}

#[cfg(not(target_os = "macos"))]
fn install_with_launchctl(_run_launchctl: bool) -> Result<LaunchdStatus> {
    status_with_launchctl(false)
}

#[cfg(target_os = "macos")]
fn remove_with_launchctl(run_launchctl: bool) -> Result<LaunchdStatus> {
    let plist_path = login_agent_path()?;

    if run_launchctl && plist_path.exists() {
        let _ = launchctl_bootout(&plist_path);
    }

    if plist_path.exists() {
        fs::remove_file(&plist_path)
            .with_context(|| format!("failed to remove {}", plist_path.display()))?;
    }

    status_with_launchctl(run_launchctl)
}

#[cfg(not(target_os = "macos"))]
fn remove_with_launchctl(_run_launchctl: bool) -> Result<LaunchdStatus> {
    status_with_launchctl(false)
}

#[cfg(target_os = "macos")]
fn status_with_launchctl(run_launchctl: bool) -> Result<LaunchdStatus> {
    let plist_path = login_agent_path()?;
    let log_paths = log_paths()?;
    let installed = plist_path.exists();
    let loaded = run_launchctl && launchctl_print().is_ok();

    Ok(LaunchdStatus {
        supported: true,
        installed,
        loaded,
        plist_path: plist_path.display().to_string(),
        executable_path: if installed {
            Some(std::env::current_exe()?.display().to_string())
        } else {
            None
        },
        stdout_log_path: log_paths.stdout.display().to_string(),
        stderr_log_path: log_paths.stderr.display().to_string(),
    })
}

#[cfg(not(target_os = "macos"))]
fn status_with_launchctl(_run_launchctl: bool) -> Result<LaunchdStatus> {
    let plist_path = dirs::home_dir()
        .map(|home| login_agent_path_in(&home))
        .unwrap_or_else(|| PathBuf::from(PLIST_FILE_NAME));
    let log_paths = dirs::cache_dir()
        .map(|cache_dir| log_paths_in(&cache_dir))
        .unwrap_or_else(|| log_paths_in(Path::new(".")));

    Ok(LaunchdStatus {
        supported: false,
        installed: false,
        loaded: false,
        plist_path: plist_path.display().to_string(),
        executable_path: None,
        stdout_log_path: log_paths.stdout.display().to_string(),
        stderr_log_path: log_paths.stderr.display().to_string(),
    })
}

#[cfg(target_os = "macos")]
fn login_agent_path() -> Result<PathBuf> {
    let home_dir = dirs::home_dir().context("failed to resolve home directory")?;
    Ok(login_agent_path_in(&home_dir))
}

fn login_agent_path_in(home_dir: &Path) -> PathBuf {
    home_dir
        .join("Library")
        .join("LaunchAgents")
        .join(PLIST_FILE_NAME)
}

fn log_paths() -> Result<LaunchdLogPaths> {
    let cache_dir = dirs::cache_dir().context("failed to resolve cache directory")?;
    Ok(log_paths_in(&cache_dir))
}

fn log_paths_in(cache_dir: &Path) -> LaunchdLogPaths {
    let authpilot_cache_dir = cache_dir.join(CACHE_DIR_NAME);
    LaunchdLogPaths {
        stdout: authpilot_cache_dir.join(STDOUT_LOG_FILE),
        stderr: authpilot_cache_dir.join(STDERR_LOG_FILE),
    }
}

fn build_login_agent_plist(executable_path: &Path, log_paths: &LaunchdLogPaths) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{executable}</string>
        <string>--background</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <key>Crashed</key>
        <true/>
    </dict>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>{stdout}</string>
    <key>StandardErrorPath</key>
    <string>{stderr}</string>
</dict>
</plist>
"#,
        label = plist_escape(LOGIN_AGENT_LABEL),
        executable = plist_escape(&executable_path.display().to_string()),
        stdout = plist_escape(&log_paths.stdout.display().to_string()),
        stderr = plist_escape(&log_paths.stderr.display().to_string())
    )
}

fn plist_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(target_os = "macos")]
fn launchctl_bootstrap(plist_path: &Path) -> Result<()> {
    run_launchctl([
        "bootstrap",
        &gui_domain()?,
        &plist_path.display().to_string(),
    ])
}

#[cfg(target_os = "macos")]
fn launchctl_bootout(plist_path: &Path) -> Result<()> {
    run_launchctl(["bootout", &gui_domain()?, &plist_path.display().to_string()])
}

#[cfg(target_os = "macos")]
fn launchctl_print() -> Result<()> {
    run_launchctl(["print", &format!("{}/{}", gui_domain()?, LOGIN_AGENT_LABEL)])
}

#[cfg(target_os = "macos")]
fn run_launchctl<const N: usize>(args: [&str; N]) -> Result<()> {
    let output = Command::new("launchctl")
        .args(args)
        .output()
        .context("failed to run launchctl")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    anyhow::bail!("launchctl failed: {}", stderr.trim());
}

#[cfg(target_os = "macos")]
fn gui_domain() -> Result<String> {
    let output = Command::new("id")
        .arg("-u")
        .output()
        .context("failed to resolve current user id")?;

    if !output.status.success() {
        anyhow::bail!(
            "id -u failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(format!(
        "gui/{}",
        String::from_utf8_lossy(&output.stdout).trim()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn login_agent_path_uses_user_launch_agents_directory() {
        let path = login_agent_path_in(Path::new("/Users/tester"));

        assert_eq!(
            path,
            PathBuf::from("/Users/tester/Library/LaunchAgents/com.authpilot.agent.plist")
        );
    }

    #[test]
    fn cache_log_paths_use_authpilot_cache_directory() {
        let paths = log_paths_in(Path::new("/Users/tester/Library/Caches"));

        assert_eq!(
            paths.stdout,
            PathBuf::from("/Users/tester/Library/Caches/AuthPilot/launchd.stdout.log")
        );
        assert_eq!(
            paths.stderr,
            PathBuf::from("/Users/tester/Library/Caches/AuthPilot/launchd.stderr.log")
        );
    }

    #[test]
    fn plist_launches_current_executable_in_background_with_crash_keepalive() {
        let plist = build_login_agent_plist(
            Path::new("/Applications/AuthPilot.app/Contents/MacOS/AuthPilot"),
            &log_paths_in(Path::new("/Users/tester/Library/Caches")),
        );

        assert!(plist.contains("<string>com.authpilot.agent</string>"));
        assert!(
            plist.contains("<string>/Applications/AuthPilot.app/Contents/MacOS/AuthPilot</string>")
        );
        assert!(plist.contains("<string>--background</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>\n    <true/>"));
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>Crashed</key>\n        <true/>"));
        assert!(plist.contains("<key>ThrottleInterval</key>\n    <integer>10</integer>"));
        assert!(plist.contains("<key>StandardOutPath</key>"));
        assert!(plist.contains(
            "<string>/Users/tester/Library/Caches/AuthPilot/launchd.stdout.log</string>"
        ));
        assert!(plist.contains("<key>StandardErrorPath</key>"));
        assert!(plist.contains(
            "<string>/Users/tester/Library/Caches/AuthPilot/launchd.stderr.log</string>"
        ));
    }
}
