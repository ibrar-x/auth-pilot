use anyhow::Result;
use chrono::{Duration, Utc};
use tauri::Emitter;

use super::session_db::SessionDb;
use super::types::SessionStatus;

const INTERRUPTION_GRACE_SECONDS: i64 = 10;

pub fn check_cycle(app_handle: &tauri::AppHandle) -> Result<()> {
    if crate::process::is_codex_desktop_running().unwrap_or(false) {
        return Ok(());
    }

    let db = SessionDb::open(&super::default_db_path()?)?;
    let cutoff = Utc::now() - Duration::seconds(INTERRUPTION_GRACE_SECONDS);
    for session in db.query_by_status(SessionStatus::Running)? {
        if session.last_seen_at > cutoff {
            continue;
        }

        db.update_status(&session.id, SessionStatus::Interrupted)?;
        let mut interrupted = session;
        interrupted.status = SessionStatus::Interrupted;
        interrupted.ended_at = Some(Utc::now());
        let _ = app_handle.emit("recovery:session-interrupted", &interrupted);
    }

    Ok(())
}
