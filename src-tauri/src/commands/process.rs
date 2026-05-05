//! Process detection commands

use std::process::Command;

#[tauri::command]
pub async fn check_codex_processes() -> Result<CodexProcessInfo, String> {
    let count = find_codex_processes().map_err(|e| e.to_string())?;

    Ok(CodexProcessInfo {
        count,
        background_count: 0,
        can_switch: count == 0,
        pids: Vec::new(),
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CodexProcessInfo {
    pub count: usize,
    pub background_count: usize,
    pub can_switch: bool,
    pub pids: Vec<u32>,
}

fn find_codex_processes() -> anyhow::Result<usize> {
    #[cfg(unix)]
    {
        let output = Command::new("osascript")
            .args(["-e", "application \"Codex\" is running"])
            .output()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.trim() == "true" {
            return Ok(1);
        }
        return Ok(0);
    }

    #[cfg(windows)]
    {
        return Ok(0);
    }

    #[allow(unreachable_code)]
    Ok(0)
}
