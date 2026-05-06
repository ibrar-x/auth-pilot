//! Process management for Codex Desktop App

use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tokio::time::sleep;

const CODEX_RECENT_ACTIVITY_SECONDS: i64 = 120;
const CODEX_TIMESTAMP_MARKER: &str = "event.timestamp=";

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    command: String,
}

/// Check if Codex Desktop App is running using osascript
pub fn is_codex_desktop_running() -> Result<bool> {
    let output = Command::new("osascript")
        .args(["-e", "application \"Codex\" is running"])
        .output()
        .context("Failed to run osascript to check Codex status")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim() == "true")
}

/// Check whether Codex Desktop appears to have active non-Electron child work.
///
/// Auto-switching should avoid restarting Codex while a chat turn or shell command
/// is still running. This is intentionally conservative: if a non-background
/// process is descended from the Codex app process tree, the caller should defer.
pub fn is_codex_desktop_busy() -> Result<bool> {
    if !is_codex_desktop_running()? {
        return Ok(false);
    }

    let output = Command::new("ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
        .context("Failed to inspect process tree")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("Failed to inspect process tree: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let processes = parse_process_snapshot(&stdout);
    if codex_has_active_descendant_processes(&processes) {
        return Ok(true);
    }

    codex_logs_have_recent_activity().or_else(|err| {
        tracing::debug!("Unable to inspect Codex activity logs: {err}");
        Ok(false)
    })
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

fn parse_process_snapshot(output: &str) -> Vec<ProcessInfo> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let mut parts = line.split_whitespace();
            let pid_part = parts.next()?;
            let ppid_part = parts.next()?;
            let pid = pid_part.parse().ok()?;
            let ppid = ppid_part.parse().ok()?;
            let command = line
                .strip_prefix(pid_part)?
                .trim_start()
                .strip_prefix(ppid_part)?
                .trim_start()
                .to_string();

            Some(ProcessInfo { pid, ppid, command })
        })
        .collect()
}

fn codex_has_active_descendant_processes(processes: &[ProcessInfo]) -> bool {
    processes
        .iter()
        .filter(|process| !is_background_codex_process(&process.command))
        .any(|process| has_codex_background_ancestor(process.ppid, processes))
}

fn has_codex_background_ancestor(mut ppid: u32, processes: &[ProcessInfo]) -> bool {
    let mut visited = Vec::new();

    while ppid != 0 && !visited.contains(&ppid) {
        visited.push(ppid);

        let Some(parent) = processes.iter().find(|process| process.pid == ppid) else {
            return false;
        };

        if is_background_codex_process(&parent.command) {
            return true;
        }

        ppid = parent.ppid;
    }

    false
}

fn is_background_codex_process(command: &str) -> bool {
    command.contains("/Codex.app/Contents/MacOS/Codex")
        || command.contains("/Codex.app/Contents/Resources/codex app-server")
        || command.contains("/Codex.app/Contents/Resources/node_repl")
        || command.contains("Codex Helper")
}

fn codex_logs_have_recent_activity() -> Result<bool> {
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(false);
    };

    let logs_db = home_dir.join(".codex").join("logs_2.sqlite");
    if !logs_db.is_file() {
        return Ok(false);
    }

    let query = "\
        SELECT feedback_log_body \
        FROM logs \
        WHERE feedback_log_body LIKE '%originator=Codex_Desktop%' \
          AND ( \
            feedback_log_body LIKE '%session_task.turn%' \
            OR feedback_log_body LIKE '%user_input_with_turn_context%' \
            OR feedback_log_body LIKE '%response.output%' \
            OR feedback_log_body LIKE '%response.completed%' \
            OR feedback_log_body LIKE '%tool_call%' \
          ) \
        ORDER BY id DESC \
        LIMIT 25;";

    let output = Command::new("sqlite3")
        .arg("-readonly")
        .arg(logs_db)
        .arg(query)
        .output()
        .context("Failed to run sqlite3 for Codex activity logs")?;

    if !output.status.success() {
        return Ok(false);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(codex_log_output_has_recent_activity(
        &stdout,
        Utc::now(),
        CODEX_RECENT_ACTIVITY_SECONDS,
    ))
}

fn codex_log_output_has_recent_activity(
    output: &str,
    now: DateTime<Utc>,
    recent_seconds: i64,
) -> bool {
    extract_codex_event_timestamps(output)
        .into_iter()
        .any(|timestamp| {
            let age_seconds = now.signed_duration_since(timestamp).num_seconds();
            (0..=recent_seconds).contains(&age_seconds)
        })
}

fn extract_codex_event_timestamps(output: &str) -> Vec<DateTime<Utc>> {
    let mut timestamps = Vec::new();
    let mut remainder = output;

    while let Some(index) = remainder.find(CODEX_TIMESTAMP_MARKER) {
        let after_marker = &remainder[index + CODEX_TIMESTAMP_MARKER.len()..];
        let timestamp = after_marker
            .split(|ch: char| ch.is_whitespace() || ch == ',' || ch == ';')
            .next()
            .unwrap_or_default()
            .trim_matches(['"', '\'']);

        if let Ok(parsed) = DateTime::parse_from_rfc3339(timestamp) {
            timestamps.push(parsed.with_timezone(&Utc));
        }

        remainder = after_marker;
    }

    timestamps
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn detects_active_shell_process_under_codex() {
        let snapshot = r#"
          100     1 /Applications/Codex.app/Contents/MacOS/Codex
          101   100 /Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled
          102   101 /bin/zsh -lc cargo test
          103   102 cargo test
        "#;

        let processes = parse_process_snapshot(snapshot);

        assert!(codex_has_active_descendant_processes(&processes));
    }

    #[test]
    fn ignores_idle_codex_background_processes() {
        let snapshot = r#"
          100     1 /Applications/Codex.app/Contents/MacOS/Codex
          101   100 /Applications/Codex.app/Contents/Frameworks/Codex Helper.app/Contents/MacOS/Codex Helper (Renderer)
          102   100 /Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled
          103   102 /Applications/Codex.app/Contents/Resources/node_repl
        "#;

        let processes = parse_process_snapshot(snapshot);

        assert!(!codex_has_active_descendant_processes(&processes));
    }

    #[test]
    fn detects_recent_codex_log_activity() {
        let now = Utc.with_ymd_and_hms(2026, 5, 6, 10, 0, 0).unwrap();
        let output =
            "event.timestamp=2026-05-06T09:59:10Z originator=Codex_Desktop session_task.turn";

        assert!(codex_log_output_has_recent_activity(output, now, 120));
    }

    #[test]
    fn ignores_stale_codex_log_activity() {
        let now = Utc.with_ymd_and_hms(2026, 5, 6, 10, 0, 0).unwrap();
        let output =
            "event.timestamp=2026-05-06T09:55:00Z originator=Codex_Desktop session_task.turn";

        assert!(!codex_log_output_has_recent_activity(output, now, 120));
    }
}
