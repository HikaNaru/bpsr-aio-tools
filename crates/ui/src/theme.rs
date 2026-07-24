use egui::{Color32, Visuals};

// Design palette
pub const ACCENT:       Color32 = Color32::from_rgb(91, 140, 255);   // #5b8cff
pub const ACCENT2:      Color32 = Color32::from_rgb(25, 214, 196);   // #19d6c4
pub const GOOD:         Color32 = Color32::from_rgb(95, 191, 138);   // #5fbf8a
pub const WARN:         Color32 = Color32::from_rgb(217, 168,  90);  // #d9a85a
pub const BAD:          Color32 = Color32::from_rgb(217, 107, 107);  // #d96b6b

// Backgrounds
pub const BG_CHROME:    Color32 = Color32::from_rgb(14, 20, 33);     // #0e1421 sidebar
pub const BG_PANEL:     Color32 = Color32::from_rgb(18, 26, 40);     // #121a28 cards
pub const BG_PANEL2:    Color32 = Color32::from_rgb(23, 34, 52);     // #172234
pub const BG_INSET:     Color32 = Color32::from_rgb(13, 19, 32);     // #0d1320 inner
pub const BG_DARK:      Color32 = Color32::from_rgb(10, 11, 14);     // #0a0b0e main bg

// Text
pub const TEXT:         Color32 = Color32::from_rgb(232, 234, 239);  // #e8eaef
pub const TEXT_MUTED:   Color32 = Color32::from_rgb(134, 139, 151);  // #868b97
pub const TEXT_FAINT:   Color32 = Color32::from_rgb(86,  90, 100);   // #565a64

// Lines / borders
pub const LINE:         Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 15); // rgba(255,255,255,0.06)
pub const LINE_ACCENT:  Color32 = Color32::from_rgba_premultiplied(91, 140, 255, 51); // rgba(91,140,255,0.2)

// Legacy aliases used elsewhere
pub const DAMAGE_BAR:   Color32 = ACCENT;

// Gear item quality ramp (ItemEntry.quality 0-5), matching BPSR-ZDPS's
// White/Green/Blue/Purple/Gold/Red convention.
pub const QUALITY_COMMON:    Color32 = Color32::from_rgb(232, 234, 239); // 0 — same as TEXT
pub const QUALITY_UNCOMMON:  Color32 = Color32::from_rgb(95,  191, 138); // 1 — green
pub const QUALITY_RARE:      Color32 = Color32::from_rgb(91,  140, 255); // 2 — blue
pub const QUALITY_EPIC:      Color32 = Color32::from_rgb(178, 122, 255); // 3 — purple
pub const QUALITY_LEGENDARY: Color32 = Color32::from_rgb(217, 168, 90);  // 4 — gold
pub const QUALITY_MYTHIC:    Color32 = Color32::from_rgb(217, 107, 107); // 5 — red

pub fn quality_color(quality: i32) -> Color32 {
    match quality {
        1 => QUALITY_UNCOMMON,
        2 => QUALITY_RARE,
        3 => QUALITY_EPIC,
        4 => QUALITY_LEGENDARY,
        5 => QUALITY_MYTHIC,
        _ => QUALITY_COMMON,
    }
}

pub fn apply(ctx: &egui::Context) {
    let mut visuals = Visuals::dark();
    visuals.panel_fill           = BG_DARK;
    visuals.window_fill          = BG_DARK;
    visuals.override_text_color  = Some(TEXT);
    visuals.widgets.noninteractive.fg_stroke.color = TEXT_MUTED;
    visuals.widgets.inactive.fg_stroke.color       = TEXT_MUTED;
    visuals.widgets.hovered.fg_stroke.color        = TEXT;
    visuals.widgets.active.fg_stroke.color         = TEXT;
    ctx.set_visuals(visuals);
}
