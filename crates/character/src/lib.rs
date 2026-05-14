use core::module::{Module, ModuleContext};
use egui::Color32;
use game::entity::Entity;
use game::state::GameState;
use parking_lot::RwLock;
use std::sync::Arc;

pub struct CharacterModule {
    game_state: Arc<RwLock<GameState>>,
}

impl CharacterModule {
    pub fn new(game_state: Arc<RwLock<GameState>>) -> Self {
        Self { game_state }
    }
}

impl Module for CharacterModule {
    fn id(&self)   -> &'static str { "character" }
    fn name(&self) -> &str         { "Character" }
    fn icon(&self) -> &str         { "👤" }

    fn update(&mut self, _ctx: &ModuleContext) {}

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        let gs = self.game_state.read();
        let Some(local_id) = gs.local_player else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Waiting for character data…").color(Color32::GRAY));
            });
            return;
        };
        let Some(entity) = gs.entities.get(&local_id) else {
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Waiting for character data…").color(Color32::GRAY));
            });
            return;
        };

        egui::ScrollArea::vertical().show(ui, |ui| {
            render_character(ui, entity);
        });
    }
}

fn render_character(ui: &mut egui::Ui, entity: &Entity) {
    let s = &entity.stats;

    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(8.0);
        ui.vertical(|ui| {
            ui.strong(&entity.name);
            if let Some(level) = s.level {
                ui.label(
                    egui::RichText::new(format!("Level {level}"))
                        .small()
                        .color(Color32::from_rgb(180, 180, 200)),
                );
            }
            if let Some(gs) = s.ability_score {
                ui.label(
                    egui::RichText::new(format!("Ability Score: {gs}"))
                        .small()
                        .color(Color32::from_rgb(180, 180, 200)),
                );
            }
        });
    });

    ui.add_space(8.0);
    egui::Grid::new("char_stats_grid")
        .num_columns(2)
        .spacing([16.0, 4.0])
        .striped(true)
        .show(ui, |ui| {
            stat_row_i64(ui, "HP",          s.hp,     s.max_hp);
            ui.end_row();
            stat_row(ui, "ATK",         s.attack.map(|v| v as u64));
            ui.end_row();
            stat_row(ui, "Armor",        s.armor.map(|v| v as u64));
            ui.end_row();
            stat_row(ui, "Strength",     s.strength.map(|v| v as u64));
            ui.end_row();
            stat_row(ui, "Endurance",    s.endurance.map(|v| v as u64));
            ui.end_row();
            stat_row_pct(ui, "Crit",         s.crit,         s.crit_pct);
            ui.end_row();
            stat_row_pct(ui, "Haste",        s.haste,        s.haste_pct);
            ui.end_row();
            stat_row_pct(ui, "Luck",         s.luck,         s.luck_pct);
            ui.end_row();
            stat_row_pct(ui, "Mastery",      s.mastery,      s.mastery_pct);
            ui.end_row();
            stat_row_pct(ui, "Versatility",  s.versatility,  s.versatility_pct);
            ui.end_row();
            stat_row_pct(ui, "Block",        s.block,        s.block_pct);
            ui.end_row();
        });
}

fn stat_label(ui: &mut egui::Ui, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .small()
            .color(Color32::from_rgb(150, 150, 170)),
    );
}

fn stat_row(ui: &mut egui::Ui, label: &str, val: Option<u64>) {
    stat_label(ui, label);
    match val {
        Some(v) => { ui.label(format!("{v}")); }
        None    => { ui.label(egui::RichText::new("—").color(Color32::DARK_GRAY)); }
    }
}

fn stat_row_i64(ui: &mut egui::Ui, label: &str, cur: Option<i64>, max: Option<i64>) {
    stat_label(ui, label);
    match (cur, max) {
        (Some(c), Some(m)) => { ui.label(format!("{c} / {m}")); }
        (Some(c), None)    => { ui.label(format!("{c}")); }
        _                  => { ui.label(egui::RichText::new("—").color(Color32::DARK_GRAY)); }
    }
}

fn stat_row_pct(ui: &mut egui::Ui, label: &str, raw: Option<u32>, pct: Option<u32>) {
    stat_label(ui, label);
    match (raw, pct) {
        (_, Some(p)) => { ui.label(format!("{:.1}%  ({raw_str})", p as f32 / 100.0,
                                            raw_str = raw.map(|r| r.to_string()).unwrap_or_default())); }
        (Some(r), _) => { ui.label(format!("{r}")); }
        _            => { ui.label(egui::RichText::new("—").color(Color32::DARK_GRAY)); }
    }
}
