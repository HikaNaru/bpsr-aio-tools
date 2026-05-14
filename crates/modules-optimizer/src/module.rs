use core::module::{Module, ModuleContext};
use game::event::{GameEvent, PlayerModule};

pub struct OptimizerModule {
    modules: Vec<PlayerModule>,
    has_data: bool,
}

impl OptimizerModule {
    pub fn new() -> Self {
        Self { modules: Vec::new(), has_data: false }
    }

    pub fn push_event(&mut self, event: GameEvent) {
        if let GameEvent::PlayerInventory { modules, .. } = event {
            self.modules = modules;
            self.has_data = true;
        }
    }
}

fn effect_name(id: i32) -> &'static str {
    match id {
        1110 => "HP Boost I",
        1111 => "HP Boost II",
        1112 => "HP Boost III",
        1113 => "HP Boost IV",
        1114 => "HP Boost V",
        1205 => "ATK Boost I",
        1206 => "ATK Boost II",
        1307 => "DEF Boost I",
        1308 => "DEF Boost II",
        1407 => "SPD Boost I",
        1408 => "SPD Boost II",
        1409 => "SPD Boost III",
        1410 => "SPD Boost IV",
        2104 => "Crit Rate Up I",
        2105 => "Crit Rate Up II",
        2204 => "Crit DMG Up I",
        2205 => "Crit DMG Up II",
        2304 => "Skill DMG Up",
        2404 => "Combo DMG Up I",
        2405 => "Combo DMG Up II",
        2406 => "Combo DMG Up III",
        _    => "Unknown Effect",
    }
}

impl Module for OptimizerModule {
    fn id(&self)   -> &'static str { "optimizer" }
    fn name(&self) -> &str         { "Module Optimizer" }
    fn icon(&self) -> &str         { "🔧" }

    fn update(&mut self, _ctx: &ModuleContext) {}

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        if !self.has_data {
            ui.centered_and_justified(|ui| {
                ui.label("Waiting for module data…\nLog in or re-enter a zone.");
            });
            return;
        }

        if self.modules.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label("No modules equipped.");
            });
            return;
        }

        egui::ScrollArea::vertical().show(ui, |ui| {
            for (i, module) in self.modules.iter().enumerate() {
                ui.group(|ui| {
                    ui.strong(format!("Module {}", i + 1));
                    ui.add_space(2.0);
                    egui::Grid::new(format!("mod_{i}"))
                        .num_columns(2)
                        .striped(true)
                        .show(ui, |ui| {
                            for effect in &module.effects {
                                ui.label(effect_name(effect.effect_id));
                                ui.label(format!("Lv. {}", effect.level));
                                ui.end_row();
                            }
                        });
                });
                ui.add_space(4.0);
            }
        });
    }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}
