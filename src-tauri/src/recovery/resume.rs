use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use chrono::Utc;

use super::hook_watcher::is_valid_uuid;
use super::session_db::SessionDb;
use super::types::{BackgroundResumeOutcome, CodexSession, ReopenOutcome, SessionStatus};

pub const RECOVERY_PROMPT: &str = "\
The previous Codex session was interrupted unexpectedly.

Before continuing:
1. Inspect git status.
2. Review recent file changes.
3. Check what was already completed.
4. Identify the last incomplete step.
5. Continue only from the next safe step.
6. Do not overwrite or repeat completed work.
7. Ask for approval before destructive commands.";

const RECOVERY_COOLDOWN_SECONDS: i64 = 30;
const MAX_RECOVERY_ATTEMPTS: u32 = 3;

pub fn codex_resume_available() -> bool {
    Command::new("codex")
        .args(["exec", "resume", "--help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

pub fn reopen_in_codex_desktop(session: &CodexSession) -> Result<ReopenOutcome> {
    if let Some(thread_id) = session.thread_id.as_deref() {
        if is_valid_uuid(thread_id) {
            let status = Command::new("open")
                .arg(format!("codex://threads/{thread_id}"))
                .status();

            if status.map(|status| status.success()).unwrap_or(false) {
                return Ok(ReopenOutcome::DeeplinkOpened);
            }
            tracing::warn!("[recovery] deeplink open failed for {}", session.id);
        } else {
            tracing::warn!(
                "[recovery] thread_id is not a UUID for {} — skipping deeplink",
                session.id
            );
        }
    }

    let workspace = PathBuf::from(&session.workspace_path);
    if workspace.is_absolute() && workspace.exists() {
        let status = Command::new("codex")
            .args(["app", &session.workspace_path])
            .status();

        if status.map(|status| status.success()).unwrap_or(false) {
            return Ok(ReopenOutcome::WorkspaceOpened);
        }
        tracing::warn!("[recovery] workspace fallback failed for {}", session.id);
    }

    Ok(ReopenOutcome::ManualRequired {
        workspace_path: non_empty_workspace(session),
    })
}

pub fn background_resume(
    session: &CodexSession,
    db: &SessionDb,
    log_root: &Path,
) -> Result<BackgroundResumeOutcome> {
    if let Some(last_at) = session.last_recovery_at {
        let elapsed = (Utc::now() - last_at).num_seconds();
        if elapsed < RECOVERY_COOLDOWN_SECONDS {
            return Ok(BackgroundResumeOutcome::CooldownActive {
                seconds_remaining: RECOVERY_COOLDOWN_SECONDS - elapsed,
            });
        }
    }

    if session.recovery_attempts >= MAX_RECOVERY_ATTEMPTS {
        return Ok(BackgroundResumeOutcome::MaxAttemptsReached);
    }

    let workspace = PathBuf::from(&session.workspace_path);
    if !workspace.is_absolute() || !workspace.exists() {
        bail!(
            "Workspace path invalid or does not exist: {}",
            session.workspace_path
        );
    }

    fs::create_dir_all(log_root)
        .with_context(|| format!("Failed to create recovery log dir {}", log_root.display()))?;
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S");
    let log_path = log_root.join(format!("{timestamp}-{}.log", session.id));
    let log_file = fs::File::create(&log_path)
        .with_context(|| format!("Failed to create recovery log {}", log_path.display()))?;
    let log_file_for_stderr = log_file
        .try_clone()
        .context("Failed to clone recovery log handle")?;

    let mut command = Command::new("codex");
    command.current_dir(&workspace).arg("exec").arg("resume");

    if let Some(session_id) = session
        .session_id
        .as_deref()
        .filter(|value| !value.is_empty())
    {
        command.arg(session_id);
    } else {
        command.arg("--last");
    }

    command
        .arg(RECOVERY_PROMPT)
        .stdout(log_file)
        .stderr(log_file_for_stderr)
        .stdin(Stdio::null());

    command
        .spawn()
        .context("Failed to spawn codex exec resume")?;

    db.increment_recovery_attempts(&session.id, Utc::now(), RECOVERY_PROMPT)?;
    db.update_status(&session.id, SessionStatus::BackgroundResumed)?;

    Ok(BackgroundResumeOutcome::Started {
        log_path: log_path.to_string_lossy().to_string(),
    })
}

pub fn open_recovery_log(log_path: &Path) -> Result<()> {
    let status = Command::new("open")
        .arg("-t")
        .arg(log_path)
        .status()
        .with_context(|| format!("Failed to open recovery log {}", log_path.display()))?;

    if !status.success() {
        bail!("Failed to open recovery log {}", log_path.display());
    }
    Ok(())
}

fn non_empty_workspace(session: &CodexSession) -> Option<String> {
    (!session.workspace_path.trim().is_empty()).then(|| session.workspace_path.clone())
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    fn session(id: &str) -> CodexSession {
        CodexSession {
            id: id.to_string(),
            thread_id: None,
            session_id: Some(uuid::Uuid::new_v4().to_string()),
            workspace_path: "/tmp".to_string(),
            account_id: None,
            process_id: None,
            started_at: Utc.with_ymd_and_hms(2026, 5, 8, 10, 0, 0).unwrap(),
            last_seen_at: Utc.with_ymd_and_hms(2026, 5, 8, 10, 5, 0).unwrap(),
            ended_at: None,
            status: SessionStatus::Interrupted,
            recovery_attempts: 0,
            last_recovery_at: None,
            last_recovery_prompt: None,
        }
    }

    #[test]
    fn background_resume_respects_cooldown() {
        let mut item = session("cooldown");
        item.last_recovery_at = Some(Utc::now() - Duration::seconds(5));
        let db = SessionDb::open(
            &std::env::temp_dir().join(format!("authpilot-recovery-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();

        let outcome = background_resume(&item, &db, &std::env::temp_dir()).unwrap();

        assert!(matches!(
            outcome,
            BackgroundResumeOutcome::CooldownActive { .. }
        ));
    }

    #[test]
    fn background_resume_respects_max_attempts() {
        let mut item = session("max");
        item.recovery_attempts = MAX_RECOVERY_ATTEMPTS;
        let db = SessionDb::open(
            &std::env::temp_dir().join(format!("authpilot-recovery-{}.db", uuid::Uuid::new_v4())),
        )
        .unwrap();

        let outcome = background_resume(&item, &db, &std::env::temp_dir()).unwrap();

        assert_eq!(outcome, BackgroundResumeOutcome::MaxAttemptsReached);
    }
}
