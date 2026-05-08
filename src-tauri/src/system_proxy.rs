//! macOS system HTTPS proxy management.
//!
//! These functions are explicit actions only. They are not called during normal
//! startup unless a later consent flow chooses to do so.

use std::fs;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::auth::storage;

const MODIFIED_SERVICES_FILENAME: &str = "system-proxy-services.json";
const SYSTEM_PROXY_LOCKFILE_NAME: &str = ".authpilot-system-proxy-active";
const PROXY_HOST: &str = "127.0.0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemProxyStatus {
    pub supported: bool,
    pub enabled_from_authpilot: bool,
    pub modified_services: Vec<String>,
    pub lockfile_exists: bool,
}

#[cfg(target_os = "macos")]
pub fn enable(port: u16) -> Result<SystemProxyStatus> {
    let services = active_network_services()?;
    for service in &services {
        run_networksetup(&secure_web_proxy_args(service, port))?;
        run_networksetup(&secure_web_proxy_state_args(service, true))?;
        run_networksetup(&proxy_bypass_domains_args(service))?;
    }

    save_modified_services(&services)?;
    write_lockfile(port)?;
    tracing::info!(
        "[system-proxy] enabled HTTPS proxy on {} service(s)",
        services.len()
    );
    status()
}

#[cfg(not(target_os = "macos"))]
pub fn enable(_port: u16) -> Result<SystemProxyStatus> {
    anyhow::bail!("macOS system proxy is only supported on macOS")
}

#[cfg(target_os = "macos")]
pub fn disable() -> Result<SystemProxyStatus> {
    let services = load_modified_services().unwrap_or_default();
    for service in &services {
        run_networksetup(&secure_web_proxy_state_args(service, false))?;
    }

    clear_modified_services()?;
    clear_lockfile()?;
    tracing::info!("[system-proxy] disabled AuthPilot-managed HTTPS proxy");
    status()
}

#[cfg(not(target_os = "macos"))]
pub fn disable() -> Result<SystemProxyStatus> {
    Ok(SystemProxyStatus {
        supported: false,
        enabled_from_authpilot: false,
        modified_services: Vec::new(),
        lockfile_exists: false,
    })
}

pub fn disable_best_effort() {
    if let Err(err) = disable() {
        tracing::warn!("[system-proxy] cleanup failed: {err}");
    }
}

#[cfg(target_os = "macos")]
pub fn watch_network_changes(proxy_port: u16) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(10));
        match status() {
            Ok(current) if current.enabled_from_authpilot => {
                if let Err(err) = reapply_to_new_services(proxy_port) {
                    tracing::warn!("[system-proxy] network reapply failed: {err}");
                }
            }
            Ok(_) => break,
            Err(err) => {
                tracing::warn!("[system-proxy] network watcher status failed: {err}");
                break;
            }
        }
    });
}

#[cfg(not(target_os = "macos"))]
pub fn watch_network_changes(_proxy_port: u16) {}

pub fn status() -> Result<SystemProxyStatus> {
    let modified_services = load_modified_services().unwrap_or_default();
    let lockfile_exists = lockfile_path()?.exists();
    Ok(SystemProxyStatus {
        supported: cfg!(target_os = "macos"),
        enabled_from_authpilot: lockfile_exists && !modified_services.is_empty(),
        modified_services,
        lockfile_exists,
    })
}

#[cfg(target_os = "macos")]
fn reapply_to_new_services(port: u16) -> Result<()> {
    let known = load_modified_services().unwrap_or_default();
    let active = active_network_services()?;
    let merged = merge_service_lists(&known, &active);

    for service in active.iter().filter(|service| !known.contains(*service)) {
        run_networksetup(&secure_web_proxy_args(service, port))?;
        run_networksetup(&secure_web_proxy_state_args(service, true))?;
        run_networksetup(&proxy_bypass_domains_args(service))?;
    }

    if merged != known {
        save_modified_services(&merged)?;
    }

    Ok(())
}

fn merge_service_lists(existing: &[String], active: &[String]) -> Vec<String> {
    let mut merged = existing.to_vec();
    for service in active {
        if !merged.contains(service) {
            merged.push(service.clone());
        }
    }
    merged
}

#[cfg(target_os = "macos")]
fn active_network_services() -> Result<Vec<String>> {
    let output = Command::new("networksetup")
        .arg("-listallnetworkservices")
        .output()
        .context("failed to list macOS network services")?;

    if !output.status.success() {
        anyhow::bail!(
            "networksetup -listallnetworkservices failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(parse_network_services(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

fn parse_network_services(output: &str) -> Vec<String> {
    output
        .lines()
        .skip_while(|line| line.contains("network services"))
        .skip(1)
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with('*'))
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(target_os = "macos")]
fn run_networksetup(args: &[String]) -> Result<()> {
    let output = Command::new("networksetup")
        .args(args)
        .output()
        .with_context(|| format!("failed to run networksetup {}", args.join(" ")))?;

    if !output.status.success() {
        anyhow::bail!(
            "networksetup {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }

    Ok(())
}

fn secure_web_proxy_args(service: &str, port: u16) -> Vec<String> {
    vec![
        "-setsecurewebproxy".to_string(),
        service.to_string(),
        PROXY_HOST.to_string(),
        port.to_string(),
    ]
}

fn secure_web_proxy_state_args(service: &str, enabled: bool) -> Vec<String> {
    vec![
        "-setsecurewebproxystate".to_string(),
        service.to_string(),
        if enabled { "on" } else { "off" }.to_string(),
    ]
}

fn proxy_bypass_domains_args(service: &str) -> Vec<String> {
    vec![
        "-setproxybypassdomains".to_string(),
        service.to_string(),
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "*.local".to_string(),
        "169.254/16".to_string(),
    ]
}

fn save_modified_services(services: &[String]) -> Result<()> {
    let path = modified_services_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create system proxy state directory {}",
                parent.display()
            )
        })?;
    }

    fs::write(&path, serde_json::to_string_pretty(services)?)
        .with_context(|| format!("failed to save system proxy services {}", path.display()))?;
    Ok(())
}

fn load_modified_services() -> Result<Vec<String>> {
    let path = modified_services_path()?;
    if !path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&path)
        .with_context(|| format!("failed to read system proxy services {}", path.display()))?;
    serde_json::from_str(&contents)
        .with_context(|| format!("failed to parse system proxy services {}", path.display()))
}

fn clear_modified_services() -> Result<()> {
    let path = modified_services_path()?;
    if path.exists() {
        fs::remove_file(&path).with_context(|| {
            format!("failed to remove system proxy services {}", path.display())
        })?;
    }
    Ok(())
}

fn write_lockfile(port: u16) -> Result<()> {
    let path = lockfile_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create system proxy lockfile directory {}",
                parent.display()
            )
        })?;
    }
    fs::write(&path, format!("pid={}\nport={port}\n", std::process::id()))
        .with_context(|| format!("failed to write system proxy lockfile {}", path.display()))?;
    Ok(())
}

fn clear_lockfile() -> Result<()> {
    let path = lockfile_path()?;
    if path.exists() {
        fs::remove_file(&path).with_context(|| {
            format!("failed to remove system proxy lockfile {}", path.display())
        })?;
    }
    Ok(())
}

fn modified_services_path() -> Result<std::path::PathBuf> {
    Ok(storage::get_config_dir()?.join(MODIFIED_SERVICES_FILENAME))
}

fn lockfile_path() -> Result<std::path::PathBuf> {
    Ok(storage::get_config_dir()?.join(SYSTEM_PROXY_LOCKFILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_enabled_network_services() {
        let output = "An asterisk (*) denotes that a network service is disabled.\nWi-Fi\nUSB 10/100/1000 LAN\n*Thunderbolt Bridge\n\n";

        assert_eq!(
            parse_network_services(output),
            vec!["Wi-Fi".to_string(), "USB 10/100/1000 LAN".to_string()]
        );
    }

    #[test]
    fn builds_secure_web_proxy_command_args() {
        assert_eq!(
            secure_web_proxy_args("Wi-Fi", 18080),
            vec!["-setsecurewebproxy", "Wi-Fi", "127.0.0.1", "18080"]
        );
    }

    #[test]
    fn builds_secure_web_proxy_state_args() {
        assert_eq!(
            secure_web_proxy_state_args("Wi-Fi", true),
            vec!["-setsecurewebproxystate", "Wi-Fi", "on"]
        );
        assert_eq!(
            secure_web_proxy_state_args("Wi-Fi", false),
            vec!["-setsecurewebproxystate", "Wi-Fi", "off"]
        );
    }

    #[test]
    fn builds_proxy_bypass_domains_args() {
        assert_eq!(
            proxy_bypass_domains_args("Wi-Fi"),
            vec![
                "-setproxybypassdomains",
                "Wi-Fi",
                "localhost",
                "127.0.0.1",
                "*.local",
                "169.254/16"
            ]
        );
    }

    #[test]
    fn merges_new_services_without_duplicates() {
        let existing = vec!["Wi-Fi".to_string()];
        let active = vec!["Wi-Fi".to_string(), "VPN".to_string()];

        assert_eq!(
            merge_service_lists(&existing, &active),
            vec!["Wi-Fi".to_string(), "VPN".to_string()]
        );
    }
}
