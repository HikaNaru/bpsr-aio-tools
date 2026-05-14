use core::module::{Module, ModuleContext};

pub struct OptimizerModule;

impl OptimizerModule {
    pub fn new() -> Self {
        Self
    }
}

impl Module for OptimizerModule {
    fn id(&self)   -> &'static str { "optimizer" }
    fn name(&self) -> &str         { "Optimizer" }
    fn icon(&self) -> &str         { "🔧" }

    fn update(&mut self, _ctx: &ModuleContext) {}

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        ui.centered_and_justified(|ui| {
            ui.label("Build Optimizer — coming soon.");
        });
    }
}
