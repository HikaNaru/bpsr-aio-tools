use core::module::Module;
use egui::{Color32, FontId, Ui};

const SIDEBAR_WIDTH: f32 = 140.0;
const BTN_SIZE:      f32 = 40.0;

pub struct SidebarResult {
    pub active:           usize,
    pub settings_clicked: bool,
    pub debug_clicked:    bool,
}

/// Renders icon sidebar.
pub fn sidebar(
    ui: &mut Ui,
    modules: &[Box<dyn Module>],
    active: usize,
    show_settings: bool,
    show_debug: bool,
    debug_enabled: bool,
) -> SidebarResult {
    let mut result = SidebarResult {
        active,
        settings_clicked: false,
        debug_clicked: false,
    };

    ui.allocate_ui_with_layout(
        egui::vec2(SIDEBAR_WIDTH, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.painter().rect_filled(ui.clip_rect(), 0.0, egui::Color32::from_rgb(20, 22, 26));

            // Module buttons
            for (i, module) in modules.iter().enumerate() {
                let selected = !show_settings && !show_debug && i == active;
                sidebar_btn(ui, module.icon(), module.name(), selected, |_| {
                    result.active = i;
                    result.settings_clicked = false;
                    result.debug_clicked = false;
                });
            }

            // Bottom buttons (settings + optional debug)
            let bottom_count = if debug_enabled { 2 } else { 1 };
            let bottom_height = BTN_SIZE * bottom_count as f32;
            let space = ui.available_height() - bottom_height;
            if space > 0.0 {
                ui.add_space(space);
            }

            if debug_enabled {
                sidebar_btn(ui, "🐛", "Debug", show_debug, |_| {
                    result.debug_clicked = true;
                });
            }

            sidebar_btn(ui, "⚙", "Settings", show_settings, |_| {
                result.settings_clicked = true;
            });
        },
    );

    result
}

fn sidebar_btn(
    ui: &mut Ui,
    icon: &str,
    label: &str,
    selected: bool,
    on_click: impl FnOnce(&egui::Response),
) {
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
        on_click(&response);
    }
    let draw_bg = if response.hovered() && !selected {
        Color32::from_rgb(40, 42, 50)
    } else {
        bg
    };
    ui.painter().rect_filled(rect, 4.0, draw_bg);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{icon} {label}"),
        FontId::proportional(11.0),
        Color32::from_rgb(200, 200, 210),
    );
}
