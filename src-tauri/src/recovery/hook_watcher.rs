use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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

    if payload.clean_exit {
        Ok(None)
    } else {
        Ok(Some(payload))
    }
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

        assert_eq!(parse_hook_file(&path).unwrap(), None);
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
}
