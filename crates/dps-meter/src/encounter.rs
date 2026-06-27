use crate::meter::PlayerMeter;
use core::types::EntityId;
use game::event::CombatEvent;
use indexmap::IndexMap;
use serde::Serialize;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Encounter {
    pub id:                 Uuid,
    #[serde(skip)]
    pub start_time:         Instant,
    #[serde(skip)]
    pub end_time:           Option<Instant>,
    pub players:            IndexMap<EntityId, PlayerMeter>,
    pub total_damage:       u64,
    pub total_damage_taken: u64,
    pub total_healing:      u64,
}

impl Encounter {
    pub fn new() -> Self {
        Self {
            id:                 Uuid::new_v4(),
            start_time:         Instant::now(),
            end_time:           None,
            players:            IndexMap::new(),
            total_damage:       0,
            total_damage_taken: 0,
            total_healing:      0,
        }
    }

    pub fn apply(&mut self, event: &CombatEvent, name_resolver: impl Fn(EntityId) -> String) {
        let meter = self.players.entry(event.source_id).or_insert_with(|| {
            PlayerMeter::new(event.source_id, name_resolver(event.source_id))
        });
        meter.apply(event);
        self.total_damage += event.damage;
    }

    pub fn apply_heal(&mut self, source_id: EntityId, amount: u64, name_resolver: impl Fn(EntityId) -> String) {
        let meter = self.players.entry(source_id).or_insert_with(|| {
            PlayerMeter::new(source_id, name_resolver(source_id))
        });
        meter.apply_heal(amount);
        self.total_healing += amount;
    }

    pub fn apply_taken(&mut self, target_id: EntityId, damage: u64) {
        if let Some(meter) = self.players.get_mut(&target_id) {
            meter.damage_taken += damage;
            self.total_damage_taken += damage;
        }
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
        v.sort_by(|a, b| b.total_damage.cmp(&a.total_damage));
        v
    }
}
