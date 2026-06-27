use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FishingConfig {
    // --- Game window ---
    pub game_window_title: String,
    #[serde(skip)]
    pub window_origin: (i32, i32),

    // --- Fishing mode entry ---
    pub fishing_key: String,
    pub fishing_mode_region:          [i32; 4],
    pub fishing_mode_hue_center:      f32,
    pub fishing_mode_hue_range:       f32,
    pub fishing_mode_min_saturation:  f32,
    pub fishing_mode_min_pixels:      u32,

    // --- Rod selection ---
    pub fishing_rod_region:          [i32; 4],
    pub fishing_rod_hue_center:      f32,
    pub fishing_rod_hue_range:       f32,
    pub fishing_rod_min_saturation:  f32,
    pub fishing_rod_min_pixels:      u32,
    /// Key to open the equipment / rod selection menu
    pub rod_slot_key: String,
    /// Screen region [ox,oy,w,h] of the "Use" button in the rod selection dialog
    pub rod_use_region: [i32; 4],

    // --- Bite detection ---
    pub detect_region:       [i32; 4],
    pub bite_hue_center:     f32,
    pub bite_hue_range:      f32,
    pub bite_min_saturation: f32,
    pub bite_min_pixels:     u32,

    // --- Casting / timing ---
    pub cast_delay_ms:   u64,
    pub bite_timeout_ms: u64,
    pub cooldown_ms:     u64,

    // --- Reeling minigame ---
    pub reel_hold_ms:           u64,
    pub reel_pause_ms:          u64,
    /// Max time to spend reeling before giving up (fish escaped / stuck)
    pub reel_timeout_ms:        u64,

    // --- Arrow indicators (steer left / steer right during reeling) ---
    pub left_arrow_region:      [i32; 4],
    pub right_arrow_region:     [i32; 4],
    pub arrow_hue_center:       f32,
    pub arrow_hue_range:        f32,
    pub arrow_min_saturation:   f32,
    pub arrow_min_pixels:       u32,

    // --- Tension bar (reeling UI) ---
    /// Region covering the tension bar. Presence = still reeling; absence = fish escaped.
    pub tension_bar_region:          [i32; 4],
    pub tension_bar_hue_center:      f32,
    pub tension_bar_hue_range:       f32,
    pub tension_bar_min_saturation:  f32,
    /// Min matching pixels to consider bar "present"
    pub tension_bar_min_pixels:      u32,

    // --- Tension percentage (reeling UI) ---
    /// Region covering the fillable portion of the tension bar (full bar width, left→right).
    /// Percentage = matching pixels / total pixels × 100.
    pub tension_pct_region: [i32; 4],

    // --- Fish caught detection ---
    /// Per-pixel brightness threshold (0-255) — pixel counts as "bright" if avg(r,g,b) >= this
    pub fish_caught_threshold: u8,
    /// Minimum number of bright pixels required to confirm the continue button is visible
    pub fish_caught_min_pixels: u32,
    /// Screen region [ox,oy,w,h] of the "Continue fishing" button (used for both detect + click)
    pub fish_caught_region: [i32; 4],
}

impl FishingConfig {
    pub fn config_path() -> std::path::PathBuf {
        let mut p = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
        p.push("config");
        p.push("fishing.json");
        p
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(p) = path.parent() { let _ = std::fs::create_dir_all(p); }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

impl Default for FishingConfig {
    fn default() -> Self {
        Self {
            game_window_title: "Blue Protocol: Star Resonance".to_string(),
            window_origin:     (0, 0),

            // All position defaults calibrated for 1600×900 windowed. Auto-scaled at runtime.
            fishing_key:                "f".to_string(),
            // Detect the orange ← back button at top-left (only present in fishing mode).
            fishing_mode_region:        [0, 0, 70, 70],
            fishing_mode_hue_center:    20.0,
            fishing_mode_hue_range:     15.0,
            fishing_mode_min_saturation: 0.8,
            fishing_mode_min_pixels:    100,

            fishing_rod_region:         [1350, 810, 220, 75],
            fishing_rod_hue_center:     30.0,
            fishing_rod_hue_range:      40.0,
            fishing_rod_min_saturation: 0.3,
            fishing_rod_min_pixels:     5,
            rod_slot_key:               "m".to_string(),
            rod_use_region:             [1355, 485, 150, 55],

            detect_region:       [720, 350, 150, 150],
            bite_hue_center:     30.0,
            bite_hue_range:      25.0,
            bite_min_saturation: 0.6,
            bite_min_pixels:     20,

            cast_delay_ms:   1500,
            bite_timeout_ms: 30_000,
            cooldown_ms:     500,

            reel_hold_ms:           400,
            reel_pause_ms:          300,
            reel_timeout_ms:        30_000,

            // Left/right arrow indicators — close to bite detect region [700,350,200,200]
            left_arrow_region:      [660, 400, 150, 100],
            right_arrow_region:     [860, 400, 150, 100],
            arrow_hue_center:       30.0,
            arrow_hue_range:        25.0,
            arrow_min_saturation:   0.6,
            arrow_min_pixels:       20,

            tension_bar_region:          [520, 710, 80, 70],
            tension_bar_hue_center:      30.0,
            tension_bar_hue_range:       30.0,
            tension_bar_min_saturation:  0.6,
            tension_bar_min_pixels:      30,

            tension_pct_region:          [970, 690, 75, 40],

            fish_caught_threshold:  180,
            fish_caught_min_pixels: 500,
            fish_caught_region:    [1200, 785, 250, 60],
        }
    }
}

