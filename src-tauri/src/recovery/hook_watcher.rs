use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::Utc;
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use super::session_db::SessionDb;
use super::types::{CodexSession, SessionStatus};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HookPayload {
    #[serde(default)]
    pub hook_event_name: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub transcript_path: Option<PathBuf>,
    #[serde(default, alias = "cwd")]
    pub workspace_path: Option<PathBuf>,
    #[serde(default, alias = "pid")]
    pub process_id: Option<u32>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub clean_exit: bool,
}

pub fn parse_hook_file(path: &Path) -> Result<Option<HookPayload>> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("Failed to read {}", path.display()))?;
    let trimmed = contents.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let first_json = trimmed
        .lines()
        .take_while(|line| !line.contains("__authpilot_clean_exit"))
        .collect::<Vec<_>>()
        .join("\n");

    if first_json.trim().is_empty() {
        return Ok(None);
    }

    let mut payload: HookPayload = serde_json::from_str(first_json.trim())
        .with_context(|| format!("Failed to parse hook payload {}", path.display()))?;
    if trimmed.contains("__authpilot_clean_exit") {
        payload.clean_exit = true;
    }

    Ok(Some(payload))
}

pub fn active_session_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Unable to find home directory")?;
    Ok(home
        .join("Library")
        .join("Application Support")
        .join("AuthPilot")
        .join("codex-active-session.json"))
}

pub fn start(app_handle: tauri::AppHandle) -> Result<()> {
    let active_path = active_session_path()?;
    if let Some(parent) = active_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }

    ingest_hook_file(&active_path, &app_handle)?;

    std::thread::spawn(move || {
        if let Err(err) = watch_loop(active_path, app_handle) {
            tracing::warn!("[recovery] hook watcher stopped: {err}");
        }
    });

    Ok(())
}

pub fn ingest_hook_file(path: &Path, app_handle: &tauri::AppHandle) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    let Some(payload) = parse_hook_file(path)? else {
        return Ok(());
    };
    let db = SessionDb::open(&super::default_db_path()?)?;

    if payload.clean_exit {
        if let Some(id) = session_id_from_payload(&payload) {
            db.update_status(&id, SessionStatus::Completed)?;
            let _ = app_handle.emit("recovery:session-resolved", serde_json::json!({ "id": id }));
        }
        return Ok(());
    }

    let Some(session) = session_from_payload(payload) else {
        return Ok(());
    };

    db.upsert_session(&session)?;
    Ok(())
}

pub fn derive_thread_id(payload: &HookPayload) -> Option<String> {
    if let Some(session_id) = payload
        .session_id
        .as_deref()
        .filter(|value| is_valid_uuid(value))
    {
        return Some(session_id.to_string());
    }

    payload
        .transcript_path
        .as_deref()
        .and_then(thread_id_from_transcript_path)
}

pub fn is_valid_uuid(value: &str) -> bool {
    uuid::Uuid::parse_str(value).is_ok()
}

fn thread_id_from_transcript_path(path: &Path) -> Option<String> {
    if let Some(parent_id) = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .filter(|value| is_valid_uuid(value))
    {
        return Some(parent_id.to_string());
    }

    let stem = path.file_stem()?.to_str()?;
    if stem.len() < 36 {
        return None;
    }
    let candidate = &stem[stem.len() - 36..];
    is_valid_uuid(candidate).then(|| candidate.to_string())
}

fn watch_loop(active_path: PathBuf, app_handle: tauri::AppHandle) -> Result<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    let parent = active_path
        .parent()
        .context("Active session path has no parent")?
        .to_path_buf();
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            let _ = tx.send(res);
        },
        Config::default().with_poll_interval(Duration::from_secs(2)),
    )?;
    watcher.watch(&parent, RecursiveMode::NonRecursive)?;

    for event in rx {
        match event {
            Ok(event) if event.paths.iter().any(|path| path == &active_path) => {
                std::thread::sleep(Duration::from_millis(250));
                if let Err(err) = ingest_hook_file(&active_path, &app_handle) {
                    tracing::warn!("[recovery] failed to ingest hook file: {err}");
                }
            }
            Ok(_) => {}
            Err(err) => tracing::warn!("[recovery] hook watcher event failed: {err}"),
        }
    }
    Ok(())
}

fn session_from_payload(payload: HookPayload) -> Option<CodexSession> {
    let id = session_id_from_payload(&payload)?;
    let session_id = payload.session_id.clone();
    let thread_id = derive_thread_id(&payload);
    let now = Utc::now();

    Some(CodexSession {
        id,
        thread_id,
        session_id,
        workspace_path: payload
            .workspace_path
            .as_deref()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_default(),
        account_id: payload.account_id,
        process_id: payload.process_id,
        started_at: now,
        last_seen_at: now,
        ended_at: None,
        status: SessionStatus::Running,
        recovery_attempts: 0,
        last_recovery_at: None,
        last_recovery_prompt: None,
    })
}

fn session_id_from_payload(payload: &HookPayload) -> Option<String> {
    derive_thread_id(payload)
        .or_else(|| payload.session_id.clone())
        .or_else(|| {
            payload
                .transcript_path
                .as_deref()
                .and_then(thread_id_from_transcript_path)
        })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn temp_file(contents: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("authpilot-hook-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn parse_hook_file_reads_valid_json() {
        let id = uuid::Uuid::new_v4().to_string();
        let path = temp_file(&format!(
            r#"{{"session_id":"{id}","transcript_path":"/tmp/rollout-{id}.jsonl","cwd":"/tmp/work","pid":42}}"#
        ));

        let payload = parse_hook_file(&path).unwrap().unwrap();

        assert_eq!(payload.session_id.as_deref(), Some(id.as_str()));
        assert_eq!(
            payload.workspace_path.as_deref(),
            Some(Path::new("/tmp/work"))
        );
        assert_eq!(payload.process_id, Some(42));
    }

    #[test]
    fn parse_hook_file_detects_clean_exit_sentinel() {
        let path = temp_file(r#"{"session_id":"not-a-uuid"}"#.to_string().as_str());
        fs::write(
            &path,
            r#"{"session_id":"not-a-uuid"}
{"__authpilot_clean_exit":true}
"#,
        )
        .unwrap();

        let payload = parse_hook_file(&path).unwrap().unwrap();
        assert!(payload.clean_exit);
    }

    #[test]
    fn derive_thread_id_uses_valid_session_id() {
        let id = uuid::Uuid::new_v4().to_string();
        let payload = HookPayload {
            hook_event_name: None,
            session_id: Some(id.clone()),
            transcript_path: Some(PathBuf::from("/tmp/rollout-not-a-uuid.jsonl")),
            workspace_path: None,
            process_id: None,
            account_id: None,
            clean_exit: false,
        };

        assert_eq!(derive_thread_id(&payload), Some(id));
    }

    #[test]
    fn derive_thread_id_falls_back_to_transcript_path() {
        let id = uuid::Uuid::new_v4().to_string();
        let payload = HookPayload {
            hook_event_name: None,
            session_id: Some("not-a-uuid".to_string()),
            transcript_path: Some(PathBuf::from(format!(
                "/tmp/.codex/transcripts/{id}/transcript.json"
            ))),
            workspace_path: None,
            process_id: None,
            account_id: None,
            clean_exit: false,
        };

        assert_eq!(derive_thread_id(&payload), Some(id));
    }

    #[test]
    fn is_valid_uuid_rejects_non_uuid() {
        assert!(!is_valid_uuid("not-a-uuid"));
    }

    #[test]
    fn session_from_payload_uses_uuid_id_and_workspace() {
        let id = uuid::Uuid::new_v4().to_string();
        let session = session_from_payload(HookPayload {
            hook_event_name: None,
            session_id: Some(id.clone()),
            transcript_path: None,
            workspace_path: Some(PathBuf::from("/tmp/work")),
            process_id: Some(123),
            account_id: Some("account-a".to_string()),
            clean_exit: false,
        })
        .unwrap();

        assert_eq!(session.id, id);
        assert_eq!(session.workspace_path, "/tmp/work");
        assert_eq!(session.process_id, Some(123));
        assert_eq!(session.status, SessionStatus::Running);
    }
}
