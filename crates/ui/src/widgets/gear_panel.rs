use crate::icons::IconCache;
use crate::theme;
use core::data_tables::{AttrLine, GearAttrRoll};
use std::collections::HashMap;

/// One equipped gear slot, decoupled from the caller's source type (live
/// `game::event::EquipSlot` cache or history's `SavedGearSlot`) so both can feed
/// the same render function.
pub struct GearSlotView {
    pub slot:      i32,
    pub item_id:   i32,
    pub item_name: String,
}

/// One equipped Imagine (companion skill in slot position 7/8).
pub struct ImagineView {
    pub skill_id: i32,
    pub tier:     i32,
    pub name:     String,
}

/// Per-panel UI state: icon texture cache + which Breakthrough level is selected
/// per slot. Only one gear panel is shown on screen at a time in this app, so
/// keying the selection by slot id alone (not by entity) is enough.
#[derive(Default)]
pub struct GearPanelState {
    pub icons:              IconCache,
    pub breakthrough_level: HashMap<i32, i32>,
    pub show_unknown_attrs: bool,
}

const ICON_SIZE: f32 = 32.0;
const TEXT_SIZE: f32 = 12.0;

/// Renders just the gear-slot grid (icons, quality colors, attribute rolls,
/// Breakthrough selector). Callers that want Imagines too call `render_imagines`
/// separately — kept apart so they can be placed in their own columns/panels.
pub fn render_gear(ui: &mut egui::Ui, state: &mut GearPanelState, slots: &[GearSlotView]) {
    if slots.is_empty() {
        ui.label(egui::RichText::new("Not available").size(TEXT_SIZE).color(theme::TEXT_FAINT));
        return;
    }

    let ctx = ui.ctx().clone();

    ui.checkbox(&mut state.show_unknown_attrs, "Show Unknown Attributes")
        .on_hover_text("Show placeholder lines for Rare (enchant) and Reforged (recast) attributes that this app can't resolve yet.");
    ui.add_space(4.0);

    let mut sorted: Vec<&GearSlotView> = slots.iter().collect();
    sorted.sort_by_key(|s| s.slot);
    for pair in sorted.chunks(2) {
        ui.columns(2, |cols| {
            render_slot(&mut cols[0], &ctx, state, pair[0]);
            if let Some(second) = pair.get(1) {
                render_slot(&mut cols[1], &ctx, state, second);
            }
        });
        ui.add_space(4.0);
    }
}

/// Renders just the Imagines list (or "None equipped"). See `render_gear`.
pub fn render_imagines(ui: &mut egui::Ui, state: &mut GearPanelState, imagines: &[ImagineView]) {
    let ctx = ui.ctx().clone();
    if imagines.is_empty() {
        ui.label(egui::RichText::new("None equipped").size(TEXT_SIZE).color(theme::TEXT_FAINT));
    } else {
        for im in imagines {
            render_imagine(ui, &ctx, state, im);
        }
    }
}

fn render_slot(ui: &mut egui::Ui, ctx: &egui::Context, state: &mut GearPanelState, slot: &GearSlotView) {
    let slot_name = core::data_tables::gear_slot_name(slot.slot);

    ui.set_width(ui.available_width());
    egui::Frame::group(ui.style())
        .fill(theme::BG_PANEL2)
        .inner_margin(egui::Margin::same(5.0))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let has_item = slot.item_id != 0 && !slot.item_name.is_empty();
                let icon_path = has_item.then(|| core::DATA.item_icon(slot.item_id)).flatten();
                match icon_path.and_then(|p| state.icons.get(ctx, "Items", p)) {
                    Some(tex) => { ui.image((tex.id(), egui::vec2(ICON_SIZE, ICON_SIZE))); }
                    None => placeholder(ui, quality_of(slot.item_id)),
                }

                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(slot_name).size(TEXT_SIZE).color(theme::TEXT_MUTED));
                    if slot.item_id == 0 || slot.item_name.is_empty() {
                        ui.label(egui::RichText::new("— empty —").size(TEXT_SIZE).color(theme::TEXT_FAINT));
                    } else {
                        let quality = quality_of(slot.item_id);
                        let color = theme::quality_color(quality.unwrap_or(0));
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(&slot.item_name).size(TEXT_SIZE).color(color));
                            if let Some(roll) = core::DATA.gear_attr_roll(slot.item_id) {
                                ui.label(egui::RichText::new(format!("GS {}", roll.equip_gs)).size(TEXT_SIZE).color(theme::TEXT_FAINT));
                            }
                            if quality.unwrap_or(0) < 4 {
                                ui.label(egui::RichText::new("⚠").size(TEXT_SIZE).color(theme::WARN))
                                    .on_hover_text("Below Mythic/Legendary quality — Advanced Attributes may be inaccurate.");
                            }
                        });
                    }
                });
            });

            if slot.item_id != 0 && !slot.item_name.is_empty() {
                if let Some(roll) = core::DATA.gear_attr_roll(slot.item_id) {
                    ui.separator();
                    render_attr_roll(ui, state, slot.slot, &roll);
                }
            }
        });
}

fn render_attr_roll(ui: &mut egui::Ui, state: &mut GearPanelState, slot: i32, roll: &GearAttrRoll) {
    let show_unknown = state.show_unknown_attrs;

    let (basic, advanced) = if !roll.breakthroughs.is_empty() {
        let selected = state.breakthrough_level.entry(slot).or_insert(0);
        egui::ComboBox::from_id_salt(("gear_breakthrough", slot))
            .selected_text(if *selected == 0 {
                format!("Breakthrough 0 (GS {})", roll.equip_gs)
            } else {
                roll.breakthroughs.iter().find(|b| b.level == *selected)
                    .map(|b| format!("Breakthrough {} (GS {})", b.level, b.equip_gs))
                    .unwrap_or_default()
            })
            .show_ui(ui, |ui| {
                if ui.selectable_label(*selected == 0, format!("0 (GS {})", roll.equip_gs)).clicked() {
                    *selected = 0;
                }
                for bt in &roll.breakthroughs {
                    if ui.selectable_label(*selected == bt.level, format!("{} (GS {})", bt.level, bt.equip_gs)).clicked() {
                        *selected = bt.level;
                    }
                }
            });

        if *selected == 0 {
            (&roll.basic, &roll.advanced)
        } else if let Some(bt) = roll.breakthroughs.iter().find(|b| b.level == *selected) {
            (&bt.basic, &bt.advanced)
        } else {
            (&roll.basic, &roll.advanced)
        }
    } else {
        (&roll.basic, &roll.advanced)
    };

    if !basic.is_empty() {
        ui.label(egui::RichText::new("Basic Attributes").small().strong().color(theme::TEXT_MUTED));
        for line in basic {
            attr_line(ui, line);
        }
    }
    if !advanced.is_empty() {
        ui.label(egui::RichText::new("Advanced Attributes").small().strong().color(theme::TEXT_MUTED));
        for line in advanced {
            attr_line(ui, line);
        }
        if show_unknown {
            ui.label(egui::RichText::new("Rare Attribute: <UNKNOWN>").small().color(theme::TEXT_FAINT));
            ui.label(egui::RichText::new("Reforged Attribute: <UNKNOWN>").small().color(theme::TEXT_FAINT));
        }
    }
}

fn attr_line(ui: &mut egui::Ui, line: &AttrLine) {
    let suffix = if line.is_percent { "%" } else { "" };
    ui.label(
        egui::RichText::new(format!("  {}: {}{suffix} - {}{suffix}", line.name, line.min, line.max))
            .size(TEXT_SIZE).color(theme::TEXT),
    );
}

fn render_imagine(ui: &mut egui::Ui, ctx: &egui::Context, state: &mut GearPanelState, im: &ImagineView) {
    ui.horizontal(|ui| {
        let icon_path = core::DATA.skill_icon(im.skill_id as u32);
        let tex = icon_path.and_then(|p| state.icons.get(ctx, "Skills_Imagines", p));
        if let Some(tex) = tex {
            ui.image((tex.id(), egui::vec2(ICON_SIZE, ICON_SIZE)));
        } else {
            placeholder(ui, None);
        }
        let name = if im.name.is_empty() { format!("Skill #{}", im.skill_id) } else { im.name.clone() };
        ui.label(egui::RichText::new(format!("{name} (Tier {})", im.tier)).size(TEXT_SIZE).color(theme::TEXT));
    });
}

fn placeholder(ui: &mut egui::Ui, quality: Option<i32>) {
    let color = theme::quality_color(quality.unwrap_or(0));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ICON_SIZE, ICON_SIZE), egui::Sense::hover());
    ui.painter().rect_filled(rect, 4.0, color.gamma_multiply(0.25));
    ui.painter().rect_stroke(rect, 4.0, egui::Stroke::new(1.0, color));
}

fn quality_of(item_id: i32) -> Option<i32> {
    if item_id == 0 { return None; }
    core::DATA.item_quality(item_id)
}
