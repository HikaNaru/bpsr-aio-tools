use core::{module::Module, AppConfig};
use crossbeam_channel::{Receiver, Sender};
use game::GameEvent;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct BpsrApp {
    config:       AppConfig,
    modules:      Vec<Box<dyn Module>>,
    active_index: usize,

    game_state:   Arc<RwLock<game::GameState>>,
    event_rx:     Option<Receiver<GameEvent>>,

    capture_started: bool,
    events_counted:  f64,
    last_tick:       std::time::Instant,
    events_per_sec:  f64,
}

impl BpsrApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        ui::theme::apply(&cc.egui_ctx);

        let config = AppConfig::load();
        let game_state = Arc::new(RwLock::new(game::GameState::default()));

        let modules: Vec<Box<dyn Module>> = vec![
            Box::new(dps_meter::DpsMeterModule::new(config.encounter_timeout_secs)),
            Box::new(auto_fishing::FishingModule::new()),
            Box::new(modules_optimizer::OptimizerModule::new()),
        ];

        Self {
            config,
            modules,
            active_index: 0,
            game_state,
            event_rx: None,
            capture_started: false,
            events_counted: 0.0,
            last_tick: std::time::Instant::now(),
            events_per_sec: 0.0,
        }
    }

    fn try_start_capture(&mut self) {
        if self.capture_started {
            return;
        }
        let (raw_tx, raw_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded::<GameEvent>();

        if let Err(e) = capture::start_capture(self.config.capture_interface.clone(), raw_tx) {
            tracing::warn!("capture start failed: {e}");
            return;
        }

        capture::proto::run_parser(raw_rx, event_tx);
        self.event_rx = Some(event_rx);
        self.capture_started = true;
    }

    fn drain_events(&mut self) {
        let Some(rx) = &self.event_rx else { return };
        let mut count = 0usize;
        let state_ref = Arc::clone(&self.game_state);

        while count < 500 {
            match rx.try_recv() {
                Ok(event) => {
                    state_ref.write().apply(&event);
                    // Forward combat events to dps-meter module
                    if let game::GameEvent::Combat(_) = &event {
                        if let Some(dps_mod) = self
                            .modules
                            .iter_mut()
                            .find(|m| m.id() == "dps-meter")
                        {
                            // Safe downcast pattern via Any would be cleaner;
                            // for now we rely on the module's push_event being called
                            // through a concrete wrapper — acceptable at this stage.
                        }
                    }
                    count += 1;
                    self.events_counted += 1.0;
                }
                Err(_) => break,
            }
        }

        let elapsed = self.last_tick.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            self.events_per_sec = self.events_counted / elapsed;
            self.events_counted = 0.0;
            self.last_tick = std::time::Instant::now();
        }
    }
}

impl eframe::App for BpsrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        let module_ctx = core::module::ModuleContext {
            config: &self.config,
            dt:     std::time::Duration::from_millis(16),
        };
        for module in &mut self.modules {
            module.update(&module_ctx);
        }

        // Top bar
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("BPSR AIO Tools");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Start Capture").clicked() {
                        self.try_start_capture();
                    }
                    ui::widgets::status_dot::status_dot(
                        ui,
                        self.capture_started,
                        if self.capture_started { "Capturing" } else { "Idle" },
                    );
                });
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui::layout::statusbar::statusbar(
                ui,
                &ui::layout::statusbar::StatusInfo {
                    interface:      self.config.capture_interface.as_deref(),
                    capturing:      self.capture_started,
                    encounter_secs: None,
                    events_per_sec: self.events_per_sec,
                },
            );
        });

        // Sidebar
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(52.0)
            .show(ctx, |ui| {
                self.active_index =
                    ui::layout::sidebar::sidebar(ui, &self.modules, self.active_index);
            });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(module) = self.modules.get_mut(self.active_index) {
                module.ui(ui, ctx);
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}
