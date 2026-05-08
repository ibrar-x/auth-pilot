use anyhow::{Context, Result};
use tauri::{AppHandle, Manager};

use crate::types::AppSettings;

const LOGIN_AGENT_LABEL: &str = "com.user.authpilot.login";
const BUNDLE_IDENTIFIER: &str = "com.user.authpilot";

pub fn apply_settings(settings: &AppSettings) -> Result<()> {
    sync_start_at_login(settings.start_at_login)?;
    apply_dock_visibility(settings.show_in_dock);
    Ok(())
}

pub fn show_main_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(popup) = app.get_webview_window("tray-popup") {
        let _ = popup.hide();
    }

    if let Some(window) = app.get_webview_window("main") {
        window.show()?;
        window.unminimize()?;
        window.set_focus()?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn sync_start_at_login(enabled: bool) -> Result<()> {
    let path = login_agent_path()?;

    if enabled {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create LaunchAgents directory: {}",
                    parent.display()
                )
            })?;
        }

        std::fs::write(&path, build_login_agent_plist())
            .with_context(|| format!("Failed to write login agent: {}", path.display()))?;
    } else if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove login agent: {}", path.display()))?;
    }

    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn sync_start_at_login(_enabled: bool) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn login_agent_path() -> Result<std::path::PathBuf> {
    let home_dir = dirs::home_dir().context("Unable to resolve home directory")?;
    Ok(home_dir
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LOGIN_AGENT_LABEL}.plist")))
}

#[cfg(target_os = "macos")]
pub fn is_start_at_login_enabled() -> bool {
    login_agent_path().is_ok_and(|path| path.exists())
}

#[cfg(not(target_os = "macos"))]
pub fn is_start_at_login_enabled() -> bool {
    false
}

fn build_login_agent_plist() -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LOGIN_AGENT_LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/open</string>
        <string>-b</string>
        <string>{BUNDLE_IDENTIFIER}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
    )
}

#[cfg(target_os = "macos")]
fn apply_dock_visibility(show_in_dock: bool) {
    #[allow(unexpected_cfgs)]
    unsafe {
        use objc::runtime::Class;
        use objc::{msg_send, sel, sel_impl};

        let Some(cls) = Class::get("NSApplication") else {
            return;
        };

        let app: *mut objc::runtime::Object = msg_send![cls, sharedApplication];
        let policy = if show_in_dock { 0i64 } else { 1i64 };
        let _: bool = msg_send![app, setActivationPolicy: policy];
    }
}

#[cfg(not(target_os = "macos"))]
fn apply_dock_visibility(_show_in_dock: bool) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AppSettings;

    #[test]
    fn app_behavior_settings_default_to_tray_only_without_login_launch() {
        let settings = AppSettings::default();

        assert!(!settings.start_at_login);
        assert!(!settings.show_in_dock);
    }

    #[test]
    fn login_agent_plist_launches_authpilot_bundle_once_at_login() {
        let plist = build_login_agent_plist();

        assert!(plist.contains("<string>com.user.authpilot.login</string>"));
        assert!(plist.contains("<string>/usr/bin/open</string>"));
        assert!(plist.contains("<string>-b</string>"));
        assert!(plist.contains("<string>com.user.authpilot</string>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
        assert!(!plist.contains("<key>KeepAlive</key>"));
    }
}
