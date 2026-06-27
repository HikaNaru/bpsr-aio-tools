use crate::bot::FishingState;
use crate::config::FishingConfig;
use crate::detector::{BaitPosition, capture_screen, check_window_focus, detect_bait_position_on_image, detect_fish_caught, detect_fish_caught_on_image, detect_fishing_mode, detect_fishing_rod, detect_tension_bar_on_image, find_game_window, focus_game_window, measure_tension_pct_on_image, resolve_region};
use core::module::{Module, ModuleContext};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use ui::theme;

pub struct FishingModule {
    config:        FishingConfig,
    enabled:       bool,
    state:         Arc<Mutex<FishingState>>,
    fish_count:    Arc<AtomicU32>,
    failed_count:  Arc<AtomicU32>,
    /// Tension bar fill percentage (0–100) updated each reeling frame.
    tension_pct:   Arc<AtomicU32>,
    bot_thread:    Option<std::thread::JoinHandle<()>>,
    stop_tx:       Option<std::sync::mpsc::Sender<()>>,
    debug_texture:    Option<egui::TextureHandle>,
    debug_error:      Option<String>,
    detected_window:  Option<(i32, i32, u32, u32)>,
    game_window_id:   String,
    paused:           Arc<AtomicBool>,
    last_window_size: (u32, u32),
    session_start:    Option<Instant>,
}

/// Scale all position/region offsets from one game content resolution to another.
fn scale_regions(cfg: &mut FishingConfig, from: (u32, u32), to: (u32, u32)) {
    if from == to { return; }
    let sx = to.0 as f64 / from.0 as f64;
    let sy = to.1 as f64 / from.1 as f64;
    fn sr(r: &mut [i32; 4], sx: f64, sy: f64) {
        r[0] = (r[0] as f64 * sx).round() as i32;
        r[1] = (r[1] as f64 * sy).round() as i32;
        r[2] = (r[2] as f64 * sx).round() as i32;
        r[3] = (r[3] as f64 * sy).round() as i32;
    }
    sr(&mut cfg.fishing_mode_region, sx, sy);
    sr(&mut cfg.fishing_rod_region, sx, sy);
    sr(&mut cfg.rod_use_region, sx, sy);
    sr(&mut cfg.detect_region, sx, sy);
    sr(&mut cfg.left_arrow_region, sx, sy);
    sr(&mut cfg.right_arrow_region, sx, sy);
    sr(&mut cfg.tension_bar_region, sx, sy);
    sr(&mut cfg.tension_pct_region, sx, sy);
    sr(&mut cfg.fish_caught_region, sx, sy);
}

impl FishingModule {
    pub fn new() -> Self {
        let mut config = FishingConfig::load();
        const BASE: (u32, u32) = (1600, 900);
        let mut game_window_id = String::new();
        let mut detected_window = None;
        let last_window_size = if let Some((id, wx, wy, ww, wh)) =
            find_game_window(&config.game_window_title)
        {
            config.window_origin = (wx, wy);
            scale_regions(&mut config, BASE, (ww, wh));
            game_window_id = id;
            detected_window = Some((wx, wy, ww, wh));
            (ww, wh)
        } else {
            BASE
        };
        Self {
            config,
            enabled:       false,
            state:         Arc::new(Mutex::new(FishingState::CheckingState)),
            fish_count:    Arc::new(AtomicU32::new(0)),
            failed_count:  Arc::new(AtomicU32::new(0)),
            tension_pct:   Arc::new(AtomicU32::new(0)),
            bot_thread:    None,
            stop_tx:       None,
            debug_texture:   None,
            debug_error:     None,
            detected_window,
            game_window_id,
            paused:          Arc::new(AtomicBool::new(false)),
            last_window_size,
            session_start:   None,
        }
    }

    /// Apply a newly detected window: scale config offsets, update origin + size.
    fn apply_window_detection(&mut self, id: String, wx: i32, wy: i32, ww: u32, wh: u32) {
        scale_regions(&mut self.config, self.last_window_size, (ww, wh));
        self.config.window_origin = (wx, wy);
        self.detected_window = Some((wx, wy, ww, wh));
        self.game_window_id = id;
        self.last_window_size = (ww, wh);
    }

    fn focus_game(&self) {
        focus_game_window(&self.game_window_id);
        if let Some((wx, wy, ww, wh)) = self.detected_window {
            let cx = wx + ww as i32 / 2;
            let cy = wy + wh as i32 / 2;
            let _ = std::process::Command::new("xdotool")
                .args(["mousemove", &cx.to_string(), &cy.to_string()])
                .status();
        }
    }

    fn start_bot(&mut self) {
        self.focus_game();

        self.fish_count.store(0, Ordering::Relaxed);
        self.failed_count.store(0, Ordering::Relaxed);
        self.session_start = Some(Instant::now());

        self.paused.store(false, Ordering::Relaxed);
        let paused_arc       = Arc::clone(&self.paused);
        let window_id        = self.game_window_id.clone();
        let fish_count_arc   = Arc::clone(&self.fish_count);
        let failed_count_arc = Arc::clone(&self.failed_count);
        let tension_pct_arc  = Arc::clone(&self.tension_pct);

        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let state_arc = Arc::clone(&self.state);
        let mut cfg = self.config.clone();

        let handle = std::thread::Builder::new()
            .name("auto-fishing".into())
            .spawn(move || {
                let mut input = match crate::input::InputController::new(&window_id) {
                    Ok(i) => i,
                    Err(e) => {
                        tracing::error!("input init failed: {e}");
                        return;
                    }
                };

                let mut focus_tick = 0u32;
                let mut lmb_held = false;
                let mut held_steer_key: Option<String> = None;
                let mut bar_absent_streak: u32 = 0;
                // True while tension is >= 80% and cooling down; resume when < 50%.
                let mut tension_cooldown = false;
                'outer: loop {
                    if stop_rx.try_recv().is_ok() {
                        break 'outer;
                    }

                    focus_tick += 1;
                    if focus_tick >= 5 {
                        focus_tick = 0;
                        paused_arc.store(!check_window_focus(&window_id), Ordering::Relaxed);
                    }

                    if paused_arc.load(Ordering::Relaxed) {
                        if let Some(ref key) = held_steer_key { let _ = input.key_up(key); }
                        held_steer_key = None;
                        if lmb_held {
                            eprintln!("[auto-fishing] OUTER PAUSED: releasing LMB (focus lost, window_id='{window_id}')");
                            let _ = input.mouse_up();
                            lmb_held = false;
                        }
                        std::thread::sleep(Duration::from_millis(200));
                        continue;
                    }

                    let current = state_arc.lock().unwrap().clone();

                    match current {
                        FishingState::CheckingState => {
                            if let Some((_, wx, wy, _, _)) = find_game_window(&cfg.game_window_title) {
                                cfg.window_origin = (wx, wy);
                            }
                            std::thread::sleep(Duration::from_millis(300));

                            // Fish caught screen may still be visible after CPU-spike delay.
                            // Click continue again and wait longer before re-checking.
                            if detect_fish_caught(&cfg).unwrap_or(false) {
                                eprintln!("[auto-fishing] CheckingState: fish caught screen still visible, retrying continue click");
                                let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.fish_caught_region);
                                let _ = input.click_at(rx + rw / 2, ry + rh / 2);
                                *state_arc.lock().unwrap() = FishingState::Cooldown {
                                    until: Instant::now() + Duration::from_millis(2000),
                                };
                                continue;
                            }

                            let in_mode = detect_fishing_mode(&cfg).unwrap_or(false);
                            if !in_mode {
                                *state_arc.lock().unwrap() = FishingState::EnteringFishingMode;
                            } else if detect_fishing_rod(&cfg).unwrap_or(false) {
                                *state_arc.lock().unwrap() = FishingState::SelectingRod;
                            } else {
                                *state_arc.lock().unwrap() = FishingState::Idle;
                            }
                        }
                        FishingState::EnteringFishingMode => {
                            eprintln!("[auto-fishing] Entering fishing mode (pressing '{}')", cfg.fishing_key);
                            let _ = input.press_key(&cfg.fishing_key);
                            // Poll up to 10s for fishing mode confirmation.
                            // On timeout → CheckingState (retry F press).
                            let deadline = Instant::now() + Duration::from_secs(10);
                            let mut confirmed = false;
                            loop {
                                std::thread::sleep(Duration::from_millis(300));
                                if stop_rx.try_recv().is_ok() {
                                    break 'outer;
                                }
                                if detect_fishing_mode(&cfg).unwrap_or(false) {
                                    confirmed = true;
                                    break;
                                }
                                if Instant::now() >= deadline {
                                    break;
                                }
                            }
                            if confirmed {
                                if detect_fishing_rod(&cfg).unwrap_or(false) {
                                    *state_arc.lock().unwrap() = FishingState::SelectingRod;
                                } else {
                                    *state_arc.lock().unwrap() = FishingState::Idle;
                                }
                            } else {
                                *state_arc.lock().unwrap() = FishingState::CheckingState;
                            }
                        }
                        FishingState::SelectingRod => {
                            eprintln!("[auto-fishing] Checking rod (no rod equipped, opening equipment)");
                            let _ = input.press_key(&cfg.rod_slot_key);
                            std::thread::sleep(Duration::from_millis(800));
                            eprintln!("[auto-fishing] Using rod");
                            let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.rod_use_region);
                            let _ = input.click_at(rx + rw / 2, ry + rh / 2);
                            std::thread::sleep(Duration::from_millis(500));
                            // Close menu regardless — same key as open (toggle).
                            let _ = input.press_key(&cfg.rod_slot_key);
                            std::thread::sleep(Duration::from_millis(1000));
                            *state_arc.lock().unwrap() = FishingState::Idle;
                        }
                        FishingState::Idle => {
                            eprintln!("[auto-fishing] Casting");
                            let _ = input.click_mouse_left();
                            *state_arc.lock().unwrap() = FishingState::Casting;
                        }
                        FishingState::Casting => {
                            // Poll for rod UI to disappear — signals line is in water.
                            // cast_delay_ms is the max wait before assuming cast succeeded.
                            let deadline = Instant::now() + Duration::from_millis(cfg.cast_delay_ms);
                            loop {
                                std::thread::sleep(Duration::from_millis(100));
                                if stop_rx.try_recv().is_ok() { break 'outer; }
                                // Rod UI absent = cast successful, line in water
                                if !detect_fishing_rod(&cfg).unwrap_or(true) {
                                    break;
                                }
                                if Instant::now() >= deadline {
                                    break;
                                }
                            }
                            eprintln!("[auto-fishing] Waiting for bite");
                            *state_arc.lock().unwrap() =
                                FishingState::WaitingBite { cast_at: Instant::now() };
                        }
                        FishingState::WaitingBite { cast_at } => {
                            if cast_at.elapsed() > Duration::from_millis(cfg.bite_timeout_ms) {
                                // Re-check fishing mode on timeout; if not in mode (false positive
                                // slipped through CheckingState), re-enter properly.
                                if !detect_fishing_mode(&cfg).unwrap_or(true) {
                                    *state_arc.lock().unwrap() = FishingState::CheckingState;
                                } else {
                                    *state_arc.lock().unwrap() = FishingState::Idle;
                                }
                                continue;
                            }
                            if crate::detector::detect_bite(&cfg).unwrap_or(false) {
                                let _ = input.click_mouse_left();
                                *state_arc.lock().unwrap() =
                                    FishingState::Reeling { started_at: Instant::now() };
                            } else {
                                std::thread::sleep(Duration::from_millis(50));
                            }
                        }
                        FishingState::Reeling { started_at } => {
                            let elapsed = started_at.elapsed();
                            let timed_out = elapsed > Duration::from_millis(cfg.reel_timeout_ms);

                            // Hard timeout — release everything, check result.
                            if timed_out {
                                if let Some(ref key) = held_steer_key { let _ = input.key_up(key); }
                                held_steer_key = None;
                                if lmb_held { let _ = input.mouse_up(); lmb_held = false; }
                                tension_cooldown = false;
                                tension_pct_arc.store(0, Ordering::Relaxed);
                                eprintln!("[auto-fishing] exit: TIMEOUT at {}ms", elapsed.as_millis());
                                std::thread::sleep(Duration::from_millis(300));
                                if detect_fish_caught(&cfg).unwrap_or(false) {
                                    *state_arc.lock().unwrap() = FishingState::FishCaught;
                                } else {
                                    eprintln!("[auto-fishing] Fish escaped (timeout)");
                                    failed_count_arc.fetch_add(1, Ordering::Relaxed);
                                    *state_arc.lock().unwrap() = FishingState::Cooldown {
                                        until: Instant::now() + Duration::from_millis(cfg.cooldown_ms),
                                    };
                                }
                                continue;
                            }

                            // One screenshot covers all checks this iteration — arrow indicator
                            // is visible for only ~1s; separate captures would miss it.
                            let screenshot = match capture_screen() {
                                Ok(s) => s,
                                Err(_) => { std::thread::sleep(Duration::from_millis(50)); continue; }
                            };

                            // Measure and publish tension percentage.
                            let pct = measure_tension_pct_on_image(&cfg, &screenshot);
                            tension_pct_arc.store(pct.round() as u32, Ordering::Relaxed);
                            eprintln!("[auto-fishing] tension: {pct:.0}%");

                            // Tension gate: release at ≥80%, resume at <50%.
                            if pct >= 80.0 {
                                if lmb_held {
                                    let _ = input.mouse_up();
                                    lmb_held = false;
                                    tension_cooldown = true;
                                    eprintln!("[auto-fishing] tension {pct:.0}% >= 80%: releasing LMB");
                                }
                            } else if tension_cooldown && pct < 50.0 {
                                tension_cooldown = false;
                                eprintln!("[auto-fishing] tension {pct:.0}% < 50%: resuming LMB");
                            }

                            // LMB management — only hold when not in tension cooldown.
                            if !tension_cooldown {
                                if !lmb_held {
                                    eprintln!("[auto-fishing] holding LMB");
                                    let _ = input.mouse_down();
                                    lmb_held = true;
                                    held_steer_key = None;
                                    bar_absent_streak = 0;
                                } else {
                                    // Re-assert every iteration — XTest state may not persist on Xwayland.
                                    let _ = input.mouse_down();
                                }
                            }

                            // Primary exit: fish-caught screen appeared (skip first 1s).
                            if elapsed > Duration::from_millis(1000)
                                && detect_fish_caught_on_image(&cfg, &screenshot)
                            {
                                if let Some(ref key) = held_steer_key { let _ = input.key_up(key); }
                                held_steer_key = None;
                                if lmb_held { let _ = input.mouse_up(); lmb_held = false; }
                                tension_cooldown = false;
                                tension_pct_arc.store(0, Ordering::Relaxed);
                                eprintln!("[auto-fishing] exit: FISH_CAUGHT detected at {}ms", elapsed.as_millis());
                                *state_arc.lock().unwrap() = FishingState::FishCaught;
                                continue;
                            }

                            // Secondary exit: tension bar absent 10+ reads after 2s grace.
                            if elapsed > Duration::from_millis(2000) {
                                if !detect_tension_bar_on_image(&cfg, &screenshot) {
                                    bar_absent_streak += 1;
                                } else {
                                    bar_absent_streak = 0;
                                }
                                if bar_absent_streak >= 10 {
                                    if let Some(ref key) = held_steer_key { let _ = input.key_up(key); }
                                    held_steer_key = None;
                                    if lmb_held { let _ = input.mouse_up(); lmb_held = false; }
                                    tension_cooldown = false;
                                    tension_pct_arc.store(0, Ordering::Relaxed);
                                    eprintln!("[auto-fishing] STOPPED: bar_absent_streak={bar_absent_streak} (tension bar absent exit fired — LMB released here)");
                                    std::thread::sleep(Duration::from_millis(300));
                                    if detect_fish_caught(&cfg).unwrap_or(false) {
                                        *state_arc.lock().unwrap() = FishingState::FishCaught;
                                    } else {
                                        eprintln!("[auto-fishing] Fish escaped (tension bar absent)");
                                        failed_count_arc.fetch_add(1, Ordering::Relaxed);
                                        *state_arc.lock().unwrap() = FishingState::Cooldown {
                                            until: Instant::now() + Duration::from_millis(cfg.cooldown_ms),
                                        };
                                    }
                                    continue;
                                }
                            } else {
                                bar_absent_streak = 0;
                            }

                            // Focus check — pause if window lost focus.
                            if paused_arc.load(Ordering::Relaxed) {
                                if let Some(ref key) = held_steer_key { let _ = input.key_up(key); }
                                held_steer_key = None;
                                if lmb_held { let _ = input.mouse_up(); lmb_held = false; }
                                tension_cooldown = false;
                                eprintln!("[auto-fishing] exit: PAUSED (focus lost) at {}ms", elapsed.as_millis());
                                continue;
                            }

                            // Steer — always active, regardless of tension cooldown.
                            {
                                let desired: Option<&str> = match detect_bait_position_on_image(&cfg, &screenshot) {
                                    BaitPosition::Left   => Some("a"),
                                    BaitPosition::Right  => Some("d"),
                                    BaitPosition::Center => None,
                                };
                                match (held_steer_key.as_deref(), desired) {
                                    (Some(cur), Some(want)) if cur == want => {
                                        // Resend keydown — Wayland XTest state does not persist.
                                        let _ = input.key_down(cur);
                                    }
                                    (Some(cur), Some(want)) => {
                                        let _ = input.key_up(cur);
                                        let _ = input.key_down(want);
                                        eprintln!("[auto-fishing] steer: switch '{cur}' → '{want}'");
                                        held_steer_key = Some(want.to_string());
                                    }
                                    (Some(cur), None) => {
                                        // Center: keep holding previous key (Wayland resend).
                                        let _ = input.key_down(cur);
                                    }
                                    (None, Some(want)) => {
                                        let _ = input.key_down(want);
                                        eprintln!("[auto-fishing] steer: hold '{want}'");
                                        held_steer_key = Some(want.to_string());
                                    }
                                    (None, None) => {}
                                }
                            }
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        FishingState::FishCaught => {
                            eprintln!("[auto-fishing] Fish caught! Clicking continue");
                            fish_count_arc.fetch_add(1, Ordering::Relaxed);
                            std::thread::sleep(Duration::from_millis(500));
                            let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.fish_caught_region);
                            let _ = input.click_at(rx + rw / 2, ry + rh / 2);
                            *state_arc.lock().unwrap() = FishingState::Cooldown {
                                until: Instant::now() + Duration::from_millis(cfg.cooldown_ms),
                            };
                        }
                        FishingState::Cooldown { until } => {
                            let remaining = until.duration_since(Instant::now());
                            if remaining.is_zero() {
                                // Recheck rod after each fish escape/catch before next cast
                                *state_arc.lock().unwrap() = FishingState::CheckingState;
                            } else {
                                std::thread::sleep(remaining.min(Duration::from_millis(50)));
                            }
                        }
                    }
                }
                if let Some(ref key) = held_steer_key { let _ = input.key_up(key); }
                if lmb_held {
                    let _ = input.mouse_up();
                }
                *state_arc.lock().unwrap() = FishingState::CheckingState;
            })
            .expect("fishing thread");

        self.bot_thread = Some(handle);
        self.stop_tx    = Some(stop_tx);
    }

    fn stop_bot(&mut self) {
        if let Some(tx) = self.stop_tx.take() {
            let _ = tx.send(());
        }
    }
}

fn draw_rect_border(img: &mut image::RgbaImage, region: [i32; 4], color: [u8; 4]) {
    let [rx, ry, rw, rh] = region;
    let iw = img.width() as i32;
    let ih = img.height() as i32;
    let x0 = rx.clamp(0, iw - 1);
    let y0 = ry.clamp(0, ih - 1);
    let x1 = (rx + rw - 1).clamp(0, iw - 1);
    let y1 = (ry + rh - 1).clamp(0, ih - 1);
    let px = image::Rgba(color);
    let t = 3i32;
    for x in x0..=x1 {
        for d in 0..t {
            let yu = (y0 + d).clamp(0, ih - 1) as u32;
            let yd = (y1 - d).clamp(0, ih - 1) as u32;
            img.put_pixel(x as u32, yu, px);
            img.put_pixel(x as u32, yd, px);
        }
    }
    for y in y0..=y1 {
        for d in 0..t {
            let xl = (x0 + d).clamp(0, iw - 1) as u32;
            let xr = (x1 - d).clamp(0, iw - 1) as u32;
            img.put_pixel(xl, y as u32, px);
            img.put_pixel(xr, y as u32, px);
        }
    }
}

fn capture_debug_preview(
    cfg: &FishingConfig,
    detected_window: Option<(i32, i32, u32, u32)>,
    ctx: &egui::Context,
) -> Result<egui::TextureHandle, String> {
    let screenshot = crate::detector::capture_screen().map_err(|e| e.to_string())?;

    // Crop to game window bounds when known; otherwise use full screen.
    let (mut img, draw_origin) = if let Some((wx, wy, ww, wh)) = detected_window {
        let iw = screenshot.width() as i32;
        let ih = screenshot.height() as i32;
        let cx = wx.clamp(0, iw - 1) as u32;
        let cy = wy.clamp(0, ih - 1) as u32;
        let cw = ww.min((iw - wx.max(0)) as u32);
        let ch = wh.min((ih - wy.max(0)) as u32);
        (image::imageops::crop_imm(&screenshot, cx, cy, cw, ch).to_image(), (0i32, 0i32))
    } else {
        (screenshot, cfg.window_origin)
    };

    // Border coords: offsets + draw_origin (= (0,0) when cropped, window_origin otherwise).
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.fishing_mode_region),  [220, 100, 255, 255]);
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.fishing_rod_region),   [255, 220,   0, 255]);
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.detect_region),        [255, 150,   0, 255]);
    // Rod use button region (green)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.rod_use_region),    [100, 255, 100, 255]);
    // Tension bar region (cyan)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.tension_bar_region), [0, 220, 200, 255]);
    // Tension pct region (teal)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.tension_pct_region), [0, 200, 140, 255]);
    // Continue fishing button region (blue)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.fish_caught_region), [50, 180, 255, 255]);
    // Left arrow region (magenta)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.left_arrow_region),  [255, 80, 200, 255]);
    // Right arrow region (pink)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.right_arrow_region), [255, 160, 220, 255]);

    let save_path = {
        let mut p = std::env::current_dir()
            .unwrap_or_else(|_| std::path::PathBuf::from("."));
        p.push("tmp");
        let _ = std::fs::create_dir_all(&p);
        p.push("fishing_debug.png");
        p
    };
    if let Err(e) = img.save(&save_path) {
        tracing::warn!("debug save failed: {e}");
    }

    let w = (img.width() / 2).max(1);
    let h = (img.height() / 2).max(1);
    let small = image::imageops::resize(&img, w, h, image::imageops::FilterType::Nearest);

    let size = [small.width() as usize, small.height() as usize];
    let pixels: Vec<egui::Color32> = small
        .pixels()
        .map(|p| egui::Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
        .collect();
    let color_image = egui::ColorImage { size, pixels };
    Ok(ctx.load_texture("debug-preview", color_image, egui::TextureOptions::default()))
}

fn state_color(state: &FishingState) -> egui::Color32 {
    match state {
        FishingState::Idle | FishingState::FishCaught         => theme::GOOD,
        FishingState::WaitingBite { .. } | FishingState::Reeling { .. } => theme::ACCENT2,
        FishingState::Cooldown { .. }                          => theme::WARN,
        _                                                      => theme::ACCENT,
    }
}

impl Module for FishingModule {
    fn id(&self)   -> &'static str { "auto-fishing" }
    fn name(&self) -> &str         { "Auto Fishing" }
    fn icon(&self) -> &str         { "🎣" }

    fn update(&mut self, _ctx: &ModuleContext) {}

    fn on_enable(&mut self)  { self.start_bot(); }
    fn on_disable(&mut self) { self.stop_bot(); }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        let paused      = self.paused.load(Ordering::Relaxed);
        let state_guard = self.state.lock().unwrap();
        let (dot_color, state_label) = if self.enabled && paused {
            (theme::BAD, "Paused (game not focused)")
        } else {
            (state_color(&state_guard), state_guard.label())
        };
        drop(state_guard);

        ui.heading("Auto Fishing");
        ui.separator();

        // ── Status dot + label ───────────────────────────────────────────
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, dot_color);
            ui.colored_label(dot_color, state_label);

            let tension = self.tension_pct.load(Ordering::Relaxed);
            if tension > 0 {
                ui.separator();
                let t_color = if tension >= 80 {
                    theme::BAD
                } else if tension >= 50 {
                    theme::WARN
                } else {
                    theme::GOOD
                };
                ui.colored_label(t_color, format!("Tension: {tension}%"));
            }
        });

        // ── Session stats ────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let caught = self.fish_count.load(Ordering::Relaxed);
            let failed = self.failed_count.load(Ordering::Relaxed);
            ui.colored_label(theme::GOOD, format!("Caught: {caught}"));
            ui.colored_label(theme::BAD,  format!("Failed: {failed}"));
            if ui.small_button("Reset").clicked() {
                self.fish_count.store(0, Ordering::Relaxed);
                self.failed_count.store(0, Ordering::Relaxed);
                self.session_start = None;
            }
            if let Some(start) = self.session_start {
                let secs  = start.elapsed().as_secs();
                let h     = secs / 3600;
                let m     = (secs % 3600) / 60;
                let s     = secs % 60;
                ui.separator();
                ui.colored_label(theme::TEXT_MUTED, format!("{h:02}:{m:02}:{s:02}"));
                if secs > 30 {
                    let rate = caught as f64 / (secs as f64 / 3600.0);
                    ui.colored_label(theme::TEXT_MUTED, format!("{rate:.1}/hr"));
                }
                let total = caught + failed;
                if total > 0 {
                    let pct = caught as f64 / total as f64 * 100.0;
                    let col = if pct >= 80.0 { theme::GOOD } else if pct >= 50.0 { theme::WARN } else { theme::BAD };
                    ui.colored_label(col, format!("{pct:.0}%"));
                }
            }
        });

        ui.separator();

        // ── Controls ─────────────────────────────────────────────────────
        if self.enabled && paused {
            ui.horizontal(|ui| {
                if ui.button("▶ Resume").clicked() {
                    self.focus_game();
                    self.paused.store(false, Ordering::Relaxed);
                }
                if ui.button("⏹ Stop").clicked() {
                    self.enabled = false;
                    self.on_disable();
                }
            });
        } else if self.enabled {
            if ui.button("⏹ Stop").clicked() {
                self.enabled = false;
                self.on_disable();
            }
        } else if ui.button("▶ Start Fishing").clicked() {
            self.enabled = true;
            self.on_enable();
        }

        // ── Settings ─────────────────────────────────────────────────────
        ui.separator();
        ui.collapsing("Settings", |ui| {
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .id_salt("fishing_settings_scroll")
                .show(ui, |ui| {

                    // Game Window & Hotkeys
                    ui.collapsing("Game Window & Hotkeys", |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Window title:");
                            ui.text_edit_singleline(&mut self.config.game_window_title);
                        });
                        ui.horizontal(|ui| {
                            if ui.button("Detect window").clicked() {
                                if let Some((id, wx, wy, ww, wh)) =
                                    find_game_window(&self.config.game_window_title)
                                {
                                    self.apply_window_detection(id, wx, wy, ww, wh);
                                }
                            }
                            match self.detected_window {
                                Some((x, y, w, h)) => { ui.label(format!("({x},{y}) {w}×{h}")); }
                                None               => { ui.label("not detected"); }
                            }
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Enter fishing mode:");
                            ui.add(egui::TextEdit::singleline(&mut self.config.fishing_key).desired_width(40.0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Open rod equipment:");
                            ui.add(egui::TextEdit::singleline(&mut self.config.rod_slot_key).desired_width(40.0));
                        });
                    });

                    // Fishing Mode Entry
                    ui.collapsing("Fishing Mode Entry", |ui| {
                        let dv = |v: &mut i32, ui: &mut egui::Ui| {
                            ui.add(egui::DragValue::new(v).range(-200..=3840));
                        };
                        ui.horizontal(|ui| {
                            ui.label("Hue center (°):");
                            ui.add(egui::Slider::new(&mut self.config.fishing_mode_hue_center, 0.0..=360.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Hue range (±°):");
                            ui.add(egui::Slider::new(&mut self.config.fishing_mode_hue_range, 5.0..=60.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min saturation:");
                            ui.add(egui::Slider::new(&mut self.config.fishing_mode_min_saturation, 0.1..=1.0).fixed_decimals(2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min pixels:");
                            ui.add(egui::DragValue::new(&mut self.config.fishing_mode_min_pixels).range(1..=200));
                        });
                        ui.separator();
                        ui.label("Region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.fishing_mode_region { dv(v, ui); } });
                    });

                    // Rod Selection
                    ui.collapsing("Rod Selection", |ui| {
                        let dv = |v: &mut i32, ui: &mut egui::Ui| {
                            ui.add(egui::DragValue::new(v).range(-200..=3840));
                        };
                        ui.horizontal(|ui| {
                            ui.label("Hue center (°):");
                            ui.add(egui::Slider::new(&mut self.config.fishing_rod_hue_center, 0.0..=360.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Hue range (±°):");
                            ui.add(egui::Slider::new(&mut self.config.fishing_rod_hue_range, 5.0..=80.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min saturation:");
                            ui.add(egui::Slider::new(&mut self.config.fishing_rod_min_saturation, 0.1..=1.0).fixed_decimals(2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min pixels:");
                            ui.add(egui::DragValue::new(&mut self.config.fishing_rod_min_pixels).range(1..=200));
                        });
                        ui.separator();
                        ui.label("Rod detect region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.fishing_rod_region { dv(v, ui); } });
                        ui.label("Use rod button [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.rod_use_region { dv(v, ui); } });
                    });

                    // Bite Detection & Casting
                    ui.collapsing("Bite Detection & Casting", |ui| {
                        let dv = |v: &mut i32, ui: &mut egui::Ui| {
                            ui.add(egui::DragValue::new(v).range(-200..=3840));
                        };
                        ui.horizontal(|ui| {
                            ui.label("Hue center (°):");
                            ui.add(egui::Slider::new(&mut self.config.bite_hue_center, 0.0..=360.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Hue range (±°):");
                            ui.add(egui::Slider::new(&mut self.config.bite_hue_range, 5.0..=60.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min saturation:");
                            ui.add(egui::Slider::new(&mut self.config.bite_min_saturation, 0.1..=1.0).fixed_decimals(2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min pixels:");
                            ui.add(egui::DragValue::new(&mut self.config.bite_min_pixels).range(1..=500));
                        });
                        ui.separator();
                        ui.horizontal(|ui| {
                            ui.label("Cast delay (ms):");
                            ui.add(egui::DragValue::new(&mut self.config.cast_delay_ms).range(500..=5000));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Bite timeout (ms):");
                            ui.add(egui::DragValue::new(&mut self.config.bite_timeout_ms).range(5000..=60000));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Cooldown (ms):");
                            ui.add(egui::DragValue::new(&mut self.config.cooldown_ms).range(100..=3000));
                        });
                        ui.separator();
                        ui.label("Bite detect region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.detect_region { dv(v, ui); } });
                    });

                    // Reeling
                    ui.collapsing("Reeling", |ui| {
                        let dv = |v: &mut i32, ui: &mut egui::Ui| {
                            ui.add(egui::DragValue::new(v).range(-200..=3840));
                        };
                        ui.strong("Arrow indicators");
                        ui.horizontal(|ui| {
                            ui.label("Hue center (°):");
                            ui.add(egui::Slider::new(&mut self.config.arrow_hue_center, 0.0..=360.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Hue range (±°):");
                            ui.add(egui::Slider::new(&mut self.config.arrow_hue_range, 5.0..=60.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min saturation:");
                            ui.add(egui::Slider::new(&mut self.config.arrow_min_saturation, 0.1..=1.0).fixed_decimals(2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min value (brightness):");
                            ui.add(egui::Slider::new(&mut self.config.arrow_min_value, 0.0..=1.0).fixed_decimals(2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min pixels:");
                            ui.add(egui::DragValue::new(&mut self.config.arrow_min_pixels).range(1..=500));
                        });
                        ui.label("Left arrow region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.left_arrow_region { dv(v, ui); } });
                        ui.label("Right arrow region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.right_arrow_region { dv(v, ui); } });
                        ui.separator();
                        ui.strong("Timings");
                        ui.horizontal(|ui| {
                            ui.label("Reel timeout (ms):");
                            ui.add(egui::DragValue::new(&mut self.config.reel_timeout_ms).range(5000..=120_000));
                        });
                        ui.separator();
                        ui.strong("Tension bar");
                        ui.horizontal(|ui| {
                            ui.label("Hue center (°):");
                            ui.add(egui::Slider::new(&mut self.config.tension_bar_hue_center, 0.0..=360.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Hue range (±°):");
                            ui.add(egui::Slider::new(&mut self.config.tension_bar_hue_range, 5.0..=60.0).fixed_decimals(0));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min saturation:");
                            ui.add(egui::Slider::new(&mut self.config.tension_bar_min_saturation, 0.1..=1.0).fixed_decimals(2));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min pixels (presence threshold):");
                            ui.add(egui::DragValue::new(&mut self.config.tension_bar_min_pixels).range(1..=500));
                        });
                        ui.label("Tension bar region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.tension_bar_region { dv(v, ui); } });
                        ui.separator();
                        ui.strong("Tension percentage");
                        ui.label("Full fillable bar region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.tension_pct_region { dv(v, ui); } });
                    });

                    // Fish Caught
                    ui.collapsing("Fish Caught", |ui| {
                        let dv = |v: &mut i32, ui: &mut egui::Ui| {
                            ui.add(egui::DragValue::new(v).range(-200..=3840));
                        };
                        ui.horizontal(|ui| {
                            ui.label("Brightness threshold (per pixel):");
                            ui.add(egui::Slider::new(&mut self.config.fish_caught_threshold, 50..=250));
                        });
                        ui.horizontal(|ui| {
                            ui.label("Min bright pixels:");
                            ui.add(egui::DragValue::new(&mut self.config.fish_caught_min_pixels).range(10..=5000));
                        });
                        ui.separator();
                        ui.label("Continue button region [ox, oy, w, h]:");
                        ui.horizontal(|ui| { for v in &mut self.config.fish_caught_region { dv(v, ui); } });
                    });

                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("💾 Save Config").clicked() {
                            let mut to_save = self.config.clone();
                            scale_regions(&mut to_save, self.last_window_size, (1600, 900));
                            to_save.save();
                        }
                        if ui.button("Reset to Defaults").clicked() {
                            let base = self.last_window_size;
                            self.config = FishingConfig::default();
                            scale_regions(&mut self.config, (1600, 900), base);
                            if let Some((wx, wy, _, _)) = self.detected_window {
                                self.config.window_origin = (wx, wy);
                            }
                        }
                    });
                });
        });

        // ── Regions & Preview ────────────────────────────────────────────
        ui.separator();
        ui.collapsing("Regions & Preview", |ui| {
            // Color legend
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(egui::Color32::from_rgb(220, 100, 255), "■ Fishing mode");
                ui.colored_label(egui::Color32::from_rgb(255, 220,   0), "■ Fishing rod");
                ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "■ Rod use btn");
                ui.colored_label(egui::Color32::from_rgb(255, 150,   0), "■ Bite");
                ui.colored_label(egui::Color32::from_rgb(  0, 220, 200), "■ Tension bar");
                ui.colored_label(egui::Color32::from_rgb(  0, 200, 140), "■ Tension pct");
                ui.colored_label(egui::Color32::from_rgb( 50, 180, 255), "■ Continue btn");
                ui.colored_label(egui::Color32::from_rgb(255,  80, 200), "■ Left arrow");
                ui.colored_label(egui::Color32::from_rgb(255, 160, 220), "■ Right arrow");
            });
            ui.horizontal(|ui| {
                if ui.button("Capture preview").clicked() {
                    if let Some((id, wx, wy, ww, wh)) =
                        find_game_window(&self.config.game_window_title)
                    {
                        self.apply_window_detection(id, wx, wy, ww, wh);
                    }
                    match capture_debug_preview(&self.config, self.detected_window, _egui_ctx) {
                        Ok(tex) => { self.debug_texture = Some(tex); self.debug_error = None; }
                        Err(e)  => { self.debug_texture = None; self.debug_error = Some(e); }
                    }
                }
                if self.debug_texture.is_some() {
                    if ui.button("Open image").clicked() {
                        let path = std::env::current_dir()
                            .unwrap_or_else(|_| std::path::PathBuf::from("."))
                            .join("tmp")
                            .join("fishing_debug.png");
                        let _ = std::process::Command::new("xdg-open").arg(path).spawn();
                    }
                }
            });
            if let Some(err) = &self.debug_error {
                ui.colored_label(egui::Color32::RED, format!("Capture failed: {err}"));
            }
            if let Some(tex) = &self.debug_texture {
                let size = tex.size_vec2();
                let max_w = ui.available_width();
                let scale = (max_w / size.x).min(1.0);
                ui.add(egui::Image::new(tex).fit_to_exact_size(size * scale));
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
