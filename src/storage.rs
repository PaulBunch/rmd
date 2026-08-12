use crate::types::Reminder;
use anyhow::Result;
use std::path::PathBuf;

/// State directory
pub fn get_state_dir() -> PathBuf {
    if let Ok(path) = std::env::var("RMD_STATE_DIR") {
        PathBuf::from(path)
    } else {
        dirs::state_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .expect("Cannot find home dir")
                    .join(".local/state")
            })
            .join("rmd")
    }
}

/// Get the socket path: $RMD_SOCKET_PATH -> /run/user/<UID>/rmd.sock -> /tmp/rmd.sock
pub fn get_socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("RMD_SOCKET_PATH") {
        PathBuf::from(path)
    } else {
        dirs::runtime_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("rmd.sock")
    }
}

/// Get the state file path: $RMD_STATE_DIR/reminders.json or ~/.local/state/rmd/reminders.json
pub fn get_state_path() -> PathBuf {
    let dir = get_state_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join("reminders.json")
}

/// Atomic JSON serialization and disk flush for reminders
pub fn save_reminders(reminders: &[Reminder]) -> Result<()> {
    let path = get_state_path();
    let tmp_path = path.with_extension("json.tmp");

    let data = serde_json::to_string_pretty(reminders)?;
    std::fs::write(&tmp_path, data)?;

    // Ensure data is flushed to disk and perform an atomic rename
    let file = std::fs::File::open(&tmp_path)?;
    file.sync_all()?;
    std::fs::rename(tmp_path, path)?;

    Ok(())
}

/// Load reminders from disk
pub fn load_reminders() -> Vec<Reminder> {
    let path = get_state_path();
    if !path.exists() {
        return Vec::new();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}
