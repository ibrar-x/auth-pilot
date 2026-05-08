use std::path::Path;

use anyhow::{Context, Result};
use tauri::{AppHandle, Emitter, Manager};

use crate::recovery::resume::{
    background_resume, codex_resume_available, open_recovery_log, reopen_in_codex_desktop,
    RECOVERY_PROMPT,
};
use crate::recovery::session_db::SessionDb;
use crate::recovery::{BackgroundResumeOutcome, CodexSession, ReopenOutcome, SessionStatus};

#[tauri::command]
pub fn recovery_list_interrupted() -> Result<Vec<CodexSession>, String> {
    with_recovery_db(|db| db.query_by_status(SessionStatus::Interrupted))
}

#[tauri::command]
pub fn recovery_resume_available() -> Result<bool, String> {
    Ok(codex_resume_available())
}

#[tauri::command]
pub fn recovery_copy_prompt() -> Result<String, String> {
    Ok(RECOVERY_PROMPT.to_string())
}

#[tauri::command]
pub fn recovery_reopen(session_id: String, app: AppHandle) -> Result<ReopenOutcome, String> {
    let outcome = with_recovery_db(|db| {
        let session = get_session(db, &session_id)?;
        reopen_in_codex_desktop(&session)
    })?;
    let _ = app.emit("recovery:reopen-outcome", &outcome);
    Ok(outcome)
}

#[tauri::command]
pub fn recovery_background_resume(
    session_id: String,
    app: AppHandle,
) -> Result<BackgroundResumeOutcome, String> {
    let log_dir = app
        .path()
        .app_data_dir()
        .map_err(|err| err.to_string())?
        .join("recovery-logs");
    let outcome = with_recovery_db(|db| {
        let session = get_session(db, &session_id)?;
        background_resume(&session, db, &log_dir)
    })?;
    emit_sessions_list(&app);
    let _ = app.emit("recovery:resume-outcome", &outcome);
    Ok(outcome)
}

#[tauri::command]
pub fn recovery_ignore(session_id: String, app: AppHandle) -> Result<(), String> {
    with_recovery_db(|db| db.update_status(&session_id, SessionStatus::Ignored))?;
    emit_sessions_list(&app);
    let _ = app.emit(
        "recovery:session-resolved",
        serde_json::json!({ "id": session_id }),
    );
    Ok(())
}

#[tauri::command]
pub fn recovery_open_log(log_path: String) -> Result<(), String> {
    open_recovery_log(Path::new(&log_path)).map_err(|err| err.to_string())
}

fn with_recovery_db<T>(f: impl FnOnce(&SessionDb) -> Result<T>) -> Result<T, String> {
    let path = crate::recovery::default_db_path().map_err(|err| err.to_string())?;
    let db = SessionDb::open(&path).map_err(|err| err.to_string())?;
    f(&db).map_err(|err| err.to_string())
}

fn get_session(db: &SessionDb, id: &str) -> Result<CodexSession> {
    db.get(id)?
        .with_context(|| format!("Recovery session not found: {id}"))
}

fn emit_sessions_list(app: &AppHandle) {
    let sessions = with_recovery_db(|db| db.query_by_status(SessionStatus::Interrupted));
    if let Ok(sessions) = sessions {
        let _ = app.emit("recovery:sessions-list", sessions);
    }
}
