use core::{module::Module, AppConfig};
use crossbeam_channel::Receiver;
use game::GameEvent;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct BpsrApp {
    config:        AppConfig,
    modules:       Vec<Box<dyn Module>>,
    active_index:  usize,
    show_settings: bool,
    show_debug:    bool,
    debug_enabled: bool,
    locked_pos:    Option<egui::Pos2>,

    game_state:          Arc<RwLock<game::GameState>>,
    event_rx:            Option<Receiver<GameEvent>>,
    char_info_expanded:  bool,

    capture_started: bool,
    events_counted:  f64,
    last_tick:       std::time::Instant,
    events_per_sec:  f64,

}

impl BpsrApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        ui::theme::apply(&cc.egui_ctx);

        let mut config = AppConfig::load();
        config.click_passthrough = false;

        let game_state = Arc::new(RwLock::new(game::GameState::default()));
        let debug_enabled = std::env::var("DEBUG").is_ok();

        let modules: Vec<Box<dyn Module>> = vec![
            Box::new(dps_meter::DpsMeterModule::new(config.encounter_timeout_secs)),
            Box::new(modules_optimizer::OptimizerModule::new()),
            Box::new(auto_fishing::FishingModule::new()),
        ];

        Self {
            config,
            modules,
            active_index:       0,
            show_settings:      false,
            show_debug:         false,
            debug_enabled,
            locked_pos:         None,
            game_state,
            event_rx:           None,
            char_info_expanded: false,
            capture_started:    false,
            events_counted:     0.0,
            last_tick:          std::time::Instant::now(),
            events_per_sec:     0.0,
        }
    }

    fn try_start_capture(&mut self) {
        if self.capture_started {
            return;
        }
        let (raw_tx, raw_rx) = crossbeam_channel::unbounded();
        let (event_tx, event_rx) = crossbeam_channel::unbounded::<GameEvent>();

        if let Err(e) = capture::start_capture(raw_tx) {
            tracing::warn!("capture start failed: {e}");
            return;
        }

        capture::proto::run_parser(raw_rx, event_tx);
        self.event_rx = Some(event_rx);
        self.capture_started = true;
    }

    fn drain_events(&mut self) {
        let Some(rx) = &self.event_rx else { return };
        let mut batch = Vec::with_capacity(500);
        while batch.len() < 500 {
            match rx.try_recv() {
                Ok(e)  => batch.push(e),
                Err(_) => break,
            }
        }

        for event in &batch {
            self.game_state.write().apply(event);

            // Forward to DPS meter via downcast
            if let Some(dps) = self.modules.iter_mut()
                .find(|m| m.id() == "dps-meter")
                .and_then(|m| m.as_any_mut().downcast_mut::<dps_meter::DpsMeterModule>())
            {
                dps.push_event(event.clone());
            }

            // Forward to optimizer
            if let Some(opt) = self.modules.iter_mut()
                .find(|m| m.id() == "optimizer")
                .and_then(|m| m.as_any_mut().downcast_mut::<modules_optimizer::OptimizerModule>())
            {
                opt.push_event(event.clone());
            }
        }

        self.events_counted += batch.len() as f64;
        let elapsed = self.last_tick.elapsed().as_secs_f64();
        if elapsed >= 1.0 {
            self.events_per_sec = self.events_counted / elapsed;
            self.events_counted = 0.0;
            self.last_tick = std::time::Instant::now();
        }
    }

    fn apply_window_effects(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(
            self.config.click_passthrough,
        ));

        // Keep decorations in sync with lock state (send every frame so unlock always restores)
        ctx.send_viewport_cmd(egui::ViewportCommand::Decorations(!self.config.lock_position));

        if self.config.lock_position {
            let current = ctx.input(|i| i.viewport().outer_rect).map(|r| r.min);
            if self.locked_pos.is_none() {
                self.locked_pos = current;
            } else if current != self.locked_pos {
                if let Some(pos) = self.locked_pos {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos));
                }
            }
        } else {
            self.locked_pos = None;
        }

        if self.config.opacity < 0.999 {
            let alpha = (self.config.opacity.clamp(0.0, 1.0) * 255.0) as u8;
            let mut visuals = ctx.style().visuals.clone();
            let base = egui::Color32::from_rgba_unmultiplied(20, 22, 26, alpha);
            visuals.window_fill = base;
            visuals.panel_fill  = base;
            ctx.set_visuals(visuals);
        }
    }

    fn render_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Settings");
        ui.separator();

        ui.strong("Window");
        ui.add_space(4.0);

        if ui.checkbox(&mut self.config.lock_position, "Lock window position").changed() {
            self.config.save().ok();
        }
        if ui.checkbox(&mut self.config.click_passthrough, "Click pass-through").changed() {
            self.config.save().ok();
        }
        ui.horizontal(|ui| {
            ui.label("Transparency");
            if ui
                .add(
                    egui::Slider::new(&mut self.config.opacity, 0.1f32..=1.0)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                )
                .changed()
            {
                self.config.save().ok();
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.strong("Capture");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("DPS window (secs)");
            if ui.add(egui::Slider::new(&mut self.config.dps_window_secs, 1u32..=10)).changed() {
                self.config.save().ok();
            }
        });

        ui.horizontal(|ui| {
            ui.label("Encounter timeout (secs)");
            if ui.add(egui::Slider::new(&mut self.config.encounter_timeout_secs, 5u32..=120)).changed() {
                self.config.save().ok();
            }
        });
    }

    fn render_debug(&mut self, ui: &mut egui::Ui) {
        ui.heading("Debug — Packet Pipeline");
        ui.separator();

        // Pipeline stats
        let stats = capture::debug_stats::load_all();
        egui::Grid::new("debug_pipeline")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                ui.label("Raw pcap packets");   ui.label(fmt_count(stats.raw_packets));        ui.end_row();
                ui.label("Payloads extracted"); ui.label(fmt_count(stats.payloads_extracted)); ui.end_row();
                ui.label("Frames processed");   ui.label(fmt_count(stats.frames_processed));   ui.end_row();
                ui.label("Events dispatched");  ui.label(fmt_count(stats.events_dispatched));  ui.end_row();
                ui.label("Unknown opcode hits");ui.label(fmt_count(stats.unknown_dispatches)); ui.end_row();
            });

        // Unknown opcodes
        ui.add_space(6.0);
        ui.separator();
        ui.strong("Unknown Opcodes (top 20)");
        let opcode_stats = capture::proto::packets::unknown::opcode_stats();
        if opcode_stats.is_empty() {
            ui.label("—");
        } else {
            egui::ScrollArea::vertical()
                .id_salt("opcodes_scroll")
                .max_height(100.0)
                .show(ui, |ui| {
                    egui::Grid::new("opcodes_grid").num_columns(2).striped(true).show(ui, |ui| {
                        ui.strong("Opcode"); ui.strong("Count"); ui.end_row();
                        for (opcode, count) in opcode_stats.iter().take(20) {
                            ui.label(egui::RichText::new(format!("{:#010x}", opcode)).monospace());
                            ui.label(fmt_count(*count));
                            ui.end_row();
                        }
                    });
                });
        }

        // Event log
        ui.add_space(6.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.strong("Events");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("Clear").clicked() {
                    capture::debug_stats::RECENT_EVENTS.lock().unwrap().clear();
                }
            });
        });

        let events = capture::debug_stats::RECENT_EVENTS.lock().unwrap();
        egui::ScrollArea::vertical()
            .id_salt("events_scroll")
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in events.iter() {
                    let color = event_line_color(line);
                    ui.label(
                        egui::RichText::new(line)
                            .monospace()
                            .size(11.0)
                            .color(color),
                    );
                }
            });
        drop(events);
    }
}

fn fmt_count(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else {
        n.to_string()
    }
}

impl eframe::App for BpsrApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.drain_events();

        // Auto-start capture when DPS meter is active
        if !self.capture_started && !self.show_settings {
            if let Some(module) = self.modules.get(self.active_index) {
                if module.id() == "dps-meter" {
                    self.try_start_capture();
                }
            }
        }

        let module_ctx = core::module::ModuleContext {
            config: &self.config,
            dt:     std::time::Duration::from_millis(16),
        };
        for module in &mut self.modules {
            module.update(&module_ctx);
        }

        self.apply_window_effects(ctx);

        // Top bar
        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.strong("BPSR AIO Tools");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui::widgets::status_dot::status_dot(
                        ui,
                        self.capture_started,
                        if self.capture_started { "Capturing" } else { "Idle" },
                    );

                    let passthrough_label = if self.config.click_passthrough { "👆" } else { "🖱" };
                    if ui
                        .button(passthrough_label)
                        .on_hover_text(if self.config.click_passthrough { "Click pass-through ON" } else { "Click pass-through OFF" })
                        .clicked()
                    {
                        self.config.click_passthrough = !self.config.click_passthrough;
                        self.config.save().ok();
                    }

                    let lock_label = if self.config.lock_position { "🔒" } else { "🔓" };
                    if ui
                        .button(lock_label)
                        .on_hover_text(if self.config.lock_position { "Position locked" } else { "Position unlocked" })
                        .clicked()
                    {
                        self.config.lock_position = !self.config.lock_position;
                        self.locked_pos = None;
                        self.config.save().ok();
                    }
                });
            });
        });

        // Status bar
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui::layout::statusbar::statusbar(
                ui,
                &ui::layout::statusbar::StatusInfo {
                    interface:      None,
                    capturing:      self.capture_started,
                    encounter_secs: None,
                    events_per_sec: self.events_per_sec,
                },
            );
        });

        // Player info panel (collapsible)
        egui::TopBottomPanel::top("player_info")
            .resizable(false)
            .show(ctx, |ui| {
                let gs = self.game_state.read();

                // Header row — always visible, clickable to toggle expand
                let header_resp = ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let arrow = if self.char_info_expanded { "▼" } else { "▶" };
                    ui.label(egui::RichText::new(arrow).small().color(egui::Color32::from_rgb(120, 120, 150)));
                    ui.add_space(4.0);

                    if let Some(local_id) = gs.local_player {
                        if let Some(entity) = gs.entities.get(&local_id) {
                            let (dot_rect, _) = ui.allocate_exact_size(
                                egui::vec2(24.0, 24.0), egui::Sense::hover(),
                            );
                            ui.painter().circle_filled(
                                dot_rect.center(), 10.0, class_color(entity.class_id),
                            );
                            ui.add_space(6.0);
                            ui.vertical(|ui| {
                                let level_str = entity.stats.level
                                    .map(|l| format!(" Lv.{l}"))
                                    .unwrap_or_default();
                                ui.strong(format!("{}{}", entity.name, level_str));
                                if let Some(c) = entity.class_id {
                                    let hp_str = match (entity.stats.hp, entity.stats.max_hp) {
                                        (Some(h), Some(m)) => format!("  HP {h}/{m}"),
                                        _ => String::new(),
                                    };
                                    ui.label(
                                        egui::RichText::new(format!("{}{}", class_name(c), hp_str))
                                            .small()
                                            .color(egui::Color32::from_rgb(160, 160, 180)),
                                    );
                                }
                            });
                        } else {
                            ui.label("—");
                        }
                    } else {
                        ui.label(
                            egui::RichText::new("Connecting…")
                                .italics()
                                .color(egui::Color32::GRAY),
                        );
                    }

                    if let Some(zone) = &gs.zone_name {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(zone)
                                    .small()
                                    .color(egui::Color32::from_rgb(120, 120, 150)),
                            );
                        });
                    }
                });
                if header_resp.response.interact(egui::Sense::click()).clicked() {
                    drop(gs);
                    self.char_info_expanded = !self.char_info_expanded;
                } else if self.char_info_expanded {
                    if let Some(local_id) = gs.local_player {
                        if let Some(entity) = gs.entities.get(&local_id) {
                            ui.add_space(4.0);
                            ui.separator();
                            render_char_stats(ui, &entity.stats);
                            ui.add_space(4.0);
                        }
                    }
                }
            });

        // Sidebar
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(140.0)
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let r = ui::layout::sidebar::sidebar(
                    ui,
                    &self.modules,
                    self.active_index,
                    self.show_settings,
                    self.show_debug,
                    self.debug_enabled,
                );
                if r.settings_clicked {
                    self.show_settings = !self.show_settings;
                    self.show_debug = false;
                } else if r.debug_clicked {
                    self.show_debug = !self.show_debug;
                    self.show_settings = false;
                } else if r.active != self.active_index {
                    self.active_index = r.active;
                    self.show_settings = false;
                    self.show_debug = false;
                }
            });

        // Main content
        egui::CentralPanel::default().show(ctx, |ui| {
            if self.show_settings {
                self.render_settings(ui);
            } else if self.show_debug {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.render_debug(ui);
                });
            } else if let Some(module) = self.modules.get_mut(self.active_index) {
                module.ui(ui, ctx);
            }
        });

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn event_line_color(line: &str) -> egui::Color32 {
    if line.starts_with("[Combat]") {
        egui::Color32::from_rgb(255, 160, 80)
    } else if line.starts_with("[EntityName]") {
        egui::Color32::from_rgb(100, 210, 255)
    } else if line.starts_with("[LocalPlayer]") {
        egui::Color32::from_rgb(100, 220, 120)
    } else if line.starts_with("[ZoneChange]") {
        egui::Color32::from_rgb(200, 180, 100)
    } else if line.starts_with("[Unknown]") {
        egui::Color32::from_rgb(150, 100, 100)
    } else {
        egui::Color32::from_rgb(180, 180, 190)
    }
}

fn render_char_stats(ui: &mut egui::Ui, s: &game::entity::CharStats) {
    use egui::Color32;
    let dim = Color32::from_rgb(140, 140, 160);
    let val_or_dash = |v: Option<u64>| -> String {
        v.map(|n| n.to_string()).unwrap_or_else(|| "—".into())
    };
    let pct_str = |raw: Option<u32>, pct: Option<u32>| -> String {
        match (raw, pct) {
            (_, Some(p)) => format!("{:.1}%  ({})", p as f32 / 100.0, raw.map(|r| r.to_string()).unwrap_or_default()),
            (Some(r), _) => r.to_string(),
            _            => "—".into(),
        }
    };

    egui::Grid::new("char_stats")
        .num_columns(4)
        .spacing([12.0, 2.0])
        .show(ui, |ui| {
            // Row 1: HP, ATK
            ui.label(egui::RichText::new("HP").small().color(dim));
            let hp_str = match (s.hp, s.max_hp) {
                (Some(h), Some(m)) => format!("{h} / {m}"),
                (Some(h), _)       => h.to_string(),
                _                  => "—".into(),
            };
            ui.label(egui::RichText::new(hp_str).small());
            ui.label(egui::RichText::new("ATK").small().color(dim));
            ui.label(egui::RichText::new(val_or_dash(s.attack.map(|v| v as u64))).small());
            ui.end_row();
            // Row 2: Strength, Endurance
            ui.label(egui::RichText::new("STR").small().color(dim));
            ui.label(egui::RichText::new(val_or_dash(s.strength.map(|v| v as u64))).small());
            ui.label(egui::RichText::new("END").small().color(dim));
            ui.label(egui::RichText::new(val_or_dash(s.endurance.map(|v| v as u64))).small());
            ui.end_row();
            // Row 3: Armor, Ability Score
            ui.label(egui::RichText::new("Armor").small().color(dim));
            ui.label(egui::RichText::new(val_or_dash(s.armor.map(|v| v as u64))).small());
            ui.label(egui::RichText::new("Score").small().color(dim));
            ui.label(egui::RichText::new(val_or_dash(s.ability_score.map(|v| v as u64))).small());
            ui.end_row();
            // Row 4: Crit, Haste
            ui.label(egui::RichText::new("Crit").small().color(dim));
            ui.label(egui::RichText::new(pct_str(s.crit, s.crit_pct)).small());
            ui.label(egui::RichText::new("Haste").small().color(dim));
            ui.label(egui::RichText::new(pct_str(s.haste, s.haste_pct)).small());
            ui.end_row();
            // Row 5: Luck, Mastery
            ui.label(egui::RichText::new("Luck").small().color(dim));
            ui.label(egui::RichText::new(pct_str(s.luck, s.luck_pct)).small());
            ui.label(egui::RichText::new("Mastery").small().color(dim));
            ui.label(egui::RichText::new(pct_str(s.mastery, s.mastery_pct)).small());
            ui.end_row();
            // Row 6: Versatility, Block
            ui.label(egui::RichText::new("Versatility").small().color(dim));
            ui.label(egui::RichText::new(pct_str(s.versatility, s.versatility_pct)).small());
            ui.label(egui::RichText::new("Block").small().color(dim));
            ui.label(egui::RichText::new(pct_str(s.block, s.block_pct)).small());
            ui.end_row();
        });
}

fn class_color(class_id: Option<u32>) -> egui::Color32 {
    match class_id {
        Some(1)  => egui::Color32::from_rgb( 90, 160, 255), // Stormblade — blue
        Some(2)  => egui::Color32::from_rgb(100, 200, 255), // FrostMage — ice blue
        Some(3)  => egui::Color32::from_rgb(255, 140,  70), // TwinStriker — orange
        Some(4)  => egui::Color32::from_rgb(130, 220, 130), // WindKnight — green
        Some(5)  => egui::Color32::from_rgb( 80, 200, 160), // VerdantOracle — teal
        Some(8)  => egui::Color32::from_rgb(255, 220,  60), // ThunderHandCannon — yellow
        Some(9)  => egui::Color32::from_rgb(180, 100,  50), // HeavyGuardian — brown
        Some(10) => egui::Color32::from_rgb(160,  70, 200), // DarkSpiritDance — purple
        Some(11) => egui::Color32::from_rgb(180, 220,  80), // Marksman — lime
        Some(12) => egui::Color32::from_rgb(220, 180,  60), // ShieldKnight — gold
        Some(13) => egui::Color32::from_rgb(220,  80, 130), // BeatPerformer — pink
        _        => egui::Color32::from_rgb(100, 100, 120),
    }
}

fn class_name(id: u32) -> &'static str {
    match id {
        1  => "Stormblade",
        2  => "Frost Mage",
        3  => "Twin Striker",
        4  => "Wind Knight",
        5  => "Verdant Oracle",
        8  => "Thunder Cannon",
        9  => "Heavy Guardian",
        10 => "Dark Spirit Dance",
        11 => "Marksman",
        12 => "Shield Knight",
        13 => "Beat Performer",
        _  => "Unknown",
    }
}
