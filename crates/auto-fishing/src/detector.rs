use crate::config::FishingConfig;
use anyhow::Result;
use image::RgbaImage;

/// Capture the primary monitor. Tries xcap first, then falls back to external
/// screenshot tools for Wayland (spectacle, grim) and X11 (scrot, import).
pub fn capture_screen() -> Result<RgbaImage> {
    // 1. xcap
    if let Ok(monitors) = xcap::Monitor::all() {
        if let Some(monitor) = monitors.into_iter().next() {
            if let Ok(img) = monitor.capture_image() {
                return Ok(img);
            }
        }
    }

    // 2. External tool fallback — write to temp file, load it.
    let tmp = std::env::temp_dir().join("bpsr_screenshot.png");
    let tmp_str = tmp.to_string_lossy();

    // spectacle (KDE Plasma Wayland/X11) — stderr suppressed to hide tesseract warnings
    if std::process::Command::new("spectacle")
        .args(["-b", "-n", "-o", &tmp_str])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return load_rgba(&tmp);
    }

    // grim (wlroots-based compositors: Sway, Hyprland)
    if std::process::Command::new("grim")
        .arg(&*tmp_str)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return load_rgba(&tmp);
    }

    // scrot (X11)
    if std::process::Command::new("scrot")
        .arg(&*tmp_str)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return load_rgba(&tmp);
    }

    // import from ImageMagick (X11)
    if std::process::Command::new("import")
        .args(["-window", "root", &*tmp_str])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return load_rgba(&tmp);
    }

    anyhow::bail!(
        "xcap failed (Wayland portal error) and no fallback tool found. \
         Install one of: spectacle, grim, scrot, or imagemagick. \
         Alternatively run the app under an X11 session."
    )
}

fn load_rgba(path: &std::path::Path) -> Result<RgbaImage> {
    Ok(image::open(path)?.to_rgba8())
}

/// Find a visible window whose title contains `title`.
/// Returns (window_id, x, y, width, height). Tries xdotool first, then wmctrl.
pub fn find_game_window(title: &str) -> Option<(String, i32, i32, u32, u32)> {
    if title.is_empty() {
        return None;
    }
    if let Some(r) = find_via_xdotool(title) {
        return Some(r);
    }
    find_via_wmctrl(title)
}

/// Activate (focus) a game window by its xdotool window ID.
pub fn focus_game_window(window_id: &str) -> bool {
    if window_id.is_empty() { return false; }
    std::process::Command::new("xdotool")
        .args(["windowactivate", "--sync", window_id])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Returns true if the given xdotool window ID currently has input focus.
pub fn check_window_focus(window_id: &str) -> bool {
    if window_id.is_empty() { return true; }
    let out = std::process::Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim() == window_id)
        .unwrap_or(true)
}

fn find_via_xdotool(title: &str) -> Option<(String, i32, i32, u32, u32)> {
    let id_out = std::process::Command::new("xdotool")
        .args(["search", "--onlyvisible", "--name", title])
        .output()
        .ok()?;
    if !id_out.status.success() {
        return None;
    }
    let id_str = String::from_utf8_lossy(&id_out.stdout);
    let win_id = id_str.lines().next()?.trim().to_string();
    if win_id.is_empty() {
        return None;
    }
    let geom_out = std::process::Command::new("xdotool")
        .args(["getwindowgeometry", "--shell", &win_id])
        .output()
        .ok()?;
    if !geom_out.status.success() {
        return None;
    }
    let (x, y, w, h) = parse_xdotool_geom(&String::from_utf8_lossy(&geom_out.stdout))?;
    Some((win_id, x, y, w, h))
}

fn parse_xdotool_geom(text: &str) -> Option<(i32, i32, u32, u32)> {
    let mut x = 0i32;
    let mut y = 0i32;
    let mut w = 0u32;
    let mut h = 0u32;
    for line in text.lines() {
        if let Some(v) = line.strip_prefix("X=")      { x = v.trim().parse().ok()?; }
        if let Some(v) = line.strip_prefix("Y=")      { y = v.trim().parse().ok()?; }
        if let Some(v) = line.strip_prefix("WIDTH=")  { w = v.trim().parse().ok()?; }
        if let Some(v) = line.strip_prefix("HEIGHT=") { h = v.trim().parse().ok()?; }
    }
    if w > 0 && h > 0 { Some((x, y, w, h)) } else { None }
}

fn find_via_wmctrl(title: &str) -> Option<(String, i32, i32, u32, u32)> {
    let out = std::process::Command::new("wmctrl")
        .args(["-l", "-G"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let title_lower = title.to_lowercase();
    for line in text.lines() {
        // columns: id  desktop  x  y  w  h  hostname  title...
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 8 {
            let win_title = cols[7..].join(" ").to_lowercase();
            if win_title.contains(&title_lower) {
                let x: i32 = cols[2].parse().unwrap_or(0);
                let y: i32 = cols[3].parse().unwrap_or(0);
                let w: u32 = cols[4].parse().unwrap_or(0);
                let h: u32 = cols[5].parse().unwrap_or(0);
                if w > 0 && h > 0 {
                    return Some((cols[0].to_string(), x, y, w, h));
                }
            }
        }
    }
    None
}

/// Resolve a region offset relative to the game window origin to absolute screen coords.
pub fn resolve_region(origin: (i32, i32), region: [i32; 4]) -> [i32; 4] {
    [region[0] + origin.0, region[1] + origin.1, region[2], region[3]]
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BaitPosition {
    Left,
    Center,
    Right,
}

/// Scan `fishing_mode_region` for the fishing UI indicator (bait icon).
/// Returns true when in fishing mode.
pub fn detect_fishing_mode(cfg: &FishingConfig) -> Result<bool> {
    hue_count_region(
        resolve_region(cfg.window_origin, cfg.fishing_mode_region),
        cfg.fishing_mode_hue_center,
        cfg.fishing_mode_hue_range,
        cfg.fishing_mode_min_saturation,
        cfg.fishing_mode_min_pixels,
    )
}

/// Scan `fishing_rod_region` for the rod slot indicator.
/// Returns true when no rod is equipped (rod slot shows an add/empty indicator).
pub fn detect_fishing_rod(cfg: &FishingConfig) -> Result<bool> {
    hue_count_region(
        resolve_region(cfg.window_origin, cfg.fishing_rod_region),
        cfg.fishing_rod_hue_center,
        cfg.fishing_rod_hue_range,
        cfg.fishing_rod_min_saturation,
        cfg.fishing_rod_min_pixels,
    )
}

/// Returns true when the reeling tension bar is visible (colored zones present).
/// Absence = fish escaped or fish caught.
pub fn detect_tension_bar(cfg: &FishingConfig) -> Result<bool> {
    hue_count_region(
        resolve_region(cfg.window_origin, cfg.tension_bar_region),
        cfg.tension_bar_hue_center,
        cfg.tension_bar_hue_range,
        cfg.tension_bar_min_saturation,
        cfg.tension_bar_min_pixels,
    )
}

/// Shared helper: count pixels matching hue/saturation in a region (captures screen internally).
fn hue_count_region(
    region: [i32; 4],
    hue_center: f32,
    hue_range: f32,
    min_sat: f32,
    min_pixels: u32,
) -> Result<bool> {
    let screenshot = capture_screen()?;
    Ok(hue_count_on_image(&screenshot, region, hue_center, hue_range, min_sat, min_pixels))
}

/// Same as `hue_count_region` but operates on an already-captured image.
fn hue_count_on_image(
    img: &RgbaImage,
    region: [i32; 4],
    hue_center: f32,
    hue_range: f32,
    min_sat: f32,
    min_pixels: u32,
) -> bool {
    let [rx, ry, rw, rh] = region;
    let img_w = img.width() as i32;
    let img_h = img.height() as i32;

    let x0 = rx.clamp(0, img_w - 1) as u32;
    let y0 = ry.clamp(0, img_h - 1) as u32;
    let x1 = (rx + rw).clamp(0, img_w) as u32;
    let y1 = (ry + rh).clamp(0, img_h) as u32;

    let mut count: u32 = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = img.get_pixel(x, y);
            let (h, s, _v) = rgb_to_hsv(p[0], p[1], p[2]);
            if s >= min_sat && hue_matches(h, hue_center, hue_range) {
                count += 1;
                if count >= min_pixels {
                    return true;
                }
            }
        }
    }
    false
}

/// Scan `detect_region` for the orange "!" bite indicator using HSV hue detection.
pub fn detect_bite(cfg: &FishingConfig) -> Result<bool> {
    hue_count_region(
        resolve_region(cfg.window_origin, cfg.detect_region),
        cfg.bite_hue_center,
        cfg.bite_hue_range,
        cfg.bite_min_saturation,
        cfg.bite_min_pixels,
    )
}

/// Scan `lure_region` for pixels matching the lure's hue (HSV-based, lighting-invariant).
/// Average the X positions of matching pixels → classify Left / Center / Right.
/// Returns Center when no matching pixels found (lure not visible).
pub fn detect_bait_position(cfg: &FishingConfig) -> Result<BaitPosition> {
    let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.lure_region);
    let screenshot = capture_screen().unwrap_or_else(|_| RgbaImage::new(1, 1));
    let img_w = screenshot.width() as i32;
    let img_h = screenshot.height() as i32;

    let x0 = rx.clamp(0, img_w - 1) as u32;
    let y0 = ry.clamp(0, img_h - 1) as u32;
    let x1 = (rx + rw).clamp(0, img_w) as u32;
    let y1 = (ry + rh).clamp(0, img_h) as u32;

    let mut sum_x: u64 = 0;
    let mut count: u64 = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = screenshot.get_pixel(x, y);
            let (h, s, _v) = rgb_to_hsv(p[0], p[1], p[2]);
            if s >= cfg.lure_min_saturation && hue_matches(h, cfg.lure_hue_center, cfg.lure_hue_range) {
                sum_x += x as u64;
                count += 1;
            }
        }
    }

    if count == 0 {
        return Ok(BaitPosition::Center);
    }

    let lure_x = (sum_x / count) as i32;
    let margin  = (rw as f32 * cfg.bait_center_margin_pct) as i32;
    let center_x = rx + rw / 2;
    if lure_x < center_x - margin {
        Ok(BaitPosition::Left)
    } else if lure_x > center_x + margin {
        Ok(BaitPosition::Right)
    } else {
        Ok(BaitPosition::Center)
    }
}

/// Detect if the fish-caught result screen is showing.
/// Dual condition (single screenshot):
///   1. ≥ fish_caught_min_pixels bright pixels in fish_caught_region
///   2. Orange back button gone (fishing mode UI replaced by result screen)
pub fn detect_fish_caught(cfg: &FishingConfig) -> Result<bool> {
    let screenshot = capture_screen()?;
    let img_w = screenshot.width() as i32;
    let img_h = screenshot.height() as i32;

    // Condition 1: enough bright pixels in the continue button region
    let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.fish_caught_region);
    let x0 = rx.clamp(0, img_w - 1) as u32;
    let y0 = ry.clamp(0, img_h - 1) as u32;
    let x1 = (rx + rw).clamp(0, img_w) as u32;
    let y1 = (ry + rh).clamp(0, img_h) as u32;

    if x1 <= x0 || y1 <= y0 {
        return Ok(false);
    }

    let mut bright: u32 = 0;
    'outer: {
        for y in y0..y1 {
            for x in x0..x1 {
                let p = screenshot.get_pixel(x, y);
                let luma = (p[0] as u32 + p[1] as u32 + p[2] as u32) / 3;
                if luma >= cfg.fish_caught_threshold as u32 {
                    bright += 1;
                    if bright >= cfg.fish_caught_min_pixels {
                        break 'outer;
                    }
                }
            }
        }
    }
    if bright < cfg.fish_caught_min_pixels {
        return Ok(false);
    }

    // Condition 2: orange back button absent → result screen replaced fishing UI
    let orange_visible = hue_count_on_image(
        &screenshot,
        resolve_region(cfg.window_origin, cfg.fishing_mode_region),
        cfg.fishing_mode_hue_center,
        cfg.fishing_mode_hue_range,
        cfg.fishing_mode_min_saturation,
        cfg.fishing_mode_min_pixels,
    );
    Ok(!orange_visible)
}

/// Convert RGB (0–255) to HSV (h: 0–360, s: 0–1, v: 0–1).
fn rgb_to_hsv(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;

    let h = if delta == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let h = if h < 0.0 { h + 360.0 } else { h };
    let s = if max == 0.0 { 0.0 } else { delta / max };
    (h, s, max)
}

/// True when `hue` is within `center ± range` degrees (handles 0/360 wrap).
fn hue_matches(hue: f32, center: f32, range: f32) -> bool {
    let diff = (hue - center).abs() % 360.0;
    let diff = if diff > 180.0 { 360.0 - diff } else { diff };
    diff <= range
}
