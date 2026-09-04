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
    pub default_time: String,
    pub dbus_service: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            time_format: TimeFormat::Human,
            limit: 5,
            default_time: "09:00".to_string(),
            dbus_service: "org.freedesktop.Notifications".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default_values() {
        let config = Config::default();
        assert_eq!(config.dbus_service, "org.freedesktop.Notifications");
        assert_eq!(config.limit, 5);
        assert_eq!(config.default_time, "09:00");
    }

    #[test]
    fn test_config_deserialization_fallback() {
        // Test that old JSON without dbus_service populates the default value
        let json_data = r#"{"limit": 10, "default_time": "10:00"}"#;
        let config: Config = serde_json::from_str(json_data).unwrap();
        assert_eq!(config.dbus_service, "org.freedesktop.Notifications");
        assert_eq!(config.limit, 10);
    }
}
