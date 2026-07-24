use core::{module::Module, AppConfig};
use crossbeam_channel::Receiver;
use game::event::MatchAlertKind;
use game::GameEvent;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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
    last_tick:       Instant,
    events_per_sec:  f64,

    // (message, received_at) — shown as in-app banner, auto-dismissed after 15s
    match_alert_banner: Option<(String, Instant)>,

    class_textures: HashMap<u32, egui::TextureHandle>,
}

/// Merges a bundled CJK-subset font into the Proportional family as a fallback,
/// after the default Latin font, so any character the Latin font lacks a glyph for
/// (Chinese/Japanese names/descriptions this app has no English source for) renders
/// as a real glyph instead of a tofu box. Loaded from disk like every other asset
/// (`data/Fonts/...`, same convention as `data/Images/...`) — optional, skipped with
/// a log warning if the file isn't present rather than failing startup.
fn add_cjk_fallback_font(fonts: &mut egui::FontDefinitions) {
    let path = core::data_tables::data_dir().join("Fonts").join("NotoSansCJK-SC-Subset.otf");
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("CJK fallback font missing ({}): {e}", path.display());
            return;
        }
    };

    const KEY: &str = "cjk_fallback";
    fonts.font_data.insert(KEY.to_owned(), egui::FontData::from_owned(bytes).into());
    fonts.families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .push(KEY.to_owned());
}

impl BpsrApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        ui::theme::apply(&cc.egui_ctx);

        let mut fonts = egui::FontDefinitions::default();
        egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
        add_cjk_fallback_font(&mut fonts);
        cc.egui_ctx.set_fonts(fonts);

        let mut config = AppConfig::load();
        config.click_passthrough = false;

        let game_state = Arc::new(RwLock::new(game::GameState::default()));
        let debug_enabled = std::env::var("DEBUG").is_ok();

        // Run encounter auto-cleanup on startup
        {
            let store = encounter_store::EncounterStore::open();
            store.auto_cleanup(config.encounter_max_count, config.encounter_max_age_days);
        }

        let modules: Vec<Box<dyn Module>> = vec![
            Box::new(dps_meter::DpsMeterModule::new(config.encounter_timeout_secs)),
            Box::new(dps_meter::EncounterHistoryModule::new()),
            Box::new(chat::ChatModule::new()),
            Box::new(cooldown_tracker::CooldownModule::new()),
            Box::new(bptimer::BPTimerModule::new()),
            Box::new(threat_meter::ThreatModule::new()),
            Box::new(modules_optimizer::OptimizerModule::new()),
            Box::new(auto_fishing::FishingModule::new()),
        ];

        let class_textures = load_class_textures(&cc.egui_ctx);

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
            capture_started:     false,
            events_counted:      0.0,
            last_tick:           Instant::now(),
            events_per_sec:      0.0,
            match_alert_banner:  None,
            class_textures,
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

            // Forward Chat events to chat module
            if matches!(event, game::GameEvent::Chat { .. }) {
                if let Some(chat) = self.modules.iter_mut()
                    .find(|m| m.id() == "chat")
                    .and_then(|m| m.as_any_mut().downcast_mut::<chat::ChatModule>())
                {
                    chat.push_event(event.clone());
                }
            }

            // Forward Combat + EntityName to cooldown tracker
            if matches!(event, game::GameEvent::Combat(_) | game::GameEvent::EntityName { .. } | game::GameEvent::EntityDespawn { .. }) {
                if let Some(cd) = self.modules.iter_mut()
                    .find(|m| m.id() == "cooldown")
                    .and_then(|m| m.as_any_mut().downcast_mut::<cooldown_tracker::CooldownModule>())
                {
                    cd.push_event(event.clone());
                }
            }

            // Forward ThreatUpdate + supporting events to threat module
            if matches!(event,
                game::GameEvent::ThreatUpdate { .. }
                | game::GameEvent::EntityName { .. }
                | game::GameEvent::EntityDespawn { .. }
                | game::GameEvent::ZoneChange { .. })
            {
                if let Some(tm) = self.modules.iter_mut()
                    .find(|m| m.id() == "threat-meter")
                    .and_then(|m| m.as_any_mut().downcast_mut::<threat_meter::ThreatModule>())
                {
                    tm.push_event(event.clone());
                }
            }

            // Matchmaking alerts
            if let game::GameEvent::MatchmakingAlert { kind } = event {
                let msg = match kind {
                    MatchAlertKind::QueuePop   => "Queue popped! Accept now.",
                    MatchAlertKind::ReadyCheck  => "Ready check in progress!",
                };
                self.match_alert_banner = Some((msg.to_string(), Instant::now()));
                if self.config.match_notify {
                    core::notification::notify("BPSR Match", msg, true);
                }
                if self.config.match_sound {
                    play_alert_beep();
                }
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

        // Apply opacity to visuals-driven fills (status bar etc.)
        let alpha = (self.config.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut visuals = ctx.style().visuals.clone();
        visuals.window_fill = egui::Color32::from_rgba_unmultiplied(10, 11, 14, alpha);
        visuals.panel_fill  = egui::Color32::from_rgba_unmultiplied(10, 11, 14, alpha);
        ctx.set_visuals(visuals);
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
                    egui::Slider::new(&mut self.config.opacity, 0.0f32..=1.0)
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

        ui.add_space(8.0);
        ui.separator();
        ui.strong("Matchmaking Alerts");
        ui.add_space(4.0);

        if ui.checkbox(&mut self.config.match_notify, "Desktop notification (notify-send)").changed() {
            self.config.save().ok();
        }
        if ui.checkbox(&mut self.config.match_sound, "Sound alert").changed() {
            self.config.save().ok();
        }
        if ui.button("Test alert").clicked() {
            self.match_alert_banner = Some(("Queue popped! Accept now.".to_string(), Instant::now()));
            if self.config.match_notify {
                core::notification::notify("BPSR Match", "Queue popped! Accept now.", true);
            }
            if self.config.match_sound {
                play_alert_beep();
            }
        }

        ui.add_space(8.0);
        ui.separator();
        ui.strong("Discord Webhook");
        ui.add_space(4.0);

        if ui.checkbox(&mut self.config.discord_enabled, "Post encounter results to Discord").changed() {
            self.config.save().ok();
        }
        ui.horizontal(|ui| {
            ui.label("Webhook URL:");
            if ui.add(
                egui::TextEdit::singleline(&mut self.config.discord_webhook_url)
                    .hint_text("https://discord.com/api/webhooks/...")
                    .desired_width(280.0)
            ).lost_focus() {
                self.config.save().ok();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Min players to post:");
            if ui.add(
                egui::DragValue::new(&mut self.config.discord_min_players)
                    .range(1usize..=8)
            ).changed() {
                self.config.save().ok();
            }
        });

        ui.add_space(8.0);
        ui.separator();
        ui.strong("BPTimer");
        ui.add_space(4.0);

        if ui.checkbox(&mut self.config.bptimer_enabled, "Connect to BPTimer spawn tracker").changed() {
            self.config.save().ok();
        }
        ui.horizontal(|ui| {
            ui.label("WebSocket URL:");
            if ui.add(
                egui::TextEdit::singleline(&mut self.config.bptimer_ws_url)
                    .hint_text("ws://...")
                    .desired_width(280.0)
            ).lost_focus() {
                self.config.save().ok();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Report URL:");
            if ui.add(
                egui::TextEdit::singleline(&mut self.config.bptimer_report_url)
                    .hint_text("https://... (POST kill reports)")
                    .desired_width(280.0)
            ).lost_focus() {
                self.config.save().ok();
            }
        });
        ui.horizontal(|ui| {
            ui.label("Region filter:");
            if ui.add(
                egui::TextEdit::singleline(&mut self.config.bptimer_region)
                    .hint_text("Leave empty for all regions")
                    .desired_width(160.0)
            ).lost_focus() {
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

        // Opacity helper: multiply base color's alpha by current opacity
        let alpha = (self.config.opacity.clamp(0.0, 1.0) * 255.0).round() as u8;
        let fade = |c: egui::Color32| -> egui::Color32 {
            let a = ((c.a() as u16 * alpha as u16) / 255) as u8;
            egui::Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
        };

        // Title bar
        egui::TopBottomPanel::top("top_bar")
            .frame(egui::Frame::none()
                .fill(fade(ui::theme::BG_CHROME))
                .inner_margin(egui::Margin::symmetric(12.0, 8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    // Logo diamond
                    let (logo_rect, _) = ui.allocate_exact_size(
                        egui::vec2(22.0, 22.0), egui::Sense::hover(),
                    );
                    ui.painter().rect_filled(logo_rect, 6.0, ui::theme::ACCENT);
                    let inner = logo_rect.shrink(5.0);
                    ui.painter().rect_filled(inner, 2.0, ui::theme::BG_DARK);

                    ui.add_space(8.0);

                    ui.vertical(|ui| {
                        ui.label(
                            egui::RichText::new("BPSR AIO TOOLS")
                                .strong()
                                .size(12.0)
                                .color(ui::theme::TEXT),
                        );
                        ui.label(
                            egui::RichText::new("BLUE PROTOCOL: STAR RESONANCE")
                                .size(8.5)
                                .color(ui::theme::TEXT_FAINT),
                        );
                    });

                    ui.add_space(10.0);

                    // Status dot
                    ui::widgets::status_dot::status_dot(
                        ui,
                        self.capture_started,
                        if self.capture_started { "hooked" } else { "idle" },
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Window control buttons
                        let lock_label = if self.config.lock_position { egui_phosphor::regular::LOCK } else { egui_phosphor::regular::LOCK_OPEN };
                        if ui.button(lock_label)
                            .on_hover_text(if self.config.lock_position { "Position locked" } else { "Position unlocked" })
                            .clicked()
                        {
                            self.config.lock_position = !self.config.lock_position;
                            self.locked_pos = None;
                            self.config.save().ok();
                        }

                        let passthrough_label = if self.config.click_passthrough { egui_phosphor::regular::HAND_POINTING } else { egui_phosphor::regular::MOUSE };
                        if ui.button(passthrough_label)
                            .on_hover_text(if self.config.click_passthrough { "Click pass-through ON" } else { "Click pass-through OFF" })
                            .clicked()
                        {
                            self.config.click_passthrough = !self.config.click_passthrough;
                            self.config.save().ok();
                        }

                        ui.add_space(8.0);

                        // Opacity slider
                        ui.label(egui::RichText::new("α").color(ui::theme::TEXT_MUTED).size(11.0));
                        ui.add_space(2.0);
                        ui.spacing_mut().slider_width = 70.0;
                        if ui.add(
                            egui::Slider::new(&mut self.config.opacity, 0.0f32..=1.0)
                                .show_value(false),
                        ).changed() {
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

        // Player info / breadcrumb panel
        egui::TopBottomPanel::top("player_info")
            .resizable(false)
            .frame(egui::Frame::none()
                .fill(fade(ui::theme::BG_PANEL))
                .inner_margin(egui::Margin::symmetric(16.0, 10.0)))
            .show(ctx, |ui| {
                let gs = self.game_state.read();

                let header_resp = ui.horizontal(|ui| {
                    // Module breadcrumb (left)
                    let screen_title = if self.show_settings {
                        "Settings"
                    } else if self.show_debug {
                        "Debug"
                    } else {
                        self.modules.get(self.active_index)
                            .map(|m| m.name())
                            .unwrap_or("—")
                    };
                    ui.label(egui::RichText::new(screen_title).strong().size(15.0).color(ui::theme::TEXT));

                    // Character profile card (right-aligned)
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let arrow = if self.char_info_expanded { egui_phosphor::regular::CARET_DOWN } else { egui_phosphor::regular::CARET_RIGHT };
                        ui.label(egui::RichText::new(arrow).size(10.0).color(ui::theme::TEXT_FAINT));
                        ui.add_space(4.0);

                        if let Some(local_id) = gs.local_player {
                            if let Some(entity) = gs.entities.get(&local_id) {
                                // Zone name
                                if let Some(zone) = &gs.zone_name {
                                    ui.label(
                                        egui::RichText::new(zone)
                                            .size(10.0)
                                            .color(ui::theme::TEXT_FAINT),
                                    );
                                    ui.add_space(6.0);
                                }

                                // Name + level
                                let level_str = entity.stats.level
                                    .map(|l| format!("  Lv.{l}"))
                                    .unwrap_or_default();
                                ui.label(
                                    egui::RichText::new(format!("{}{}", entity.name, level_str))
                                        .size(12.5)
                                        .strong()
                                        .color(ui::theme::TEXT),
                                );
                                ui.add_space(4.0);

                                // Class color dot
                                let cls_col = class_color(entity.class_id);
                                let (dot_rect, _) = ui.allocate_exact_size(
                                    egui::vec2(10.0, 10.0), egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(dot_rect.center(), 5.0, cls_col);
                            }
                        } else {
                            ui.label(egui::RichText::new("Connecting…").italics().size(11.0).color(ui::theme::TEXT_FAINT));
                        }
                    });
                });

                if header_resp.response.interact(egui::Sense::click()).clicked() {
                    drop(gs);
                    self.char_info_expanded = !self.char_info_expanded;
                } else if self.char_info_expanded {
                    if let Some(local_id) = gs.local_player {
                        if let Some(entity) = gs.entities.get(&local_id) {
                            ui.add_space(8.0);
                            let cls_tex = entity.class_id.and_then(|id| self.class_textures.get(&id));
                            render_profile_card(ui, &entity.name, entity.class_id, &entity.stats, cls_tex);
                        }
                    }
                }
            });

        // Sidebar
        egui::SidePanel::left("sidebar")
            .resizable(false)
            .exact_width(150.0)
            .frame(egui::Frame::none())
            .show(ctx, |ui| {
                let r = ui::layout::sidebar::sidebar(
                    ui,
                    &self.modules,
                    self.active_index,
                    self.show_settings,
                    self.show_debug,
                    self.debug_enabled,
                    alpha,
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
        egui::CentralPanel::default()
            .frame(egui::Frame::none()
                .fill(fade(ui::theme::BG_DARK))
                .inner_margin(egui::Margin::same(16.0)))
            .show(ctx, |ui| {
                if self.show_settings {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.render_settings(ui);
                    });
                } else if self.show_debug {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.render_debug(ui);
                    });
                } else if let Some(module) = self.modules.get_mut(self.active_index) {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        module.ui(ui, ctx);
                    });
                }
            });

        // Match alert banner (floating window, auto-dismiss after 15s)
        // Match alert banner (floating window, auto-dismiss after 15s)
        let dismiss_alert = if let Some((msg, at)) = &self.match_alert_banner {
            if at.elapsed().as_secs() >= 15 {
                true
            } else {
                let msg = msg.clone();
                let mut dismiss = false;
                egui::Window::new(format!("{} Match Alert", egui_phosphor::regular::SWORD))
                    .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, 60.0))
                    .collapsible(false)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.label(
                            egui::RichText::new(&msg)
                                .strong()
                                .size(15.0)
                                .color(egui::Color32::from_rgb(255, 220, 60)),
                        );
                        if ui.button("Dismiss").clicked() {
                            dismiss = true;
                        }
                    });
                dismiss
            }
        } else {
            false
        };
        if dismiss_alert {
            self.match_alert_banner = None;
        }

        ctx.request_repaint_after(std::time::Duration::from_millis(16));
    }
}

fn play_alert_beep() {
    std::thread::spawn(|| {
        use rodio::source::Source;
        if let Ok((_stream, handle)) = rodio::OutputStream::try_default() {
            if let Ok(sink) = rodio::Sink::try_new(&handle) {
                let tone1 = rodio::source::SineWave::new(880.0)
                    .take_duration(std::time::Duration::from_millis(200))
                    .amplify(0.5);
                let tone2 = rodio::source::SineWave::new(1100.0)
                    .take_duration(std::time::Duration::from_millis(200))
                    .amplify(0.5);
                sink.append(tone1);
                sink.append(tone2);
                sink.sleep_until_end();
            }
        }
    });
}

fn load_class_textures(ctx: &egui::Context) -> HashMap<u32, egui::TextureHandle> {
    let mut map = HashMap::new();
    for class_id in [1u32, 2, 3, 4, 5, 9, 11, 12, 13] {
        let path = format!("data/Images/Professions/128/Profession_{class_id}.png");
        let Ok(bytes) = std::fs::read(&path) else { continue };
        let Ok(img) = image::load_from_memory(&bytes) else { continue };
        let rgba = img.to_rgba8();
        let size = [rgba.width() as usize, rgba.height() as usize];
        let color_img = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
        let handle = ctx.load_texture(
            format!("class_{class_id}"),
            color_img,
            egui::TextureOptions::LINEAR,
        );
        map.insert(class_id, handle);
    }
    map
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

fn render_profile_card(
    ui: &mut egui::Ui,
    name: &str,
    class_id: Option<u32>,
    s: &game::entity::CharStats,
    class_texture: Option<&egui::TextureHandle>,
) {
    let cls_col  = class_color(class_id);
    let cls_name = class_name(class_id.unwrap_or(0));

    ui.horizontal(|ui| {
        // Avatar — class image or color circle fallback
        let (av_rect, _) = ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
        if let Some(tex) = class_texture {
            ui.painter().image(
                tex.id(),
                av_rect,
                egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        } else {
            ui.painter().circle_filled(av_rect.center(), 22.0, cls_col);
            let initials: String = name.chars()
                .filter(|c| c.is_uppercase() || name.starts_with(*c))
                .take(2)
                .collect::<String>()
                .to_uppercase();
            let init_str = if initials.is_empty() {
                name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default()
            } else { initials };
            ui.painter().text(
                av_rect.center(),
                egui::Align2::CENTER_CENTER,
                &init_str,
                egui::FontId::proportional(16.0),
                egui::Color32::WHITE,
            );
        }

        ui.add_space(10.0);

        // Name + class column
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(name).strong().size(14.0).color(ui::theme::TEXT));
            ui.horizontal(|ui| {
                // Class colored badge
                let (badge_rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(badge_rect.center(), egui::vec2(7.0, 7.0))
                        .translate(egui::vec2(0.0, 1.0)),
                    2.0,
                    cls_col,
                );
                ui.label(egui::RichText::new(cls_name).size(11.5).color(cls_col));
                if let Some(lv) = s.level {
                    ui.label(egui::RichText::new(format!("· Lv.{lv}")).size(11.0).color(ui::theme::TEXT_FAINT));
                }
            });
        });

        // AS + SS + HP — painter-drawn for pixel-precise vertical centering
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let has_as = s.ability_score.is_some();
            let has_ss = s.season_strength.filter(|&v| v > 0).is_some();
            let has_hp = s.hp.is_some() && s.max_hp.is_some();
            if !has_as && !has_ss && !has_hp { return; }

            let as_w: f32 = if has_as { 64.0 } else { 0.0 };
            let ss_w: f32 = if has_ss { 52.0 } else { 0.0 };
            let hp_w: f32 = if has_hp { 94.0 } else { 0.0 };
            let gap  = 8.0_f32;
            let n_gaps = (has_as as usize + has_ss as usize + has_hp as usize).saturating_sub(1) as f32;
            let total = as_w + ss_w + hp_w + n_gaps * gap;

            let (rect, _) = ui.allocate_exact_size(egui::vec2(total, 44.0), egui::Sense::hover());
            let cy      = rect.center().y;
            let val_y   = cy - 7.0;
            let lbl_y   = cy + 9.0;
            let painter = ui.painter();

            let mut x = rect.min.x;
            if has_as {
                painter.text(egui::pos2(x + as_w * 0.5, val_y), egui::Align2::CENTER_CENTER,
                    s.ability_score.unwrap().to_string(), egui::FontId::proportional(18.0), ui::theme::ACCENT);
                painter.text(egui::pos2(x + as_w * 0.5, lbl_y), egui::Align2::CENTER_CENTER,
                    "AS", egui::FontId::proportional(8.5), ui::theme::TEXT_FAINT);
                x += as_w + if has_ss || has_hp { gap } else { 0.0 };
            }
            if has_ss {
                painter.text(egui::pos2(x + ss_w * 0.5, val_y), egui::Align2::CENTER_CENTER,
                    format!("{}", s.season_strength.unwrap_or(0)), egui::FontId::proportional(16.0), ui::theme::ACCENT);
                painter.text(egui::pos2(x + ss_w * 0.5, lbl_y), egui::Align2::CENTER_CENTER,
                    "SS", egui::FontId::proportional(8.5), ui::theme::TEXT_FAINT);
                x += ss_w + if has_hp { gap } else { 0.0 };
            }
            if has_hp {
                let hp_str = format!("{} / {}", fmt_stat_i64(s.hp.unwrap()), fmt_stat_i64(s.max_hp.unwrap()));
                painter.text(egui::pos2(x + hp_w * 0.5, val_y), egui::Align2::CENTER_CENTER,
                    hp_str, egui::FontId::proportional(11.5), ui::theme::ACCENT2);
                painter.text(egui::pos2(x + hp_w * 0.5, lbl_y), egui::Align2::CENTER_CENTER,
                    "HP", egui::FontId::proportional(8.5), ui::theme::TEXT_FAINT);
            }
        });
    });

    ui.add_space(8.0);

    // ── Main Stats ────────────────────────────────────────────────────────────
    let has_main = s.max_hp.is_some() || s.attack.is_some() || s.strength.is_some() || s.endurance.is_some();
    if has_main {
        ui.label(egui::RichText::new("MAIN STATS").size(9.0).color(ui::theme::TEXT_FAINT));
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            if let Some(v) = s.max_hp   { stat_chip(ui, "MAX HP",    &fmt_stat_i64(v)); }
            if let Some(v) = s.attack   { stat_chip(ui, "ATK",       &fmt_stat_i64(v)); }
            if let Some(v) = s.armor    { stat_chip(ui, "DEF",       &fmt_stat_i64(v)); }
            if let Some(v) = s.strength { stat_chip(ui, "STR",       &v.to_string()); }
            if let Some(v) = s.endurance{ stat_chip(ui, "END",       &v.to_string()); }
        });
        ui.add_space(8.0);
    }

    // ── Attributes ────────────────────────────────────────────────────────────
    let pct = |raw: Option<u32>, p: Option<u32>| -> Option<String> {
        match (raw, p) {
            (_, Some(v)) => Some(format!("{:.1}%", v as f32 / 100.0)),
            (Some(v), _) => Some(format!("{:.1}%", v as f32 / 100.0)),
            _            => None,
        }
    };
    let has_attrs = s.crit_pct.is_some() || s.crit.is_some()
        || s.haste_pct.is_some() || s.haste.is_some()
        || s.luck_pct.is_some() || s.mastery_pct.is_some()
        || s.versatility_pct.is_some() || s.block_pct.is_some()
        || s.atk_speed_pct.is_some() || s.cast_speed_pct.is_some()
        || s.crit_damage.is_some();
    if has_attrs {
        ui.label(egui::RichText::new("ATTRIBUTES").size(9.0).color(ui::theme::TEXT_FAINT));
        ui.add_space(3.0);
        ui.horizontal_wrapped(|ui| {
            if let Some(v) = pct(s.crit, s.crit_pct)             { stat_chip(ui, "CRIT",      &v); }
            if let Some(v) = pct(s.haste, s.haste_pct)           { stat_chip(ui, "HASTE",     &v); }
            if let Some(v) = pct(s.luck, s.luck_pct)             { stat_chip(ui, "LUCK",      &v); }
            if let Some(v) = pct(s.mastery, s.mastery_pct)       { stat_chip(ui, "MASTERY",   &v); }
            if let Some(v) = pct(s.versatility, s.versatility_pct){ stat_chip(ui, "VERS",     &v); }
            if let Some(v) = pct(s.block, s.block_pct)           { stat_chip(ui, "BLOCK",     &v); }
            if let Some(v) = s.atk_speed_pct  { stat_chip(ui, "ATK SPD",  &format!("{:.1}%", v as f32 / 100.0)); }
            if let Some(v) = s.cast_speed_pct { stat_chip(ui, "CAST SPD", &format!("{:.1}%", v as f32 / 100.0)); }
            if let Some(v) = s.crit_damage    { stat_chip(ui, "CRIT DMG", &format!("{:.1}%", v as f32 / 100.0)); }
        });
    }
}

fn fmt_stat_i64(v: i64) -> String {
    if v >= 1_000_000 { format!("{:.2}M", v as f64 / 1_000_000.0) }
    else if v >= 1_000 { format!("{:.1}K", v as f64 / 1_000.0) }
    else { v.to_string() }
}

fn stat_chip(ui: &mut egui::Ui, label: &str, value: &str) {
    egui::Frame::none()
        .fill(ui::theme::BG_INSET)
        .stroke(egui::Stroke::new(1.0, ui::theme::LINE))
        .rounding(6.0)
        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).size(8.5).color(ui::theme::TEXT_FAINT));
            ui.label(egui::RichText::new(value).size(11.5).strong().color(ui::theme::TEXT));
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
