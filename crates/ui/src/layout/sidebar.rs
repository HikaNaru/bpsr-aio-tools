use core::module::Module;
use egui::{Color32, Ui};

const SIDEBAR_WIDTH: f32 = 52.0;
const BTN_SIZE:      f32 = 44.0;

/// Renders icon sidebar. Returns new active index if changed.
pub fn sidebar(ui: &mut Ui, modules: &[Box<dyn Module>], active: usize) -> usize {
    let mut next = active;
    ui.allocate_ui_with_layout(
        egui::vec2(SIDEBAR_WIDTH, ui.available_height()),
        egui::Layout::top_down(egui::Align::Center),
        |ui| {
            ui.set_width(SIDEBAR_WIDTH);
            for (i, module) in modules.iter().enumerate() {
                let selected = i == active;
                let bg = if selected {
                    Color32::from_rgb(50, 80, 120)
                } else {
                    Color32::TRANSPARENT
                };
                let (rect, response) = ui.allocate_exact_size(
                    egui::vec2(SIDEBAR_WIDTH, BTN_SIZE),
                    egui::Sense::click(),
                );
                if response.clicked() {
                    next = i;
                }
                ui.painter().rect_filled(rect, 4.0, bg);
                ui.allocate_ui_at_rect(rect, |ui| {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new(format!("{}\n{}", module.icon(), module.name()))
                                .size(11.0),
                        );
                    });
                });
            }
        },
    );
    next
}
