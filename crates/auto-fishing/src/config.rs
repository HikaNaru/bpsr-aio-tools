use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FishingConfig {
    /// Key name to cast and reel (e.g. "Space", "f", "Return")
    pub action_key:      String,
    /// Screen region to monitor [x, y, width, height]
    pub detect_region:   [i32; 4],
    /// Color change threshold 0..255 to trigger reel
    pub color_threshold: u8,
    /// Delay between cast and starting detection (ms)
    pub cast_delay_ms:   u64,
    /// Max wait for a bite before re-casting (ms)
    pub bite_timeout_ms: u64,
    /// Delay after reeling before next cast (ms)
    pub cooldown_ms:     u64,
}

impl Default for FishingConfig {
    fn default() -> Self {
        Self {
            action_key:      "Space".to_string(),
            detect_region:   [800, 400, 200, 100],
            color_threshold: 30,
            cast_delay_ms:   1500,
            bite_timeout_ms: 30_000,
            cooldown_ms:     500,
        }
    }
}

impl FishingConfig {
    pub fn enigo_key(&self) -> enigo::Key {
        match self.action_key.as_str() {
            "Space"  | "space"  => enigo::Key::Space,
            "Return" | "Enter"  => enigo::Key::Return,
            "f" | "F"           => enigo::Key::Unicode('f'),
            "r" | "R"           => enigo::Key::Unicode('r'),
            other if other.len() == 1 => {
                enigo::Key::Unicode(other.chars().next().unwrap())
            }
            _ => enigo::Key::Space,
        }
    }
}
