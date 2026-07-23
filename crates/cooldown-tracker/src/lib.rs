use core::{module::{Module, ModuleContext}, types::EntityId};
use game::GameEvent;
use std::collections::HashMap;
use std::time::{Duration, Instant};

// ── Data types ────────────────────────────────────────────────────────────────

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackedEntity {
    pub uid:  u64,
    pub name: String,
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct TrackedSkill {
    pub skill_id:      u32,
    pub label:         String,
    pub cooldown_secs: f32,
}

// ── Module ────────────────────────────────────────────────────────────────────

pub struct CooldownModule {
    entities:  Vec<TrackedEntity>,
    skills:    Vec<TrackedSkill>,
    /// (entity_uid, skill_id) → last cast Instant
    cooldowns: HashMap<(u64, u32), Instant>,
    #[allow(dead_code)]
    uid_map:   HashMap<u64, EntityId>,
    /// name (lowercased) → uid
    name_uid:  HashMap<String, u64>,
    pending:   Vec<GameEvent>,

    show_config:     bool,
    add_entity_name: String,
    add_entity_uid:  String,
    add_skill_id:    String,
    add_skill_label: String,
    add_skill_cd:    f32,
}

impl CooldownModule {
    pub fn new() -> Self {
        Self {
            entities:        Vec::new(),
            skills:          Vec::new(),
            cooldowns:       HashMap::new(),
            uid_map:         HashMap::new(),
            name_uid:        HashMap::new(),
            pending:         Vec::new(),
            show_config:     true,
            add_entity_name: String::new(),
            add_entity_uid:  String::new(),
            add_skill_id:    String::new(),
            add_skill_label: String::new(),
            add_skill_cd:    10.0,
        }
    }

    pub fn push_event(&mut self, event: GameEvent) {
        self.pending.push(event);
    }
}

impl Default for CooldownModule {
    fn default() -> Self { Self::new() }
}

impl Module for CooldownModule {
    fn id(&self)   -> &'static str { "cooldown" }
    fn name(&self) -> &str         { "Cooldowns" }
    fn icon(&self) -> &str         { egui_phosphor::regular::CLOCK_COUNTDOWN }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, _ctx: &ModuleContext) {
        let events: Vec<_> = self.pending.drain(..).collect();
        for event in events {
            match event {
                GameEvent::EntityName { id, name, .. } => {
                    // Build name→uid reverse map so we can match tracked entities by name
                    if !name.is_empty() {
                        self.name_uid.insert(name.to_lowercase(), id.0);
                        // Backfill any tracked entity whose name matches
                        for ent in &mut self.entities {
                            if ent.name.to_lowercase() == name.to_lowercase() && ent.uid == 0 {
                                ent.uid = id.0;
                            }
                        }
                    }
                }
                GameEvent::EntityDespawn { id } => {
                    self.name_uid.retain(|_, &mut uid| uid != id.0);
                }
                GameEvent::Combat(c) => {
                    let src_uid = c.source_id.0;
                    // Check if this source matches any tracked entity
                    let is_tracked = self.entities.iter().any(|e| e.uid == src_uid);
                    if is_tracked {
                        let skill_id = c.skill_id;
                        let is_tracked_skill = self.skills.iter().any(|s| s.skill_id == skill_id);
                        if is_tracked_skill {
                            self.cooldowns.insert((src_uid, skill_id), Instant::now());
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.heading("Cooldown Tracker");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let gear = egui_phosphor::regular::GEAR;
                let cfg_label = if self.show_config { format!("{gear} Hide Config") } else { format!("{gear} Config") };
                if ui.button(cfg_label).clicked() {
                    self.show_config = !self.show_config;
                }
            });
        });
        ui.separator();

        if self.show_config {
            self.render_config(ui);
            ui.separator();
        }

        self.render_live(ui);
    }
}

// ── Config panel ──────────────────────────────────────────────────────────────

impl CooldownModule {
    fn render_config(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |cols| {
            // Left: entities
            cols[0].strong("Tracked Players");
            cols[0].add_space(4.0);

            let mut remove_ent: Option<usize> = None;
            let mut move_up:    Option<usize> = None;
            let mut move_dn:    Option<usize> = None;
            let n = self.entities.len();

            for (i, ent) in self.entities.iter().enumerate() {
                cols[0].horizontal(|ui| {
                    ui.label(format!("{} ({})", ent.name, format_uid(ent.uid)));
                    if n > 1 {
                        if ui.small_button(egui_phosphor::regular::CARET_UP).clicked() && i > 0     { move_up = Some(i); }
                        if ui.small_button(egui_phosphor::regular::CARET_DOWN).clicked() && i + 1 < n { move_dn = Some(i); }
                    }
                    if ui.small_button(egui_phosphor::regular::X).clicked() { remove_ent = Some(i); }
                });
            }
            if let Some(i) = move_up { self.entities.swap(i, i - 1); }
            if let Some(i) = move_dn { self.entities.swap(i, i + 1); }
            if let Some(i) = remove_ent { self.entities.remove(i); }

            cols[0].add_space(6.0);
            cols[0].strong("Add player");

            cols[0].horizontal(|ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.add_entity_name);
            });
            cols[0].horizontal(|ui| {
                ui.label("UID (hex, optional):");
                ui.text_edit_singleline(&mut self.add_entity_uid);
            });

            if cols[0].button("Add").clicked() {
                let name = self.add_entity_name.trim().to_string();
                if !name.is_empty() {
                    let uid = u64::from_str_radix(
                        self.add_entity_uid.trim().trim_start_matches("0x"),
                        16
                    ).unwrap_or_else(|_| {
                        // Try to resolve from name_uid map
                        self.name_uid.get(&name.to_lowercase()).copied().unwrap_or(0)
                    });
                    self.entities.push(TrackedEntity { uid, name });
                    self.add_entity_name.clear();
                    self.add_entity_uid.clear();
                }
            }

            // Right: skills
            cols[1].strong("Tracked Skills");
            cols[1].add_space(4.0);

            let mut remove_sk: Option<usize> = None;
            for (i, sk) in self.skills.iter().enumerate() {
                cols[1].horizontal(|ui| {
                    ui.label(format!("{} (id:{} {}s)", sk.label, sk.skill_id, sk.cooldown_secs));
                    if ui.small_button(egui_phosphor::regular::X).clicked() { remove_sk = Some(i); }
                });
            }
            if let Some(i) = remove_sk { self.skills.remove(i); }

            cols[1].add_space(6.0);
            cols[1].strong("Add skill");

            cols[1].horizontal(|ui| {
                ui.label("Skill ID:");
                ui.text_edit_singleline(&mut self.add_skill_id);
            });
            cols[1].horizontal(|ui| {
                ui.label("Label:");
                ui.text_edit_singleline(&mut self.add_skill_label);
            });
            cols[1].horizontal(|ui| {
                ui.label("CD (secs):");
                ui.add(egui::DragValue::new(&mut self.add_skill_cd).range(0.5..=600.0).speed(0.5));
            });

            if cols[1].button("Add").clicked() {
                let id_str = self.add_skill_id.trim().to_string();
                if let Ok(skill_id) = id_str.parse::<u32>() {
                    let label = if self.add_skill_label.trim().is_empty() {
                        format!("Skill {skill_id}")
                    } else {
                        self.add_skill_label.trim().to_string()
                    };
                    if !self.skills.iter().any(|s| s.skill_id == skill_id) {
                        self.skills.push(TrackedSkill {
                            skill_id,
                            label,
                            cooldown_secs: self.add_skill_cd,
                        });
                    }
                    self.add_skill_id.clear();
                    self.add_skill_label.clear();
                }
            }
        });
    }
}

// ── Live display ──────────────────────────────────────────────────────────────

impl CooldownModule {
    fn render_live(&self, ui: &mut egui::Ui) {
        if self.entities.is_empty() || self.skills.is_empty() {
            ui.label(
                egui::RichText::new(format!("Add players and skills in {} Config to track cooldowns.", egui_phosphor::regular::GEAR))
                    .color(egui::Color32::from_rgb(140, 140, 160))
            );
            return;
        }

        let now = Instant::now();

        egui::ScrollArea::vertical().id_salt("cd_live").show(ui, |ui| {
            for ent in &self.entities {
                let name = if !ent.name.is_empty() {
                    ent.name.as_str()
                } else {
                    "Unknown"
                };

                ui.horizontal(|ui| {
                    ui.strong(name);
                    if ent.uid != 0 {
                        ui.label(
                            egui::RichText::new(format!("({})", format_uid(ent.uid)))
                                .small()
                                .color(egui::Color32::from_rgb(120, 120, 140))
                        );
                    }
                });

                ui.horizontal_wrapped(|ui| {
                    for sk in &self.skills {
                        render_cd_pill(ui, ent.uid, sk, &self.cooldowns, now);
                    }
                });

                ui.add_space(4.0);
            }
        });
    }
}

fn render_cd_pill(
    ui:        &mut egui::Ui,
    entity_uid: u64,
    skill:     &TrackedSkill,
    cooldowns: &HashMap<(u64, u32), Instant>,
    now:       Instant,
) {
    let (remaining, frac) = match cooldowns.get(&(entity_uid, skill.skill_id)) {
        None => {
            // Never observed
            let label = format!("? {}", skill.label);
            ui.label(
                egui::RichText::new(label)
                    .small()
                    .color(egui::Color32::GRAY)
            );
            return;
        }
        Some(&last_cast) => {
            let cd = Duration::from_secs_f32(skill.cooldown_secs);
            let elapsed = now.duration_since(last_cast);
            if elapsed >= cd {
                (0.0f64, 1.0f32) // ready
            } else {
                let rem = (cd - elapsed).as_secs_f64();
                let frac = elapsed.as_secs_f32() / skill.cooldown_secs;
                (rem, frac)
            }
        }
    };

    let desired = egui::vec2(80.0, 36.0);
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter();

    // Background
    painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(30, 32, 38));

    if remaining <= 0.0 {
        // Ready — green fill
        painter.rect_filled(rect, 4.0, egui::Color32::from_rgb(40, 120, 60));
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            format!("{} {}", egui_phosphor::regular::CHECK, skill.label),
            egui::FontId::proportional(11.0),
            egui::Color32::from_rgb(180, 255, 180),
        );
    } else {
        // CD progress bar
        let bar_h = 3.0;
        let bar_rect = egui::Rect::from_min_size(
            egui::pos2(rect.left(), rect.bottom() - bar_h),
            egui::vec2(rect.width() * frac, bar_h),
        );
        let cd_color = if remaining < 3.0 {
            egui::Color32::from_rgb(240, 100, 60)
        } else if remaining < 8.0 {
            egui::Color32::from_rgb(220, 180, 40)
        } else {
            egui::Color32::from_rgb(80, 120, 200)
        };
        painter.rect_filled(bar_rect, 0.0, cd_color);

        painter.text(
            rect.center() - egui::vec2(0.0, 4.0),
            egui::Align2::CENTER_CENTER,
            &skill.label,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(200, 200, 210),
        );
        painter.text(
            rect.center() + egui::vec2(0.0, 8.0),
            egui::Align2::CENTER_CENTER,
            format!("{:.1}s", remaining),
            egui::FontId::proportional(11.0),
            cd_color,
        );
    }
}

fn format_uid(uid: u64) -> String {
    if uid == 0 { "?".to_string() } else { format!("{uid:#x}") }
}
