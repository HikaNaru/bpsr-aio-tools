use crate::bot::FishingState;
use crate::config::FishingConfig;
use core::module::{Module, ModuleContext};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct FishingModule {
    config:    FishingConfig,
    enabled:   bool,
    state:     Arc<Mutex<FishingState>>,
    fish_count: u32,
    bot_thread: Option<std::thread::JoinHandle<()>>,
    stop_tx:   Option<std::sync::mpsc::Sender<()>>,
}

impl FishingModule {
    pub fn new() -> Self {
        Self {
            config:     FishingConfig::default(),
            enabled:    false,
            state:      Arc::new(Mutex::new(FishingState::Idle)),
            fish_count: 0,
            bot_thread: None,
            stop_tx:    None,
        }
    }

    fn start_bot(&mut self) {
        let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
        let state_arc = Arc::clone(&self.state);
        let cfg = self.config.clone();

        let handle = std::thread::Builder::new()
            .name("auto-fishing".into())
            .spawn(move || {
                let mut baseline = [0u8; 3];
                let mut input = match crate::input::InputController::new() {
                    Ok(i) => i,
                    Err(e) => {
                        tracing::error!("input init failed: {e}");
                        return;
                    }
                };

                loop {
                    if stop_rx.try_recv().is_ok() {
                        break;
                    }

                    let current = state_arc.lock().unwrap().clone();

                    match current {
                        FishingState::Idle => {
                            // Cast
                            let _ = input.press_key(cfg.enigo_key());
                            *state_arc.lock().unwrap() = FishingState::Casting;
                        }
                        FishingState::Casting => {
                            std::thread::sleep(Duration::from_millis(cfg.cast_delay_ms));
                            baseline = crate::detector::sample_baseline(&cfg).unwrap_or([0, 0, 0]);
                            *state_arc.lock().unwrap() =
                                FishingState::WaitingBite { cast_at: Instant::now() };
                        }
                        FishingState::WaitingBite { cast_at } => {
                            let timed_out = cast_at.elapsed()
                                > Duration::from_millis(cfg.bite_timeout_ms);
                            if timed_out {
                                *state_arc.lock().unwrap() = FishingState::Idle;
                                continue;
                            }
                            let bite = crate::detector::detect_bite(&cfg, &baseline)
                                .unwrap_or(false);
                            if bite {
                                let _ = input.press_key(cfg.enigo_key());
                                *state_arc.lock().unwrap() = FishingState::Reeling;
                            } else {
                                std::thread::sleep(Duration::from_millis(50));
                            }
                        }
                        FishingState::Reeling => {
                            std::thread::sleep(Duration::from_millis(500));
                            *state_arc.lock().unwrap() = FishingState::Cooldown {
                                until: Instant::now()
                                    + Duration::from_millis(cfg.cooldown_ms),
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
                *state_arc.lock().unwrap() = FishingState::Idle;
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
            ui.strong(state_label);
        });
        ui.label(format!("Fish caught: {}", self.fish_count));

        ui.separator();

        let was_enabled = self.enabled;
        ui.checkbox(&mut self.enabled, "Enable bot");
        if self.enabled != was_enabled {
            if self.enabled {
                self.on_enable();
            } else {
                self.on_disable();
            }
        }

        ui.separator();
        ui.collapsing("Settings", |ui| {
            ui.horizontal(|ui| {
                ui.label("Color threshold:");
                ui.add(egui::Slider::new(&mut self.config.color_threshold, 1..=100));
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
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
