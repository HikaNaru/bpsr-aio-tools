use crate::theme;
use core::module::Module;
use egui::{Color32, FontId, Rounding, Ui, Vec2};

const SIDEBAR_W: f32 = 150.0;
const BTN_H:     f32 = 44.0;

pub struct SidebarResult {
    pub active:           usize,
    pub settings_clicked: bool,
    pub debug_clicked:    bool,
}

pub fn sidebar(
    ui: &mut Ui,
    modules: &[Box<dyn Module>],
    active: usize,
    show_settings: bool,
    show_debug: bool,
    debug_enabled: bool,
    alpha: u8,
) -> SidebarResult {
    let mut result = SidebarResult {
        active,
        settings_clicked: false,
        debug_clicked:    false,
    };

    let fade = |c: Color32| -> Color32 {
        let a = ((c.a() as u16 * alpha as u16) / 255) as u8;
        Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
    };

    ui.allocate_ui_with_layout(
        Vec2::new(SIDEBAR_W, ui.available_height()),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            // Background fill
            ui.painter().rect_filled(ui.clip_rect(), 0.0, fade(theme::BG_CHROME));

            // Module buttons
            for (i, module) in modules.iter().enumerate() {
                let selected = !show_settings && !show_debug && i == active;
                if sidebar_btn(ui, module.icon(), module.name(), selected, &fade) {
                    result.active = i;
                    result.settings_clicked = false;
                    result.debug_clicked = false;
                }
            }

            // Spacer to push bottom buttons down
            let bottom_count = if debug_enabled { 2 } else { 1 };
            let bottom_h = BTN_H * bottom_count as f32;
            let space = ui.available_height() - bottom_h;
            if space > 0.0 {
                ui.add_space(space);
            }

            if debug_enabled {
                if sidebar_btn(ui, "◑", "Debug", show_debug, &fade) {
                    result.debug_clicked = true;
                }
            }

            if sidebar_btn(ui, "⚙", "Settings", show_settings, &fade) {
                result.settings_clicked = true;
            }
        },
    );

    result
}

/// Returns true if clicked.
fn sidebar_btn(ui: &mut Ui, icon: &str, label: &str, selected: bool, fade: &dyn Fn(Color32) -> Color32) -> bool {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(SIDEBAR_W, BTN_H),
        egui::Sense::click(),
    );

    let hovered = response.hovered();

    // Background
    let bg = if selected {
        fade(Color32::from_rgba_premultiplied(91, 140, 255, 30))
    } else if hovered {
        fade(Color32::from_rgba_premultiplied(255, 255, 255, 8))
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, Rounding::same(6.0), bg);

    // Left active stripe + glow layers
    if selected {
        let stripe = egui::Rect::from_min_size(
            rect.min,
            Vec2::new(3.0, BTN_H - 12.0),
        ).translate(Vec2::new(0.0, 6.0));
        // Outer glow (wider, very faint)
        let glow = stripe.expand2(Vec2::new(4.0, 2.0));
        ui.painter().rect_filled(glow, Rounding::same(5.0),
            Color32::from_rgba_premultiplied(91, 140, 255, 20));
        // Mid glow
        let glow2 = stripe.expand2(Vec2::new(2.0, 1.0));
        ui.painter().rect_filled(glow2, Rounding::same(4.0),
            Color32::from_rgba_premultiplied(91, 140, 255, 40));
        // Core stripe
        ui.painter().rect_filled(stripe, Rounding::same(3.0), theme::ACCENT);
    }

    // Icon + label — icon centered in left area, label follows
    let text_color = if selected { theme::TEXT } else { theme::TEXT_MUTED };
    let icon_x = rect.min.x + 22.0;
    let center_y = rect.center().y;

    ui.painter().text(
        egui::pos2(icon_x, center_y),
        egui::Align2::CENTER_CENTER,
        icon,
        FontId::proportional(15.0),
        text_color,
    );
    ui.painter().text(
        egui::pos2(icon_x + 18.0, center_y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(11.5),
        text_color,
    );

    response.clicked()
}
