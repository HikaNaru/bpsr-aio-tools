use egui::{Color32, Ui};

/// Render a proportional damage bar. `fraction` is 0.0..=1.0.
pub fn damage_bar(ui: &mut Ui, fraction: f32, width: f32, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 2.0, Color32::from_gray(40));
    if fraction > 0.0 {
        let mut bar = rect;
        bar.set_right(rect.left() + (rect.width() * fraction.clamp(0.0, 1.0)));
        ui.painter().rect_filled(bar, 2.0, color);
    }
}
