//! Process management for Codex Desktop App

use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use tokio::time::sleep;
use uuid::Uuid;

const CODEX_RECENT_ACTIVITY_SECONDS: i64 = 120;
const CODEX_SESSION_ACTIVITY_SECONDS: i64 = 300;
const CODEX_TIMESTAMP_MARKER: &str = "event.timestamp=";
const MAX_SESSION_SCAN_DEPTH: usize = 6;
const MAX_SESSION_FILES_SCANNED: usize = 5000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessInfo {
    pid: u32,
    ppid: u32,
    command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecentCodexSession {
    pub session_id: Uuid,
    pub transcript_path: PathBuf,
    pub workspace_path: Option<PathBuf>,
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

    if has_active_codex_cli_process(&processes) {
        return Ok(true);
    }

    if codex_session_files_have_recent_activity().or_else(|err| {
        tracing::debug!("Unable to inspect Codex session files: {err}");
        Ok::<bool, anyhow::Error>(false)
    })? {
        return Ok(true);
    }

    if !is_codex_desktop_running()? {
        return Ok(false);
    }

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

pub fn resume_codex_session_continue(session: &RecentCodexSession) -> Result<()> {
    tracing::info!(
        "Resuming Codex session {} from {}",
        session.session_id,
        session.transcript_path.display()
    );

    let mut command = Command::new("codex");
    command.args([
        "exec",
        "resume",
        &session.session_id.to_string(),
        "continue",
    ]);

    if let Some(workspace_path) = &session.workspace_path {
        command.current_dir(workspace_path);
    }

    command
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("Failed to spawn codex session resume command")?;

    Ok(())
}

pub fn latest_recent_codex_session() -> Result<Option<RecentCodexSession>> {
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(None);
    };

    let sessions_dir = home_dir.join(".codex").join("sessions");
    if !sessions_dir.is_dir() {
        return Ok(None);
    }

    latest_recent_codex_session_in(&sessions_dir)
}

fn latest_recent_codex_session_in(sessions_dir: &Path) -> Result<Option<RecentCodexSession>> {
    let mut stack = vec![(sessions_dir.to_path_buf(), 0usize)];
    let mut files_scanned = 0usize;
    let mut latest: Option<(SystemTime, PathBuf, Uuid)> = None;

    while let Some((path, depth)) = stack.pop() {
        if depth > MAX_SESSION_SCAN_DEPTH {
            continue;
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(err) => {
                tracing::debug!(
                    "Unable to read Codex session path {}: {err}",
                    path.display()
                );
                continue;
            }
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if metadata.is_dir() {
                stack.push((entry_path, depth + 1));
                continue;
            }

            if !metadata.is_file() || !is_rollout_jsonl_path(&entry_path) {
                continue;
            }

            files_scanned += 1;
            if files_scanned > MAX_SESSION_FILES_SCANNED {
                tracing::debug!(
                    "Stopped Codex session scan after {MAX_SESSION_FILES_SCANNED} files"
                );
                break;
            }

            let Some(session_id) = session_id_from_rollout_path(&entry_path) else {
                continue;
            };

            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            let should_replace =
                latest
                    .as_ref()
                    .is_none_or(|(latest_modified_at, latest_path, _)| {
                        modified_at > *latest_modified_at
                            || (modified_at == *latest_modified_at && entry_path > *latest_path)
                    });

            if should_replace {
                latest = Some((modified_at, entry_path, session_id));
            }
        }
    }

    let Some((_, transcript_path, session_id)) = latest else {
        return Ok(None);
    };

    Ok(Some(RecentCodexSession {
        session_id,
        workspace_path: workspace_path_from_transcript(&transcript_path)?,
        transcript_path,
    }))
}

fn session_id_from_rollout_path(path: &Path) -> Option<Uuid> {
    let file_stem = path.file_stem()?.to_str()?;
    let suffix = last_n_chars(file_stem, 36)?;

    Uuid::parse_str(&suffix).ok()
}

fn last_n_chars(value: &str, count: usize) -> Option<String> {
    let chars: Vec<char> = value.chars().rev().take(count).collect();
    if chars.len() != count {
        return None;
    }

    Some(chars.into_iter().rev().collect())
}

fn is_rollout_jsonl_path(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name.starts_with("rollout-") && file_name.ends_with(".jsonl")
}

fn workspace_path_from_transcript(transcript_path: &Path) -> Result<Option<PathBuf>> {
    let file = fs::File::open(transcript_path)
        .with_context(|| format!("Failed to open transcript {}", transcript_path.display()))?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(25) {
        let line = line
            .with_context(|| format!("Failed to read transcript {}", transcript_path.display()))?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };

        if value.get("type").and_then(|kind| kind.as_str()) != Some("session_meta") {
            continue;
        }

        let Some(cwd) = value
            .get("payload")
            .and_then(|payload| payload.get("cwd"))
            .and_then(|cwd| cwd.as_str())
        else {
            return Ok(None);
        };

        return Ok(Some(PathBuf::from(cwd)));
    }

    Ok(None)
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
        .filter(|process| !is_ignored_codex_process(&process.command))
        .any(|process| has_codex_background_ancestor(process.ppid, processes))
}

fn has_active_codex_cli_process(processes: &[ProcessInfo]) -> bool {
    processes
        .iter()
        .filter(|process| !is_ignored_codex_process(&process.command))
        .any(|process| command_executable_basename(&process.command) == Some("codex"))
}

fn command_executable_basename(command: &str) -> Option<&str> {
    let executable = command.split_whitespace().next()?;
    executable.rsplit('/').next()
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

fn is_ignored_codex_process(command: &str) -> bool {
    is_background_codex_process(command) || is_codex_service_process(command)
}

fn is_codex_service_process(command: &str) -> bool {
    command.contains("notify-mcp/notify-mcp.sh")
        || command.contains("@playwright/mcp")
        || command.contains("playwright-mcp")
}

fn codex_session_files_have_recent_activity() -> Result<bool> {
    let Some(home_dir) = dirs::home_dir() else {
        return Ok(false);
    };

    let sessions_dir = home_dir.join(".codex").join("sessions");
    if !sessions_dir.is_dir() {
        return Ok(false);
    }

    codex_session_files_have_recent_activity_in(
        &sessions_dir,
        SystemTime::now(),
        Duration::from_secs(CODEX_SESSION_ACTIVITY_SECONDS as u64),
    )
}

fn codex_session_files_have_recent_activity_in(
    sessions_dir: &Path,
    now: SystemTime,
    recent_window: Duration,
) -> Result<bool> {
    let mut stack = vec![(sessions_dir.to_path_buf(), 0usize)];
    let mut files_scanned = 0usize;

    while let Some((path, depth)) = stack.pop() {
        if depth > MAX_SESSION_SCAN_DEPTH {
            continue;
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(err) => {
                tracing::debug!(
                    "Unable to read Codex session path {}: {err}",
                    path.display()
                );
                continue;
            }
        };

        for entry in entries.flatten() {
            let entry_path = entry.path();
            let Ok(metadata) = entry.metadata() else {
                continue;
            };

            if metadata.is_dir() {
                stack.push((entry_path, depth + 1));
                continue;
            }

            if !metadata.is_file() || !is_codex_session_activity_file(&entry_path) {
                continue;
            }

            files_scanned += 1;
            if files_scanned > MAX_SESSION_FILES_SCANNED {
                tracing::debug!(
                    "Stopped Codex session scan after {MAX_SESSION_FILES_SCANNED} files"
                );
                return Ok(false);
            }

            let Ok(modified_at) = metadata.modified() else {
                continue;
            };

            if system_time_age_within(now, modified_at, recent_window) {
                tracing::info!("Codex session file is active: {}", entry_path.display());
                return Ok(true);
            }
        }
    }

    Ok(false)
}

fn is_codex_session_activity_file(path: &Path) -> bool {
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };

    file_name == "log.jsonl"
        || file_name.ends_with(".jsonl")
        || file_name.ends_with(".lock")
        || file_name.ends_with(".tmp")
        || file_name.contains("lock")
}

fn system_time_age_within(now: SystemTime, timestamp: SystemTime, window: Duration) -> bool {
    match now.duration_since(timestamp) {
        Ok(age) => age <= window,
        Err(_) => true,
    }
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
    use std::fs;
    use std::time::SystemTime;

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
    fn ignores_codex_mcp_service_processes() {
        let snapshot = r#"
          100     1 /Applications/Codex.app/Contents/MacOS/Codex
          101   100 /Applications/Codex.app/Contents/Resources/codex app-server --analytics-default-enabled
          102   101 /Applications/Codex.app/Contents/Resources/node_repl
          103   101 bash /Users/example/notify-mcp/notify-mcp.sh
          104   101 npm exec @playwright/mcp@latest
          105   104 node /Users/example/.npm/_npx/123/node_modules/.bin/playwright-mcp
        "#;

        let processes = parse_process_snapshot(snapshot);

        assert!(!codex_has_active_descendant_processes(&processes));
    }

    #[test]
    fn detects_standalone_codex_cli_process() {
        let snapshot = r#"
          200     1 /bin/zsh
          201   200 /opt/homebrew/bin/codex exec run tests
        "#;

        let processes = parse_process_snapshot(snapshot);

        assert!(has_active_codex_cli_process(&processes));
    }

    #[test]
    fn detects_recent_codex_session_transcript_activity() {
        let sessions_dir =
            std::env::temp_dir().join(format!("authpilot-session-test-{}", uuid::Uuid::new_v4()));
        let day_dir = sessions_dir.join("2026").join("05").join("07");
        fs::create_dir_all(&day_dir).unwrap();
        fs::write(day_dir.join("rollout-2026-05-07T10-00-00.jsonl"), "{}\n").unwrap();

        let active = codex_session_files_have_recent_activity_in(
            &sessions_dir,
            SystemTime::now(),
            Duration::from_secs(300),
        )
        .unwrap();

        let _ = fs::remove_dir_all(&sessions_dir);
        assert!(active);
    }

    #[test]
    fn ignores_non_session_activity_files() {
        let sessions_dir =
            std::env::temp_dir().join(format!("authpilot-session-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&sessions_dir).unwrap();
        fs::write(sessions_dir.join("notes.txt"), "not a session").unwrap();

        let active = codex_session_files_have_recent_activity_in(
            &sessions_dir,
            SystemTime::now(),
            Duration::from_secs(300),
        )
        .unwrap();

        let _ = fs::remove_dir_all(&sessions_dir);
        assert!(!active);
    }

    #[test]
    fn extracts_session_id_from_rollout_uuid_suffix() {
        let session_id = uuid::Uuid::new_v4();
        let path = Path::new("/tmp/rollout-2026-05-08T12-00-00-")
            .with_file_name(format!("rollout-2026-05-08T12-00-00-{session_id}.jsonl"));

        assert_eq!(session_id_from_rollout_path(&path), Some(session_id));
    }

    #[test]
    fn ignores_rollout_paths_without_valid_uuid_suffix() {
        let path = Path::new("/tmp/rollout-2026-05-08T12-00-00-not-a-session.jsonl");

        assert_eq!(session_id_from_rollout_path(path), None);
    }

    #[test]
    fn selects_newest_recent_codex_session_with_valid_uuid_suffix() {
        let sessions_dir =
            std::env::temp_dir().join(format!("authpilot-session-test-{}", uuid::Uuid::new_v4()));
        let old_dir = sessions_dir.join("2026").join("05").join("07");
        let new_dir = sessions_dir.join("2026").join("05").join("08");
        fs::create_dir_all(&old_dir).unwrap();
        fs::create_dir_all(&new_dir).unwrap();

        let old_session_id = uuid::Uuid::new_v4();
        let new_session_id = uuid::Uuid::new_v4();
        fs::write(
            old_dir.join(format!(
                "rollout-2026-05-07T10-00-00-{old_session_id}.jsonl"
            )),
            "{}\n",
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(10));
        let new_transcript_path = new_dir.join(format!(
            "rollout-2026-05-08T10-00-00-{new_session_id}.jsonl"
        ));
        fs::write(&new_transcript_path, "{}\n").unwrap();
        std::thread::sleep(Duration::from_millis(10));
        fs::write(
            new_dir.join("rollout-2026-05-08T11-00-00-not-a-session.jsonl"),
            "{}\n",
        )
        .unwrap();

        let session = latest_recent_codex_session_in(&sessions_dir)
            .unwrap()
            .unwrap();

        let _ = fs::remove_dir_all(&sessions_dir);
        assert_eq!(session.session_id, new_session_id);
        assert_eq!(session.transcript_path, new_transcript_path);
        assert_eq!(session.workspace_path, None);
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
