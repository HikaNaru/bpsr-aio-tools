use crate::AppConfig;

pub struct ModuleContext<'a> {
    pub config: &'a AppConfig,
    pub dt:     std::time::Duration,
}

pub trait Module: Send + Sync + 'static {
    fn id(&self) -> &'static str;
    fn name(&self) -> &str;
    fn icon(&self) -> &str;

    fn update(&mut self, ctx: &ModuleContext);
    fn ui(&mut self, ui: &mut egui::Ui, egui_ctx: &egui::Context);

    fn is_active(&self) -> bool { true }
    fn on_enable(&mut self) {}
    fn on_disable(&mut self) {}

    /// For downcasting to concrete module types in app.rs.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}
