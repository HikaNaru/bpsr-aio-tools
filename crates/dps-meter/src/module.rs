use crate::dps_state::DpsState;
use crate::encounter::Encounter;
use crate::meter::{PlayerMeter, SkillStat};
use core::{
    module::{Module, ModuleContext},
    types::EntityId,
};
use encounter_store::{EncounterStore, SavedEncounter, SavedPlayerMeter, SavedSkillStat};
use game::entity::CharStats;
use game::GameEvent;
use std::collections::HashMap;
use std::time::Instant;
#[allow(unused_imports)]
use egui_plot;

// ── Benchmark ─────────────────────────────────────────────────────────────────

pub struct BenchmarkConfig {
    pub duration_secs: u32,
    pub single_target: bool,
    pub auto_target:   bool,
    pub target_uid:    String,  // hex string; empty = auto
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            duration_secs: 60,
            single_target: false,
            auto_target:   true,
            target_uid:    String::new(),
        }
    }
}

struct BenchResult {
    duration_secs: f64,
    total_damage:  u64,
    dps:           f64,
    hit_count:     u64,
    crit_count:    u64,
    skills:        Vec<SkillStat>,
}

// ── Module struct ─────────────────────────────────────────────────────────────

pub struct DpsMeterModule {
    state:            DpsState,
    names:            HashMap<EntityId, String>,
    classes:          HashMap<EntityId, u32>,
    stats_cache:      HashMap<EntityId, CharStats>,
    pending:          Vec<GameEvent>,
    selected_enc:     Option<usize>,
    selected_player:  Option<EntityId>,
    pinned_player:    Option<EntityId>,
    dps_window_secs:  f64,
    current_zone:     String,
    current_zone_id:  Option<u32>,
    store:            EncounterStore,
    rank_mode:        u8,  // 0=DPS 1=Taken 2=Healing

    // Benchmark
    bench_config:    BenchmarkConfig,
    bench_active:    bool,
    bench_start:     Option<Instant>,
    bench_target:    Option<EntityId>,
    bench_result:    Option<BenchResult>,
    show_bench:      bool,
}

impl DpsMeterModule {
    pub fn new(encounter_timeout_secs: u32) -> Self {
        Self {
            state:           DpsState::new(encounter_timeout_secs),
            names:           HashMap::new(),
            classes:         HashMap::new(),
            stats_cache:     HashMap::new(),
            pending:         Vec::new(),
            selected_enc:    None,
            selected_player: None,
            pinned_player:   None,
            dps_window_secs: 3.0,
            current_zone:    String::new(),
            current_zone_id: None,
            store:           EncounterStore::open(),
            rank_mode:       0,
            bench_config:    BenchmarkConfig::default(),
            bench_active:    false,
            bench_start:     None,
            bench_target:    None,
            bench_result:    None,
            show_bench:      false,
        }
    }

    fn bench_start_encounter(&mut self) {
        self.state.finish_active();
        self.state.active = Some(Encounter::new());
        self.bench_start = Some(Instant::now());
        self.bench_target = None;
    }

    fn bench_finish(&mut self) {
        // Snapshot result from active encounter before closing
        if let Some(enc) = &self.state.active {
            let duration = self.bench_start
                .map(|s| s.elapsed().as_secs_f64())
                .unwrap_or(enc.elapsed().as_secs_f64());
            let duration = duration.max(1.0);

            // Sum across all players (or only local if single-target already filtered)
            let total_damage = enc.total_damage;
            let dps          = total_damage as f64 / duration;
            let mut hit_count  = 0u64;
            let mut crit_count = 0u64;
            let mut all_skills: HashMap<u32, SkillStat> = HashMap::new();
            for p in enc.players.values() {
                hit_count  += p.hit_count;
                crit_count += p.crit_count;
                for (id, sk) in &p.skill_breakdown {
                    let e = all_skills.entry(*id).or_insert_with(|| SkillStat {
                        skill_id: *id, ..Default::default()
                    });
                    e.total_dmg += sk.total_dmg;
                    e.hits      += sk.hits;
                    e.crits     += sk.crits;
                    if sk.max_hit > e.max_hit { e.max_hit = sk.max_hit; }
                }
            }
            let mut skills: Vec<SkillStat> = all_skills.into_values().collect();
            skills.sort_by(|a, b| b.total_dmg.cmp(&a.total_dmg));

            self.bench_result = Some(BenchResult {
                duration_secs: duration,
                total_damage,
                dps,
                hit_count,
                crit_count,
                skills,
            });
        }
        self.state.finish_active();
        self.bench_active = false;
        self.bench_start  = None;
        self.bench_target = None;
    }

    pub fn push_event(&mut self, event: GameEvent) {
        self.pending.push(event);
    }

    fn save_encounter(&self, enc: &Encounter) {
        let saved = to_saved_encounter(enc, &self.current_zone, &self.classes, &self.stats_cache);
        let store = self.store.clone();
        std::thread::spawn(move || {
            if let Err(e) = store.save(&saved) {
                tracing::warn!("encounter save failed: {e}");
            }
        });
    }
}

impl Module for DpsMeterModule {
    fn id(&self)   -> &'static str { "dps-meter" }
    fn name(&self) -> &str         { "DPS Meter" }
    fn icon(&self) -> &str         { "📊" }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, ctx: &ModuleContext) {
        self.dps_window_secs = ctx.config.dps_window_secs as f64;
        self.state.tick();

        let events: Vec<GameEvent> = self.pending.drain(..).collect();

        for event in events {
            match &event {
                GameEvent::EntityName { id, name, class } => {
                    if !name.is_empty() {
                        self.names.insert(*id, name.clone());
                    }
                    if let Some(c) = class {
                        self.classes.insert(*id, *c);
                    }
                    if let Some(enc) = &mut self.state.active {
                        if let Some(meter) = enc.players.get_mut(id) {
                            if !name.is_empty() { meter.player_name = name.clone(); }
                        }
                    }
                    for enc in &mut self.state.past {
                        if let Some(meter) = enc.players.get_mut(id) {
                            if !name.is_empty() { meter.player_name = name.clone(); }
                        }
                    }
                }
                GameEvent::EntityDespawn { id } => {
                    self.names.remove(id);
                    self.classes.remove(id);
                    self.stats_cache.remove(id);
                }
                GameEvent::EntityStats { id, stats } => {
                    self.stats_cache.entry(*id).or_default().merge(stats);
                }
                GameEvent::ZoneChange { zone_id, zone_name } => {
                    let actually_changed = Some(*zone_id) != self.current_zone_id;
                    self.current_zone_id = Some(*zone_id);
                    if !zone_name.is_empty() {
                        self.current_zone = zone_name.clone();
                    }
                    if actually_changed {
                        self.state.apply_state_event(&event);
                        if self.pinned_player.is_none() {
                            self.selected_player = None;
                        }
                    }
                }
                GameEvent::DungeonState { .. } => {
                    self.state.apply_state_event(&event);
                }
                GameEvent::Heal(h) => {
                    if self.names.contains_key(&h.source_id) {
                        let names = &self.names;
                        if let Some(enc) = &mut self.state.active {
                            enc.apply_heal(h.source_id, h.damage, |id| {
                                names.get(&id).cloned()
                                    .unwrap_or_else(|| format!("Entity {:x}", id.0))
                            });
                        }
                    }
                }
                GameEvent::Chat { .. } | GameEvent::MatchmakingAlert { .. } => {}
                GameEvent::Combat(c) => {
                    if self.bench_active {
                        // Start bench encounter on first hit
                        if self.bench_start.is_none() {
                            self.bench_start_encounter();
                        }
                        // Lock target on first hit in single-target mode
                        if self.bench_config.single_target && self.bench_target.is_none() {
                            if self.bench_config.auto_target {
                                self.bench_target = Some(c.target_id);
                            } else if let Ok(uid) = u64::from_str_radix(
                                self.bench_config.target_uid.trim().trim_start_matches("0x"), 16
                            ) {
                                if uid != 0 { self.bench_target = Some(EntityId(uid)); }
                            }
                        }
                        // Filter out off-target hits
                        if self.bench_config.single_target {
                            if let Some(tgt) = self.bench_target {
                                if c.target_id != tgt { continue; }
                            }
                        }
                    }
                    let names = &self.names;
                    self.state.apply_event(&event, |id| {
                        names.get(&id).cloned()
                            .unwrap_or_else(|| format!("Entity {:x}", id.0))
                    });
                    // Track damage taken for known players
                    if names.contains_key(&c.target_id) {
                        if let Some(enc) = &mut self.state.active {
                            enc.apply_taken(c.target_id, c.damage);
                        }
                    }
                }
                _ => {}
            }
        }

        // Check bench timer expiry
        if self.bench_active {
            if let Some(start) = self.bench_start {
                if start.elapsed().as_secs() >= self.bench_config.duration_secs as u64 {
                    self.bench_finish();
                }
            }
        }

        // Persist any newly finished encounters; fire discord if configured
        let finished: Vec<_> = self.state.newly_finished.drain(..).collect();
        for enc in &finished {
            self.save_encounter(enc);
            if ctx.config.discord_enabled
                && !ctx.config.discord_webhook_url.is_empty()
                && enc.players.len() >= ctx.config.discord_min_players
            {
                let report = to_discord_report(enc, &self.current_zone, &self.stats_cache, &self.classes);
                core::discord::send_report_async(ctx.config.discord_webhook_url.clone(), report);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        let enc_data = match self.selected_enc {
            None    => self.state.active.as_ref(),
            Some(i) => self.state.past.get(i),
        }.map(|enc| {
            let elapsed   = enc.elapsed().as_secs_f64();
            let players: Vec<_> = enc.players_by_damage().into_iter().cloned().collect();
            let max_dmg   = players.first().map(|p| p.total_damage).unwrap_or(1);
            let total_dmg = enc.total_damage;
            (elapsed, players, max_dmg, total_dmg)
        });

        // ── Tab row ──────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            let live_sel = !self.show_bench && self.selected_enc.is_none();
            if tab_btn(ui, "Live", live_sel).clicked() {
                self.selected_enc = None;
                self.show_bench = false;
            }
            for i in (0..self.state.past.len()).rev().take(5) {
                let enc = &self.state.past[i];
                let label = format!("#{}", self.state.past.len() - i);
                let sel = !self.show_bench && self.selected_enc == Some(i);
                if tab_btn(ui, &label, sel)
                    .on_hover_text(format!("{:.0}s", enc.elapsed().as_secs_f64()))
                    .clicked()
                {
                    self.selected_enc = Some(i);
                    self.show_bench = false;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let bench_label = if self.bench_active {
                    let rem = self.bench_start.map(|s| {
                        self.bench_config.duration_secs.saturating_sub(s.elapsed().as_secs() as u32)
                    }).unwrap_or(self.bench_config.duration_secs);
                    format!("🎯 {}s", rem)
                } else {
                    "🎯 Bench".to_string()
                };
                let bench_color = if self.bench_active { ui::theme::WARN } else { ui::theme::TEXT_MUTED };
                if tab_btn(ui, &bench_label, self.show_bench)
                    .on_hover_text("Benchmark mode")
                    .clicked()
                {
                    self.show_bench = !self.show_bench;
                }
                // color override after button paint — just set label color on hover text
                let _ = bench_color;
            });
        });

        ui.add_space(6.0);

        if self.show_bench {
            self.render_bench(ui);
            return;
        }

        match enc_data {
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("No active encounter. Start combat to begin tracking.")
                            .color(ui::theme::TEXT_MUTED),
                    );
                });
            }
            Some((elapsed, players, max_dmg, _total_dmg)) => {
                let mut do_reset = false;
                let mut new_selection: Option<Option<EntityId>> = None;

                egui::ScrollArea::vertical()
                    .id_salt("dps_main_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                ui.style_mut().interaction.selectable_labels = false;

                // ── Encounter header ──────────────────────────────────────────
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{:02}:{:02}",
                            elapsed as u64 / 60,
                            elapsed as u64 % 60
                        ))
                        .strong()
                        .size(13.0)
                        .color(ui::theme::ACCENT2),
                    );
                    if !self.current_zone.is_empty() {
                        ui.label(
                            egui::RichText::new(&self.current_zone)
                                .size(11.0)
                                .color(ui::theme::TEXT_FAINT),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Reset").clicked() {
                            do_reset = true;
                        }
                    });
                });

                if do_reset {
                    self.state.reset();
                    self.selected_enc = None;
                    self.selected_player = None;
                    return;
                }

                // ── Summary tiles — player-only sums ─────────────────────────
                let names = &self.names;
                let player_total_dmg: u64 = players.iter()
                    .filter(|p| names.contains_key(&p.entity_id))
                    .map(|p| p.total_damage).sum();
                let player_total_taken: u64 = players.iter()
                    .filter(|p| names.contains_key(&p.entity_id))
                    .map(|p| p.damage_taken).sum();
                let player_total_healed: u64 = players.iter()
                    .filter(|p| names.contains_key(&p.entity_id))
                    .map(|p| p.total_healing).sum();
                let dps      = if elapsed > 0.0 { player_total_dmg as f64   / elapsed } else { 0.0 };
                let taken_ps = if elapsed > 0.0 { player_total_taken as f64  / elapsed } else { 0.0 };
                let heal_ps  = if elapsed > 0.0 { player_total_healed as f64 / elapsed } else { 0.0 };

                ui.add_space(8.0);
                let tile_w = 130.0;
                egui::ScrollArea::horizontal()
                    .id_salt("tile_hscroll")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            summary_tile(ui, tile_w, "TOTAL DMG",  &fmt_damage(player_total_dmg), ui::theme::TEXT,   false);
                            ui.add_space(8.0);
                            summary_tile(ui, tile_w, "DPS",        &fmt_damage(dps as u64),      ui::theme::ACCENT, false);
                            ui.add_space(8.0);
                            summary_tile(ui, tile_w, "DMG TAKEN/s", &fmt_damage(taken_ps as u64), ui::theme::WARN,   false);
                            ui.add_space(8.0);
                            summary_tile(ui, tile_w, "HEALING/s",  &fmt_damage(heal_ps as u64),   ui::theme::GOOD,   false);
                            ui.add_space(8.0);
                            summary_tile(ui, tile_w, "DURATION",
                                &format!("{:02}:{:02}  {}p", elapsed as u64 / 60, elapsed as u64 % 60, players.len()),
                                ui::theme::ACCENT2, false);
                        });
                    });

                ui.add_space(10.0);

                // ── Two-column body (responsive) ──────────────────────────────
                let available = ui.available_width();
                let use_columns = available >= 480.0;
                let left_w = if use_columns { available * 0.56 } else { available };
                let right_w = if use_columns { available - left_w - 10.0 } else { available };

                // ── Party Ranking panel (shared closure) ──────────────────────
                let mut render_party_panel = |ui: &mut egui::Ui,
                                             new_sel: &mut Option<Option<EntityId>>| {
                    panel_card(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("PARTY RANKING").strong().size(11.0).color(ui::theme::TEXT));
                            ui.add_space(4.0);
                            ui.label(egui::RichText::new("· click for breakdown").size(10.0).color(ui::theme::TEXT_FAINT));
                        });
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            for (idx, label) in ["Top DPS", "Taken", "Healing"].iter().enumerate() {
                                if rank_tab_btn(ui, label, self.rank_mode == idx as u8).clicked() {
                                    self.rank_mode = idx as u8;
                                }
                            }
                        });
                        ui.add_space(8.0);
                        // Sort by selected mode
                        let mut ranked: Vec<_> = players.iter().collect();
                        match self.rank_mode {
                            1 => ranked.sort_by(|a, b| b.damage_taken.cmp(&a.damage_taken)),
                            2 => ranked.sort_by(|a, b| b.total_healing.cmp(&a.total_healing)),
                            _ => {} // already sorted by total_damage
                        }
                        let max_rank_val = match self.rank_mode {
                            1 => ranked.first().map(|p| p.damage_taken).unwrap_or(1).max(1),
                            2 => ranked.first().map(|p| p.total_healing).unwrap_or(1).max(1),
                            _ => max_dmg,
                        };
                        egui::ScrollArea::vertical()
                            .id_salt("party_scroll")
                            .max_height(200.0)
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                            .show(ui, |ui| {
                                ui.style_mut().interaction.selectable_labels = false;
                                for (rank, player) in ranked.iter().enumerate() {
                                    let (rank_value, rank_total) = match self.rank_mode {
                                        1 => (
                                            if elapsed > 0.0 { player.damage_taken as f64 / elapsed } else { 0.0 },
                                            player.damage_taken,
                                        ),
                                        2 => (
                                            if elapsed > 0.0 { player.total_healing as f64 / elapsed } else { 0.0 },
                                            player.total_healing,
                                        ),
                                        _ => (player.avg_dps(elapsed), player.total_damage),
                                    };
                                    let bar_frac = (rank_total as f32 / max_rank_val as f32).min(1.0);
                                    let is_sel   = self.selected_player == Some(player.entity_id);
                                    let class_id = self.classes.get(&player.entity_id).copied();
                                    let stats    = self.stats_cache.get(&player.entity_id);
                                    let spec     = player_spec(player);
                                    let is_player = names.contains_key(&player.entity_id);
                                    let resp = player_row(ui, rank + 1, player, rank_value, rank_total, bar_frac, is_sel, class_id, stats, spec, is_player);
                                    if resp.clicked() {
                                        *new_sel = Some(if is_sel { None } else { Some(player.entity_id) });
                                    }
                                }
                            });
                        if let Some(sel_id) = self.selected_player {
                            if let Some(player) = players.iter().find(|p| p.entity_id == sel_id) {
                                if !player.dps_timeline.is_empty() {
                                    ui.add_space(8.0);
                                    ui.label(egui::RichText::new("DPS OVER TIME").size(10.0).color(ui::theme::TEXT_FAINT));
                                    let plot_points: egui_plot::PlotPoints = player.dps_timeline
                                        .iter().map(|&(x, y)| [x, y]).collect();
                                    egui_plot::Plot::new(format!("dps_chart_{}", sel_id.0))
                                        .height(60.0).show_axes([false, false]).show_grid(false)
                                        .allow_zoom(false).allow_drag(false)
                                        .show(ui, |plot_ui| {
                                            plot_ui.line(egui_plot::Line::new(plot_points).color(ui::theme::ACCENT).width(1.5));
                                        });
                                }
                            }
                        }
                    });
                };

                let mut render_skill_panel = |ui: &mut egui::Ui,
                                             new_sel: &mut Option<Option<EntityId>>| {
                    panel_card(ui, |ui| {
                        ui.label(egui::RichText::new("SKILL BREAKDOWN").strong().size(11.0).color(ui::theme::TEXT));
                        if let Some(sel_id) = self.selected_player {
                            if let Some(player) = players.iter().find(|p| p.entity_id == sel_id) {
                                let class_id  = self.classes.get(&sel_id).copied();
                                let spec_str  = player_spec(player).map(|s| format!(" · {s}")).unwrap_or_default();
                                ui.label(
                                    egui::RichText::new(format!("{} · {}{}", player.player_name, class_name(class_id), spec_str))
                                        .size(11.0).color(ui::theme::ACCENT),
                                );
                                ui.add_space(8.0);
                                let total = player.total_damage.max(1);
                                let mut skills: Vec<_> = player.skill_breakdown.values().collect();
                                skills.sort_by(|a, b| b.total_dmg.cmp(&a.total_dmg));
                                egui::ScrollArea::vertical()
                                    .id_salt(format!("skill_scroll_{}", sel_id.0))
                                    .min_scrolled_height(220.0)
                                    .max_height(400.0)
                                    .show(ui, |ui| {
                                        for sk in &skills {
                                            let share    = sk.total_dmg as f64 / total as f64;
                                            let sk_crit  = if sk.hits > 0 { sk.crits as f64 / sk.hits as f64 * 100.0 } else { 0.0 };
                                            let sk_label = core::DATA.skill_name(sk.skill_id)
                                                .map(|s| s.to_string())
                                                .unwrap_or_else(|| format!("#{}", sk.skill_id));
                                            skill_row(ui, &sk_label, sk.total_dmg, share, sk.hits, sk_crit);
                                            ui.add_space(4.0);
                                        }
                                    });
                                ui.add_space(10.0);
                                let stats  = self.stats_cache.get(&sel_id);
                                let pinned = &mut self.pinned_player;
                                render_inspector(ui, player, stats, elapsed, sel_id, pinned);
                            } else {
                                if self.pinned_player != Some(sel_id) { *new_sel = Some(None); }
                                ui.label(egui::RichText::new("Player not in this encounter.").size(11.0).color(ui::theme::TEXT_FAINT));
                            }
                        } else {
                            ui.add_space(16.0);
                            ui.label(egui::RichText::new("Click a player to see skill breakdown.").size(11.0).color(ui::theme::TEXT_FAINT));
                        }
                    });
                };

                if use_columns {
                    ui.horizontal_top(|ui| {
                        ui.vertical(|ui| {
                            ui.set_width(left_w);
                            render_party_panel(ui, &mut new_selection);
                        });
                        ui.add_space(10.0);
                        ui.vertical(|ui| {
                            ui.set_width(right_w);
                            render_skill_panel(ui, &mut new_selection);
                        });
                    });
                } else {
                    render_party_panel(ui, &mut new_selection);
                    ui.add_space(10.0);
                    render_skill_panel(ui, &mut new_selection);
                }

                    }); // end ScrollArea

                if let Some(sel) = new_selection {
                    self.selected_player = sel;
                    if sel.is_none() {
                        self.pinned_player = None;
                    }
                }
            }
        }
    }
}

// ── Benchmark panel ───────────────────────────────────────────────────────────

impl DpsMeterModule {
    fn render_bench(&mut self, ui: &mut egui::Ui) {
        // ── Config ───────────────────────────────────────────────────────────
        ui.strong("Configuration");
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            ui.label("Duration (secs):");
            ui.add(
                egui::DragValue::new(&mut self.bench_config.duration_secs)
                    .range(5u32..=600)
                    .speed(1)
            );
        });

        ui.checkbox(&mut self.bench_config.single_target, "Single-target (filter by first enemy hit)");

        if self.bench_config.single_target {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.bench_config.auto_target, "Auto-detect target");
                if !self.bench_config.auto_target {
                    ui.label("UID (hex):");
                    ui.text_edit_singleline(&mut self.bench_config.target_uid);
                }
            });
        }

        ui.add_space(6.0);

        // ── Controls ─────────────────────────────────────────────────────────
        ui.horizontal(|ui| {
            if self.bench_active {
                let elapsed = self.bench_start.map(|s| s.elapsed().as_secs_f64()).unwrap_or(0.0);
                let total   = self.bench_config.duration_secs as f64;
                let rem     = (total - elapsed).max(0.0);

                ui.label(
                    egui::RichText::new(format!("Running — {:.0}s remaining", rem))
                        .color(egui::Color32::from_rgb(240, 160, 40))
                        .strong(),
                );

                // Progress bar
                let frac = (elapsed / total).min(1.0) as f32;
                let (bar_rect, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width() - 60.0, 8.0),
                    egui::Sense::hover(),
                );
                ui.painter().rect_filled(bar_rect, 4.0, egui::Color32::from_rgb(40, 40, 50));
                let mut fill = bar_rect;
                fill.set_right(bar_rect.left() + bar_rect.width() * frac);
                ui.painter().rect_filled(fill, 4.0, egui::Color32::from_rgb(220, 140, 30));

                if ui.button("Stop").clicked() {
                    self.bench_finish();
                }
            } else {
                let btn = ui.button(
                    egui::RichText::new("▶ Start Benchmark").color(egui::Color32::from_rgb(100, 220, 120))
                );
                if btn.clicked() {
                    self.bench_active = true;
                    self.bench_start  = None; // timer starts on first combat hit
                    self.bench_result = None;
                    self.bench_target = None;
                }
                if self.bench_result.is_some() && ui.button("Clear Results").clicked() {
                    self.bench_result = None;
                }
            }
        });

        // ── Results ───────────────────────────────────────────────────────────
        if let Some(r) = &self.bench_result {
            ui.separator();
            ui.strong("Results");
            ui.add_space(4.0);

            let crit_pct = if r.hit_count > 0 {
                r.crit_count as f64 / r.hit_count as f64 * 100.0
            } else { 0.0 };

            egui::Grid::new("bench_result_summary")
                .num_columns(4)
                .show(ui, |ui| {
                    ui.strong("Duration"); ui.strong("Total DMG"); ui.strong("DPS"); ui.strong("Crit%");
                    ui.end_row();
                    ui.label(format!("{:.1}s", r.duration_secs));
                    ui.label(fmt_damage(r.total_damage));
                    ui.label(format!("{:.0}", r.dps));
                    ui.label(format!("{:.1}%", crit_pct));
                    ui.end_row();
                });

            if !r.skills.is_empty() {
                ui.add_space(4.0);
                ui.label(egui::RichText::new("Skill breakdown").small().strong());
                let total = r.total_damage.max(1);

                egui::ScrollArea::vertical().id_salt("bench_skills").max_height(150.0).show(ui, |ui| {
                    egui::Grid::new("bench_skill_grid").num_columns(5).striped(true).show(ui, |ui| {
                        ui.small("Skill"); ui.small("Damage"); ui.small("Hits"); ui.small("Crit%"); ui.small("Share");
                        ui.end_row();
                        for sk in &r.skills {
                            let sk_crit = if sk.hits > 0 { sk.crits as f64 / sk.hits as f64 * 100.0 } else { 0.0 };
                            let share   = sk.total_dmg as f64 / total as f64 * 100.0;
                            let skill_label = core::DATA.skill_name(sk.skill_id)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("#{}", sk.skill_id));
                            ui.label(egui::RichText::new(&skill_label).small());
                            ui.label(egui::RichText::new(fmt_damage(sk.total_dmg)).small());
                            ui.label(egui::RichText::new(sk.hits.to_string()).small());
                            ui.label(egui::RichText::new(format!("{:.1}%", sk_crit)).small());
                            ui.label(egui::RichText::new(format!("{:.1}%", share)).small());
                            ui.end_row();
                        }
                    });
                });
            }
        } else if !self.bench_active {
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("No results yet. Start a benchmark and deal damage.")
                    .color(egui::Color32::from_rgb(140, 140, 160))
            );
        }
    }
}

// ── Inspector panel ───────────────────────────────────────────────────────────

fn render_inspector(
    ui:       &mut egui::Ui,
    player:   &PlayerMeter,
    stats:    Option<&CharStats>,
    duration: f64,
    id:       EntityId,
    pinned:   &mut Option<EntityId>,
) {
    let is_pinned = *pinned == Some(id);

    ui.horizontal(|ui| {
        ui.strong(&player.player_name);
        let pin_label = if is_pinned { "📌 Unpin" } else { "📌 Pin" };
        if ui.small_button(pin_label).on_hover_text("Pin — keep inspector across encounters").clicked() {
            *pinned = if is_pinned { None } else { Some(id) };
        }
    });

    // ── Combat summary ───────────────────────────────────────────────────────
    let dps      = if duration > 0.0 { player.total_damage as f64 / duration } else { 0.0 };
    let heal_ps  = if duration > 0.0 { player.total_healing as f64 / duration } else { 0.0 };
    let crit_pct = player.crit_rate() * 100.0;

    egui::Grid::new("inspector_combat")
        .num_columns(4)
        .striped(false)
        .show(ui, |ui| {
            ui.small("Total DMG"); ui.small("DPS");        ui.small("Hits");     ui.small("Crit%");
            ui.end_row();
            ui.label(fmt_damage(player.total_damage));
            ui.label(format!("{:.0}", dps));
            ui.label(player.hit_count.to_string());
            ui.label(format!("{:.1}%", crit_pct));
            ui.end_row();

            ui.small("DMG Taken"); ui.small("Total Heal"); ui.small("Heal/s");   ui.small("");
            ui.end_row();
            ui.label(fmt_damage(player.damage_taken));
            ui.label(fmt_damage(player.total_healing));
            ui.label(fmt_damage(heal_ps as u64));
            ui.label("");
            ui.end_row();
        });

    // ── Character stats ──────────────────────────────────────────────────────
    if let Some(s) = stats {
        let has_any = s.level.is_some() || s.ability_score.is_some() || s.attack.is_some()
            || s.crit_pct.is_some() || s.haste_pct.is_some() || s.mastery_pct.is_some();
        if has_any {
            ui.add_space(4.0);
            egui::Grid::new("inspector_stats")
                .num_columns(4)
                .striped(false)
                .show(ui, |ui| {
                    if let (Some(lv), Some(gs)) = (s.level, s.ability_score) {
                        ui.small("Lv"); ui.label(lv.to_string());
                        ui.small("AS"); ui.label(gs.to_string());
                        ui.end_row();
                    }
                    if let Some(atk) = s.attack {
                        ui.small("ATK"); ui.label(atk.to_string());
                        if let Some(arm) = s.armor {
                            ui.small("DEF"); ui.label(arm.to_string());
                        } else {
                            ui.label(""); ui.label("");
                        }
                        ui.end_row();
                    }
                    let stat_rows: &[(&str, Option<u32>)] = &[
                        ("Crit%",      s.crit_pct),
                        ("Haste%",     s.haste_pct),
                        ("Mastery%",   s.mastery_pct),
                        ("Luck%",      s.luck_pct),
                        ("Vers%",      s.versatility_pct),
                        ("Block%",     s.block_pct),
                        ("CritDmg%",   s.crit_damage),
                    ];
                    let mut col = 0usize;
                    for (name, val) in stat_rows {
                        if let Some(v) = val {
                            ui.small(*name);
                            ui.label(format!("{:.1}%", *v as f64 / 100.0));
                            col += 1;
                            if col == 2 { ui.end_row(); col = 0; }
                        }
                    }
                    if col != 0 { ui.end_row(); }
                });
        }
    }

    // ── DPS sparkline ────────────────────────────────────────────────────────
    if !player.dps_timeline.is_empty() {
        ui.add_space(4.0);
        let plot_points: egui_plot::PlotPoints = player.dps_timeline
            .iter().map(|&(x, y)| [x, y]).collect();
        egui_plot::Plot::new(format!("inspector_spark_{}", id.0))
            .height(50.0)
            .show_axes([false, false])
            .show_grid(false)
            .allow_zoom(false)
            .allow_drag(false)
            .show(ui, |plot_ui| {
                plot_ui.line(egui_plot::Line::new(plot_points));
            });
    }

}

// ── Conversion helpers ────────────────────────────────────────────────────────

fn to_saved_encounter(
    enc: &Encounter,
    zone: &str,
    classes: &HashMap<EntityId, u32>,
    stats_cache: &HashMap<EntityId, CharStats>,
) -> SavedEncounter {
    let duration = enc.elapsed();
    let now_utc  = chrono::Utc::now();
    let age      = std::time::Instant::now().duration_since(enc.start_time);
    let started_at = now_utc - chrono::Duration::from_std(age).unwrap_or_default();
    let ended_at   = started_at + chrono::Duration::from_std(duration).unwrap_or_default();

    let players = enc.players.values()
        .map(|p| to_saved_player(p, classes, stats_cache.get(&p.entity_id)))
        .collect();

    SavedEncounter {
        id:           enc.id,
        scene_name:   zone.to_string(),
        started_at,
        ended_at,
        duration_secs: duration.as_secs_f64(),
        players,
        total_damage:  enc.total_damage,
    }
}

fn to_saved_player(p: &PlayerMeter, classes: &HashMap<EntityId, u32>, stats: Option<&CharStats>) -> SavedPlayerMeter {
    let skills = p.skill_breakdown.values().map(|s| SavedSkillStat {
        skill_id:  s.skill_id,
        total_dmg: s.total_dmg,
        hits:      s.hits,
        crits:     s.crits,
        max_hit:   s.max_hit,
    }).collect();

    SavedPlayerMeter {
        entity_id:      p.entity_id.0,
        name:           p.player_name.clone(),
        class_id:       classes.get(&p.entity_id).copied(),
        total_damage:   p.total_damage,
        hit_count:      p.hit_count,
        crit_count:     p.crit_count,
        skills,
        damage_taken:   p.damage_taken,
        total_healing:  p.total_healing,
        ability_score:  stats.and_then(|s| s.ability_score),
        season_strength: stats.and_then(|s| s.season_strength),
        crit_pct:       stats.and_then(|s| s.crit_pct),
        luck_pct:       stats.and_then(|s| s.luck_pct),
        crit_damage:    stats.and_then(|s| s.crit_damage),
        spec:           player_spec(p).map(|s| s.to_string()),
    }
}

// ── Discord report builder ────────────────────────────────────────────────────

fn to_discord_report(enc: &Encounter, zone: &str, stats_cache: &HashMap<EntityId, CharStats>, classes: &HashMap<EntityId, u32>) -> core::discord::DiscordReport {
    let age = Instant::now().duration_since(enc.start_time);
    let started_at_secs = chrono::Utc::now().timestamp() - age.as_secs() as i64;
    let duration = enc.elapsed().as_secs_f64().max(1.0);

    let mut players: Vec<_> = enc.players.values()
        .map(|p| {
            let stats    = stats_cache.get(&p.entity_id);
            let class_id = classes.get(&p.entity_id).copied();
            core::discord::PlayerSummary {
                name:            p.player_name.clone(),
                class_name:      class_name(class_id),
                spec:            player_spec(p).map(|s| s.to_string()),
                total_damage:    p.total_damage,
                dps:             p.total_damage as f64 / duration,
                crit_pct:        p.crit_rate() * 100.0,
                crit_damage_pct: stats.and_then(|s| s.crit_damage).map(|v| v as f64 / 100.0),
                luck_pct:        stats.and_then(|s| s.luck_pct).map(|v| v as f64 / 100.0),
                damage_taken:    p.damage_taken,
                total_healing:   p.total_healing,
                heal_ps:         p.total_healing as f64 / duration,
                ability_score:   stats.and_then(|s| s.ability_score),
                season_strength: stats.and_then(|s| s.season_strength),
            }
        })
        .collect();
    players.sort_by(|a, b| b.total_damage.cmp(&a.total_damage));

    core::discord::DiscordReport {
        scene_name:      zone.to_string(),
        duration_secs:   duration,
        players,
        total_damage:    enc.total_damage,
        started_at_secs,
    }
}

// ── Format helpers ────────────────────────────────────────────────────────────

pub fn fmt_damage(dmg: u64) -> String {
    if dmg >= 1_000_000 {
        format!("{:.2}M", dmg as f64 / 1_000_000.0)
    } else if dmg >= 1_000 {
        format!("{:.1}K", dmg as f64 / 1_000.0)
    } else {
        format!("{dmg}")
    }
}

// ── UI Widget helpers ─────────────────────────────────────────────────────────

fn tab_btn(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = egui::RichText::new(label).size(11.5).color(
        if selected { ui::theme::TEXT } else { ui::theme::TEXT_MUTED }
    );
    let btn = egui::Button::new(text)
        .fill(if selected {
            egui::Color32::from_rgba_premultiplied(91, 140, 255, 25)
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::new(
            1.0,
            if selected { ui::theme::LINE_ACCENT } else { ui::theme::LINE },
        ))
        .rounding(6.0);
    ui.add(btn)
}

fn rank_tab_btn(ui: &mut egui::Ui, label: &str, selected: bool) -> egui::Response {
    let text = egui::RichText::new(label).size(11.0).color(
        if selected { ui::theme::TEXT } else { ui::theme::TEXT_MUTED }
    );
    let btn = egui::Button::new(text)
        .fill(if selected {
            egui::Color32::from_rgba_premultiplied(91, 140, 255, 20)
        } else {
            egui::Color32::TRANSPARENT
        })
        .stroke(egui::Stroke::new(1.0, if selected { ui::theme::LINE_ACCENT } else { ui::theme::LINE }))
        .rounding(6.0);
    ui.add(btn)
}

fn summary_tile(ui: &mut egui::Ui, width: f32, label: &str, value: &str, color: egui::Color32, highlight: bool) {
    let border = if highlight { ui::theme::LINE_ACCENT } else { ui::theme::LINE };
    egui::Frame::none()
        .fill(ui::theme::BG_PANEL)
        .stroke(egui::Stroke::new(1.0, border))
        .rounding(8.0)
        .inner_margin(egui::Margin::same(10.0))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.label(
                egui::RichText::new(label)
                    .size(9.5)
                    .color(ui::theme::TEXT_FAINT),
            );
            ui.label(
                egui::RichText::new(value)
                    .size(18.0)
                    .strong()
                    .color(color),
            );
        });
}

fn panel_card(ui: &mut egui::Ui, content: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::none()
        .fill(ui::theme::BG_PANEL)
        .stroke(egui::Stroke::new(1.0, ui::theme::LINE))
        .rounding(10.0)
        .inner_margin(egui::Margin::same(14.0))
        .show(ui, content);
}

fn player_row(
    ui: &mut egui::Ui,
    rank: usize,
    player: &PlayerMeter,
    rank_value: f64,
    rank_total: u64,
    bar_frac: f32,
    selected: bool,
    class_id: Option<u32>,
    stats: Option<&CharStats>,
    spec: Option<&'static str>,
    is_player: bool,
) -> egui::Response {
    let bg = if selected {
        egui::Color32::from_rgba_premultiplied(91, 140, 255, 15)
    } else {
        ui::theme::BG_INSET
    };
    let border = if selected { ui::theme::LINE_ACCENT } else { egui::Color32::TRANSPARENT };

    let gs_str = stats.and_then(|s| s.ability_score).map(|gs| {
        if let Some(ss) = stats.and_then(|s| s.season_strength) {
            if ss > 0 { format!("{gs}+{ss}") } else { gs.to_string() }
        } else { gs.to_string() }
    });
    let row_h = if gs_str.is_some() { 52.0 } else { 44.0 };

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), row_h),
        egui::Sense::click(),
    );

    let hovered = response.hovered();
    let draw_bg = if hovered && !selected {
        egui::Color32::from_rgba_premultiplied(255, 255, 255, 8)
    } else { bg };

    ui.painter().rect_filled(rect, 8.0, draw_bg);
    ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, border));

    // Rank number
    ui.painter().text(
        egui::pos2(rect.min.x + 14.0, rect.center().y),
        egui::Align2::CENTER_CENTER,
        rank.to_string(),
        egui::FontId::monospace(11.0),
        ui::theme::TEXT_FAINT,
    );

    // Name + class + GS
    let name_x = rect.min.x + 26.0;
    let (name_y, cls_y, gs_y) = if gs_str.is_some() {
        (rect.center().y - 14.0, rect.center().y, rect.center().y + 14.0)
    } else {
        (rect.center().y - 7.0, rect.center().y + 8.0, 0.0)
    };
    ui.painter().text(
        egui::pos2(name_x, name_y),
        egui::Align2::LEFT_CENTER,
        &player.player_name,
        egui::FontId::proportional(12.5),
        ui::theme::TEXT,
    );
    let cls_label = if is_player {
        if let Some(s) = spec {
            s.to_string()
        } else {
            class_name(class_id).to_string()
        }
    } else {
        "Monster".to_string()
    };
    let cls_color = if is_player { class_color_egui(class_id) } else { ui::theme::TEXT_MUTED };
    ui.painter().text(
        egui::pos2(name_x, cls_y),
        egui::Align2::LEFT_CENTER,
        cls_label,
        egui::FontId::proportional(9.5),
        cls_color,
    );
    if let Some(ref gs) = gs_str {
        ui.painter().text(
            egui::pos2(name_x, gs_y),
            egui::Align2::LEFT_CENTER,
            gs,
            egui::FontId::monospace(8.5),
            ui::theme::TEXT_FAINT,
        );
    }

    // Damage bar
    let bar_x = rect.min.x + 120.0;
    let bar_w = rect.width() - 120.0 - 84.0;
    let bar_rect = egui::Rect::from_min_size(
        egui::pos2(bar_x, rect.center().y - 5.0),
        egui::vec2(bar_w, 10.0),
    );
    ui.painter().rect_filled(bar_rect, 4.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 13));
    let fill_w = (bar_w * bar_frac).max(0.0);
    if fill_w > 0.0 {
        let fill = egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 10.0));
        let bar_color = if selected { ui::theme::ACCENT } else { egui::Color32::from_rgb(60, 100, 160) };
        ui.painter().rect_filled(fill, 4.0, bar_color);
    }

    // rank_value/s + total
    let right_x = rect.max.x - 8.0;
    ui.painter().text(
        egui::pos2(right_x, rect.center().y - 7.0),
        egui::Align2::RIGHT_CENTER,
        fmt_damage(rank_value as u64),
        egui::FontId::monospace(12.5),
        if selected { ui::theme::ACCENT } else { ui::theme::TEXT },
    );
    ui.painter().text(
        egui::pos2(right_x, rect.center().y + 8.0),
        egui::Align2::RIGHT_CENTER,
        fmt_damage(rank_total),
        egui::FontId::monospace(9.5),
        ui::theme::TEXT_FAINT,
    );

    response
}

fn skill_row(ui: &mut egui::Ui, name: &str, damage: u64, share: f64, hits: u64, crit_pct: f64) {
    // Truncate long skill names to prevent overflow
    let display_name = if name.len() > 26 {
        format!("{}…", &name[..24])
    } else {
        name.to_string()
    };
    ui.label(egui::RichText::new(&display_name).size(11.5).color(ui::theme::TEXT));
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(fmt_damage(damage)).size(11.0).color(ui::theme::TEXT_MUTED));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(egui::RichText::new(format!("{:.0}%", share * 100.0)).size(11.0).color(ui::theme::TEXT_MUTED));
        });
    });

    // Share bar
    let (bar_rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 5.0),
        egui::Sense::hover(),
    );
    ui.painter().rect_filled(bar_rect, 3.0, egui::Color32::from_rgba_premultiplied(255, 255, 255, 15));
    let fill_w = bar_rect.width() * share as f32;
    if fill_w > 0.0 {
        let fill = egui::Rect::from_min_size(bar_rect.min, egui::vec2(fill_w, 5.0));
        ui.painter().rect_filled(fill, 3.0, ui::theme::ACCENT);
    }

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(format!("{} hits", hits)).size(10.0).color(ui::theme::TEXT_FAINT));
        ui.add_space(10.0);
        ui.label(egui::RichText::new(format!("{:.1}% crit", crit_pct)).size(10.0).color(ui::theme::TEXT_FAINT));
    });
}

fn skill_spec(skill_id: u32) -> Option<&'static str> {
    match skill_id {
        1714 | 1734 => Some("Iaido"),
        1715 | 1740 | 1741 | 179906 => Some("Moonstrike"),
        120901 | 120902 => Some("Icicle"),
        1241 => Some("Frostbeam"),
        35107 | 35108 | 35109 | 160102 => Some("Formless"),
        1606 | 1621 | 1622 | 1623 => Some("Crimson"),
        1405 | 1418 => Some("Vanguard"),
        1419 => Some("Skyward"),
        1518 | 1541 | 21402 => Some("Smite"),
        20301 => Some("Lifebind"),
        199902 => Some("Earthfort"),
        1930 | 1931 | 1934 | 1935 => Some("Block"),
        2292 | 1700820 | 1700825 | 1700827 => Some("Wildpack"),
        220112 | 2203622 | 220106 => Some("Falconry"),
        2405 => Some("Recovery"),
        2406 => Some("Shield"),
        2321 | 2335 => Some("Dissonance"),
        2301 | 2336 | 2361 | 55302 => Some("Concerto"),
        _ => None,
    }
}

fn player_spec(player: &PlayerMeter) -> Option<&'static str> {
    let mut counts: HashMap<&'static str, u64> = HashMap::new();
    for (id, sk) in &player.skill_breakdown {
        if let Some(spec) = skill_spec(*id) {
            *counts.entry(spec).or_default() += sk.total_dmg;
        }
    }
    counts.into_iter().max_by_key(|(_, dmg)| *dmg).map(|(spec, _)| spec)
}

pub fn class_name(class_id: Option<u32>) -> &'static str {
    match class_id {
        Some(1)  => "Stormblade",
        Some(2)  => "Frost Mage",
        Some(3)  => "Twin Striker",
        Some(4)  => "Wind Knight",
        Some(5)  => "Verdant Oracle",
        Some(8)  => "Thunder Cannon",
        Some(9)  => "Heavy Guardian",
        Some(10) => "Dark Spirit",
        Some(11) => "Marksman",
        Some(12) => "Shield Knight",
        Some(13) => "Beat Performer",
        _        => "Unknown",
    }
}

fn class_color_egui(class_id: Option<u32>) -> egui::Color32 {
    match class_id {
        Some(1)  => egui::Color32::from_rgb( 90, 160, 255),
        Some(2)  => egui::Color32::from_rgb(100, 200, 255),
        Some(3)  => egui::Color32::from_rgb(255, 140,  70),
        Some(4)  => egui::Color32::from_rgb(130, 220, 130),
        Some(5)  => egui::Color32::from_rgb( 80, 200, 160),
        Some(8)  => egui::Color32::from_rgb(255, 220,  60),
        Some(9)  => egui::Color32::from_rgb(180, 100,  50),
        Some(10) => egui::Color32::from_rgb(160,  70, 200),
        Some(11) => egui::Color32::from_rgb(180, 220,  80),
        Some(12) => egui::Color32::from_rgb(220, 180,  60),
        Some(13) => egui::Color32::from_rgb(220,  80, 130),
        _        => ui::theme::TEXT_MUTED,
    }
}
