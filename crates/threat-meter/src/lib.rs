use core::{module::{Module, ModuleContext}, types::EntityId};
use game::GameEvent;
use std::collections::HashMap;
use std::time::Instant;

const ENTRY_TTL_SECS: u64 = 60;

// ── Data model ────────────────────────────────────────────────────────────────

struct PlayerThreat {
    threat:       u64,
    #[allow(dead_code)]
    last_update:  Instant,
}

struct TargetRow {
    players:       HashMap<EntityId, PlayerThreat>,
    last_activity: Instant,
}

// ── Module ────────────────────────────────────────────────────────────────────

pub struct ThreatModule {
    pending:       Vec<GameEvent>,
    names:         HashMap<EntityId, String>,
    // outer = target (boss), inner = player -> threat
    table:         HashMap<EntityId, TargetRow>,
    selected_tgt:  Option<EntityId>,
}

impl ThreatModule {
    pub fn new() -> Self {
        Self {
            pending:      Vec::new(),
            names:        HashMap::new(),
            table:        HashMap::new(),
            selected_tgt: None,
        }
    }

    pub fn push_event(&mut self, event: GameEvent) {
        self.pending.push(event);
    }
}

impl Default for ThreatModule {
    fn default() -> Self { Self::new() }
}

impl Module for ThreatModule {
    fn id(&self)   -> &'static str { "threat-meter" }
    fn name(&self) -> &str         { "Threat" }
    fn icon(&self) -> &str         { "🎯" }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, _ctx: &ModuleContext) {
        let events: Vec<GameEvent> = self.pending.drain(..).collect();
        for event in events {
            match event {
                GameEvent::EntityName { id, name, .. } => {
                    if !name.is_empty() { self.names.insert(id, name); }
                }
                GameEvent::EntityDespawn { id } => {
                    self.names.remove(&id);
                    self.table.remove(&id);
                    if self.selected_tgt == Some(id) { self.selected_tgt = None; }
                }
                GameEvent::ZoneChange { .. } => {
                    self.table.clear();
                    self.selected_tgt = None;
                }
                GameEvent::ThreatUpdate { target_id, entity_id, threat } => {
                    let row = self.table.entry(target_id).or_insert_with(|| TargetRow {
                        players:       HashMap::new(),
                        last_activity: Instant::now(),
                    });
                    row.last_activity = Instant::now();
                    row.players.insert(entity_id, PlayerThreat {
                        threat,
                        last_update: Instant::now(),
                    });
                    // Auto-select first target seen
                    if self.selected_tgt.is_none() {
                        self.selected_tgt = Some(target_id);
                    }
                }
                _ => {}
            }
        }

        // Evict stale target rows
        self.table.retain(|_, row| row.last_activity.elapsed().as_secs() < ENTRY_TTL_SECS);
        if let Some(sel) = self.selected_tgt {
            if !self.table.contains_key(&sel) { self.selected_tgt = None; }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _ctx: &egui::Context) {
        if self.table.is_empty() {
            // Placeholder — no threat data yet
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    egui::RichText::new("🎯 Threat Meter")
                        .strong()
                        .size(16.0)
                        .color(egui::Color32::from_rgb(180, 180, 200))
                );
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new("No threat data received.")
                        .color(egui::Color32::GRAY)
                );
                ui.add_space(6.0);
                ui.label(
                    egui::RichText::new(
                        "Threat packet format not yet identified.\n\
                         Use the Debug panel (unknown opcodes) during\n\
                         combat to locate the threat update packets."
                    )
                    .small()
                    .color(egui::Color32::from_rgb(120, 120, 140))
                );
            });
            return;
        }

        // ── Target selector ───────────────────────────────────────────────────
        let target_ids: Vec<EntityId> = {
            let mut v: Vec<EntityId> = self.table.keys().copied().collect();
            v.sort_by_key(|id| id.0);
            v
        };

        ui.horizontal(|ui| {
            ui.label("Target:");
            for &tgt_id in &target_ids {
                let label = self.names
                    .get(&tgt_id)
                    .map(|n| n.as_str())
                    .unwrap_or("???");
                let is_sel = self.selected_tgt == Some(tgt_id);
                if ui.selectable_label(is_sel, label).clicked() {
                    self.selected_tgt = Some(tgt_id);
                }
            }
        });
        ui.separator();

        // ── Threat table for selected target ──────────────────────────────────
        let sel_id = match self.selected_tgt {
            Some(id) => id,
            None     => return,
        };
        let Some(row) = self.table.get(&sel_id) else { return };

        // Sort players by threat descending
        let mut entries: Vec<(EntityId, u64)> = row.players
            .iter()
            .map(|(&eid, pt)| (eid, pt.threat))
            .collect();
        entries.sort_by(|a, b| b.1.cmp(&a.1));

        let max_threat = entries.first().map(|(_, t)| *t).unwrap_or(1).max(1);
        let names      = &self.names;

        egui::Grid::new("threat_table")
            .num_columns(4)
            .striped(true)
            .min_col_width(50.0)
            .show(ui, |ui| {
                ui.strong("#");
                ui.strong("Player");
                ui.strong("Threat");
                ui.strong("%");
                ui.end_row();

                for (rank, (eid, threat)) in entries.iter().enumerate() {
                    let pct      = *threat as f64 / max_threat as f64 * 100.0;
                    let bar_frac = (*threat as f32 / max_threat as f32).clamp(0.0, 1.0);
                    let is_tank  = rank == 0;
                    let name     = names.get(eid)
                        .map(|n| n.as_str())
                        .unwrap_or("Unknown");

                    // Rank — skull for top threat
                    if is_tank {
                        ui.label(egui::RichText::new("💀").strong());
                    } else {
                        ui.label(format!("{}", rank + 1));
                    }

                    // Name + bar
                    ui.vertical(|ui| {
                        let name_color = if is_tank {
                            egui::Color32::from_rgb(240, 100, 80)
                        } else {
                            ui.style().visuals.text_color()
                        };
                        ui.label(egui::RichText::new(name).color(name_color));
                        let (rect, _) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width().max(60.0), 4.0),
                            egui::Sense::hover(),
                        );
                        ui.painter().rect_filled(rect, 0.0, egui::Color32::DARK_GRAY);
                        let bar_color = if is_tank {
                            egui::Color32::from_rgb(220, 60, 60)
                        } else {
                            egui::Color32::from_rgb(70, 130, 180)
                        };
                        let mut fill = rect;
                        fill.set_right(rect.left() + rect.width() * bar_frac);
                        ui.painter().rect_filled(fill, 0.0, bar_color);
                    });

                    ui.label(fmt_threat(*threat));
                    ui.label(format!("{:.1}%", pct));
                    ui.end_row();
                }
            });
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn fmt_threat(v: u64) -> String {
    if v >= 1_000_000 {
        format!("{:.2}M", v as f64 / 1_000_000.0)
    } else if v >= 1_000 {
        format!("{:.1}K", v as f64 / 1_000.0)
    } else {
        format!("{v}")
    }
}
