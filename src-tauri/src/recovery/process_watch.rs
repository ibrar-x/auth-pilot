use anyhow::Result;
use chrono::{Duration, Utc};
use tauri::Emitter;

use crate::process::ProcessInfo;

use super::session_db::SessionDb;
use super::types::{CodexSession, SessionStatus};

const INTERRUPTION_GRACE_SECONDS: i64 = 10;

pub fn check_cycle(app_handle: &tauri::AppHandle) -> Result<()> {
    let processes = crate::process::fetch_process_list().unwrap_or_else(|err| {
        tracing::warn!("[recovery] failed to fetch process list: {err}");
        Vec::new()
    });
    let db = SessionDb::open(&super::default_db_path()?)?;
    let interrupted = mark_interrupted_sessions(&db, &processes, Utc::now())?;

    for session in interrupted {
        let _ = app_handle.emit("recovery:session-interrupted", &session);
    }

    Ok(())
}

pub fn mark_interrupted_sessions(
    db: &SessionDb,
    processes: &[ProcessInfo],
    now: chrono::DateTime<Utc>,
) -> Result<Vec<CodexSession>> {
    let cutoff = now - Duration::seconds(INTERRUPTION_GRACE_SECONDS);
    let mut interrupted = Vec::new();

    for session in db.query_by_status(SessionStatus::Running)? {
        if session.last_seen_at > cutoff || is_session_visible(&session, processes) {
            continue;
        }

        db.update_status(&session.id, SessionStatus::Interrupted)?;
        let mut session = session;
        session.status = SessionStatus::Interrupted;
        session.ended_at = Some(now);
        interrupted.push(session);
    }

    Ok(interrupted)
}

fn is_session_visible(session: &CodexSession, processes: &[ProcessInfo]) -> bool {
    if let Some(pid) = session.process_id {
        if processes.iter().any(|process| process.pid == pid) {
            return true;
        }
    }

    if !session.workspace_path.is_empty()
        && processes.iter().any(|process| {
            process.cwd.as_deref() == Some(session.workspace_path.as_str())
                && is_codex_like_process(&process.command)
        })
    {
        return true;
    }

    session.process_id.is_none()
        && processes
            .iter()
            .any(|process| is_codex_desktop_process(&process.command))
}

fn is_codex_like_process(command: &str) -> bool {
    command_executable_basename(command) == Some("codex") || is_codex_desktop_process(command)
}

fn is_codex_desktop_process(command: &str) -> bool {
    command.contains("/Codex.app/Contents/MacOS/Codex")
        || command.contains("/Codex.app/Contents/Resources/codex app-server")
}

fn command_executable_basename(command: &str) -> Option<&str> {
    command.split_whitespace().next()?.rsplit('/').next()
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;

    fn temp_db() -> SessionDb {
        SessionDb::open(&std::env::temp_dir().join(format!(
            "authpilot-process-watch-{}.db",
            uuid::Uuid::new_v4()
        )))
        .unwrap()
    }

    fn session(
        id: &str,
        process_id: Option<u32>,
        workspace_path: &str,
        last_seen_offset_seconds: i64,
    ) -> CodexSession {
        let now = Utc.with_ymd_and_hms(2026, 5, 8, 16, 0, 0).unwrap();
        CodexSession {
            id: id.to_string(),
            thread_id: Some(id.to_string()),
            session_id: Some(id.to_string()),
            workspace_path: workspace_path.to_string(),
            account_id: None,
            process_id,
            started_at: now,
            last_seen_at: now - Duration::seconds(last_seen_offset_seconds),
            ended_at: None,
            status: SessionStatus::Running,
            recovery_attempts: 0,
            last_recovery_at: None,
            last_recovery_prompt: None,
        }
    }

    fn process(pid: u32, command: &str, cwd: Option<&str>) -> ProcessInfo {
        ProcessInfo {
            pid,
            ppid: 0,
            command: command.to_string(),
            cwd: cwd.map(str::to_string),
        }
    }

    #[test]
    fn known_pid_present_keeps_session_running() {
        let db = temp_db();
        let now = Utc.with_ymd_and_hms(2026, 5, 8, 16, 0, 0).unwrap();
        db.upsert_session(&session("one", Some(42), "/tmp/work", 60))
            .unwrap();

        let interrupted =
            mark_interrupted_sessions(&db, &[process(42, "/usr/bin/codex", None)], now).unwrap();

        assert!(interrupted.is_empty());
        assert_eq!(
            db.get("one").unwrap().unwrap().status,
            SessionStatus::Running
        );
    }

    #[test]
    fn known_pid_absent_under_grace_keeps_session_running() {
        let db = temp_db();
        let now = Utc.with_ymd_and_hms(2026, 5, 8, 16, 0, 0).unwrap();
        db.upsert_session(&session("one", Some(42), "/tmp/work", 5))
            .unwrap();

        let interrupted = mark_interrupted_sessions(&db, &[], now).unwrap();

        assert!(interrupted.is_empty());
        assert_eq!(
            db.get("one").unwrap().unwrap().status,
            SessionStatus::Running
        );
    }

    #[test]
    fn known_pid_absent_after_grace_marks_interrupted() {
        let db = temp_db();
        let now = Utc.with_ymd_and_hms(2026, 5, 8, 16, 0, 0).unwrap();
        db.upsert_session(&session("one", Some(42), "/tmp/work", 60))
            .unwrap();

        let interrupted = mark_interrupted_sessions(&db, &[], now).unwrap();

        assert_eq!(interrupted.len(), 1);
        assert_eq!(
            db.get("one").unwrap().unwrap().status,
            SessionStatus::Interrupted
        );
    }

    #[test]
    fn matching_workspace_keeps_session_running_without_pid() {
        let db = temp_db();
        let now = Utc.with_ymd_and_hms(2026, 5, 8, 16, 0, 0).unwrap();
        db.upsert_session(&session("one", None, "/tmp/work", 60))
            .unwrap();

        let interrupted = mark_interrupted_sessions(
            &db,
            &[process(9, "/opt/homebrew/bin/codex", Some("/tmp/work"))],
            now,
        )
        .unwrap();

        assert!(interrupted.is_empty());
        assert_eq!(
            db.get("one").unwrap().unwrap().status,
            SessionStatus::Running
        );
    }
}
