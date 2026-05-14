use egui::{Color32, Ui};

pub fn status_dot(ui: &mut Ui, connected: bool, label: &str) {
    let color = if connected {
        Color32::from_rgb(80, 200, 100)
    } else {
        Color32::from_rgb(200, 80, 80)
    };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    ui.label(label);
}
