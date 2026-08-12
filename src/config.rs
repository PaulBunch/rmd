use crate::types::TimeFormat;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Application configuration structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub time_format: TimeFormat,
    pub limit: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            time_format: TimeFormat::Human,
            limit: 5, // 0 = no limit (or other default value)
        }
    }
}

/// Configuration directory
pub fn get_config_dir() -> PathBuf {
    if let Ok(path) = std::env::var("RMD_CONFIG_DIR") {
        PathBuf::from(path)
    } else {
        dirs::config_dir()
            .unwrap_or_else(|| {
                dirs::home_dir()
                    .expect("Cannot find home dir")
                    .join(".config")
            })
            .join("rmd")
    }
}

/// Get the config file path: $RMD_CONFIG_DIR/config.json or ~/.config/rmd/config.json
pub fn get_config_path() -> PathBuf {
    let dir = get_config_dir();
    std::fs::create_dir_all(&dir).ok();
    dir.join("config.json")
}

/// Load configuration from disk or return default
pub fn load_config() -> Config {
    let path = get_config_path();
    if !path.exists() {
        return Config::default();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|data| serde_json::from_str(&data).ok())
        .unwrap_or_default()
}

/// Save configuration to disk atomically
pub fn save_config(config: &Config) -> Result<()> {
    let path = get_config_path();
    let tmp_path = path.with_extension("json.tmp");

    let data = serde_json::to_string_pretty(config)?;
    std::fs::write(&tmp_path, data)?;

    let file = std::fs::File::open(&tmp_path)?;
    file.sync_all()?;
    std::fs::rename(tmp_path, path)?;

    Ok(())
}
