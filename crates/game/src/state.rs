use crate::entity::Entity;
use crate::event::{DungeonStateKind, GameEvent};
use core::types::EntityId;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct GameState {
    pub entities:       HashMap<EntityId, Entity>,
    pub local_player:   Option<EntityId>,
    pub zone_id:        Option<u32>,
    pub zone_name:      Option<String>,
    pub dungeon_state:  Option<DungeonStateKind>,
}

impl GameState {
    pub fn apply(&mut self, event: &GameEvent) {
        match event {
            GameEvent::EntityName { id, name, class } => {
                let entity = self.entities.entry(*id).or_insert_with(|| Entity::new(*id));
                entity.name = name.clone();
                entity.class_id = *class;
            }
            GameEvent::LocalPlayer { id } => {
                self.local_player = Some(*id);
                let entity = self.entities.entry(*id).or_insert_with(|| Entity::new(*id));
                entity.is_local = true;
            }
            GameEvent::ZoneChange { zone_id, zone_name } => {
                let zone_changed = self.zone_id != Some(*zone_id);
                self.zone_id   = Some(*zone_id);
                self.zone_name = Some(zone_name.clone());
                if zone_changed {
                    self.entities.clear();
                }
            }
            GameEvent::EntityDespawn { id } => {
                self.entities.remove(id);
            }
            GameEvent::EntityStats { id, stats } => {
                let entity = self.entities.entry(*id).or_insert_with(|| Entity::new(*id));
                entity.stats.merge(stats);
            }
            GameEvent::DungeonState { state } => {
                self.dungeon_state = Some(state.clone());
            }
            GameEvent::Combat(_)
            | GameEvent::Heal(_)
            | GameEvent::PlayerInventory { .. }
            | GameEvent::Chat { .. }
            | GameEvent::MatchmakingAlert { .. }
            | GameEvent::ThreatUpdate { .. }
            | GameEvent::Unknown { .. } => {}
        }
    }

    pub fn entity_name(&self, id: EntityId) -> &str {
        self.entities
            .get(&id)
            .map(|e| e.name.as_str())
            .unwrap_or("Unknown")
    }
}
