use crate::config::FishingConfig;
use anyhow::Result;

/// Captures a screen region and checks whether a color change exceeds the threshold.
/// Returns true when a fish bite is detected.
pub fn detect_bite(cfg: &FishingConfig, baseline: &[u8; 3]) -> Result<bool> {
    let [x, y, w, h] = cfg.detect_region;
    let monitors = xcap::Monitor::all()?;
    let Some(monitor) = monitors.first() else {
        return Ok(false);
    };

    let screenshot = monitor.capture_image()?;
    let width  = screenshot.width() as i32;
    let height = screenshot.height() as i32;

    // Sample center pixel of the detection region
    let px = (x + w / 2).clamp(0, width  - 1) as u32;
    let py = (y + h / 2).clamp(0, height - 1) as u32;

    let pixel = screenshot.get_pixel(px, py);
    let diff = color_diff([pixel[0], pixel[1], pixel[2]], *baseline);
    Ok(diff > cfg.color_threshold)
}

/// Sample the baseline color at the detection region center.
pub fn sample_baseline(cfg: &FishingConfig) -> Result<[u8; 3]> {
    let [x, y, w, h] = cfg.detect_region;
    let monitors = xcap::Monitor::all()?;
    let Some(monitor) = monitors.first() else {
        return Ok([0, 0, 0]);
    };
    let screenshot = monitor.capture_image()?;
    let width  = screenshot.width() as i32;
    let height = screenshot.height() as i32;
    let px = (x + w / 2).clamp(0, width  - 1) as u32;
    let py = (y + h / 2).clamp(0, height - 1) as u32;
    let p = screenshot.get_pixel(px, py);
    Ok([p[0], p[1], p[2]])
}

fn color_diff(a: [u8; 3], b: [u8; 3]) -> u8 {
    let dr = a[0].abs_diff(b[0]) as u32;
    let dg = a[1].abs_diff(b[1]) as u32;
    let db = a[2].abs_diff(b[2]) as u32;
    ((dr + dg + db) / 3) as u8
}
