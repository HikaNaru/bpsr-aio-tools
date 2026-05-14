use egui::{Color32, FontDefinitions, Style, Visuals};

pub const ACCENT:      Color32 = Color32::from_rgb(70, 130, 180);
pub const DAMAGE_BAR:  Color32 = Color32::from_rgb(70, 130, 180);
pub const BG_DARK:     Color32 = Color32::from_rgb(20, 22, 26);
pub const TEXT_DIM:    Color32 = Color32::from_rgb(160, 160, 170);

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill       = BG_DARK;
    visuals.window_fill      = BG_DARK;
    visuals.override_text_color = None;
    ctx.set_visuals(visuals);
}
