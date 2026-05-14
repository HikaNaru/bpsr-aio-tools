use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub capture_interface: Option<String>,
    pub dps_window_secs:   u32,
    pub encounter_timeout_secs: u32,
    pub always_on_top:     bool,
    pub opacity:           f32,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            capture_interface:      None,
            dps_window_secs:        3,
            encounter_timeout_secs: 30,
            always_on_top:          true,
            opacity:                1.0,
        }
    }
}

impl AppConfig {
    pub fn config_path() -> PathBuf {
        let base = dirs_next::config_dir()
            .unwrap_or_else(|| PathBuf::from("."));
        base.join("bpsr").join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), crate::AppError> {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(&path, json)?;
        Ok(())
    }
}
