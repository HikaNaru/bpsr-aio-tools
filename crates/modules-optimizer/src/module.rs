use core::module::{Module, ModuleContext};
use crossbeam_channel::{bounded, Receiver};
use game::event::{GameEvent, PlayerModule};
use ui::theme;

use crate::optimizer::{self, ModComboResult, SolverConfig, StatMode, StatPriority};

const ICON_SIZE: f32 = 28.0;
const SMALL_ICON_SIZE: f32 = 18.0;
const TEXT_SIZE: f32 = 12.0;
const INVENTORY_CARD_WIDTH: f32 = 220.0;

fn effect_name(id: i32) -> &'static str {
    core::DATA.effect_name(id).unwrap_or("Unknown")
}

/// Visual rarity color for a module instance, driven by how many stats it actually
/// rolled — NOT the same as `DATA.mod_quality_tier` (the ModTable base tier used for
/// solver filtering). In-game, a 3-stat module reads as Legendary/gold and a 2-stat
/// module reads as Epic/purple regardless of its base tier's own label.
fn module_rarity_color(effect_count: usize) -> egui::Color32 {
    match effect_count {
        n if n >= 3 => theme::quality_color(4), // gold — Legendary
        2 => theme::quality_color(3),           // purple — Epic
        1 => theme::quality_color(2),           // blue — Rare
        _ => theme::TEXT_MUTED,
    }
}

fn icon_placeholder(ui: &mut egui::Ui, size: f32, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color.gamma_multiply(0.25));
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, color));
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum OptimizerTab {
    Optimizer,
    Inventory,
    Settings,
}

impl OptimizerTab {
    fn label(self) -> &'static str {
        match self {
            Self::Optimizer => "Optimizer",
            Self::Inventory => "Module Inventory",
            Self::Settings => "Settings",
        }
    }
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

    active_tab: OptimizerTab,
    icons: ui::icons::IconCache,
}

impl OptimizerModule {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            has_data: false,
            config: SolverConfig::default(),
            add_stat_selected: 0,
            results: Vec::new(),
            calc_state: CalcState::Idle,
            preset_code: String::new(),
            preset_error: String::new(),
            active_tab: OptimizerTab::Optimizer,
            icons: ui::icons::IconCache::default(),
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

    // ── Tab bar ──────────────────────────────────────────────────────────────

    fn render_tab_bar(&mut self, ui: &mut egui::Ui) {
        let tabs = [OptimizerTab::Optimizer, OptimizerTab::Inventory, OptimizerTab::Settings];
        ui.horizontal(|ui| {
            for tab in tabs {
                if ui.selectable_label(self.active_tab == tab, tab.label()).clicked() {
                    self.active_tab = tab;
                }
            }
        });
    }

    // ── Optimizer tab ────────────────────────────────────────────────────────

    fn render_optimizer_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, max_height: f32) {
        ui.columns(2, |cols| {
            egui::ScrollArea::vertical().id_salt("optimizer_config_scroll").max_height(max_height).show(&mut cols[0], |ui| {
                self.render_config_panel(ui, ctx);
            });
            egui::ScrollArea::vertical().id_salt("optimizer_results_scroll").max_height(max_height).show(&mut cols[1], |ui| {
                self.render_results_panel(ui, ctx);
            });
        });
    }

    fn render_config_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(egui::RichText::new("Config").strong().size(14.0).color(theme::TEXT));
        ui.add_space(4.0);

        egui::Frame::group(ui.style())
            .fill(theme::BG_PANEL)
            .inner_margin(egui::Margin::same(6.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Quality").strong().small().color(theme::TEXT_MUTED));
                ui.add_space(2.0);
                self.render_quality_filters(ui);
            });

        ui.add_space(6.0);

        egui::Frame::group(ui.style())
            .fill(theme::BG_PANEL)
            .inner_margin(egui::Margin::same(6.0))
            .show(ui, |ui| {
                ui.label(egui::RichText::new("Stat Priority").strong().small().color(theme::TEXT_MUTED));
                ui.add_space(2.0);

                let avail = self.available_effects();
                if !avail.contains(&self.add_stat_selected) {
                    if let Some(&first) = avail.first() {
                        self.add_stat_selected = first;
                    }
                }

                let mut to_remove: Option<usize> = None;
                let mut swap: Option<(usize, usize)> = None;
                let prio_len = self.config.priorities.len();

                egui::Grid::new("prio_grid")
                    .num_columns(5)
                    .striped(true)
                    .show(ui, |ui| {
                        for i in 0..prio_len {
                            self.render_stat_priority_row(ui, ctx, i, prio_len, &mut swap, &mut to_remove);
                            ui.end_row();
                        }
                    });

                if let Some(i) = to_remove { self.config.priorities.remove(i); }
                if let Some((a, b)) = swap { self.config.priorities.swap(a, b); }

                ui.add_space(4.0);
                self.render_add_stat_row(ui, ctx, &avail);
            });
    }

    fn render_quality_filters(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.config.quality_filter.basic,
                egui::RichText::new("Basic").color(theme::quality_color(2)));
            ui.checkbox(&mut self.config.quality_filter.advanced,
                egui::RichText::new("Advanced").color(theme::quality_color(3)));
            ui.checkbox(&mut self.config.quality_filter.excellent,
                egui::RichText::new("Excellent").color(theme::quality_color(4)));
        });
    }

    fn render_stat_priority_row(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        i: usize,
        prio_len: usize,
        swap: &mut Option<(usize, usize)>,
        to_remove: &mut Option<usize>,
    ) {
        let effect_id = self.config.priorities[i].effect_id;

        // Up / down
        ui.vertical(|ui| {
            ui.set_min_width(18.0);
            if ui.small_button(egui_phosphor::regular::CARET_UP).clicked() && i > 0 {
                *swap = Some((i - 1, i));
            }
            if ui.small_button(egui_phosphor::regular::CARET_DOWN).clicked() && i + 1 < prio_len {
                *swap = Some((i, i + 1));
            }
        });

        // Icon + name
        ui.horizontal(|ui| {
            let icon_path = core::DATA.effect_icon(effect_id);
            match icon_path.and_then(|p| self.icons.get(ctx, "Modules", p)) {
                Some(tex) => { ui.image((tex.id(), egui::vec2(ICON_SIZE, ICON_SIZE))); }
                None => icon_placeholder(ui, ICON_SIZE, theme::ACCENT2),
            }
            ui.label(egui::RichText::new(effect_name(effect_id)).size(TEXT_SIZE).color(theme::TEXT));
        });

        // Mode toggle
        {
            let prio = &mut self.config.priorities[i];
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
        }

        // Req level
        ui.add(egui::DragValue::new(&mut self.config.priorities[i].req_level)
            .range(0u8..=20u8)
            .prefix("min: "));

        // Remove
        if ui.button(egui_phosphor::regular::TRASH).on_hover_text("Remove").clicked() {
            *to_remove = Some(i);
        }
    }

    fn render_add_stat_row(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, avail: &[i32]) {
        ui.horizontal(|ui| {
            let icon_path = core::DATA.effect_icon(self.add_stat_selected);
            match icon_path.and_then(|p| self.icons.get(ctx, "Modules", p)) {
                Some(tex) => { ui.image((tex.id(), egui::vec2(SMALL_ICON_SIZE, SMALL_ICON_SIZE))); }
                None => icon_placeholder(ui, SMALL_ICON_SIZE, theme::TEXT_FAINT),
            }
            egui::ComboBox::from_id_salt("add_stat_combo")
                .selected_text(effect_name(self.add_stat_selected))
                .width(140.0)
                .show_ui(ui, |ui| {
                    for &eid in avail {
                        let already = self.config.priorities.iter().any(|p| p.effect_id == eid);
                        if !already {
                            ui.selectable_value(&mut self.add_stat_selected, eid, effect_name(eid));
                        }
                    }
                });
            if ui.button(format!("{} Add", egui_phosphor::regular::PLUS)).clicked() {
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
    }

    fn render_results_panel(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(egui::RichText::new("Results").strong().size(14.0).color(theme::TEXT));
        ui.add_space(4.0);

        if self.results.is_empty() {
            if matches!(self.calc_state, CalcState::Done) {
                ui.colored_label(theme::WARN, "No results found. Check that your requirements can be met with current modules.");
            } else {
                ui.label(egui::RichText::new("Press Calculate to generate module combinations.").color(theme::TEXT_FAINT));
            }
            return;
        }

        let results = self.results.clone();
        for (rank, result) in results.iter().enumerate() {
            self.render_result_card(ui, ctx, rank, result);
        }
    }

    fn render_result_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, rank: usize, result: &ModComboResult) {
        egui::Frame::group(ui.style())
            .fill(theme::BG_PANEL2)
            .inner_margin(egui::Margin::same(6.0))
            .show(ui, |ui| {
                let header = format!(
                    "Result: {} (Ability Score: {}) [ZScore: {}]",
                    rank + 1, result.ability_score, result.score,
                );
                egui::CollapsingHeader::new(egui::RichText::new(header).color(theme::ACCENT))
                    .id_salt(("opt_result", rank))
                    .default_open(rank == 0)
                    .show(ui, |ui| {
                        self.render_stat_tiles(ui, ctx, &result.all_stats);
                        ui.add_space(4.0);
                        ui.separator();
                        self.render_module_breakdown(ui, ctx, result);
                    });
            });
        ui.add_space(4.0);
    }

    fn render_stat_tiles(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, stats: &[(i32, u8)]) {
        ui.horizontal_wrapped(|ui| {
            for &(effect_id, total) in stats {
                ui.vertical(|ui| {
                    ui.set_width(ICON_SIZE + 24.0);
                    let icon_path = core::DATA.effect_icon(effect_id);
                    match icon_path.and_then(|p| self.icons.get(ctx, "Modules", p)) {
                        Some(tex) => { ui.image((tex.id(), egui::vec2(ICON_SIZE, ICON_SIZE))); }
                        None => icon_placeholder(ui, ICON_SIZE, theme::ACCENT2),
                    }
                    ui.label(egui::RichText::new(effect_name(effect_id)).size(TEXT_SIZE).color(theme::TEXT_MUTED));
                    ui.label(egui::RichText::new(format!("+{total}")).size(TEXT_SIZE).strong().color(theme::GOOD));
                });
                ui.add_space(6.0);
            }
        });
    }

    fn render_module_breakdown(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, result: &ModComboResult) {
        ui.label(egui::RichText::new("Modules").strong().small().color(theme::TEXT_MUTED));
        for &idx in &result.module_indices {
            let Some(module) = self.modules.get(idx).cloned() else { continue };
            let config_id = module.config_id;
            let name = core::DATA.mod_display_name(config_id).unwrap_or_else(|| "Unknown Module".to_string());
            let tier = core::DATA.mod_quality_tier(config_id).unwrap_or(0);
            let mod_type = core::DATA.mod_type(config_id).unwrap_or(0);
            let icon_name = core::DATA.mod_type_icon_name(mod_type, tier);
            let rarity_color = module_rarity_color(module.effects.len());

            ui.add_space(4.0);
            ui.horizontal(|ui| {
                match icon_name.and_then(|n| self.icons.get(ctx, "Modules", &n)) {
                    Some(tex) => { ui.image((tex.id(), egui::vec2(ICON_SIZE, ICON_SIZE))); }
                    None => icon_placeholder(ui, ICON_SIZE, rarity_color),
                }
                ui.label(egui::RichText::new(&name).strong().size(TEXT_SIZE).color(rarity_color));
            });

            let stats: Vec<(i32, u8)> = module.effects.iter()
                .map(|e| (e.effect_id, e.level.max(0) as u8))
                .collect();
            self.render_stat_tiles(ui, ctx, &stats);
        }
    }

    // ── Module Inventory tab ─────────────────────────────────────────────────

    fn render_inventory_tab(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, max_height: f32) {
        ui.label(egui::RichText::new(format!("Total: {}", self.modules.len()))
            .strong().color(theme::TEXT));
        ui.add_space(4.0);

        let mut sorted: Vec<PlayerModule> = self.modules.clone();
        sorted.sort_by_key(|m| {
            let mod_type = core::DATA.mod_type(m.config_id).unwrap_or(0);
            (std::cmp::Reverse(m.effects.len()), mod_type)
        });

        // Responsive grid: number of columns depends on how wide the panel currently
        // is, so the inventory reflows as the app window is resized.
        let card_width = INVENTORY_CARD_WIDTH + 2.0 * ui.spacing().item_spacing.x;
        let columns = ((ui.available_width() / card_width).floor() as usize).max(1);

        egui::ScrollArea::vertical().id_salt("optimizer_inventory_scroll").max_height(max_height).show(ui, |ui| {
            for chunk in sorted.chunks(columns) {
                ui.columns(columns, |cols| {
                    for (col, module) in cols.iter_mut().zip(chunk.iter()) {
                        self.render_inventory_card(col, ctx, module);
                    }
                });
                ui.add_space(4.0);
            }
        });
    }

    fn render_inventory_card(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, module: &PlayerModule) {
        let config_id = module.config_id;
        let name = core::DATA.mod_display_name(config_id).unwrap_or_else(|| "Unknown Module".to_string());
        let tier = core::DATA.mod_quality_tier(config_id).unwrap_or(0);
        let mod_type = core::DATA.mod_type(config_id).unwrap_or(0);
        let icon_name = core::DATA.mod_type_icon_name(mod_type, tier);
        let rarity_color = module_rarity_color(module.effects.len());

        egui::Frame::group(ui.style())
            .fill(theme::BG_PANEL2)
            .inner_margin(egui::Margin::same(5.0))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    match icon_name.and_then(|n| self.icons.get(ctx, "Modules", &n)) {
                        Some(tex) => { ui.image((tex.id(), egui::vec2(ICON_SIZE, ICON_SIZE))); }
                        None => icon_placeholder(ui, ICON_SIZE, rarity_color),
                    }
                    ui.label(egui::RichText::new(&name).strong().size(TEXT_SIZE).color(rarity_color));
                });
                let stats: Vec<(i32, u8)> = module.effects.iter()
                    .map(|e| (e.effect_id, e.level.max(0) as u8))
                    .collect();
                self.render_stat_tiles(ui, ctx, &stats);
            });
    }

    // ── Settings tab ─────────────────────────────────────────────────────────

    fn render_settings_tab(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Presets").strong().color(theme::TEXT));
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Preset code:");
            ui.add(egui::TextEdit::singleline(&mut self.preset_code)
                .hint_text("ZMO:…")
                .desired_width(240.0));
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
            ui.colored_label(theme::BAD, &self.preset_error);
        }

        ui.add_space(8.0);
        ui.separator();
        ui.label(egui::RichText::new("Solver").strong().color(theme::TEXT));
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("Slots:");
            ui.add(egui::Slider::new(&mut self.config.num_modules, 1..=5));
        });
        ui.checkbox(&mut self.config.value_all_stats, "Score all stats");
    }

    // ── Status bar ───────────────────────────────────────────────────────────

    fn render_status_bar(&mut self, ui: &mut egui::Ui) {
        let is_running = matches!(self.calc_state, CalcState::Running(_));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{} modules", self.modules.len())).color(theme::TEXT_MUTED));
            if matches!(self.calc_state, CalcState::Done) {
                ui.label(egui::RichText::new(format!("· {} results", self.results.len())).color(theme::TEXT_MUTED));
            }

            let label = if is_running {
                format!("{} Calculating…", egui_phosphor::regular::HOURGLASS)
            } else {
                format!("{} Calculate", egui_phosphor::regular::PLAY)
            };
            let available = ui.available_width();
            let btn = egui::Button::new(egui::RichText::new(label).size(14.0))
                .min_size(egui::vec2(available, 28.0));
            if ui.add_enabled(!is_running, btn).clicked() {
                self.start_calculation();
            }
        });
        ui.add_space(4.0);
    }
}

impl Module for OptimizerModule {
    fn id(&self)   -> &'static str { "optimizer" }
    fn name(&self) -> &str         { "Module Optimizer" }
    fn icon(&self) -> &str         { egui_phosphor::regular::PUZZLE_PIECE }

    fn update(&mut self, _ctx: &ModuleContext) {
        self.poll_result();
    }

    fn ui(&mut self, ui: &mut egui::Ui, egui_ctx: &egui::Context) {
        if !self.has_data {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Waiting for module data…\nLog in or re-enter a zone.")
                    .color(theme::TEXT_FAINT));
            });
            return;
        }

        ui.horizontal(|ui| {
            ui.heading("Module Optimizer");
        });
        ui.separator();
        self.render_tab_bar(ui);
        ui.separator();

        // Pin the Calculate bar to the bottom of the panel first — this both makes
        // it stick to the bottom edge (rather than trailing right after whatever
        // height the tab content happens to be) and shrinks `ui`'s remaining rect,
        // so reading `available_height()` afterward gives the correctly-reduced
        // space left for the tab content's own scroll areas to be capped to
        // (preventing a long results/inventory list from growing past it and
        // painting over the button).
        egui::TopBottomPanel::bottom("optimizer_status_bar")
            .show_inside(ui, |ui| self.render_status_bar(ui));

        let content_height = ui.available_height();

        match self.active_tab {
            OptimizerTab::Optimizer => self.render_optimizer_tab(ui, egui_ctx, content_height),
            OptimizerTab::Inventory => self.render_inventory_tab(ui, egui_ctx, content_height),
            OptimizerTab::Settings  => self.render_settings_tab(ui),
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
