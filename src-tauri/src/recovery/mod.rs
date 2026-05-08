pub mod hook_watcher;
pub mod hooks;
pub mod process_watch;
pub mod resume;
pub mod session_db;
pub mod types;

use std::path::PathBuf;

use anyhow::{Context, Result};

pub use types::{BackgroundResumeOutcome, CodexSession, ReopenOutcome, SessionStatus};

pub fn default_db_path() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Unable to find home directory")?;
    Ok(home.join(".authpilot").join("authpilot.db"))
}
