use anyhow::Result;
use tauri::{AppHandle, Manager};

use crate::types::AppSettings;

pub fn apply_settings(settings: &AppSettings) -> Result<()> {
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
pub fn is_start_at_login_enabled() -> bool {
    crate::launchd::status()
        .map(|status| status.installed)
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
pub fn is_start_at_login_enabled() -> bool {
    false
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
    use crate::types::AppSettings;

    #[test]
    fn app_behavior_settings_default_to_tray_only_without_login_launch() {
        let settings = AppSettings::default();

        assert!(!settings.start_at_login);
        assert!(!settings.show_in_dock);
    }

    // launchd plist rendering is covered in crate::launchd tests.
}
