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
            run(&["key", "--clearmodifiers", key])
        } else {
            run(&["key", "--window", &id, "--clearmodifiers", key])
        }
    }

    /// Press a key down (does not release). No --clearmodifiers to avoid releasing LMB.
    pub fn key_down(&mut self, key: &str) -> Result<()> {
        run(&["keydown", key])
    }

    /// Release a previously pressed key.
    pub fn key_up(&mut self, key: &str) -> Result<()> {
        run(&["keyup", key])
    }

    /// Click left mouse button at current cursor position.
    pub fn click_mouse_left(&mut self) -> Result<()> {
        run(&["click", "--clearmodifiers", "1"])
    }

    /// Hold left mouse button for `duration_ms` then release.
    pub fn hold_mouse_left(&mut self, duration_ms: u64) -> Result<()> {
        run(&["mousedown", "1"])?;
        std::thread::sleep(Duration::from_millis(duration_ms));
        run(&["mouseup", "1"])?;
        Ok(())
    }

    pub fn mouse_down(&mut self) -> Result<()> {
        run(&["mousedown", "1"])
    }

    pub fn mouse_up(&mut self) -> Result<()> {
        run(&["mouseup", "1"])
    }

    /// Move cursor to absolute screen position then left-click.
    pub fn click_at(&mut self, x: i32, y: i32) -> Result<()> {
        run(&["mousemove", "--sync", &x.to_string(), &y.to_string()])?;
        run(&["click", "--clearmodifiers", "1"])?;
        Ok(())
    }
}

fn run(args: &[&str]) -> Result<()> {
    let status = std::process::Command::new("xdotool")
        .args(args)
        .status()?;
    if !status.success() {
        anyhow::bail!("xdotool {:?} failed", args);
    }
    Ok(())
}
