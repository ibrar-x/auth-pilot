use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodexSession {
    pub id: String,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub workspace_path: String,
    pub account_id: Option<String>,
    pub process_id: Option<u32>,
    pub started_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: SessionStatus,
    pub recovery_attempts: u32,
    pub last_recovery_at: Option<DateTime<Utc>>,
    pub last_recovery_prompt: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Interrupted,
    Ignored,
    BackgroundResumed,
}

impl SessionStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Interrupted => "interrupted",
            Self::Ignored => "ignored",
            Self::BackgroundResumed => "background_resumed",
        }
    }

    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "running" => Some(Self::Running),
            "completed" => Some(Self::Completed),
            "interrupted" => Some(Self::Interrupted),
            "ignored" => Some(Self::Ignored),
            "background_resumed" => Some(Self::BackgroundResumed),
            _ => None,
        }
    }
}
