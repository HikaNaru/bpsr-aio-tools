use crate::theme;
use egui::Ui;

pub fn status_dot(ui: &mut Ui, connected: bool, label: &str) {
    let color = if connected { theme::GOOD } else { theme::BAD };
    let (rect, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    ui.label(
        egui::RichText::new(label)
            .size(10.5)
            .color(theme::TEXT_MUTED),
    );
}
