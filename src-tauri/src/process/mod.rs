//! Process management for Codex Desktop App

use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::time::sleep;

/// Check if Codex Desktop App is running using osascript
pub fn is_codex_desktop_running() -> Result<bool> {
    let output = Command::new("osascript")
        .args(["-e", "application \"Codex\" is running"])
        .output()
        .context("Failed to run osascript to check Codex status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "true")
}

/// Kill Codex Desktop App gracefully using osascript, fallback to pkill
pub async fn kill_codex_desktop() -> Result<()> {
    tracing::info!("Sending quit signal to Codex");

    let _ = Command::new("osascript")
        .args(["-e", "quit application \"Codex\""])
        .output()
        .context("Failed to send quit signal to Codex")?;

    // Wait up to 3 seconds for graceful quit
    for _ in 0..30 {
        sleep(Duration::from_millis(100)).await;
        if !is_codex_desktop_running()? {
            tracing::info!("Codex quit gracefully");
            return Ok(());
        }
    }

    tracing::warn!("Codex did not quit gracefully, using pkill fallback");
    let output = Command::new("pkill")
        .args(["-x", "Codex"])
        .output()
        .context("Failed to run pkill")?;

    if !output.status.success() {
        tracing::warn!("pkill returned non-zero exit code");
    }

    sleep(Duration::from_millis(500)).await;
    Ok(())
}

/// Launch Codex Desktop App
pub async fn launch_codex_desktop() -> Result<()> {
    tracing::info!("Launching Codex");

    let output = Command::new("open")
        .args(["-a", "Codex"])
        .output()
        .context("Failed to launch Codex")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to launch Codex: {}", stderr.trim());
    }

    Ok(())
}
