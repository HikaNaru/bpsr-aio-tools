use crate::meter::PlayerMeter;
use core::types::EntityId;
pub use encounter_store::EncounterOutcome;
use game::event::CombatEvent;
use indexmap::IndexMap;
use serde::Serialize;
use std::time::{Duration, Instant};
use uuid::Uuid;

/// Marks the start of a new phase within a single encounter (e.g. a boss's
/// next attack pattern). The phase's end is the next marker's start_offset_secs,
/// or the encounter's end for the last phase.
#[derive(Debug, Clone, Serialize)]
pub struct PhaseMarker {
    pub name:             String,
    pub start_offset_secs: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Encounter {
    pub id:                 Uuid,
    #[serde(skip)]
    pub start_time:         Instant,
    #[serde(skip)]
    pub end_time:           Option<Instant>,
    pub zone_name:          String,
    pub zone_id:            u32,
    pub outcome:            EncounterOutcome,
    pub phases:             Vec<PhaseMarker>,
    pub players:            IndexMap<EntityId, PlayerMeter>,
    pub total_damage:       u64,
    pub total_damage_taken: u64,
    pub total_healing:      u64,
}

impl Encounter {
    pub fn new(zone_name: String, zone_id: u32) -> Self {
        Self {
            id:                 Uuid::new_v4(),
            start_time:         Instant::now(),
            end_time:           None,
            zone_name,
            zone_id,
            outcome:            EncounterOutcome::Unknown,
            phases:             Vec::new(),
            players:            IndexMap::new(),
            total_damage:       0,
            total_damage_taken: 0,
            total_healing:      0,
        }
    }

    fn time_secs(&self, at: Instant) -> f64 {
        at.duration_since(self.start_time).as_secs_f64()
    }

    pub fn apply(&mut self, event: &CombatEvent, name_resolver: impl Fn(EntityId) -> String) {
        let time_secs = self.time_secs(event.timestamp);
        let meter = self.players.entry(event.source_id).or_insert_with(|| {
            PlayerMeter::new(event.source_id, name_resolver(event.source_id))
        });
        meter.apply(event, time_secs);
        self.total_damage += event.damage;
    }

    pub fn apply_heal(&mut self, event: &CombatEvent, overheal: u64, name_resolver: impl Fn(EntityId) -> String) {
        let time_secs = self.time_secs(event.timestamp);
        let meter = self.players.entry(event.source_id).or_insert_with(|| {
            PlayerMeter::new(event.source_id, name_resolver(event.source_id))
        });
        meter.apply_heal(event, overheal, time_secs);
        self.total_healing += event.damage;
    }

    pub fn apply_taken(&mut self, event: &CombatEvent) {
        let time_secs = self.time_secs(event.timestamp);
        if let Some(meter) = self.players.get_mut(&event.target_id) {
            meter.apply_taken(event, time_secs);
            self.total_damage_taken += event.damage;
        }
    }

    /// Records a shield-gain snapshot for `entity_id`. `new_gain` is the sum of
    /// InitialValue for any shield uuid not previously seen for this entity —
    /// dedup happens at the module level (shields persist across many packets).
    pub fn apply_shield_gain(&mut self, entity_id: EntityId, new_gain: u64, name_resolver: impl Fn(EntityId) -> String) {
        if new_gain == 0 { return; }
        let meter = self.players.entry(entity_id).or_insert_with(|| {
            PlayerMeter::new(entity_id, name_resolver(entity_id))
        });
        meter.shield_gain += new_gain;
    }

    pub fn record_death(&mut self, entity_id: EntityId, name_resolver: impl Fn(EntityId) -> String) {
        let meter = self.players.entry(entity_id).or_insert_with(|| {
            PlayerMeter::new(entity_id, name_resolver(entity_id))
        });
        meter.deaths += 1;
    }

    pub fn push_phase(&mut self, name: String) {
        let offset = self.elapsed().as_secs_f64();
        self.phases.push(PhaseMarker { name, start_offset_secs: offset });
    }

    pub fn elapsed(&self) -> Duration {
        let end = self.end_time.unwrap_or_else(Instant::now);
        end.duration_since(self.start_time)
    }

    pub fn is_active(&self) -> bool {
        self.end_time.is_none()
    }

    pub fn finish(&mut self) {
        self.end_time = Some(Instant::now());
    }

    pub fn players_by_damage(&self) -> Vec<&PlayerMeter> {
        let mut v: Vec<_> = self.players.values().collect();
        v.sort_by(|a, b| b.total_damage().cmp(&a.total_damage()));
        v
    }
}
