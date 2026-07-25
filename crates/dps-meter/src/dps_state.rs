use crate::encounter::{Encounter, EncounterOutcome};
use core::types::EntityId;
use game::event::DungeonStateKind;
use game::GameEvent;
use tracing::debug;

const MAX_PAST_ENCOUNTERS: usize = 20;

pub struct DpsState {
    pub active:         Option<Encounter>,
    pub past:           Vec<Encounter>,
    /// Encounters finished this tick — drained by the module to persist to disk.
    pub newly_finished: Vec<Encounter>,
    /// True while inside a dungeon/training instance; suppresses timer-based splits.
    pub in_dungeon:      bool,
    /// True from the first End/Settlement/Vote signal until Playing/Active/
    /// Ready or a zone change is seen again. End/Settlement/Vote arrive as
    /// separate events several seconds apart (not one atomic transition), so
    /// without this a stray hit landing in that gap (trailing DoT/heal tick,
    /// etc.) would spawn a short-lived "phantom" encounter that the next
    /// state signal immediately finishes — showing up as a duplicate,
    /// few-second-long entry right next to the real fight in History.
    post_settlement:     bool,
}

impl DpsState {
    pub fn new(_encounter_timeout_secs: u32) -> Self {
        Self {
            active:         None,
            past:           Vec::new(),
            newly_finished: Vec::new(),
            in_dungeon:     false,
            post_settlement: false,
        }
    }

    pub fn apply_event<F>(&mut self, event: &GameEvent, zone_name: &str, zone_id: u32, name_resolver: F)
    where
        F: Fn(EntityId) -> String,
    {
        let GameEvent::Combat(combat) = event else {
            return;
        };

        // Stray hits after End/Settlement/Vote (results screen) shouldn't
        // spin up a new encounter — see post_settlement doc comment.
        if self.active.is_none() && self.post_settlement {
            return;
        }

        // Start a new encounter only if there is none — no timer-based splits.
        // Encounters end only via ZoneChange, DungeonState::End/Settlement/Vote,
        // or the manual Reset button (matching ZDPS behavior).
        if self.active.is_none() {
            // Diagnostic: pinpointing what stray hit spawns a short "phantom"
            // encounter right after a real fight ends (duplicate-save
            // investigation). Remove once root-caused.
            debug!(
                "new encounter started zone={} first_hit: source={:?} target={:?} skill={} dmg={} is_dot={}",
                zone_name, combat.source_id, combat.target_id, combat.skill_id, combat.damage, combat.is_dot
            );
            self.active = Some(Encounter::new(zone_name.to_string(), zone_id));
        }

        if let Some(enc) = &mut self.active {
            enc.apply(combat, |id| name_resolver(id));
        }
    }

    pub fn tick(&mut self) {
        // No-op — timer-based timeout removed. ZDPS never calls its equivalent.
    }

    pub fn finish_active(&mut self) {
        if let Some(mut enc) = self.active.take() {
            enc.finish();
            debug!(
                "encounter finished id={} zone={} duration={:.1}s total_damage={} outcome={:?} players={}",
                enc.id, enc.zone_name, enc.elapsed().as_secs_f64(), enc.total_damage, enc.outcome, enc.players.len()
            );
            if enc.total_damage > 0 {
                self.newly_finished.push(enc.clone());
                self.past.push(enc);
                if self.past.len() > MAX_PAST_ENCOUNTERS {
                    self.past.remove(0);
                }
            }
        }
    }

    /// Handle state-transition events (dungeon state, zone change).
    /// Call this from the module before apply_event so combat events land in
    /// the right encounter bucket.
    pub fn apply_state_event(&mut self, event: &GameEvent) {
        match event {
            GameEvent::DungeonState { state } => match state {
                // Active / Ready: dungeon instance exists but hasn't started.
                // Guild Center / Training Areas send these before Playing — mark as
                // in_dungeon so the timeout is suppressed while the player is inside.
                DungeonStateKind::Active | DungeonStateKind::Ready => {
                    self.post_settlement = false;
                    if !self.in_dungeon {
                        self.in_dungeon = true;
                        // Don't end the current encounter — just suppress timeout.
                    }
                }
                DungeonStateKind::Playing => {
                    // Transition into Playing: end any overworld encounter and mark
                    // in_dungeon. Do NOT create an encounter here — first combat event
                    // will start it. Repeated Playing heartbeats → no-op.
                    self.post_settlement = false;
                    if !self.in_dungeon {
                        self.finish_active();
                        self.in_dungeon = true;
                    }
                }
                DungeonStateKind::End
                | DungeonStateKind::Settlement
                | DungeonStateKind::Vote => {
                    if let Some(enc) = &mut self.active {
                        if enc.outcome == EncounterOutcome::Unknown {
                            enc.outcome = EncounterOutcome::Cleared;
                        }
                    }
                    self.finish_active();
                    self.in_dungeon = false;
                    self.post_settlement = true;
                }
                DungeonStateKind::Null => {
                    // Open-world: clear dungeon flag, keep encounter alive.
                    self.in_dungeon = false;
                }
            },
            GameEvent::ZoneChange { .. } => {
                self.post_settlement = false;
                if let Some(enc) = &mut self.active {
                    if enc.outcome == EncounterOutcome::Unknown {
                        enc.outcome = EncounterOutcome::Cleared;
                    }
                }
                self.finish_active();
                self.in_dungeon = false;
            }
            _ => {}
        }
    }

    pub fn reset(&mut self) {
        if let Some(enc) = &mut self.active {
            if enc.outcome == EncounterOutcome::Unknown {
                enc.outcome = EncounterOutcome::ManualStop;
            }
        }
        self.finish_active();
        self.in_dungeon = false;
        self.post_settlement = false;
    }
}
