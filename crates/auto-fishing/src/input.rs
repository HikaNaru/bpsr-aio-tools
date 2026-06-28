use anyhow::Result;
use std::time::Duration;

pub struct InputController {
    window_id: String,
}

impl InputController {
    pub fn new(window_id: &str) -> Result<Self> {
        Ok(Self { window_id: window_id.to_string() })
    }

    /// Press and release a key. Uses `--window` to target game directly (no focus needed).
    pub fn press_key(&mut self, key: &str) -> Result<()> {
        let id = self.window_id.clone();
        if id.is_empty() {
            xdo(&["key", "--clearmodifiers", key])
        } else {
            xdo(&["key", "--window", &id, "--clearmodifiers", key])
        }
    }

    /// Press a key down (does not release). No --clearmodifiers to avoid releasing LMB.
    pub fn key_down(&mut self, key: &str) -> Result<()> {
        xdo(&["keydown", key])
    }

    /// Release a previously pressed key.
    pub fn key_up(&mut self, key: &str) -> Result<()> {
        xdo(&["keyup", key])
    }

    /// Click left mouse button at current cursor position.
    /// Uses ydotool (evdev level) so Wayland compositor sees it as real input —
    /// required to re-acquire pointer lock when cursor gets stuck visible.
    pub fn click_mouse_left(&mut self) -> Result<()> {
        ydo(&["click", "0xC0"])
    }

    /// Hold left mouse button for `duration_ms` then release.
    pub fn hold_mouse_left(&mut self, duration_ms: u64) -> Result<()> {
        ydo(&["click", "0x40"])?;
        std::thread::sleep(Duration::from_millis(duration_ms));
        ydo(&["click", "0x80"])?;
        Ok(())
    }

    pub fn mouse_down(&mut self) -> Result<()> {
        ydo(&["click", "0x40"])
    }

    pub fn mouse_up(&mut self) -> Result<()> {
        ydo(&["click", "0x80"])
    }

    /// Release then immediately press LMB.
    /// Re-asserts hold: XTest no-ops plain down when already down; ydotool always generates the event.
    pub fn mouse_repress(&mut self) -> Result<()> {
        ydo(&["click", "0x80"])?;
        ydo(&["click", "0x40"])
    }

    /// Move cursor to absolute screen position then left-click.
    pub fn click_at(&mut self, x: i32, y: i32) -> Result<()> {
        xdo(&["mousemove", "--sync", &x.to_string(), &y.to_string()])?;
        ydo(&["click", "0xC0"])?;
        Ok(())
    }
}

fn xdo(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("xdotool")
        .args(args)
        .status()?;
    if !status.success() {
        anyhow::bail!("xdotool {:?} failed", args);
    }
    Ok(())
}

fn ydo(args: &[&str]) -> Result<()> {
    let output = std::process::Command::new("ydotool")
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .output()?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ydotool {:?} failed: {}", args, stderr.trim());
    }
    Ok(())
}
