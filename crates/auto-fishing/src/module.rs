use crate::bot::FishingState;
use crate::config::FishingConfig;
use crate::detector::{BaitPosition, check_window_focus, detect_fish_caught, detect_fishing_mode, detect_fishing_rod, detect_tension_bar, find_game_window, focus_game_window, resolve_region};
use core::module::{Module, ModuleContext};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct FishingModule {
    config:        FishingConfig,
    enabled:       bool,
    state:         Arc<Mutex<FishingState>>,
    fish_count:    Arc<AtomicU32>,
    failed_count:  Arc<AtomicU32>,
    bot_thread:    Option<std::thread::JoinHandle<()>>,
    stop_tx:       Option<std::sync::mpsc::Sender<()>>,
    debug_texture:    Option<egui::TextureHandle>,
    debug_error:      Option<String>,
    detected_window:  Option<(i32, i32, u32, u32)>,
    game_window_id:   String,
    paused:           Arc<AtomicBool>,
    last_window_size: (u32, u32),
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
    sr(&mut cfg.lure_region, sx, sy);
    sr(&mut cfg.tension_bar_region, sx, sy);
    sr(&mut cfg.fish_caught_region, sx, sy);
}

impl FishingModule {
    pub fn new() -> Self {
        let mut config = FishingConfig::default();
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
            bot_thread:    None,
            stop_tx:       None,
            debug_texture:   None,
            debug_error:     None,
            detected_window,
            game_window_id,
            paused:          Arc::new(AtomicBool::new(false)),
            last_window_size,
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

        self.paused.store(false, Ordering::Relaxed);
        let paused_arc    = Arc::clone(&self.paused);
        let window_id     = self.game_window_id.clone();
        let fish_count_arc   = Arc::clone(&self.fish_count);
        let failed_count_arc = Arc::clone(&self.failed_count);

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
                'outer: loop {
                    if stop_rx.try_recv().is_ok() {
                        break 'outer;
                    }

                    focus_tick += 1;
                    if focus_tick >= 30 {
                        focus_tick = 0;
                        paused_arc.store(!check_window_focus(&window_id), Ordering::Relaxed);
                    }

                    if paused_arc.load(Ordering::Relaxed) {
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
                            let _ = input.press_key(&cfg.rod_slot_key);
                            std::thread::sleep(Duration::from_millis(800));
                            let [rx, ry, rw, rh] = resolve_region(cfg.window_origin, cfg.rod_use_region);
                            let _ = input.click_at(rx + rw / 2, ry + rh / 2);
                            std::thread::sleep(Duration::from_millis(500));
                            // Close menu regardless — same key as open (toggle).
                            let _ = input.press_key(&cfg.rod_slot_key);
                            std::thread::sleep(Duration::from_millis(1000));
                            *state_arc.lock().unwrap() = FishingState::Idle;
                        }
                        FishingState::Idle => {
                            let _ = input.click_mouse_left();
                            *state_arc.lock().unwrap() = FishingState::Casting;
                        }
                        FishingState::Casting => {
                            std::thread::sleep(Duration::from_millis(cfg.cast_delay_ms));
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
                            let timed_out = started_at.elapsed()
                                > Duration::from_millis(cfg.reel_timeout_ms);
                            let bar_present = detect_tension_bar(&cfg).unwrap_or(true);

                            if !bar_present || timed_out {
                                // Tension bar gone or timeout — determine outcome
                                std::thread::sleep(Duration::from_millis(300));
                                if detect_fish_caught(&cfg).unwrap_or(false) {
                                    *state_arc.lock().unwrap() = FishingState::FishCaught;
                                } else {
                                    failed_count_arc.fetch_add(1, Ordering::Relaxed);
                                    *state_arc.lock().unwrap() = FishingState::Cooldown {
                                        until: Instant::now() + Duration::from_millis(cfg.cooldown_ms),
                                    };
                                }
                                continue;
                            }

                            // Bar present — steer + hold LMB
                            match crate::detector::detect_bait_position(&cfg)
                                .unwrap_or(BaitPosition::Center)
                            {
                                BaitPosition::Left  => { let _ = input.hold_key("a", 80); }
                                BaitPosition::Right => { let _ = input.hold_key("d", 80); }
                                BaitPosition::Center => {}
                            }
                            let _ = input.hold_mouse_left(cfg.reel_hold_ms);
                            std::thread::sleep(Duration::from_millis(cfg.reel_pause_ms));
                        }
                        FishingState::FishCaught => {
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
                                *state_arc.lock().unwrap() = FishingState::Idle;
                            } else {
                                std::thread::sleep(remaining.min(Duration::from_millis(50)));
                            }
                        }
                    }
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
    // Continue fishing button region (blue)
    draw_rect_border(&mut img, resolve_region(draw_origin, cfg.fish_caught_region), [50, 180, 255, 255]);

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

impl Module for FishingModule {
    fn id(&self)   -> &'static str { "auto-fishing" }
    fn name(&self) -> &str         { "Auto Fishing" }
    fn icon(&self) -> &str         { "🎣" }

    fn update(&mut self, _ctx: &ModuleContext) {}

    fn on_enable(&mut self)  { self.start_bot(); }
    fn on_disable(&mut self) { self.stop_bot(); }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        let state_label = self.state.lock().unwrap().label();

        ui.heading("Auto Fishing");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Status:");
            let paused = self.paused.load(Ordering::Relaxed);
            if self.enabled && paused {
                ui.strong("Paused (game not focused)");
            } else {
                ui.strong(state_label);
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!("Caught: {}", self.fish_count.load(Ordering::Relaxed)));
            ui.separator();
            ui.label(format!("Failed: {}", self.failed_count.load(Ordering::Relaxed)));
        });

        ui.separator();

        let paused = self.paused.load(Ordering::Relaxed);
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

        // ── Settings (detection params, timing, hue) ────────────────────
        ui.separator();
        ui.collapsing("Settings", |ui| {
            egui::ScrollArea::vertical()
                .max_height(400.0)
                .id_salt("fishing_settings_scroll")
                .show(ui, |ui| {
                    ui.strong("Game window");
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
                    ui.strong("Hotkeys");
                    ui.horizontal(|ui| {
                        ui.label("Enter fishing mode:");
                        ui.add(egui::TextEdit::singleline(&mut self.config.fishing_key).desired_width(40.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Open rod equipment:");
                        ui.add(egui::TextEdit::singleline(&mut self.config.rod_slot_key).desired_width(40.0));
                    });

                    ui.separator();
                    ui.strong("Fishing mode entry");
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
                    ui.strong("Fishing rod detect");
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
                    ui.strong("Bite detection");
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
                    ui.strong("Reeling");
                    ui.horizontal(|ui| {
                        ui.label("Lure hue center (°):");
                        ui.add(egui::Slider::new(&mut self.config.lure_hue_center, 0.0..=360.0).fixed_decimals(0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Lure hue range (±°):");
                        ui.add(egui::Slider::new(&mut self.config.lure_hue_range, 5.0..=60.0).fixed_decimals(0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Lure min saturation:");
                        ui.add(egui::Slider::new(&mut self.config.lure_min_saturation, 0.1..=1.0).fixed_decimals(2));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Bait center margin:");
                        ui.add(egui::Slider::new(&mut self.config.bait_center_margin_pct, 0.1..=0.4).fixed_decimals(2));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Reel hold (ms):");
                        ui.add(egui::DragValue::new(&mut self.config.reel_hold_ms).range(200..=1000));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Reel pause (ms):");
                        ui.add(egui::DragValue::new(&mut self.config.reel_pause_ms).range(100..=800));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Reel timeout (ms):");
                        ui.add(egui::DragValue::new(&mut self.config.reel_timeout_ms).range(5000..=120_000));
                    });

                    ui.separator();
                    ui.strong("Tension bar detect");
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

                    ui.separator();
                    ui.strong("Fish caught detect");
                    ui.horizontal(|ui| {
                        ui.label("Brightness threshold (per pixel):");
                        ui.add(egui::Slider::new(&mut self.config.fish_caught_threshold, 50..=250));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Min bright pixels:");
                        ui.add(egui::DragValue::new(&mut self.config.fish_caught_min_pixels).range(10..=5000));
                    });
                });
        });

        // ── Regions & Preview ────────────────────────────────────────────
        ui.separator();
        ui.collapsing("Regions & Preview", |ui| {
            // Capture / open controls
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(egui::Color32::from_rgb(220, 100, 255), "■ Fishing mode");
                ui.colored_label(egui::Color32::from_rgb(255, 220,   0), "■ Fishing rod");
                ui.colored_label(egui::Color32::from_rgb(100, 255, 100), "■ Rod use btn");
                ui.colored_label(egui::Color32::from_rgb(255, 150,   0), "■ Bite");
                ui.colored_label(egui::Color32::from_rgb(  0, 220, 200), "■ Tension bar");
                ui.colored_label(egui::Color32::from_rgb( 50, 180, 255), "■ Continue btn");
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

            // Region offsets (all relative to game window origin)
            ui.separator();
            ui.label("All offsets relative to game window top-left.");
            egui::ScrollArea::vertical()
                .max_height(300.0)
                .auto_shrink([false, false])
                .id_salt("fishing_regions_scroll")
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let dv = |v: &mut i32, ui: &mut egui::Ui| {
                        ui.add(egui::DragValue::new(v).range(-200..=3840));
                    };

                    ui.strong("Fishing mode detect [ox, oy, w, h]");
                    ui.horizontal(|ui| { for v in &mut self.config.fishing_mode_region { dv(v, ui); } });

                    ui.strong("Fishing rod detect [ox, oy, w, h]");
                    ui.horizontal(|ui| { for v in &mut self.config.fishing_rod_region { dv(v, ui); } });

                    ui.strong("Use fishing rod button [ox, oy, w, h]");
                    ui.horizontal(|ui| { for v in &mut self.config.rod_use_region { dv(v, ui); } });

                    ui.strong("Bite detect [ox, oy, w, h]");
                    ui.horizontal(|ui| { for v in &mut self.config.detect_region { dv(v, ui); } });

                    ui.strong("Tension bar [ox, oy, w, h]");
                    ui.horizontal(|ui| { for v in &mut self.config.tension_bar_region { dv(v, ui); } });

                    ui.strong("Continue fishing button [ox, oy, w, h]");
                    ui.horizontal(|ui| { for v in &mut self.config.fish_caught_region { dv(v, ui); } });
                });
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

}
