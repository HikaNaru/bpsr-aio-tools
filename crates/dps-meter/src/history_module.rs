use crate::module::{entity_category_name, entity_row_color, fmt_damage};
use core::module::{Module, ModuleContext};
use encounter_store::{EncounterOutcome, EncounterStore, EncounterSummary, SavedEncounter, SavedPlayerMeter};
use uuid::Uuid;
use ui;

const HEADER_FONT_SIZE: f32 = 11.0;
const CELL_FONT_SIZE:   f32 = 13.0;
const ROW_HEIGHT:       f32 = 26.0;
const CELL_HEIGHT:      f32 = 20.0;
const RANK_COL_WIDTH:   f32 = 28.0;
const NAME_COL_WIDTH:   f32 = 120.0;
const CLASS_COL_WIDTH:  f32 = 110.0;
const ROW_BG_ALPHA:     u8  = 77; // 0.3 opacity

/// Displayed times are WIB (UTC+7), not raw UTC — storage stays `DateTime<Utc>`.
fn to_wib(dt: chrono::DateTime<chrono::Utc>) -> chrono::DateTime<chrono::FixedOffset> {
    dt.with_timezone(&chrono::FixedOffset::east_opt(7 * 3600).unwrap())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey { Damage, DamageTaken, Healing, Hits, CritRate, Deaths }

#[derive(Debug, Clone, Copy)]
enum ClearScope { All, OlderThanDays(i64) }

pub struct EncounterHistoryModule {
    store:           EncounterStore,
    summaries:       Vec<EncounterSummary>,
    loaded:          bool,
    detail:          Option<SavedEncounter>,
    detail_player:   Option<usize>,
    pending_delete:  Option<Uuid>,
    status:          String,
    sort_key:        SortKey,
    sort_desc:       bool,
    players_only:    bool,
    selected_phase:  Option<usize>,
    renaming:        Option<Uuid>,
    rename_buf:      String,
    confirm_clear:   Option<ClearScope>,
    gear_panel_state: ui::widgets::gear_panel::GearPanelState,
}

impl EncounterHistoryModule {
    pub fn new() -> Self {
        Self {
            store:           EncounterStore::open(),
            summaries:       Vec::new(),
            loaded:          false,
            detail:          None,
            detail_player:   None,
            pending_delete:  None,
            status:          String::new(),
            sort_key:        SortKey::Damage,
            sort_desc:       true,
            players_only:    false,
            selected_phase:  None,
            renaming:        None,
            rename_buf:      String::new(),
            confirm_clear:   None,
            gear_panel_state: ui::widgets::gear_panel::GearPanelState::default(),
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
    fn icon(&self) -> &str         { egui_phosphor::regular::CLOCK_COUNTER_CLOCKWISE }

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
        if self.detail.is_some() {
            self.render_detail(ui);
            return;
        }

        // ── List view ────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("ENCOUNTER HISTORY").strong().size(11.0).color(ui::theme::TEXT_FAINT));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button(egui_phosphor::regular::ARROW_CLOCKWISE).on_hover_text("Refresh").clicked() {
                    self.refresh();
                }
                ui.menu_button(format!("Clear History {}", egui_phosphor::regular::CARET_DOWN), |ui| {
                    if ui.button("Older than 7 days").clicked() {
                        self.confirm_clear = Some(ClearScope::OlderThanDays(7));
                        ui.close_menu();
                    }
                    if ui.button("Older than 30 days").clicked() {
                        self.confirm_clear = Some(ClearScope::OlderThanDays(30));
                        ui.close_menu();
                    }
                    if ui.button("All time").clicked() {
                        self.confirm_clear = Some(ClearScope::All);
                        ui.close_menu();
                    }
                });
                if !self.status.is_empty() {
                    ui.label(
                        egui::RichText::new(&self.status).size(10.0).color(ui::theme::TEXT_FAINT)
                    );
                }
            });
        });

        if let Some(scope) = self.confirm_clear {
            let mut open = true;
            let mut do_clear = false;
            egui::Window::new("Confirm Clear History")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    let label = match scope {
                        ClearScope::All => "Delete ALL saved encounters? This cannot be undone.".to_string(),
                        ClearScope::OlderThanDays(d) => format!("Delete all encounters older than {d} days? This cannot be undone."),
                    };
                    ui.label(label);
                    ui.horizontal(|ui| {
                        if ui.button("Cancel").clicked() {
                            self.confirm_clear = None;
                        }
                        if ui.button(egui::RichText::new("Delete").color(ui::theme::BAD)).clicked() {
                            do_clear = true;
                        }
                    });
                });
            if do_clear {
                // Force-close any open detail view first — its file may be deleted.
                self.detail = None;
                self.detail_player = None;
                let result = match scope {
                    ClearScope::All => self.store.clear_all().map(|_| ()),
                    ClearScope::OlderThanDays(d) => {
                        let cutoff = chrono::Utc::now() - chrono::Duration::days(d);
                        self.store.clear_before(cutoff).map(|_| ())
                    }
                };
                match result {
                    Ok(())  => self.refresh(),
                    Err(e)  => self.status = format!("Clear failed: {e}"),
                }
                self.confirm_clear = None;
            }
            if !open { self.confirm_clear = None; }
        }

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
        let mut rename_commit: Option<(Uuid, Option<String>)> = None;

        egui::ScrollArea::vertical()
            .id_salt("history_list")
            .show(ui, |ui| {
                for (i, sum) in self.summaries.iter().enumerate() {
                    if i > 0 { ui.add_space(20.0); }
                    let date = to_wib(sum.started_at).format("%m/%d %H:%M").to_string();
                    let dur  = format_duration(sum.duration_secs);
                    let zone = if sum.scene_name.is_empty() { "Unknown Zone" } else { &sum.scene_name };
                    let display_name = sum.custom_name.as_deref().unwrap_or(zone);

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

                    // Status diamond, colored by outcome
                    let outcome_color = match sum.outcome {
                        EncounterOutcome::Cleared      => ui::theme::GOOD,
                        EncounterOutcome::Failed     => ui::theme::BAD,
                        EncounterOutcome::ManualStop => ui::theme::ACCENT2,
                        EncounterOutcome::Unknown    => ui::theme::TEXT_FAINT,
                    };
                    let diamond_center = egui::pos2(row_rect.min.x + 18.0, row_rect.center().y);
                    let d_size = 6.0;
                    let diamond = [
                        egui::pos2(diamond_center.x, diamond_center.y - d_size),
                        egui::pos2(diamond_center.x + d_size, diamond_center.y),
                        egui::pos2(diamond_center.x, diamond_center.y + d_size),
                        egui::pos2(diamond_center.x - d_size, diamond_center.y),
                    ];
                    ui.painter().add(egui::Shape::convex_polygon(
                        diamond.to_vec(), outcome_color, egui::Stroke::NONE,
                    ));

                    let outcome_label = match sum.outcome {
                        EncounterOutcome::Cleared    => "Cleared",
                        EncounterOutcome::Failed     => "Failed",
                        EncounterOutcome::ManualStop => "Manual Stop",
                        EncounterOutcome::Unknown    => "Unknown",
                    };
                    ui.painter().text(
                        diamond_center + egui::vec2(0.0, 14.0),
                        egui::Align2::CENTER_CENTER,
                        outcome_label,
                        egui::FontId::proportional(7.5),
                        outcome_color,
                    );

                    // Zone/custom name + date tag
                    if self.renaming == Some(sum.id) {
                        let edit_rect = egui::Rect::from_min_size(
                            egui::pos2(row_rect.min.x + 34.0, row_rect.center().y - 18.0),
                            egui::vec2(160.0, 18.0),
                        );
                        let resp = ui.put(edit_rect, egui::TextEdit::singleline(&mut self.rename_buf));
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            let name = self.rename_buf.trim();
                            rename_commit = Some((sum.id, if name.is_empty() { None } else { Some(name.to_string()) }));
                            self.renaming = None;
                        }
                        resp.request_focus();
                    } else {
                        ui.painter().text(
                            egui::pos2(row_rect.min.x + 34.0, row_rect.center().y - 8.0),
                            egui::Align2::LEFT_CENTER,
                            display_name,
                            egui::FontId::proportional(13.0),
                            ui::theme::TEXT,
                        );
                    }
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
                    let btn_x = row_rect.max.x - 90.0;
                    let rename_rect = egui::Rect::from_min_size(
                        egui::pos2(btn_x, row_rect.center().y - 10.0),
                        egui::vec2(26.0, 20.0),
                    );
                    let view_rect = egui::Rect::from_min_size(
                        egui::pos2(btn_x + 30.0, row_rect.center().y - 10.0),
                        egui::vec2(26.0, 20.0),
                    );
                    let del_rect = egui::Rect::from_min_size(
                        egui::pos2(btn_x + 60.0, row_rect.center().y - 10.0),
                        egui::vec2(26.0, 20.0),
                    );

                    if ui.put(rename_rect, egui::Button::new(egui_phosphor::regular::PENCIL_SIMPLE).small()).clicked() {
                        self.renaming = Some(sum.id);
                        self.rename_buf = sum.custom_name.clone().unwrap_or_default();
                    }
                    if ui.put(view_rect, egui::Button::new(egui_phosphor::regular::EYE).small()).clicked() {
                        to_load = Some(sum.id);
                    }
                    if ui.put(del_rect, egui::Button::new(egui_phosphor::regular::X).small()).clicked() {
                        to_delete = Some(sum.id);
                    }

                    if row_resp.clicked() {
                        to_load = Some(sum.id);
                    }
                }
            });

        if let Some((id, name)) = rename_commit {
            if let Err(e) = self.store.rename(id, name.clone()) {
                self.status = format!("Rename failed: {e}");
            } else if let Some(s) = self.summaries.iter_mut().find(|s| s.id == id) {
                s.custom_name = name;
            }
        }
        if let Some(id) = to_load {
            match self.store.load(id) {
                Ok(enc) => {
                    self.detail = Some(enc);
                    self.detail_player = None;
                    self.selected_phase = None;
                }
                Err(e) => self.status = format!("Load failed: {e}"),
            }
        }
        if let Some(id) = to_delete {
            self.pending_delete = Some(id);
        }
    }
}

impl EncounterHistoryModule {
    fn render_detail(&mut self, ui: &mut egui::Ui) {
        let Some(enc) = self.detail.clone() else { return };

        ui.horizontal(|ui| {
            if ui.button(format!("{} Back", egui_phosphor::regular::ARROW_LEFT)).clicked() {
                self.detail = None;
                self.detail_player = None;
                return;
            }
            let zone = if enc.scene_name.is_empty() { "Unknown Zone" } else { &enc.scene_name };
            let title = enc.custom_name.as_deref().unwrap_or(zone);
            ui.strong(title);
            let outcome_label = match enc.outcome {
                EncounterOutcome::Cleared    => "Cleared",
                EncounterOutcome::Failed     => "Failed",
                EncounterOutcome::ManualStop => "Manual Stop",
                EncounterOutcome::Unknown    => "Unknown",
            };
            let outcome_color = match enc.outcome {
                EncounterOutcome::Cleared    => ui::theme::GOOD,
                EncounterOutcome::Failed     => ui::theme::BAD,
                EncounterOutcome::ManualStop => ui::theme::ACCENT2,
                EncounterOutcome::Unknown    => ui::theme::TEXT_FAINT,
            };
            ui.label(egui::RichText::new(format!(" [{outcome_label}]")).small().color(outcome_color));
            ui.label(
                egui::RichText::new(format!(
                    "  {}  {}",
                    to_wib(enc.started_at).format("%Y-%m-%d %H:%M"),
                    format_duration(enc.duration_secs)
                ))
                .small()
                .color(egui::Color32::from_rgb(140, 140, 160)),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(&mut self.players_only, "Players Only");
            });
        });

        if !enc.phases.is_empty() {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("PHASE").size(9.5).color(ui::theme::TEXT_FAINT));
                if ui.selectable_label(self.selected_phase.is_none(), "All").clicked() {
                    self.selected_phase = None;
                }
                // Index 0 is the implicit "Phase 1" — everything before the
                // first recorded marker (see bucket_player_phases). Recorded
                // markers (enc.phases[i]) are index i+1.
                if ui.selectable_label(self.selected_phase == Some(0), "Phase 1").clicked() {
                    self.selected_phase = Some(0);
                }
                for (i, phase) in enc.phases.iter().enumerate() {
                    let idx = i + 1;
                    let label = if phase.name.is_empty() { format!("Phase {}", idx + 1) } else { phase.name.clone() };
                    if ui.selectable_label(self.selected_phase == Some(idx), label).clicked() {
                        self.selected_phase = Some(idx);
                    }
                }
            });
        }
        ui.separator();

        if self.detail.is_none() { return; }

        let dps_div = enc.duration_secs.max(1.0);

        // Per-row values, resolved either from the whole encounter or a single phase.
        struct Row<'a> {
            p:            &'a SavedPlayerMeter,
            total_damage: u64,
            hits:         u64,
            damage_taken: u64,
            total_healing: u64,
        }

        // With a phase selected, drop monsters that had zero activity in it
        // (e.g. trash mobs left over from an earlier phase) instead of
        // showing an all-zero row — players stay listed either way since
        // they're still relevant party context even if idle that phase.
        let rows: Vec<Row> = enc.players.iter()
            .filter(|p| !self.players_only || p.is_player)
            .filter_map(|p| {
                if let Some(phase_idx) = self.selected_phase {
                    let ph = p.phase_stats.iter().find(|s| s.phase_index == phase_idx);
                    let active = ph.is_some_and(|ph| ph.total_damage > 0 || ph.hits > 0 || ph.damage_taken > 0 || ph.total_healing > 0);
                    if !active && !p.is_player { return None; }
                    let ph = ph.cloned().unwrap_or_default();
                    return Some(Row { p, total_damage: ph.total_damage, hits: ph.hits, damage_taken: ph.damage_taken, total_healing: ph.total_healing });
                }
                Some(Row { p, total_damage: p.total_damage, hits: p.hit_count, damage_taken: p.damage_taken, total_healing: p.total_healing })
            })
            .collect();

        let total = rows.iter().map(|r| r.total_damage).sum::<u64>().max(1);
        let taken_total = rows.iter().map(|r| r.damage_taken).sum::<u64>().max(1);

        let mut sorted: Vec<(usize, &Row)> = rows.iter().enumerate().collect();
        let key = self.sort_key;
        sorted.sort_by(|(_, a), (_, b)| {
            let ord = match key {
                SortKey::Damage      => a.total_damage.cmp(&b.total_damage),
                SortKey::DamageTaken => a.damage_taken.cmp(&b.damage_taken),
                SortKey::Healing     => a.total_healing.cmp(&b.total_healing),
                SortKey::Hits        => a.hits.cmp(&b.hits),
                SortKey::CritRate    => a.p.crit_count.cmp(&b.p.crit_count),
                SortKey::Deaths      => a.p.deaths.cmp(&b.p.deaths),
            };
            if self.sort_desc { ord.reverse() } else { ord }
        });

        egui::ScrollArea::vertical()
            .id_salt("detail_vscroll")
            .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("detail_hscroll")
                .show(ui, |ui| {

                ui.horizontal(|ui| {
                    ui.add_sized([RANK_COL_WIDTH, 16.0], egui::Label::new(""));
                    sort_header(ui, "Player", None, &mut self.sort_key, &mut self.sort_desc, NAME_COL_WIDTH);
                    plain_header(ui, "Class", CLASS_COL_WIDTH);
                    plain_header(ui, "Spec", 90.0);
                    plain_header(ui, "AS", 55.0);
                    plain_header(ui, "SS", 55.0);
                    sort_header(ui, "Total DMG", Some(SortKey::Damage), &mut self.sort_key, &mut self.sort_desc, 130.0);
                    plain_header(ui, "DPS", 75.0);
                    sort_header(ui, "Hits", Some(SortKey::Hits), &mut self.sort_key, &mut self.sort_desc, 55.0);
                    sort_header(ui, "Crit%", Some(SortKey::CritRate), &mut self.sort_key, &mut self.sort_desc, 60.0);
                    plain_header(ui, "Lucky%", 60.0);
                    plain_header(ui, "Crit DMG", 90.0);
                    plain_header(ui, "Lucky DMG", 90.0);
                    plain_header(ui, "CritLucky DMG", 100.0);
                    plain_header(ui, "Max DPS", 90.0);
                    plain_header(ui, "Shield", 80.0);
                    sort_header(ui, "Total Heal", Some(SortKey::Healing), &mut self.sort_key, &mut self.sort_desc, 100.0);
                    plain_header(ui, "HPS", 75.0);
                    plain_header(ui, "Eff Heal", 90.0);
                    plain_header(ui, "Overheal", 90.0);
                    plain_header(ui, "Crit Heal", 90.0);
                    plain_header(ui, "Lucky Heal", 90.0);
                    plain_header(ui, "CritLucky Heal", 100.0);
                    plain_header(ui, "Max HPS", 90.0);
                    plain_header(ui, "Max HP", 90.0);
                    sort_header(ui, "DMG Taken", Some(SortKey::DamageTaken), &mut self.sort_key, &mut self.sort_desc, 100.0);
                    sort_header(ui, "Deaths", Some(SortKey::Deaths), &mut self.sort_key, &mut self.sort_desc, 60.0);
                });
                ui.add_space(2.0);

                for (rank, (i, row)) in sorted.iter().enumerate() {
                    let p = row.p;
                    let dps      = row.total_damage as f64 / dps_div;
                    let heal_ps  = row.total_healing as f64 / dps_div;
                    let crit_rt  = if row.hits > 0 { p.crit_count as f64 / row.hits as f64 * 100.0 } else { 0.0 };
                    let lucky_rt = if row.hits > 0 { p.damage_lucky_hits as f64 / row.hits as f64 * 100.0 } else { 0.0 };
                    let share    = row.total_damage as f64 / total as f64 * 100.0;
                    let taken_share = row.damage_taken as f64 / taken_total as f64 * 100.0;
                    let effective_heal = p.total_healing.saturating_sub(p.overheal);
                    let is_sel   = self.detail_player == Some(*i);

                    let row_color = entity_row_color(p);
                    let bg = egui::Color32::from_rgba_unmultiplied(row_color.r(), row_color.g(), row_color.b(), ROW_BG_ALPHA);

                    let (row_rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().max(1600.0), ROW_HEIGHT), egui::Sense::hover());
                    ui.painter().rect_filled(row_rect, 3.0, bg);
                    ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect), |ui| {
                        ui.horizontal(|ui| {
                            ui.add_sized([RANK_COL_WIDTH, CELL_HEIGHT], egui::Label::new(
                                egui::RichText::new(format!("{}", rank + 1)).size(CELL_FONT_SIZE).strong().color(ui::theme::TEXT),
                            ));
                            let name = egui::RichText::new(truncate_name(&p.name, 16)).size(CELL_FONT_SIZE).strong().color(ui::theme::TEXT);
                            if ui.add_sized([NAME_COL_WIDTH, CELL_HEIGHT], egui::SelectableLabel::new(is_sel, name)).clicked() {
                                self.detail_player = if is_sel { None } else { Some(*i) };
                            }
                            fixed_cell(ui, CLASS_COL_WIDTH, entity_category_name(p), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, p.spec.as_deref().unwrap_or("—"), CELL_FONT_SIZE);
                            fixed_cell(ui, 55.0, &p.ability_score.map(|v| v.to_string()).unwrap_or_else(|| "—".into()), CELL_FONT_SIZE);
                            fixed_cell(ui, 55.0, &p.season_strength.filter(|&v| v > 0).map(|v| format!("{v}")).unwrap_or_else(|| "—".into()), CELL_FONT_SIZE);
                            fixed_cell(ui, 130.0, &format!("{} ({:.1}%)", fmt_damage(row.total_damage), share), CELL_FONT_SIZE);
                            fixed_cell(ui, 75.0, &fmt_damage(dps as u64), CELL_FONT_SIZE);
                            fixed_cell(ui, 55.0, &row.hits.to_string(), CELL_FONT_SIZE);
                            fixed_cell(ui, 60.0, &format!("{:.1}%", crit_rt), CELL_FONT_SIZE);
                            fixed_cell(ui, 60.0, &format!("{:.1}%", lucky_rt), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.damage_crit_total), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.damage_lucky_total), CELL_FONT_SIZE);
                            fixed_cell(ui, 100.0, &fmt_damage(p.damage_crit_lucky_total), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.max_hit), CELL_FONT_SIZE);
                            fixed_cell(ui, 80.0, &fmt_damage(p.shield_gain), CELL_FONT_SIZE);
                            fixed_cell(ui, 100.0, &fmt_damage(row.total_healing), CELL_FONT_SIZE);
                            fixed_cell(ui, 75.0, &fmt_damage(heal_ps as u64), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(effective_heal), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.overheal), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.heal_crit_total), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.heal_lucky_total), CELL_FONT_SIZE);
                            fixed_cell(ui, 100.0, &fmt_damage(p.heal_crit_lucky_total), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &fmt_damage(p.heal_max_hit), CELL_FONT_SIZE);
                            fixed_cell(ui, 90.0, &p.max_hp.map(|v| fmt_damage(v.max(0) as u64)).unwrap_or_else(|| "—".into()), CELL_FONT_SIZE);
                            fixed_cell(ui, 100.0, &format!("{} ({:.1}%)", fmt_damage(row.damage_taken), taken_share), CELL_FONT_SIZE);
                            fixed_cell(ui, 60.0, &p.deaths.to_string(), CELL_FONT_SIZE);
                        });
                    });
                    ui.add_space(1.0);

                    // Inline skill breakdown + Imagines (left) and gear (right) when selected
                    if is_sel {
                        ui.horizontal(|ui| {
                            ui.add_space(24.0);
                            ui.columns(2, |cols| {
                                cols[0].label(egui::RichText::new("SKILL BREAKDOWN").small().strong().color(ui::theme::TEXT_MUTED));
                                cols[0].add_space(4.0);
                                let mut skills = if let Some(phase_idx) = self.selected_phase {
                                    p.phase_stats.iter().find(|s| s.phase_index == phase_idx)
                                        .map(|s| s.skills.clone()).unwrap_or_default()
                                } else {
                                    p.skills.clone()
                                };
                                skills.sort_by(|a, b| b.total_dmg.cmp(&a.total_dmg));
                                egui::Grid::new(format!("skills_{i}"))
                                    .num_columns(5)
                                    .striped(true)
                                    .show(&mut cols[0], |ui| {
                                        for h in ["Skill","Damage","Hits","Crit%","MaxHit"] {
                                            ui.label(egui::RichText::new(h).size(CELL_FONT_SIZE).strong());
                                        }
                                        ui.end_row();
                                        for sk in &skills {
                                            let sk_crit = if sk.hits > 0 { sk.crits as f64 / sk.hits as f64 * 100.0 } else { 0.0 };
                                            let skill_label = core::DATA.skill_name(sk.skill_id)
                                                .map(|s| s.to_string())
                                                .unwrap_or_else(|| format!("#{}", sk.skill_id));
                                            ui.label(egui::RichText::new(&skill_label).size(CELL_FONT_SIZE));
                                            ui.label(egui::RichText::new(fmt_damage(sk.total_dmg)).size(CELL_FONT_SIZE));
                                            ui.label(egui::RichText::new(sk.hits.to_string()).size(CELL_FONT_SIZE));
                                            ui.label(egui::RichText::new(format!("{:.1}%", sk_crit)).size(CELL_FONT_SIZE));
                                            ui.label(egui::RichText::new(fmt_damage(sk.max_hit)).size(CELL_FONT_SIZE));
                                            ui.end_row();
                                        }
                                    });

                                cols[0].add_space(8.0);
                                cols[0].label(egui::RichText::new("IMAGINES").small().strong().color(ui::theme::TEXT_MUTED));
                                cols[0].add_space(4.0);

                                cols[1].label(egui::RichText::new("GEAR").small().strong().color(ui::theme::TEXT_MUTED));
                                cols[1].add_space(4.0);
                                if let Some(gear) = &p.gear {
                                    let slots: Vec<_> = gear.gear.iter().map(|s| ui::widgets::gear_panel::GearSlotView {
                                        slot:      s.slot,
                                        item_id:   s.item_id,
                                        item_name: s.item_name.clone(),
                                    }).collect();
                                    let imagines: Vec<_> = gear.imagines.iter().map(|im| ui::widgets::gear_panel::ImagineView {
                                        skill_id: im.skill_id,
                                        tier:     im.tier,
                                        name:     im.name.clone(),
                                    }).collect();
                                    ui::widgets::gear_panel::render_gear(&mut cols[1], &mut self.gear_panel_state, &slots);
                                    ui::widgets::gear_panel::render_imagines(&mut cols[0], &mut self.gear_panel_state, &imagines);
                                } else if p.is_player {
                                    cols[1].label(egui::RichText::new("Not available").small().color(ui::theme::TEXT_FAINT));
                                    cols[0].label(egui::RichText::new("Not available").small().color(ui::theme::TEXT_FAINT));
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                }
            });
        });
    }
}

fn col_header(ui: &mut egui::Ui, label: &str, _width: f32) {
    ui.label(
        egui::RichText::new(label)
            .size(HEADER_FONT_SIZE)
            .color(ui::theme::TEXT),
    );
}

fn plain_header(ui: &mut egui::Ui, label: &str, width: f32) {
    ui.add_sized([width, 18.0], egui::Label::new(egui::RichText::new(label).size(HEADER_FONT_SIZE).color(ui::theme::TEXT)));
}

fn sort_header(ui: &mut egui::Ui, label: &str, key: Option<SortKey>, cur: &mut SortKey, desc: &mut bool, width: f32) {
    let Some(key) = key else { return plain_header(ui, label, width); };
    let active = *cur == key;
    let arrow = if active {
        if *desc { format!(" {}", egui_phosphor::regular::SORT_DESCENDING) } else { format!(" {}", egui_phosphor::regular::SORT_ASCENDING) }
    } else {
        String::new()
    };
    let text = egui::RichText::new(format!("{label}{arrow}"))
        .size(HEADER_FONT_SIZE)
        .color(if active { ui::theme::ACCENT } else { ui::theme::TEXT });
    if ui.add_sized([width, 18.0], egui::Button::new(text).small().fill(egui::Color32::TRANSPARENT)).clicked() {
        if active { *desc = !*desc; } else { *cur = key; *desc = true; }
    }
}

fn fixed_cell(ui: &mut egui::Ui, width: f32, text: &str, size: f32) {
    ui.add_sized([width, CELL_HEIGHT], egui::Label::new(egui::RichText::new(text).size(size).strong().color(ui::theme::TEXT)));
}

/// Truncates a display name to at most `max_chars` characters, appending an
/// ellipsis, so long monster/player names never blow past their fixed column
/// width and shove later columns out of alignment with the header.
fn truncate_name(name: &str, max_chars: usize) -> String {
    if name.chars().count() > max_chars {
        let mut s: String = name.chars().take(max_chars.saturating_sub(1)).collect();
        s.push('…');
        s
    } else {
        name.to_string()
    }
}

fn format_duration(secs: f64) -> String {
    let s = secs as u64;
    format!("{:02}:{:02}", s / 60, s % 60)
}
