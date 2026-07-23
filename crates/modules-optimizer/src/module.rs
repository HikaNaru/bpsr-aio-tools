use core::module::{Module, ModuleContext};
use crossbeam_channel::{bounded, Receiver};
use game::event::{GameEvent, PlayerModule};

use crate::optimizer::{self, ModComboResult, SolverConfig, StatMode, StatPriority};

// All valid effect IDs observed in the game, with human-readable names.
const EFFECT_NAMES: &[(i32, &str)] = &[
    (1110, "HP Boost I"),
    (1111, "HP Boost II"),
    (1112, "HP Boost III"),
    (1113, "HP Boost IV"),
    (1114, "HP Boost V"),
    (1205, "ATK Boost I"),
    (1206, "ATK Boost II"),
    (1307, "DEF Boost I"),
    (1308, "DEF Boost II"),
    (1407, "SPD Boost I"),
    (1408, "SPD Boost II"),
    (1409, "SPD Boost III"),
    (1410, "SPD Boost IV"),
    (2104, "Crit Rate Up I"),
    (2105, "Crit Rate Up II"),
    (2204, "Crit DMG Up I"),
    (2205, "Crit DMG Up II"),
    (2304, "Skill DMG Up"),
    (2404, "Combo DMG Up I"),
    (2405, "Combo DMG Up II"),
    (2406, "Combo DMG Up III"),
];

fn effect_name(id: i32) -> &'static str {
    EFFECT_NAMES.iter().find(|&&(k, _)| k == id).map(|&(_, v)| v).unwrap_or("Unknown")
}

enum CalcState {
    Idle,
    Running(Receiver<Vec<ModComboResult>>),
    Done,
}

pub struct OptimizerModule {
    modules: Vec<PlayerModule>,
    has_data: bool,

    config: SolverConfig,

    /// Which stat is selected in the "add priority" dropdown.
    add_stat_selected: i32,

    results: Vec<ModComboResult>,
    calc_state: CalcState,

    /// Preset code for copy/paste sharing.
    preset_code: String,
    preset_error: String,
    show_settings: bool,
}

impl OptimizerModule {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            has_data: false,
            config: SolverConfig::default(),
            add_stat_selected: EFFECT_NAMES[0].0,
            results: Vec::new(),
            calc_state: CalcState::Idle,
            preset_code: String::new(),
            preset_error: String::new(),
            show_settings: false,
        }
    }

    pub fn push_event(&mut self, event: GameEvent) {
        if let GameEvent::PlayerInventory { modules, .. } = event {
            self.modules = modules;
            self.has_data = true;
        }
    }

    fn available_effects(&self) -> Vec<i32> {
        let mut ids: Vec<i32> = self.modules.iter()
            .flat_map(|m| m.effects.iter().map(|e| e.effect_id))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        ids.sort_unstable();
        ids
    }

    fn start_calculation(&mut self) {
        if self.modules.is_empty() { return; }
        let modules = self.modules.clone();
        let config  = self.config.clone();
        let (tx, rx) = bounded(1);

        std::thread::Builder::new()
            .name("optimizer".into())
            .spawn(move || {
                let t = std::time::Instant::now();
                let results = optimizer::optimize(&modules, &config);
                tracing::info!("optimizer: {} results in {}ms", results.len(), t.elapsed().as_millis());
                let _ = tx.send(results);
            })
            .expect("optimizer thread");

        self.calc_state = CalcState::Running(rx);
        self.results.clear();
    }

    fn poll_result(&mut self) {
        let done = if let CalcState::Running(rx) = &self.calc_state {
            if let Ok(results) = rx.try_recv() {
                self.results = results;
                self.calc_state = CalcState::Done;
                true
            } else { false }
        } else { false };
        let _ = done;
    }

    fn encode_preset(&self) -> String {
        let parts: Vec<String> = self.config.priorities.iter().map(|p| {
            format!("{}-{}-{}", p.effect_id,
                if p.mode == StatMode::AtLeast { 0 } else { 1 },
                p.req_level)
        }).collect();
        format!("ZMO:{}", parts.join(","))
    }

    fn decode_preset(&mut self, code: &str) -> Result<(), String> {
        if !code.starts_with("ZMO:") {
            return Err("Invalid preset code (must start with ZMO:)".into());
        }
        let inner = &code[4..];
        let mut prios = Vec::new();
        for part in inner.split(',') {
            let p: Vec<&str> = part.split('-').collect();
            if p.len() != 3 {
                return Err(format!("Bad segment: {part}"));
            }
            let eid: i32 = p[0].parse().map_err(|_| format!("Bad effect_id: {}", p[0]))?;
            let mode_n: u8 = p[1].parse().map_err(|_| "Bad mode".to_string())?;
            let req: u8 = p[2].parse().map_err(|_| "Bad req_level".to_string())?;
            prios.push(StatPriority {
                effect_id: eid,
                req_level: req,
                mode: if mode_n == 0 { StatMode::AtLeast } else { StatMode::Exactly },
            });
        }
        self.config.priorities = prios;
        Ok(())
    }
}

impl Module for OptimizerModule {
    fn id(&self)   -> &'static str { "optimizer" }
    fn name(&self) -> &str         { "Module Optimizer" }
    fn icon(&self) -> &str         { egui_phosphor::regular::PUZZLE_PIECE }

    fn update(&mut self, _ctx: &ModuleContext) {
        self.poll_result();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        if !self.has_data {
            ui.centered_and_justified(|ui| {
                ui.label("Waiting for module data…\nLog in or re-enter a zone.");
            });
            return;
        }

        ui.horizontal(|ui| {
            ui.heading("Module Optimizer");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(egui_phosphor::regular::GEAR).on_hover_text("Settings / Presets").clicked() {
                    self.show_settings = !self.show_settings;
                }
            });
        });
        ui.separator();

        if self.show_settings {
            ui.collapsing("Settings & Presets", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Preset code:");
                    ui.add(egui::TextEdit::singleline(&mut self.preset_code)
                        .hint_text("ZMO:…")
                        .desired_width(200.0));
                });
                ui.horizontal(|ui| {
                    if ui.button("Copy my preset").clicked() {
                        self.preset_code = self.encode_preset();
                        ui.output_mut(|o| o.copied_text = self.preset_code.clone());
                    }
                    if ui.button("Import preset").clicked() {
                        let code = self.preset_code.trim().to_string();
                        match self.decode_preset(&code) {
                            Ok(_) => self.preset_error.clear(),
                            Err(e) => self.preset_error = e,
                        }
                    }
                });
                if !self.preset_error.is_empty() {
                    ui.colored_label(egui::Color32::RED, &self.preset_error);
                }
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Slots:");
                    ui.add(egui::Slider::new(&mut self.config.num_modules, 1..=5));
                    ui.checkbox(&mut self.config.value_all_stats, "Score all stats");
                });
            });
        }

        // ── Priority editor ──────────────────────────────────────────────────
        ui.label(egui::RichText::new("Stat Priorities").strong());
        ui.add_space(2.0);

        let avail = self.available_effects();
        let mut to_remove: Option<usize> = None;
        let mut swap: Option<(usize, usize)> = None;

        let prio_len = self.config.priorities.len();
        egui::Grid::new("prio_grid")
            .num_columns(5)
            .striped(true)
            .show(ui, |ui| {
                for (i, prio) in self.config.priorities.iter_mut().enumerate() {
                    // Up / down
                    ui.vertical(|ui| {
                        ui.set_min_width(18.0);
                        if ui.small_button(egui_phosphor::regular::CARET_UP).clicked() && i > 0 {
                            swap = Some((i - 1, i));
                        }
                        if ui.small_button(egui_phosphor::regular::CARET_DOWN).clicked() && i + 1 < prio_len {
                            swap = Some((i, i + 1));
                        }
                    });

                    // Stat name
                    ui.label(effect_name(prio.effect_id));

                    // Mode toggle
                    let mode_lbl = match prio.mode {
                        StatMode::AtLeast => "≥",
                        StatMode::Exactly => "=",
                    };
                    if ui.button(mode_lbl).on_hover_text("Toggle AtLeast / Exactly").clicked() {
                        prio.mode = match prio.mode {
                            StatMode::AtLeast => StatMode::Exactly,
                            StatMode::Exactly => StatMode::AtLeast,
                        };
                    }

                    // Req level
                    ui.add(egui::DragValue::new(&mut prio.req_level)
                        .range(0u8..=20u8)
                        .prefix("min: "));

                    // Remove
                    if ui.button(egui_phosphor::regular::X).clicked() {
                        to_remove = Some(i);
                    }

                    ui.end_row();
                }
            });

        if let Some(i) = to_remove { self.config.priorities.remove(i); }
        if let Some((a, b)) = swap { self.config.priorities.swap(a, b); }

        ui.add_space(4.0);

        // Add stat row
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("add_stat_combo")
                .selected_text(effect_name(self.add_stat_selected))
                .width(140.0)
                .show_ui(ui, |ui| {
                    for &eid in &avail {
                        let already = self.config.priorities.iter().any(|p| p.effect_id == eid);
                        if !already {
                            ui.selectable_value(&mut self.add_stat_selected, eid, effect_name(eid));
                        }
                    }
                });
            if ui.button("+ Add Priority").clicked() {
                let already = self.config.priorities.iter().any(|p| p.effect_id == self.add_stat_selected);
                if !already && avail.contains(&self.add_stat_selected) {
                    self.config.priorities.push(StatPriority {
                        effect_id: self.add_stat_selected,
                        req_level: 0,
                        mode: StatMode::AtLeast,
                    });
                }
            }
        });

        ui.separator();

        // ── Calculate button ─────────────────────────────────────────────────
        let is_running = matches!(self.calc_state, CalcState::Running(_));
        ui.horizontal(|ui| {
            let calc_btn = egui::Button::new(
                if is_running { format!("{} Calculating…", egui_phosphor::regular::HOURGLASS) } else { format!("{} Calculate", egui_phosphor::regular::PLAY) }
            );
            if ui.add_enabled(!is_running, calc_btn).clicked() {
                self.start_calculation();
            }
            ui.label(format!("({} modules)", self.modules.len()));
        });

        // ── Results ──────────────────────────────────────────────────────────
        if !self.results.is_empty() {
            ui.separator();
            ui.label(egui::RichText::new(format!("Top {} Results", self.results.len())).strong());

            egui::ScrollArea::vertical()
                .id_salt("opt_results")
                .show(ui, |ui| {
                    for (rank, result) in self.results.iter().enumerate() {
                        ui.collapsing(
                            format!("#{} — Score: {}", rank + 1, result.score),
                            |ui| {
                                // Stat breakdown for priority stats
                                if !result.priority_stats.is_empty() {
                                    ui.label(egui::RichText::new("Priority Stats").strong().small());
                                    egui::Grid::new(format!("result_prio_{rank}"))
                                        .num_columns(3)
                                        .striped(true)
                                        .show(ui, |ui| {
                                            for (eid, total) in &result.priority_stats {
                                                ui.label(effect_name(*eid));
                                                ui.label(format!("{total}"));
                                                ui.label(breakpoint_label(*total));
                                                ui.end_row();
                                            }
                                        });
                                    ui.add_space(4.0);
                                }
                                // All stats
                                if !result.all_stats.is_empty() {
                                    ui.label(egui::RichText::new("All Stats").strong().small());
                                    egui::Grid::new(format!("result_all_{rank}"))
                                        .num_columns(2)
                                        .striped(true)
                                        .show(ui, |ui| {
                                            for (eid, total) in &result.all_stats {
                                                ui.label(effect_name(*eid));
                                                ui.label(format!("{total}"));
                                                ui.end_row();
                                            }
                                        });
                                }
                            },
                        );
                    }
                });
        } else if matches!(self.calc_state, CalcState::Done) {
            ui.separator();
            ui.colored_label(egui::Color32::YELLOW,
                "No results found. Check that your requirements can be met with current modules.");
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

fn breakpoint_label(total: u8) -> String {
    let star = egui_phosphor::regular::STAR;
    match total {
        t if t >= 20 => format!("{}{}{}{}{}{} (max)", star, star, star, star, star, star),
        t if t >= 16 => star.repeat(5),
        t if t >= 12 => star.repeat(4),
        t if t >= 8  => star.repeat(3),
        t if t >= 4  => star.repeat(2),
        t if t >= 1  => star.to_string(),
        _            => "—".to_string(),
    }
}
