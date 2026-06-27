use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub dps_window_secs:      u32,
    pub encounter_timeout_secs: u32,
    pub always_on_top:        bool,
    pub opacity:              f32,
    #[serde(default)]
    pub lock_position:        bool,
    #[serde(default)]
    pub click_passthrough:    bool,
    #[serde(default = "default_encounter_max_count")]
    pub encounter_max_count:  usize,
    #[serde(default = "default_encounter_max_age_days")]
    pub encounter_max_age_days: u64,
    #[serde(default = "default_true")]
    pub match_notify: bool,
    #[serde(default = "default_true")]
    pub match_sound: bool,
    #[serde(default)]
    pub discord_enabled: bool,
    #[serde(default)]
    pub discord_webhook_url: String,
    #[serde(default = "default_discord_min_players")]
    pub discord_min_players: usize,

    #[serde(default)]
    pub bptimer_enabled: bool,
    #[serde(default)]
    pub bptimer_ws_url: String,
    #[serde(default)]
    pub bptimer_report_url: String,
    #[serde(default)]
    pub bptimer_region: String,
}

fn default_encounter_max_count() -> usize { 500 }
fn default_encounter_max_age_days() -> u64 { 90 }
fn default_true() -> bool { true }
fn default_discord_min_players() -> usize { 2 }

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            dps_window_secs:          3,
            encounter_timeout_secs:   30,
            always_on_top:            true,
            opacity:                  1.0,
            lock_position:            false,
            click_passthrough:        false,
            encounter_max_count:      500,
            encounter_max_age_days:   90,
            match_notify:             true,
            match_sound:              true,
            discord_enabled:          false,
            discord_webhook_url:      String::new(),
            discord_min_players:      2,
            bptimer_enabled:          false,
            bptimer_ws_url:           String::new(),
            bptimer_report_url:       String::new(),
            bptimer_region:           String::new(),
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
