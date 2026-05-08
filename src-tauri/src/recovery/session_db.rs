use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, types::Type, Connection, OptionalExtension};

use super::types::{CodexSession, SessionStatus};

pub struct SessionDb {
    conn: Connection,
}

impl SessionDb {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create {}", parent.display()))?;
        }

        let db = Self {
            conn: Connection::open(path)
                .with_context(|| format!("Failed to open recovery DB {}", path.display()))?,
        };
        db.migrate()?;
        Ok(db)
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self> {
        let db = Self {
            conn: Connection::open_in_memory().context("Failed to open in-memory DB")?,
        };
        db.migrate()?;
        Ok(db)
    }

    pub fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS codex_sessions (
                id TEXT PRIMARY KEY NOT NULL,
                thread_id TEXT,
                session_id TEXT,
                workspace_path TEXT NOT NULL,
                account_id TEXT,
                process_id INTEGER,
                started_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL,
                ended_at TEXT,
                status TEXT NOT NULL,
                recovery_attempts INTEGER NOT NULL DEFAULT 0,
                last_recovery_at TEXT,
                last_recovery_prompt TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_codex_sessions_status
                ON codex_sessions(status);
            CREATE INDEX IF NOT EXISTS idx_codex_sessions_last_seen_at
                ON codex_sessions(last_seen_at);
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_session(&self, session: &CodexSession) -> Result<()> {
        self.conn.execute(
            r#"
            INSERT INTO codex_sessions (
                id, thread_id, session_id, workspace_path, account_id, process_id,
                started_at, last_seen_at, ended_at, status, recovery_attempts,
                last_recovery_at, last_recovery_prompt
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            ON CONFLICT(id) DO UPDATE SET
                thread_id = excluded.thread_id,
                session_id = excluded.session_id,
                workspace_path = excluded.workspace_path,
                account_id = excluded.account_id,
                process_id = excluded.process_id,
                started_at = excluded.started_at,
                last_seen_at = excluded.last_seen_at,
                ended_at = excluded.ended_at,
                status = excluded.status,
                recovery_attempts = excluded.recovery_attempts,
                last_recovery_at = excluded.last_recovery_at,
                last_recovery_prompt = excluded.last_recovery_prompt
            "#,
            params![
                session.id,
                session.thread_id,
                session.session_id,
                session.workspace_path,
                session.account_id,
                session.process_id,
                encode_time(session.started_at),
                encode_time(session.last_seen_at),
                session.ended_at.map(encode_time),
                session.status.as_str(),
                session.recovery_attempts,
                session.last_recovery_at.map(encode_time),
                session.last_recovery_prompt,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<CodexSession>> {
        self.conn
            .query_row(
                "SELECT * FROM codex_sessions WHERE id = ?1",
                [id],
                decode_session,
            )
            .optional()
            .context("Failed to get recovery session")
    }

    pub fn query_by_status(&self, status: SessionStatus) -> Result<Vec<CodexSession>> {
        let mut stmt = self.conn.prepare(
            "SELECT * FROM codex_sessions WHERE status = ?1 ORDER BY last_seen_at DESC, started_at DESC",
        )?;
        let rows = stmt.query_map([status.as_str()], decode_session)?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .context("Failed to query recovery sessions")
    }

    pub fn update_status(&self, id: &str, status: SessionStatus) -> Result<()> {
        match status {
            SessionStatus::Completed | SessionStatus::Interrupted => {
                self.conn.execute(
                    "UPDATE codex_sessions SET status = ?1, ended_at = COALESCE(ended_at, ?2) WHERE id = ?3",
                    params![status.as_str(), encode_time(Utc::now()), id],
                )?;
            }
            _ => {
                self.conn.execute(
                    "UPDATE codex_sessions SET status = ?1 WHERE id = ?2",
                    params![status.as_str(), id],
                )?;
            }
        }
        Ok(())
    }

    pub fn increment_recovery_attempts(
        &self,
        id: &str,
        at: DateTime<Utc>,
        prompt: &str,
    ) -> Result<()> {
        self.conn.execute(
            r#"
            UPDATE codex_sessions
            SET recovery_attempts = recovery_attempts + 1,
                last_recovery_at = ?1,
                last_recovery_prompt = ?2
            WHERE id = ?3
            "#,
            params![encode_time(at), prompt, id],
        )?;
        Ok(())
    }

    pub fn mark_stale_sessions_interrupted(&self, stale_before: DateTime<Utc>) -> Result<usize> {
        let updated = self.conn.execute(
            r#"
            UPDATE codex_sessions
            SET status = ?1, ended_at = COALESCE(ended_at, last_seen_at)
            WHERE status = ?2 AND last_seen_at < ?3
            "#,
            params![
                SessionStatus::Interrupted.as_str(),
                SessionStatus::Running.as_str(),
                encode_time(stale_before),
            ],
        )?;
        Ok(updated)
    }
}

fn encode_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn decode_time(value: String) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&value)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|err| rusqlite::Error::FromSqlConversionFailure(0, Type::Text, Box::new(err)))
}

fn decode_optional_time(value: Option<String>) -> rusqlite::Result<Option<DateTime<Utc>>> {
    value.map(decode_time).transpose()
}

fn decode_status(value: String) -> rusqlite::Result<SessionStatus> {
    SessionStatus::from_str(&value).ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            Type::Text,
            format!("Unknown session status {value}").into(),
        )
    })
}

fn decode_session(row: &rusqlite::Row<'_>) -> rusqlite::Result<CodexSession> {
    let process_id: Option<i64> = row.get("process_id")?;
    let recovery_attempts: i64 = row.get("recovery_attempts")?;

    Ok(CodexSession {
        id: row.get("id")?,
        thread_id: row.get("thread_id")?,
        session_id: row.get("session_id")?,
        workspace_path: row.get("workspace_path")?,
        account_id: row.get("account_id")?,
        process_id: process_id.map(|value| value as u32),
        started_at: decode_time(row.get("started_at")?)?,
        last_seen_at: decode_time(row.get("last_seen_at")?)?,
        ended_at: decode_optional_time(row.get("ended_at")?)?,
        status: decode_status(row.get("status")?)?,
        recovery_attempts: recovery_attempts as u32,
        last_recovery_at: decode_optional_time(row.get("last_recovery_at")?)?,
        last_recovery_prompt: row.get("last_recovery_prompt")?,
    })
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};

    use super::*;

    fn session(id: &str, status: SessionStatus, last_seen_at: DateTime<Utc>) -> CodexSession {
        CodexSession {
            id: id.to_string(),
            thread_id: Some(format!("thread-{id}")),
            session_id: Some(format!("session-{id}")),
            workspace_path: format!("/tmp/workspace-{id}"),
            account_id: Some("account-a".to_string()),
            process_id: Some(1234),
            started_at: Utc.with_ymd_and_hms(2026, 5, 8, 10, 0, 0).unwrap(),
            last_seen_at,
            ended_at: None,
            status,
            recovery_attempts: 0,
            last_recovery_at: None,
            last_recovery_prompt: None,
        }
    }

    #[test]
    fn insert_session_then_query_by_id() {
        let db = SessionDb::in_memory().unwrap();
        let item = session("one", SessionStatus::Running, Utc::now());

        db.upsert_session(&item).unwrap();

        assert_eq!(db.get("one").unwrap(), Some(item));
    }

    #[test]
    fn update_status_then_query() {
        let db = SessionDb::in_memory().unwrap();
        db.upsert_session(&session("one", SessionStatus::Running, Utc::now()))
            .unwrap();

        db.update_status("one", SessionStatus::Ignored).unwrap();

        let item = db.get("one").unwrap().unwrap();
        assert_eq!(item.status, SessionStatus::Ignored);
    }

    #[test]
    fn increment_recovery_attempts_updates_prompt_and_timestamp() {
        let db = SessionDb::in_memory().unwrap();
        let at = Utc.with_ymd_and_hms(2026, 5, 8, 12, 0, 0).unwrap();
        db.upsert_session(&session("one", SessionStatus::Interrupted, Utc::now()))
            .unwrap();

        db.increment_recovery_attempts("one", at, "recover this")
            .unwrap();

        let item = db.get("one").unwrap().unwrap();
        assert_eq!(item.recovery_attempts, 1);
        assert_eq!(item.last_recovery_at, Some(at));
        assert_eq!(item.last_recovery_prompt.as_deref(), Some("recover this"));
    }

    #[test]
    fn mark_stale_sessions_interrupted_marks_old_running_sessions() {
        let db = SessionDb::in_memory().unwrap();
        let now = Utc::now();
        db.upsert_session(&session(
            "old-running",
            SessionStatus::Running,
            now - Duration::minutes(10),
        ))
        .unwrap();
        db.upsert_session(&session("fresh-running", SessionStatus::Running, now))
            .unwrap();
        db.upsert_session(&session(
            "old-completed",
            SessionStatus::Completed,
            now - Duration::minutes(10),
        ))
        .unwrap();

        let updated = db
            .mark_stale_sessions_interrupted(now - Duration::minutes(5))
            .unwrap();

        assert_eq!(updated, 1);
        assert_eq!(
            db.get("old-running").unwrap().unwrap().status,
            SessionStatus::Interrupted
        );
        assert_eq!(
            db.get("fresh-running").unwrap().unwrap().status,
            SessionStatus::Running
        );
        assert_eq!(
            db.get("old-completed").unwrap().unwrap().status,
            SessionStatus::Completed
        );
    }

    #[test]
    fn query_interrupted_sessions_returns_only_interrupted() {
        let db = SessionDb::in_memory().unwrap();
        db.upsert_session(&session("one", SessionStatus::Interrupted, Utc::now()))
            .unwrap();
        db.upsert_session(&session("two", SessionStatus::Running, Utc::now()))
            .unwrap();

        let interrupted = db.query_by_status(SessionStatus::Interrupted).unwrap();

        assert_eq!(interrupted.len(), 1);
        assert_eq!(interrupted[0].id, "one");
    }
}
