use crate::module::{class_name, fmt_damage};
use core::module::{Module, ModuleContext};
use encounter_store::{EncounterStore, EncounterSummary, SavedEncounter};
use uuid::Uuid;
use ui;

pub struct EncounterHistoryModule {
    store:          EncounterStore,
    summaries:      Vec<EncounterSummary>,
    loaded:         bool,
    detail:         Option<SavedEncounter>,
    detail_player:  Option<usize>,
    pending_delete: Option<Uuid>,
    status:         String,
}

impl EncounterHistoryModule {
    pub fn new() -> Self {
        Self {
            store:          EncounterStore::open(),
            summaries:      Vec::new(),
            loaded:         false,
            detail:         None,
            detail_player:  None,
            pending_delete: None,
            status:         String::new(),
        }
    }

    fn refresh(&mut self) {
        self.summaries = self.store.list_summaries();
        self.loaded = true;
        self.status = format!("{} encounters", self.summaries.len());
    }
}

impl Module for EncounterHistoryModule {
    fn id(&self)   -> &'static str { "history" }
    fn name(&self) -> &str         { "History" }
    fn icon(&self) -> &str         { "▣" }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, _ctx: &ModuleContext) {
        // Load on first show
        if !self.loaded {
            self.refresh();
        }

        // Process pending deletion
        if let Some(id) = self.pending_delete.take() {
            if let Err(e) = self.store.delete(id) {
                self.status = format!("Delete failed: {e}");
            } else {
                self.summaries.retain(|s| s.id != id);
                self.status = format!("{} encounters", self.summaries.len());
                if self.detail.as_ref().map(|d| d.id == id).unwrap_or(false) {
                    self.detail = None;
                    self.detail_player = None;
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        if let Some(enc) = &self.detail.clone() {
            render_detail(ui, enc, &mut self.detail_player, &mut self.detail);
            return;
        }

        // ── List view ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ENCOUNTER HISTORY").strong().size(11.0).color(ui::theme::TEXT_FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("↺").on_hover_text("Refresh").clicked() {
                    self.refresh();
                }
                if !self.status.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.status).size(10.0).color(ui::theme::TEXT_FAINT)
                    );
                }
            });
        });

        ui.add_space(4.0);

        if self.summaries.is_empty() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    egui::RichText::new("No saved encounters yet. Finish some combat and they'll appear here.")
                        .color(ui::theme::TEXT_FAINT),
                );
            });
            return;
        }

        // Column headers
        ui.horizontal(|ui| {
            ui.add_space(36.0);
            col_header(ui, "ENCOUNTER", 180.0);
            col_header(ui, "PARTY", 50.0);
            col_header(ui, "TIME", 55.0);
            col_header(ui, "TOP DPS", 80.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(8.0);
                col_header(ui, "WHEN", 100.0);
            });
        });

        ui.add_space(4.0);

        let mut to_load:   Option<Uuid> = None;
        let mut to_delete: Option<Uuid> = None;

        egui::ScrollArea::vertical()
            .id_salt("history_list")
            .show(ui, |ui| {
                for (i, sum) in self.summaries.iter().enumerate() {
                    if i > 0 { ui.add_space(20.0); }
                    let date = sum.started_at.format("%m/%d %H:%M").to_string();
                    let dur  = format_duration(sum.duration_secs);
                    let zone = if sum.scene_name.is_empty() { "Unknown Zone" } else { &sum.scene_name };

                    let (row_rect, row_resp) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width(), 56.0),
                        egui::Sense::click(),
                    );

                    let hovered = row_resp.hovered();
                    let bg = if hovered {
                        egui::Color32::from_rgba_premultiplied(91, 140, 255, 12)
                    } else {
                        ui::theme::BG_PANEL
                    };
                    ui.painter().rect_filled(row_rect, 8.0, bg);
                    ui.painter().rect_stroke(row_rect, 8.0, egui::Stroke::new(1.0, ui::theme::LINE));

                    // Status diamond
                    let diamond_center = egui::pos2(row_rect.min.x + 18.0, row_rect.center().y);
                    let d_size = 6.0;
                    let diamond = [
                        egui::pos2(diamond_center.x, diamond_center.y - d_size),
                        egui::pos2(diamond_center.x + d_size, diamond_center.y),
                        egui::pos2(diamond_center.x, diamond_center.y + d_size),
                        egui::pos2(diamond_center.x - d_size, diamond_center.y),
                    ];
                    ui.painter().add(egui::Shape::convex_polygon(
                        diamond.to_vec(), ui::theme::GOOD, egui::Stroke::NONE,
                    ));

                    // Zone name + date tag
                    ui.painter().text(
                        egui::pos2(row_rect.min.x + 34.0, row_rect.center().y - 8.0),
                        egui::Align2::LEFT_CENTER,
                        zone,
                        egui::FontId::proportional(13.0),
                        ui::theme::TEXT,
                    );
                    ui.painter().text(
                        egui::pos2(row_rect.min.x + 34.0, row_rect.center().y + 9.0),
                        egui::Align2::LEFT_CENTER,
                        &date,
                        egui::FontId::monospace(9.5),
                        ui::theme::TEXT_FAINT,
                    );

                    // Players
                    ui.painter().text(
                        egui::pos2(row_rect.min.x + 220.0, row_rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        format!("{}p", sum.player_count),
                        egui::FontId::monospace(11.5),
                        ui::theme::TEXT_MUTED,
                    );

                    // Duration
                    ui.painter().text(
                        egui::pos2(row_rect.min.x + 278.0, row_rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        &dur,
                        egui::FontId::monospace(12.0),
                        ui::theme::TEXT,
                    );

                    // Top DPS
                    ui.painter().text(
                        egui::pos2(row_rect.min.x + 350.0, row_rect.center().y - 8.0),
                        egui::Align2::LEFT_CENTER,
                        fmt_damage(sum.top_player_dps as u64),
                        egui::FontId::monospace(13.0),
                        ui::theme::ACCENT,
                    );
                    ui.painter().text(
                        egui::pos2(row_rect.min.x + 350.0, row_rect.center().y + 9.0),
                        egui::Align2::LEFT_CENTER,
                        &sum.top_player_name,
                        egui::FontId::proportional(9.5),
                        ui::theme::TEXT_FAINT,
                    );

                    // Action buttons (right side)
                    let btn_x = row_rect.max.x - 60.0;
                    let view_rect = egui::Rect::from_min_size(
                        egui::pos2(btn_x, row_rect.center().y - 10.0),
                        egui::vec2(26.0, 20.0),
                    );
                    let del_rect = egui::Rect::from_min_size(
                        egui::pos2(btn_x + 30.0, row_rect.center().y - 10.0),
                        egui::vec2(26.0, 20.0),
                    );

                    if ui.put(view_rect, egui::Button::new("▶").small()).clicked() {
                        to_load = Some(sum.id);
                    }
                    if ui.put(del_rect, egui::Button::new("✖").small()).clicked() {
                        to_delete = Some(sum.id);
                    }

                    if row_resp.clicked() {
                        to_load = Some(sum.id);
                    }
                }
            });

        if let Some(id) = to_load {
            match self.store.load(id) {
                Ok(enc) => {
                    self.detail = Some(enc);
                    self.detail_player = None;
                }
                Err(e) => self.status = format!("Load failed: {e}"),
            }
        }
        if let Some(id) = to_delete {
            self.pending_delete = Some(id);
        }
    }
}

fn col_header(ui: &mut egui::Ui, label: &str, _width: f32) {
    ui.label(
        egui::RichText::new(label)
            .size(9.5)
            .color(ui::theme::TEXT_FAINT),
    );
}

// ── Detail view ───────────────────────────────────────────────────────────────

fn render_detail(
    ui: &mut egui::Ui,
    enc: &SavedEncounter,
    detail_player: &mut Option<usize>,
    detail_slot: &mut Option<SavedEncounter>,
) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            *detail_slot = None;
            *detail_player = None;
            return;
        }
        let zone = if enc.scene_name.is_empty() { "Unknown Zone" } else { &enc.scene_name };
        ui.strong(zone);
        ui.label(
            egui::RichText::new(format!(
                "  {}  {}",
                enc.started_at.format("%Y-%m-%d %H:%M"),
                format_duration(enc.duration_secs)
            ))
            .small()
            .color(egui::Color32::from_rgb(140, 140, 160)),
        );
    });
    ui.separator();

    if detail_slot.is_none() { return; } // after back button

    let dps_div = enc.duration_secs.max(1.0);
    let total   = enc.total_damage.max(1);

    let mut sorted: Vec<_> = enc.players.iter().enumerate().collect();
    sorted.sort_by(|(_, a), (_, b)| b.total_damage.cmp(&a.total_damage));

    egui::ScrollArea::vertical()
        .id_salt("detail_vscroll")
        .show(ui, |ui| {
        egui::ScrollArea::horizontal()
            .id_salt("detail_hscroll")
            .show(ui, |ui| {
            egui::Grid::new("detail_players")
                .num_columns(16)
                .striped(true)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for h in ["#","Player","Class","Spec","AS","SS","DMG%","Hits","Total DMG","DPS","DMG Taken","Total Heal","Heal/s","Crit%","CritDmg%","Luck%"] {
                        ui.strong(h);
                    }
                    ui.end_row();

                    for (rank, (i, p)) in sorted.iter().enumerate() {
                        let dps      = p.total_damage as f64 / dps_div;
                        let heal_ps  = p.total_healing as f64 / dps_div;
                        let crit_rt  = if p.hit_count > 0 { p.crit_count as f64 / p.hit_count as f64 * 100.0 } else { 0.0 };
                        let share    = p.total_damage as f64 / total as f64 * 100.0;
                        let is_sel   = *detail_player == Some(*i);

                        ui.label(format!("{}", rank + 1));
                        if ui.selectable_label(is_sel, &p.name).clicked() {
                            *detail_player = if is_sel { None } else { Some(*i) };
                        }
                        ui.label(class_name(p.class_id));
                        ui.label(p.spec.as_deref().unwrap_or("—"));
                        ui.label(p.ability_score.map(|v| v.to_string()).unwrap_or_else(|| "—".into()));
                        ui.label(p.season_strength.filter(|&v| v > 0).map(|v| format!("{v}")).unwrap_or_else(|| "—".into()));
                        ui.label(format!("{:.1}%", share));
                        ui.label(p.hit_count.to_string());
                        ui.label(fmt_damage(p.total_damage));
                        ui.label(fmt_damage(dps as u64));
                        ui.label(fmt_damage(p.damage_taken));
                        ui.label(fmt_damage(p.total_healing));
                        ui.label(fmt_damage(heal_ps as u64));
                        ui.label(format!("{:.1}%", crit_rt));
                        ui.label(p.crit_damage.map(|v| format!("{:.1}%", v as f64 / 100.0)).unwrap_or_else(|| "—".into()));
                        ui.label(p.luck_pct.map(|v| format!("{:.1}%", v as f64 / 100.0)).unwrap_or_else(|| "—".into()));
                        ui.end_row();

                        // Inline skill breakdown when selected
                        if *detail_player == Some(*i) && !p.skills.is_empty() {
                            // span entire row width with vertical content
                            ui.label("");
                            ui.vertical(|ui| {
                                let mut skills = p.skills.clone();
                                skills.sort_by(|a, b| b.total_dmg.cmp(&a.total_dmg));
                                egui::Grid::new(format!("skills_{i}"))
                                    .num_columns(5)
                                    .striped(true)
                                    .show(ui, |ui| {
                                        for h in ["Skill","Damage","Hits","Crit%","MaxHit"] {
                                            ui.label(egui::RichText::new(h).small().strong());
                                        }
                                        ui.end_row();
                                        for sk in &skills {
                                            let sk_crit = if sk.hits > 0 { sk.crits as f64 / sk.hits as f64 * 100.0 } else { 0.0 };
                                            let skill_label = core::DATA.skill_name(sk.skill_id)
                                                .map(|s| s.to_string())
                                                .unwrap_or_else(|| format!("#{}", sk.skill_id));
                                            ui.label(egui::RichText::new(&skill_label).small());
                                            ui.label(egui::RichText::new(fmt_damage(sk.total_dmg)).small());
                                            ui.label(egui::RichText::new(sk.hits.to_string()).small());
                                            ui.label(egui::RichText::new(format!("{:.1}%", sk_crit)).small());
                                            ui.label(egui::RichText::new(fmt_damage(sk.max_hit)).small());
                                            ui.end_row();
                                        }
                                    });
                            });
                            for _ in 0..14 { ui.label(""); }
                            ui.end_row();
                        }
                    }
                });
        });
    });
}

fn format_duration(secs: f64) -> String {
    let s = secs as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}
