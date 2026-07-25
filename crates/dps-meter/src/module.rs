use crate::dps_state::DpsState;
use crate::encounter::{Encounter, EncounterOutcome};
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

/// Grace period before re-saving a just-finished encounter to disk, so that
/// gear/loadout attribute-sync packets arriving shortly after combat ends
/// (not guaranteed to be tied to combat timing) still make it into the saved
/// file. Heuristic, not a protocol guarantee.
const GEAR_RESAVE_DELAY: std::time::Duration = std::time::Duration::from_secs(3);

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
    monster_types:    HashMap<EntityId, i32>,
    is_player:        HashMap<EntityId, bool>,
    stats_cache:      HashMap<EntityId, CharStats>,
    dead_players:     std::collections::HashSet<EntityId>,
    shield_seen:      HashMap<EntityId, std::collections::HashSet<i64>>,
    gear_cache:       HashMap<EntityId, Vec<game::event::EquipSlot>>,
    loadout_cache:    HashMap<EntityId, Vec<game::event::SkillLoadoutEntry>>,
    gear_panel_state: ui::widgets::gear_panel::GearPanelState,
    // Phase-boundary detection (see plan Milestone 5 remaining sub-task).
    target_progress:      HashMap<i32, (i32, i32)>, // target_id -> (nums, complete)
    current_phase_target: Option<i32>,
    last_phase_id:        Option<i32>,
    pending:          Vec<GameEvent>,
    selected_enc:     Option<usize>,
    selected_player:  Option<EntityId>,
    pinned_player:    Option<EntityId>,
    dps_window_secs:  f64,
    current_zone:     String,
    current_zone_id:  Option<u32>,
    store:            EncounterStore,
    rank_mode:        u8,  // 0=DPS 1=Taken 2=Healing
    players_only:     bool,
    pending_gear_resave: Vec<(Instant, Encounter)>,

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
            monster_types:   HashMap::new(),
            is_player:       HashMap::new(),
            stats_cache:     HashMap::new(),
            dead_players:    std::collections::HashSet::new(),
            shield_seen:     HashMap::new(),
            gear_cache:      HashMap::new(),
            loadout_cache:   HashMap::new(),
            gear_panel_state: ui::widgets::gear_panel::GearPanelState::default(),
            target_progress:      HashMap::new(),
            current_phase_target: None,
            last_phase_id:        None,
            pending:         Vec::new(),
            selected_enc:    None,
            selected_player: None,
            pinned_player:   None,
            dps_window_secs: 3.0,
            current_zone:    String::new(),
            current_zone_id: None,
            store:           EncounterStore::open(),
            rank_mode:       0,
            players_only:    false,
            pending_gear_resave: Vec::new(),
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
        self.state.active = Some(Encounter::new(self.current_zone.clone(), self.current_zone_id.unwrap_or(0)));
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
                hit_count  += p.hit_count();
                crit_count += p.crit_count();
                for (id, sk) in &p.skill_breakdown {
                    let e = all_skills.entry(*id).or_insert_with(|| SkillStat {
                        skill_id: *id, ..Default::default()
                    });
                    e.stats.total += sk.stats.total;
                    e.stats.hits  += sk.stats.hits;
                    e.stats.crit_hits += sk.stats.crit_hits;
                    e.stats.crit_lucky_hits += sk.stats.crit_lucky_hits;
                    if sk.stats.max_hit > e.stats.max_hit { e.stats.max_hit = sk.stats.max_hit; }
                }
            }
            let mut skills: Vec<SkillStat> = all_skills.into_values().collect();
            skills.sort_by(|a, b| b.stats.total.cmp(&a.stats.total));

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

    /// If every known player participating in the active encounter is currently
    /// dead, tag it Failed and finish it (auto-stop on party wipe).
    fn check_wipe(&mut self) {
        let party_all_dead = self.state.active.as_ref().is_some_and(|enc| {
            let mut party = enc.players.keys()
                .filter(|id| self.is_player.get(id).copied().unwrap_or(false))
                .peekable();
            party.peek().is_some() && party.all(|id| self.dead_players.contains(id))
        });
        if party_all_dead {
            if let Some(enc) = &mut self.state.active {
                enc.outcome = EncounterOutcome::Failed;
            }
            self.state.finish_active();
        }
    }

    /// Zero-stat party preview shown before combat starts (e.g. right after
    /// dungeon entry, once names/loadout/gear have arrived but no damage has
    /// happened yet) — same row rendering as a live encounter, just frozen
    /// at 0 instead of "No active encounter" until the first real hit.
    fn preview_party_players(&self) -> Option<Vec<PlayerMeter>> {
        let mut players: Vec<PlayerMeter> = self.is_player.iter()
            .filter(|&(_, &is_p)| is_p)
            .filter_map(|(id, _)| Some(PlayerMeter::new(*id, self.names.get(id)?.clone())))
            .collect();
        if players.is_empty() { return None; }
        players.sort_by(|a, b| a.player_name.cmp(&b.player_name));
        Some(players)
    }

    fn save_encounter(&self, enc: &Encounter) {
        let saved = to_saved_encounter(
            enc, &self.classes, &self.monster_types, &self.stats_cache, &self.is_player, &self.gear_cache, &self.loadout_cache,
        );
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
    fn icon(&self) -> &str         { egui_phosphor::regular::CHART_BAR }

    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }

    fn update(&mut self, ctx: &ModuleContext) {
        self.dps_window_secs = ctx.config.dps_window_secs as f64;
        self.state.tick();

        let events: Vec<GameEvent> = self.pending.drain(..).collect();

        for event in events {
            match &event {
                GameEvent::EntityName { id, name, class, monster_type, is_player } => {
                    if !name.is_empty() {
                        self.names.insert(*id, name.clone());
                    }
                    if let Some(c) = class {
                        self.classes.insert(*id, *c);
                    }
                    if let Some(mt) = monster_type {
                        self.monster_types.insert(*id, *mt);
                    }
                    self.is_player.insert(*id, *is_player);
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
                    self.is_player.remove(id);
                    self.stats_cache.remove(id);
                    self.dead_players.remove(id);
                    self.shield_seen.remove(id);
                    // monster_types (like gear_cache/loadout_cache below) is deliberately NOT
                    // cleared here: a monster's config/template id is only sent once, on the
                    // initial appear/full-sync attrs — never resent on incremental deltas
                    // (confirmed via capture). Clearing it on despawn permanently loses the
                    // tier (Boss/Elite/Normal) for any monster that leaves AOI render range
                    // and comes back (roaming mobs, or simply re-entering combat) — it can
                    // never be re-resolved since the identity attr won't arrive again.
                    // gear_cache/loadout_cache are deliberately NOT cleared here: they're
                    // identity data (what someone is wearing), not live-presence data, and
                    // don't change just because the entity left AOI render range. Other
                    // players routinely despawn/respawn while still in your party — dropping
                    // their gear here is why it only ever "worked" for the local player, who
                    // never despawns. Cleared on zone change instead (session boundary).
                }
                GameEvent::EntityStats { id, stats } => {
                    let prev_state = self.stats_cache.get(id).and_then(|s| s.actor_state);
                    self.stats_cache.entry(*id).or_default().merge(stats);
                    let new_state = self.stats_cache.get(id).and_then(|s| s.actor_state);
                    if let Some(new_state) = new_state {
                        let was_dead = prev_state == Some(game::entity::ACTOR_STATE_DEAD);
                        let now_dead = new_state == game::entity::ACTOR_STATE_DEAD;
                        if now_dead && !was_dead {
                            self.dead_players.insert(*id);
                            if self.is_player.get(id).copied().unwrap_or(false) {
                                let names = &self.names;
                                if let Some(enc) = &mut self.state.active {
                                    enc.record_death(*id, |eid| {
                                        names.get(&eid).cloned()
                                            .unwrap_or_else(|| format!("Entity {:x}", eid.0))
                                    });
                                }
                                self.check_wipe();
                            }
                        } else if !now_dead && was_dead {
                            self.dead_players.remove(id);
                        }
                    }
                }
                GameEvent::ShieldList { id, shields } => {
                    let seen = self.shield_seen.entry(*id).or_default();
                    let mut new_gain = 0u64;
                    for s in shields {
                        if seen.insert(s.uuid) {
                            new_gain += s.initial_value.max(0) as u64;
                        }
                    }
                    if new_gain > 0 {
                        let names = &self.names;
                        if let Some(enc) = &mut self.state.active {
                            enc.apply_shield_gain(*id, new_gain, |eid| {
                                names.get(&eid).cloned()
                                    .unwrap_or_else(|| format!("Entity {:x}", eid.0))
                            });
                        }
                    }
                }
                GameEvent::EquipData { id, slots } => {
                    tracing::debug!("gear snapshot id={:?} gear_slots={}", id, slots.len());
                    self.gear_cache.insert(*id, slots.clone());
                }
                GameEvent::SkillLoadout { id, skills } => {
                    tracing::debug!(
                        "loadout snapshot id={:?} skill_ids={:?}",
                        id,
                        skills.iter().map(|s| s.skill_id).collect::<Vec<_>>()
                    );
                    self.loadout_cache.insert(*id, skills.clone());
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
                        // New zone/instance — stale objective progress from the
                        // previous dungeon must not leak into phase detection here.
                        self.target_progress.clear();
                        self.current_phase_target = None;
                        self.last_phase_id = None;
                        // Gear/loadout/monster-type are session-scoped identity data (see
                        // EntityDespawn above for why they survive individual despawns) —
                        // reset at the actual session/instance boundary instead.
                        self.gear_cache.clear();
                        self.loadout_cache.clear();
                        self.monster_types.clear();
                    }
                }
                GameEvent::DungeonState { .. } => {
                    self.state.apply_state_event(&event);
                }
                GameEvent::DungeonPhaseSignal { targets, phase_id } => {
                    // phase_id only ever comes from the full sync (0x17, enter/exit);
                    // the mid-run dirty channel (0x18) never carries it, so `None` here
                    // just means "this update didn't say" — not a real transition.
                    if phase_id.is_some() && *phase_id != self.last_phase_id {
                        tracing::info!("dungeon phase_id changed: {:?} -> {:?} (unverified signal, logged for capture analysis)", self.last_phase_id, phase_id);
                        self.last_phase_id = *phase_id;
                    }
                    for t in targets {
                        tracing::debug!("dungeon target {} nums={} complete={}", t.target_id, t.nums, t.complete);
                        let is_new_objective = t.complete == 0 && t.nums == 0;
                        if is_new_objective
                            && self.current_phase_target.is_some()
                            && self.current_phase_target != Some(t.target_id)
                        {
                            if let Some(enc) = &mut self.state.active {
                                let phase_name = format!("Phase {}", enc.phases.len() + 2);
                                enc.push_phase(phase_name);
                            }
                            self.current_phase_target = Some(t.target_id);
                        } else if self.current_phase_target.is_none() {
                            self.current_phase_target = Some(t.target_id);
                        }
                        self.target_progress.insert(t.target_id, (t.nums, t.complete));
                    }
                }
                GameEvent::Heal(h) => {
                    if self.names.contains_key(&h.source_id) {
                        // Overheal: target's cached HP already reflects this same delta's
                        // attribute update (attrs are applied before skill_effects per-delta),
                        // matching ZDPS's live-read-at-heal-time semantics.
                        let overheal = self.stats_cache.get(&h.target_id).and_then(|s| {
                            match (s.hp, s.max_hp) {
                                (Some(hp), Some(max))
                                    if max > 0 && hp >= 0 && hp <= max && hp + h.damage as i64 > max =>
                                {
                                    let effective = (max - hp).max(0) as u64;
                                    Some(h.damage.saturating_sub(effective))
                                }
                                _ => None,
                            }
                        }).unwrap_or(0);
                        let names = &self.names;
                        if let Some(enc) = &mut self.state.active {
                            enc.apply_heal(h, overheal, |id| {
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
                    self.state.apply_event(&event, &self.current_zone, self.current_zone_id.unwrap_or(0), |id| {
                        names.get(&id).cloned()
                            .unwrap_or_else(|| format!("Entity {:x}", id.0))
                    });
                    // Track damage taken for known players
                    if names.contains_key(&c.target_id) {
                        if let Some(enc) = &mut self.state.active {
                            enc.apply_taken(c);
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

        // Re-save any recently finished encounters once their gear-resave grace
        // period has elapsed, picking up gear/loadout attrs that arrived after
        // the fight ended (see GEAR_RESAVE_DELAY). The duplicate-History-row
        // bug this was suspected of causing was actually the End/Settlement/
        // Vote phantom-encounter bug in dps_state.rs (fixed separately) —
        // this reuses the same Encounter::id on resave, so it overwrites the
        // same file rather than creating a new entry.
        let now = Instant::now();
        let mut due_resaves = Vec::new();
        self.pending_gear_resave.retain(|(t, enc)| {
            if *t <= now { due_resaves.push(enc.clone()); false } else { true }
        });
        for enc in &due_resaves {
            self.save_encounter(enc);
        }

        // Persist any newly finished encounters; fire discord if configured
        let finished: Vec<_> = self.state.newly_finished.drain(..).collect();
        for enc in &finished {
            self.save_encounter(enc);
            self.pending_gear_resave.push((now + GEAR_RESAVE_DELAY, enc.clone()));
            let player_count = enc.players.keys()
                .filter(|id| self.is_player.get(id).copied().unwrap_or(false))
                .count();
            if ctx.config.discord_enabled
                && !ctx.config.discord_webhook_url.is_empty()
                && player_count >= ctx.config.discord_min_players
            {
                let report = to_discord_report(enc, &self.stats_cache, &self.classes, &self.is_player);
                core::discord::send_report_async(ctx.config.discord_webhook_url.clone(), report);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _egui_ctx: &egui::Context) {
        let enc_data = match self.selected_enc {
            None => self.state.active.as_ref().map(|enc| {
                let elapsed   = enc.elapsed().as_secs_f64();
                let players: Vec<_> = enc.players_by_damage().into_iter().cloned().collect();
                (elapsed, players, enc.total_damage)
            }).or_else(|| self.preview_party_players().map(|players| (0.0, players, 0u64))),
            Some(i) => self.state.past.get(i).map(|enc| {
                let elapsed   = enc.elapsed().as_secs_f64();
                let players: Vec<_> = enc.players_by_damage().into_iter().cloned().collect();
                (elapsed, players, enc.total_damage)
            }),
        };

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
                    format!("{} {}s", egui_phosphor::regular::CROSSHAIR, rem)
                } else {
                    format!("{} Bench", egui_phosphor::regular::CROSSHAIR)
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

        // Captured before the content scroll area below — inside a scroll
        // area's content ui, available_height() reports an unbounded
        // "content" size (so it can grow past the viewport), not the actual
        // window height, so this is the only point that gives a real number.
        let content_avail_h = ui.available_height();

        match enc_data {
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new("No active encounter. Start combat to begin tracking.")
                            .color(ui::theme::TEXT_MUTED),
                    );
                });
            }
            Some((elapsed, players, _total_dmg)) => {
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
                let is_player_map = &self.is_player;
                let player_total_dmg: u64 = players.iter()
                    .filter(|p| is_player_map.get(&p.entity_id).copied().unwrap_or(false))
                    .map(|p| p.total_damage()).sum();
                let player_total_taken: u64 = players.iter()
                    .filter(|p| is_player_map.get(&p.entity_id).copied().unwrap_or(false))
                    .map(|p| p.damage_taken()).sum();
                let player_total_healed: u64 = players.iter()
                    .filter(|p| is_player_map.get(&p.entity_id).copied().unwrap_or(false))
                    .map(|p| p.total_healing()).sum();
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

                // Party list height: in two-column layout it stretches to fill
                // the window (budget = height already available before this
                // point, minus the chrome rendered above/around the list —
                // header row, summary tiles, spacing, and the panel's own
                // title/tabs/checkbox row, all fixed-size regardless of
                // player count); in one-column layout it's the old fixed
                // height plus one extra player row instead of a full rescale,
                // since the list stacks above the skill panel rather than
                // being the whole column.
                const PARTY_ROW_H: f32 = 52.0;
                const PARTY_LIST_CHROME_H: f32 = 235.0;
                const PARTY_LIST_MIN_H: f32 = 160.0;
                let party_list_h = if use_columns {
                    (content_avail_h - PARTY_LIST_CHROME_H).max(PARTY_LIST_MIN_H)
                } else {
                    200.0 + PARTY_ROW_H
                };
                // Row content (rank/name/bar/numbers) needs a minimum width to
                // stay readable — below that, scroll horizontally instead of
                // squeezing everything together.
                const PARTY_ROW_MIN_W: f32 = 320.0;
                // panel_card's Frame adds 14px inner margin per side, which
                // eats into left_w before rows actually render inside it.
                const PANEL_CARD_MARGIN: f32 = 28.0;
                let party_row_w = (left_w - PANEL_CARD_MARGIN).max(PARTY_ROW_MIN_W);

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
                            ui.add_space(12.0);
                            ui.checkbox(&mut self.players_only, "Players Only");
                        });
                        ui.add_space(8.0);
                        // Sort by selected mode
                        let mut ranked: Vec<_> = players.iter()
                            .filter(|p| !self.players_only || self.is_player.get(&p.entity_id).copied().unwrap_or(false))
                            .collect();
                        match self.rank_mode {
                            1 => ranked.sort_by(|a, b| b.damage_taken().cmp(&a.damage_taken())),
                            2 => ranked.sort_by(|a, b| b.total_healing().cmp(&a.total_healing())),
                            _ => ranked.sort_by(|a, b| b.total_damage().cmp(&a.total_damage())),
                        }
                        let max_rank_val = match self.rank_mode {
                            1 => ranked.first().map(|p| p.damage_taken()).unwrap_or(1).max(1),
                            2 => ranked.first().map(|p| p.total_healing()).unwrap_or(1).max(1),
                            _ => ranked.first().map(|p| p.total_damage()).unwrap_or(1).max(1),
                        };
                        egui::ScrollArea::both()
                            .id_salt("party_scroll")
                            .max_height(party_list_h)
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                            .show(ui, |ui| {
                                ui.set_min_width(party_row_w);
                                ui.style_mut().interaction.selectable_labels = false;
                                for (rank, player) in ranked.iter().enumerate() {
                                    let (rank_value, rank_total) = match self.rank_mode {
                                        1 => (
                                            if elapsed > 0.0 { player.damage_taken() as f64 / elapsed } else { 0.0 },
                                            player.damage_taken(),
                                        ),
                                        2 => (
                                            if elapsed > 0.0 { player.total_healing() as f64 / elapsed } else { 0.0 },
                                            player.total_healing(),
                                        ),
                                        _ => (player.avg_dps(elapsed), player.total_damage()),
                                    };
                                    let bar_frac = (rank_total as f32 / max_rank_val as f32).min(1.0);
                                    let is_sel   = self.selected_player == Some(player.entity_id);
                                    let spec     = player_spec(player);
                                    let class_id = self.classes.get(&player.entity_id).copied().or_else(|| spec.and_then(spec_to_class_id));
                                    let stats    = self.stats_cache.get(&player.entity_id);
                                    let is_player = self.is_player.get(&player.entity_id).copied().unwrap_or(false);
                                    let monster_type = self.monster_types.get(&player.entity_id).copied();
                                    let resp = player_row(ui, party_row_w, rank + 1, player, rank_value, rank_total, bar_frac, is_sel, class_id, stats, spec, is_player, monster_type);
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
                                let spec      = player_spec(player);
                                let class_id  = self.classes.get(&sel_id).copied().or_else(|| spec.and_then(spec_to_class_id));
                                let spec_str  = spec.map(|s| format!(" · {s}")).unwrap_or_default();
                                ui.label(
                                    egui::RichText::new(format!("{} · {}{}", player.player_name, class_name(class_id), spec_str))
                                        .size(11.0).color(ui::theme::ACCENT),
                                );
                                ui.add_space(8.0);
                                let total = player.total_damage().max(1);
                                let mut skills: Vec<_> = player.skill_breakdown.values().collect();
                                skills.sort_by(|a, b| b.stats.total.cmp(&a.stats.total));
                                egui::ScrollArea::vertical()
                                    .id_salt(format!("skill_scroll_{}", sel_id.0))
                                    .min_scrolled_height(220.0)
                                    .max_height(400.0)
                                    .show(ui, |ui| {
                                        for sk in &skills {
                                            let share    = sk.stats.total as f64 / total as f64;
                                            let sk_crits = sk.stats.crit_hits + sk.stats.crit_lucky_hits;
                                            let sk_crit  = if sk.stats.hits > 0 { sk_crits as f64 / sk.stats.hits as f64 * 100.0 } else { 0.0 };
                                            let sk_label = core::DATA.skill_name(sk.skill_id)
                                                .map(|s| s.to_string())
                                                .unwrap_or_else(|| format!("#{}", sk.skill_id));
                                            skill_row(ui, &sk_label, sk.stats.total, share, sk.stats.hits, sk_crit);
                                            ui.add_space(4.0);
                                        }
                                    });
                                ui.add_space(10.0);
                                let stats   = self.stats_cache.get(&sel_id);
                                let gear    = self.gear_cache.get(&sel_id);
                                let loadout = self.loadout_cache.get(&sel_id);
                                let pinned  = &mut self.pinned_player;
                                let panel_state = &mut self.gear_panel_state;
                                render_inspector(ui, player, stats, elapsed, sel_id, pinned, gear, loadout, panel_state);
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
                    egui::RichText::new(format!("{} Start Benchmark", egui_phosphor::regular::PLAY)).color(egui::Color32::from_rgb(100, 220, 120))
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
                            let sk_crits = sk.stats.crit_hits + sk.stats.crit_lucky_hits;
                            let sk_crit = if sk.stats.hits > 0 { sk_crits as f64 / sk.stats.hits as f64 * 100.0 } else { 0.0 };
                            let share   = sk.stats.total as f64 / total as f64 * 100.0;
                            let skill_label = core::DATA.skill_name(sk.skill_id)
                                .map(|s| s.to_string())
                                .unwrap_or_else(|| format!("#{}", sk.skill_id));
                            ui.label(egui::RichText::new(&skill_label).small());
                            ui.label(egui::RichText::new(fmt_damage(sk.stats.total)).small());
                            ui.label(egui::RichText::new(sk.stats.hits.to_string()).small());
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
    ui:          &mut egui::Ui,
    player:      &PlayerMeter,
    stats:       Option<&CharStats>,
    duration:    f64,
    id:          EntityId,
    pinned:      &mut Option<EntityId>,
    gear:        Option<&Vec<game::event::EquipSlot>>,
    loadout:     Option<&Vec<game::event::SkillLoadoutEntry>>,
    panel_state: &mut ui::widgets::gear_panel::GearPanelState,
) {
    let is_pinned = *pinned == Some(id);

    ui.horizontal(|ui| {
        ui.strong(&player.player_name);
        let pin_label = if is_pinned { format!("{} Unpin", egui_phosphor::regular::PUSH_PIN) } else { format!("{} Pin", egui_phosphor::regular::PUSH_PIN) };
        if ui.small_button(pin_label).on_hover_text("Pin — keep inspector across encounters").clicked() {
            *pinned = if is_pinned { None } else { Some(id) };
        }
    });

    // ── Combat summary ───────────────────────────────────────────────────────
    let dps       = if duration > 0.0 { player.total_damage() as f64 / duration } else { 0.0 };
    let heal_ps   = if duration > 0.0 { player.total_healing() as f64 / duration } else { 0.0 };
    let crit_pct  = player.crit_rate() * 100.0;
    let lucky_pct = player.lucky_rate() * 100.0;

    egui::Grid::new("inspector_combat")
        .num_columns(4)
        .striped(false)
        .show(ui, |ui| {
            ui.small("Total DMG"); ui.small("DPS");        ui.small("Hits");     ui.small("Crit%");
            ui.end_row();
            ui.label(fmt_damage(player.total_damage()));
            ui.label(format!("{:.0}", dps));
            ui.label(player.hit_count().to_string());
            ui.label(format!("{:.1}%", crit_pct));
            ui.end_row();

            ui.small("DMG Taken"); ui.small("Total Heal"); ui.small("Heal/s");   ui.small("Lucky%");
            ui.end_row();
            ui.label(fmt_damage(player.damage_taken()));
            ui.label(fmt_damage(player.total_healing()));
            ui.label(fmt_damage(heal_ps as u64));
            ui.label(format!("{:.1}%", lucky_pct));
            ui.end_row();

            ui.small("Deaths"); ui.small("Shield"); ui.small("Overheal"); ui.small("");
            ui.end_row();
            ui.label(player.deaths.to_string());
            ui.label(fmt_damage(player.shield_gain));
            ui.label(fmt_damage(player.overheal));
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

    // ── Imagines & Gear ──────────────────────────────────────────────────────
    if gear.is_some() || loadout.is_some() {
        ui.add_space(6.0);
        let (slots, imagines) = to_gear_panel_views(gear, loadout);

        ui.label(egui::RichText::new("IMAGINES").strong().size(11.0).color(ui::theme::TEXT));
        ui.add_space(4.0);
        ui::widgets::gear_panel::render_imagines(ui, panel_state, &imagines);

        ui.add_space(8.0);
        ui.label(egui::RichText::new("GEAR").strong().size(11.0).color(ui::theme::TEXT));
        ui.add_space(4.0);
        ui::widgets::gear_panel::render_gear(ui, panel_state, &slots);
    }
}

// ── Conversion helpers ────────────────────────────────────────────────────────

fn to_saved_encounter(
    enc: &Encounter,
    classes: &HashMap<EntityId, u32>,
    monster_types: &HashMap<EntityId, i32>,
    stats_cache: &HashMap<EntityId, CharStats>,
    is_player: &HashMap<EntityId, bool>,
    gear_cache: &HashMap<EntityId, Vec<game::event::EquipSlot>>,
    loadout_cache: &HashMap<EntityId, Vec<game::event::SkillLoadoutEntry>>,
) -> SavedEncounter {
    let duration = enc.elapsed();
    let now_utc  = chrono::Utc::now();
    let age      = std::time::Instant::now().duration_since(enc.start_time);
    let started_at = now_utc - chrono::Duration::from_std(age).unwrap_or_default();
    let ended_at   = started_at + chrono::Duration::from_std(duration).unwrap_or_default();
    let total_elapsed = duration.as_secs_f64();

    let players = enc.players.values()
        .map(|p| {
            let gear_info = build_gear_info(gear_cache.get(&p.entity_id), loadout_cache.get(&p.entity_id));
            to_saved_player(
                p,
                classes,
                monster_types,
                stats_cache.get(&p.entity_id),
                is_player.get(&p.entity_id).copied().unwrap_or(false),
                gear_info.as_ref(),
                &enc.phases,
                total_elapsed,
            )
        })
        .collect();

    SavedEncounter {
        id:           enc.id,
        scene_name:   enc.zone_name.clone(),
        started_at,
        ended_at,
        duration_secs: total_elapsed,
        players,
        total_damage:  enc.total_damage,
        custom_name:  None,
        outcome:      enc.outcome,
        phases:       enc.phases.iter()
            .map(|m| encounter_store::SavedPhaseMarker {
                name: m.name.clone(),
                start_offset_secs: m.start_offset_secs,
            })
            .collect(),
    }
}

fn build_gear_info(
    slots:  Option<&Vec<game::event::EquipSlot>>,
    skills: Option<&Vec<game::event::SkillLoadoutEntry>>,
) -> Option<encounter_store::SavedGearInfo> {
    if slots.is_none() && skills.is_none() { return None; }
    let gear = slots.map(|v| v.iter().map(|s| encounter_store::SavedGearSlot {
        slot:      s.slot,
        item_id:   s.item_id,
        item_name: core::DATA.item_name(s.item_id).map(|n| n.to_string()).unwrap_or_default(),
    }).collect()).unwrap_or_default();
    let imagines = skills.map(|v| v.iter()
        .filter(|s| core::DATA.skill_is_imagine(s.skill_id as u32))
        .map(|s| encounter_store::SavedImagineEntry {
            skill_id: s.skill_id,
            tier:     s.tier,
            name:     core::DATA.skill_name(s.skill_id as u32).map(|n| n.to_string()).unwrap_or_default(),
        }).collect()).unwrap_or_default();
    Some(encounter_store::SavedGearInfo { gear, imagines })
}

fn to_gear_panel_views(
    slots:  Option<&Vec<game::event::EquipSlot>>,
    skills: Option<&Vec<game::event::SkillLoadoutEntry>>,
) -> (Vec<ui::widgets::gear_panel::GearSlotView>, Vec<ui::widgets::gear_panel::ImagineView>) {
    let gear = slots.map(|v| v.iter().map(|s| ui::widgets::gear_panel::GearSlotView {
        slot:      s.slot,
        item_id:   s.item_id,
        item_name: core::DATA.item_name(s.item_id).map(|n| n.to_string()).unwrap_or_default(),
    }).collect()).unwrap_or_default();
    let imagines = skills.map(|v| v.iter()
        .filter(|s| core::DATA.skill_is_imagine(s.skill_id as u32))
        .map(|s| ui::widgets::gear_panel::ImagineView {
            skill_id: s.skill_id,
            tier:     s.tier,
            name:     core::DATA.skill_name(s.skill_id as u32).map(|n| n.to_string()).unwrap_or_default(),
        }).collect()).unwrap_or_default();
    (gear, imagines)
}

/// Splits a player's timestamped hit log into per-phase stat buckets (exact
/// per-skill breakdown per phase, derived fresh from the log — not an estimate).
fn bucket_player_phases(
    p: &PlayerMeter,
    phases: &[crate::encounter::PhaseMarker],
    total_elapsed: f64,
) -> Vec<encounter_store::SavedPlayerPhaseStats> {
    if phases.is_empty() { return Vec::new(); }

    // Bucket 0 is the implicit "Phase 1" — everything before the first
    // recorded marker (markers only get pushed on a detected transition, so
    // the run always starts in an unmarked phase). Recorded markers
    // (phases[i]) become bucket i+1, matching push_phase's "Phase {len+2}"
    // naming (len at push time == i, so name == i+2 == bucket index + 1).
    let mut ranges: Vec<(f64, f64)> = vec![(0.0, phases[0].start_offset_secs)];
    ranges.extend(phases.iter().enumerate().map(|(i, m)| {
        let end = phases.get(i + 1).map(|n| n.start_offset_secs).unwrap_or(total_elapsed);
        (m.start_offset_secs, end)
    }));

    let mut buckets: Vec<encounter_store::SavedPlayerPhaseStats> = (0..ranges.len())
        .map(|i| encounter_store::SavedPlayerPhaseStats { phase_index: i, ..Default::default() })
        .collect();
    let mut skill_maps: Vec<HashMap<u32, SavedSkillStat>> = (0..ranges.len()).map(|_| HashMap::new()).collect();

    for hit in &p.hit_log {
        let Some(idx) = ranges.iter().position(|(s, e)| hit.time_secs >= *s && (hit.time_secs < *e || *e >= total_elapsed))
        else { continue };
        let bucket = &mut buckets[idx];
        match hit.kind {
            crate::meter::HitKind::Damage => {
                bucket.total_damage += hit.amount;
                bucket.hits += 1;
                let skill = skill_maps[idx].entry(hit.skill_id).or_insert_with(|| SavedSkillStat {
                    skill_id: hit.skill_id, total_dmg: 0, hits: 0, crits: 0, max_hit: 0,
                });
                skill.total_dmg += hit.amount;
                skill.hits += 1;
                if hit.is_crit || hit.is_lucky { skill.crits += 1; }
                if hit.amount > skill.max_hit { skill.max_hit = hit.amount; }
            }
            crate::meter::HitKind::Taken => bucket.damage_taken += hit.amount,
            crate::meter::HitKind::Heal  => bucket.total_healing += hit.amount,
        }
    }

    for (i, bucket) in buckets.iter_mut().enumerate() {
        let mut skills: Vec<_> = skill_maps[i].values().cloned().collect();
        skills.sort_by(|a, b| b.total_dmg.cmp(&a.total_dmg));
        bucket.skills = skills;
    }
    buckets
}

fn to_saved_player(
    p: &PlayerMeter,
    classes: &HashMap<EntityId, u32>,
    monster_types: &HashMap<EntityId, i32>,
    stats: Option<&CharStats>,
    is_player: bool,
    gear: Option<&encounter_store::SavedGearInfo>,
    phases: &[crate::encounter::PhaseMarker],
    total_elapsed: f64,
) -> SavedPlayerMeter {
    let skills = p.skill_breakdown.values().map(|s| SavedSkillStat {
        skill_id:  s.skill_id,
        total_dmg: s.stats.total,
        hits:      s.stats.hits,
        crits:     s.stats.crit_hits + s.stats.crit_lucky_hits,
        max_hit:   s.stats.max_hit,
    }).collect();

    let phase_stats = bucket_player_phases(p, phases, total_elapsed);
    let spec = player_spec(p);

    SavedPlayerMeter {
        entity_id:      p.entity_id.0,
        name:           p.player_name.clone(),
        class_id:       classes.get(&p.entity_id).copied().or_else(|| spec.and_then(spec_to_class_id)),
        monster_type:   monster_types.get(&p.entity_id).copied(),
        total_damage:   p.total_damage(),
        hit_count:      p.hit_count(),
        crit_count:     p.crit_count(),
        skills,
        damage_taken:   p.damage_taken(),
        total_healing:  p.total_healing(),
        ability_score:  stats.and_then(|s| s.ability_score),
        season_strength: stats.and_then(|s| s.season_strength),
        crit_pct:       stats.and_then(|s| s.crit_pct),
        luck_pct:       stats.and_then(|s| s.luck_pct),
        crit_damage:    stats.and_then(|s| s.crit_damage),
        spec:           spec.map(|s| s.to_string()),
        damage_lucky_hits:       p.damage_stats.lucky_hits,
        damage_crit_lucky_hits:  p.damage_stats.crit_lucky_hits,
        damage_crit_total:       p.damage_stats.crit_total,
        damage_lucky_total:      p.damage_stats.lucky_total,
        damage_crit_lucky_total: p.damage_stats.crit_lucky_total,
        max_hit:                 p.damage_stats.max_hit,
        max_dps:                 p.max_dps(),
        heal_hits:               p.heal_stats.hits,
        heal_crit_hits:          p.heal_stats.crit_hits,
        heal_lucky_hits:         p.heal_stats.lucky_hits,
        heal_crit_lucky_hits:    p.heal_stats.crit_lucky_hits,
        heal_crit_total:         p.heal_stats.crit_total,
        heal_lucky_total:        p.heal_stats.lucky_total,
        heal_crit_lucky_total:   p.heal_stats.crit_lucky_total,
        heal_max_hit:            p.heal_stats.max_hit,
        overheal:                p.overheal,
        shield_gain:             p.shield_gain,
        deaths:                  p.deaths,
        max_hp:                  stats.and_then(|s| s.max_hp),
        is_player,
        gear:                    gear.cloned(),
        phase_stats,
    }
}

// ── Discord report builder ────────────────────────────────────────────────────

fn to_discord_report(enc: &Encounter, stats_cache: &HashMap<EntityId, CharStats>, classes: &HashMap<EntityId, u32>, is_player: &HashMap<EntityId, bool>) -> core::discord::DiscordReport {
    let age = Instant::now().duration_since(enc.start_time);
    let started_at_secs = chrono::Utc::now().timestamp() - age.as_secs() as i64;
    let duration = enc.elapsed().as_secs_f64().max(1.0);

    let mut players: Vec<_> = enc.players.values()
        .filter(|p| is_player.get(&p.entity_id).copied().unwrap_or(false))
        .map(|p| {
            let stats    = stats_cache.get(&p.entity_id);
            let spec     = player_spec(p);
            let class_id = classes.get(&p.entity_id).copied().or_else(|| spec.and_then(spec_to_class_id));
            core::discord::PlayerSummary {
                name:            p.player_name.clone(),
                class_name:      class_name(class_id),
                spec:            spec.map(|s| s.to_string()),
                total_damage:    p.total_damage(),
                dps:             p.total_damage() as f64 / duration,
                crit_pct:        p.crit_rate() * 100.0,
                crit_damage_pct: stats.and_then(|s| s.crit_damage).map(|v| v as f64 / 100.0),
                luck_pct:        stats.and_then(|s| s.luck_pct).map(|v| v as f64 / 100.0),
                damage_taken:    p.damage_taken(),
                total_healing:   p.total_healing(),
                heal_ps:         p.total_healing() as f64 / duration,
                ability_score:   stats.and_then(|s| s.ability_score),
                season_strength: stats.and_then(|s| s.season_strength),
            }
        })
        .collect();
    players.sort_by(|a, b| b.total_damage.cmp(&a.total_damage));

    core::discord::DiscordReport {
        scene_name:      enc.zone_name.clone(),
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
    row_w: f32,
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
    monster_type: Option<i32>,
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
        egui::vec2(row_w, row_h),
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
        monster_type_name(monster_type).to_string()
    };
    let cls_color = if is_player { class_color_egui(class_id) } else { monster_type_color_egui(monster_type) };
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

/// Fallback class lookup when class_id was never observed via attr-sync but
/// spec was already derived from combat damage (spec implies class 1:1).
fn spec_to_class_id(spec: &str) -> Option<u32> {
    match spec {
        "Iaido" | "Moonstrike" => Some(1),
        "Icicle" | "Frostbeam" => Some(2),
        "Formless" | "Crimson" => Some(3),
        "Vanguard" | "Skyward" => Some(4),
        "Smite" | "Lifebind" => Some(5),
        "Earthfort" | "Block" => Some(9),
        "Wildpack" | "Falconry" => Some(11),
        "Recovery" | "Shield" => Some(12),
        "Dissonance" | "Concerto" => Some(13),
        _ => None,
    }
}

fn player_spec(player: &PlayerMeter) -> Option<&'static str> {
    let mut counts: HashMap<&'static str, u64> = HashMap::new();
    for (id, sk) in &player.skill_breakdown {
        if let Some(spec) = skill_spec(*id) {
            *counts.entry(spec).or_default() += sk.stats.total;
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

pub fn class_color_egui(class_id: Option<u32>) -> egui::Color32 {
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

pub fn monster_type_name(monster_type: Option<i32>) -> &'static str {
    match monster_type {
        Some(0) => "Monster",
        Some(1) => "Elite Monster",
        Some(2) => "Boss Monster",
        _       => "Unknown",
    }
}

pub fn monster_type_color_egui(monster_type: Option<i32>) -> egui::Color32 {
    match monster_type {
        Some(0) => ui::theme::TEXT_MUTED,
        Some(1) => ui::theme::WARN,
        Some(2) => ui::theme::BAD,
        _       => ui::theme::TEXT_MUTED,
    }
}

/// Class/category name for a saved row — player profession or monster tier.
pub fn entity_category_name(p: &SavedPlayerMeter) -> &'static str {
    if p.is_player {
        let class_id = p.class_id.or_else(|| spec_to_class_id(p.spec.as_deref()?));
        class_name(class_id)
    } else {
        monster_type_name(p.monster_type)
    }
}

/// Row tint color — player class color or monster tier color.
pub fn entity_row_color(p: &SavedPlayerMeter) -> egui::Color32 {
    if p.is_player {
        let class_id = p.class_id.or_else(|| spec_to_class_id(p.spec.as_deref()?));
        class_color_egui(class_id)
    } else {
        monster_type_color_egui(p.monster_type)
    }
}
