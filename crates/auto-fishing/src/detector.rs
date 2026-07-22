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

/// Read text from an absolute screen region via tesseract OCR.
/// Auto-detects dark/light background and produces black-on-white for tesseract.
/// `psm`: tesseract page segmentation mode string (e.g. "6" = block, "7" = single line).
/// Returns lowercase trimmed text, empty string on any failure.
fn ocr_region(screenshot: &RgbaImage, abs_region: [i32; 4], psm: &str) -> String {
    use std::io::Write;

    let [rx, ry, rw, rh] = abs_region;
    let img_w = screenshot.width() as i32;
    let img_h = screenshot.height() as i32;
    let x0 = rx.clamp(0, img_w - 1) as u32;
    let y0 = ry.clamp(0, img_h - 1) as u32;
    let x1 = (rx + rw).clamp(0, img_w) as u32;
    let y1 = (ry + rh).clamp(0, img_h) as u32;
    if x1 <= x0 || y1 <= y0 {
        return String::new();
    }
    let cw = x1 - x0;
    let ch = y1 - y0;

    // Avg luma → detect bg type. Dark bg has light text; light bg has dark text.
    let total = (cw * ch) as u64;
    let mut luma_sum: u64 = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = screenshot.get_pixel(x, y);
            luma_sum += (p[0] as u64 + p[1] as u64 + p[2] as u64) / 3;
        }
    }
    let avg_luma = luma_sum / total;
    let dark_bg = avg_luma < 128;

    // Build 3× black-on-white mono image for tesseract.
    const SCALE: u32 = 3;
    let mut mono = image::GrayImage::new(cw * SCALE, ch * SCALE);
    for sy in 0..ch {
        for sx in 0..cw {
            let p = screenshot.get_pixel(x0 + sx, y0 + sy);
            let luma = ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8;
            let is_text = if dark_bg { luma >= 160 } else { luma < 160 };
            let val = if is_text { 0u8 } else { 255u8 };
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    mono.put_pixel(sx * SCALE + dx, sy * SCALE + dy, image::Luma([val]));
                }
            }
        }
    }

    let mut png_bytes: Vec<u8> = Vec::new();
    if mono
        .write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png)
        .is_err()
    {
        return String::new();
    }

    let tessdata_dir = {
        let local = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("tessdata");
        if local.join("eng.traineddata").exists() { local }
        else { std::path::PathBuf::from("/usr/share/tessdata") }
    };

    let mut child = match std::process::Command::new("tesseract")
        .args(["stdin", "stdout", "--psm", psm, "--oem", "3",
               "-c", "tessedit_char_whitelist=ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789 /"])
        .env("TESSDATA_PREFIX", &tessdata_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return String::new(),
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&png_bytes);
    }

    match child.wait_with_output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).trim().to_lowercase(),
        Err(_) => String::new(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FishingRodArea {
    NoPole,     // "Add a Pole" visible — slot empty
    HasPole,    // rod name visible — rod equipped
    NotVisible, // no text — panel not open
}

/// Read `fishing_rod_region` text via OCR.
pub fn detect_fishing_rod(cfg: &FishingConfig) -> Result<FishingRodArea> {
    let screenshot = capture_screen()?;
    let abs_region = resolve_region(cfg.window_origin, cfg.fishing_rod_region);
    let text = ocr_region(&screenshot, abs_region, "6");
    // eprintln!("[rod-area-ocr] text={text:?}");
    if text.contains("add") {
        Ok(FishingRodArea::NoPole)
    } else if text.contains("fishing") {
        Ok(FishingRodArea::HasPole)
    } else {
        Ok(FishingRodArea::NotVisible)
    }
}

/// Detect "Click anywhere to close" text in `monthly_reward_region`. Captures own screenshot.
pub fn detect_monthly_reward_popup(cfg: &FishingConfig) -> Result<bool> {
    let screenshot = capture_screen()?;
    Ok(detect_monthly_reward_popup_on_image(cfg, &screenshot))
}

/// Same check against an already-captured screenshot (avoid double capture).
pub fn detect_monthly_reward_popup_on_image(cfg: &FishingConfig, screenshot: &RgbaImage) -> bool {
    let abs_region = resolve_region(cfg.window_origin, cfg.monthly_reward_region);
    let text = ocr_region(screenshot, abs_region, "7");
    text.contains("click") && text.contains("close")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RodUseButton {
    Using,
    Use,
    Unknown,
}

/// Read `rod_use_region` button text via OCR.
/// Returns `Using` when rod is already active, `Use` when available to equip.
pub fn read_rod_use_button(cfg: &FishingConfig) -> Result<RodUseButton> {
    let screenshot = capture_screen()?;
    let abs_region = resolve_region(cfg.window_origin, cfg.rod_use_region);
    let text = ocr_region(&screenshot, abs_region, "7");
    // eprintln!("[rod-use-ocr] text={text:?}");
    if text.contains("using") {
        Ok(RodUseButton::Using)
    } else if text.contains("use") {
        Ok(RodUseButton::Use)
    } else {
        Ok(RodUseButton::Unknown)
    }
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

/// Same as `detect_tension_bar` but on a pre-captured image.
pub fn detect_tension_bar_on_image(cfg: &FishingConfig, img: &RgbaImage) -> bool {
    hue_count_on_image(
        img,
        resolve_region(cfg.window_origin, cfg.tension_bar_region),
        cfg.tension_bar_hue_center,
        cfg.tension_bar_hue_range,
        cfg.tension_bar_min_saturation,
        cfg.tension_bar_min_pixels,
    )
}

/// Read tension percentage (0–100) from a pre-captured image via OCR.
/// Crops `tension_pct_region`, isolates orange text → white-on-black, pipes PNG to tesseract stdin.
/// Returns 0.0 on any failure.
pub fn measure_tension_pct_on_image(cfg: &FishingConfig, img: &RgbaImage) -> f32 {
    use std::io::Write;

    let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.tension_pct_region);
    let img_w = img.width() as i32;
    let img_h = img.height() as i32;
    let x0 = rx.clamp(0, img_w - 1) as u32;
    let y0 = ry.clamp(0, img_h - 1) as u32;
    let x1 = (rx + rw).clamp(0, img_w) as u32;
    let y1 = (ry + rh).clamp(0, img_h) as u32;
    if x1 <= x0 || y1 <= y0 {
        return 0.0;
    }

    // Preprocess: bright pixels (text) → white, dark (background) → black.
    // Use brightness threshold — avoids dependency on hue calibration for UI text.
    let cw = x1 - x0;
    let ch = y1 - y0;
    const SCALE: u32 = 3;
    let mut mono = image::GrayImage::new(cw * SCALE, ch * SCALE);
    for sy in 0..ch {
        for sx in 0..cw {
            let p = img.get_pixel(x0 + sx, y0 + sy);
            let (_h, _s, v) = rgb_to_hsv(p[0], p[1], p[2]);
            let val = if v >= 0.55 { 255u8 } else { 0u8 };
            for dy in 0..SCALE {
                for dx in 0..SCALE {
                    mono.put_pixel(sx * SCALE + dx, sy * SCALE + dy, image::Luma([val]));
                }
            }
        }
    }

    // Encode to PNG in memory.
    let mut png_bytes: Vec<u8> = Vec::new();
    if mono
        .write_to(
            &mut std::io::Cursor::new(&mut png_bytes),
            image::ImageFormat::Png,
        )
        .is_err()
    {
        return 0.0;
    }

    // Resolve tessdata dir: project-local first, then system fallback.
    let tessdata_dir = {
        let local = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."))
            .join("tessdata");
        if local.join("eng.traineddata").exists() {
            local
        } else {
            std::path::PathBuf::from("/usr/share/tessdata")
        }
    };

    // Pipe PNG to tesseract stdin: psm 7 (single line), digits + % whitelist.
    let mut child = match std::process::Command::new("tesseract")
        .args(["stdin", "stdout", "--psm", "7", "--oem", "3",
               "-c", "tessedit_char_whitelist=0123456789%"])
        .env("TESSDATA_PREFIX", &tessdata_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => { eprintln!("[tension-ocr] spawn failed: {e}"); return 0.0; }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(&png_bytes);
    }

    let output = match child.wait_with_output() {
        Ok(o) => o,
        Err(e) => { eprintln!("[tension-ocr] wait failed: {e}"); return 0.0; }
    };

    let text = String::from_utf8_lossy(&output.stdout);

    // Anchor on '%': collect digits immediately before it, then strip leading digits
    // until value fits 0–100 (handles spurious prefix chars like "114%" → "14%").
    let trimmed = text.trim();
    let digit_str: String = if let Some(pct) = trimmed.find('%') {
        trimmed[..pct].chars().filter(|c| c.is_ascii_digit()).collect()
    } else {
        trimmed.chars().filter(|c| c.is_ascii_digit()).collect()
    };

    let mut s = digit_str.as_str();
    loop {
        if s.is_empty() { return 0.0; }
        if let Ok(v) = s.parse::<f32>() {
            if v <= 100.0 { return v; }
        }
        s = &s[1..]; // strip one leading digit and retry
    }
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
    hue_value_count_on_image(img, region, hue_center, hue_range, min_sat, 0.0, min_pixels)
}

/// Like `hue_count_on_image` but also gates on a minimum HSV value (brightness).
/// Pass `min_value = 0.0` to skip the brightness gate (same as `hue_count_on_image`).
fn hue_value_count_on_image(
    img: &RgbaImage,
    region: [i32; 4],
    hue_center: f32,
    hue_range: f32,
    min_sat: f32,
    min_value: f32,
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
            let (h, s, v) = rgb_to_hsv(p[0], p[1], p[2]);
            if s >= min_sat && v >= min_value && hue_matches(h, hue_center, hue_range) {
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

/// Check each directional arrow region for the steering indicator color.
/// Left arrow visible → Left; right arrow visible → Right; neither → Center.
pub fn detect_bait_position(cfg: &FishingConfig) -> Result<BaitPosition> {
    let screenshot = capture_screen()?;
    Ok(detect_bait_position_on_image(cfg, &screenshot))
}

/// Same as `detect_bait_position` but on a pre-captured image.
pub fn detect_bait_position_on_image(cfg: &FishingConfig, img: &RgbaImage) -> BaitPosition {
    if hue_value_count_on_image(
        img,
        resolve_region(cfg.window_origin, cfg.left_arrow_region),
        cfg.arrow_hue_center,
        cfg.arrow_hue_range,
        cfg.arrow_min_saturation,
        cfg.arrow_min_value,
        cfg.arrow_min_pixels,
    ) {
        return BaitPosition::Left;
    }

    if hue_value_count_on_image(
        img,
        resolve_region(cfg.window_origin, cfg.right_arrow_region),
        cfg.arrow_hue_center,
        cfg.arrow_hue_range,
        cfg.arrow_min_saturation,
        cfg.arrow_min_value,
        cfg.arrow_min_pixels,
    ) {
        return BaitPosition::Right;
    }

    BaitPosition::Center
}

/// Merged bounding box (absolute screen coords) covering both arrow regions.
/// Arrow thread captures only this small rect instead of the full screen.
pub fn arrow_capture_region(cfg: &FishingConfig) -> [i32; 4] {
    let l = resolve_region(cfg.window_origin, cfg.left_arrow_region);
    let r = resolve_region(cfg.window_origin, cfg.right_arrow_region);
    let x0 = l[0].min(r[0]);
    let y0 = l[1].min(r[1]);
    let x1 = (l[0] + l[2]).max(r[0] + r[2]);
    let y1 = (l[1] + l[3]).max(r[1] + r[3]);
    [x0, y0, x1 - x0, y1 - y0]
}

/// Capture a specific screen region (absolute coords).
/// Tries grim -g (Wayland region), then scrot -a (X11), then full capture + crop.
pub fn capture_region(abs_region: [i32; 4]) -> Result<RgbaImage> {
    let [rx, ry, rw, rh] = abs_region;
    if rw <= 0 || rh <= 0 {
        anyhow::bail!("invalid region dimensions");
    }
    let tmp = std::env::temp_dir().join("bpsr_arrow_crop.png");
    let tmp_str = tmp.to_string_lossy();

    // grim -g "x,y WxH" — Wayland wlroots region grab (fastest path)
    let geom = format!("{},{} {}x{}", rx, ry, rw, rh);
    if std::process::Command::new("grim")
        .args(["-g", &geom, &*tmp_str])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return load_rgba(&tmp);
    }

    // scrot -a x,y,w,h — X11 region grab
    let area = format!("{},{},{},{}", rx, ry, rw, rh);
    if std::process::Command::new("scrot")
        .args(["-a", &area, &*tmp_str])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
    {
        return load_rgba(&tmp);
    }

    // Fallback: full capture + in-memory crop (always works)
    let full = capture_screen()?;
    let iw = full.width() as i32;
    let ih = full.height() as i32;
    let x0 = rx.clamp(0, iw - 1) as u32;
    let y0 = ry.clamp(0, ih - 1) as u32;
    let x1 = (rx + rw).clamp(0, iw) as u32;
    let y1 = (ry + rh).clamp(0, ih) as u32;
    if x1 <= x0 || y1 <= y0 {
        anyhow::bail!("arrow region out of screen bounds");
    }
    Ok(image::imageops::crop_imm(&full, x0, y0, x1 - x0, y1 - y0).to_image())
}

/// Detect bait position from a cropped image of the arrow bounding box.
/// `crop_origin` = absolute screen coords of the crop's top-left pixel.
pub fn detect_bait_position_from_crop(
    cfg: &FishingConfig,
    crop: &RgbaImage,
    crop_origin: (i32, i32),
) -> BaitPosition {
    let translate = |region: [i32; 4]| -> [i32; 4] {
        let abs = resolve_region(cfg.window_origin, region);
        [abs[0] - crop_origin.0, abs[1] - crop_origin.1, abs[2], abs[3]]
    };

    if hue_value_count_on_image(
        crop,
        translate(cfg.left_arrow_region),
        cfg.arrow_hue_center,
        cfg.arrow_hue_range,
        cfg.arrow_min_saturation,
        cfg.arrow_min_value,
        cfg.arrow_min_pixels,
    ) {
        return BaitPosition::Left;
    }

    if hue_value_count_on_image(
        crop,
        translate(cfg.right_arrow_region),
        cfg.arrow_hue_center,
        cfg.arrow_hue_range,
        cfg.arrow_min_saturation,
        cfg.arrow_min_value,
        cfg.arrow_min_pixels,
    ) {
        return BaitPosition::Right;
    }

    BaitPosition::Center
}

/// Detect if the fish-caught result screen is showing.
/// Dual condition (single screenshot):
///   1. ≥ fish_caught_min_pixels bright pixels in fish_caught_region
///   2. Orange back button gone (fishing mode UI replaced by result screen)
pub fn detect_fish_caught(cfg: &FishingConfig) -> Result<bool> {
    let screenshot = capture_screen()?;
    Ok(detect_fish_caught_on_image(cfg, &screenshot))
}

/// Same as `detect_fish_caught` but on a pre-captured image.
pub fn detect_fish_caught_on_image(cfg: &FishingConfig, screenshot: &RgbaImage) -> bool {
    let img_w = screenshot.width() as i32;
    let img_h = screenshot.height() as i32;

    let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.fish_caught_region);
    let x0 = rx.clamp(0, img_w - 1) as u32;
    let y0 = ry.clamp(0, img_h - 1) as u32;
    let x1 = (rx + rw).clamp(0, img_w) as u32;
    let y1 = (ry + rh).clamp(0, img_h) as u32;

    if x1 <= x0 || y1 <= y0 {
        return false;
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
        return false;
    }

    let orange_visible = hue_count_on_image(
        screenshot,
        resolve_region(cfg.window_origin, cfg.fishing_mode_region),
        cfg.fishing_mode_hue_center,
        cfg.fishing_mode_hue_range,
        cfg.fishing_mode_min_saturation,
        cfg.fishing_mode_min_pixels,
    );
    !orange_visible
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
