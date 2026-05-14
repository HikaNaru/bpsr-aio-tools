use crate::entity::Entity;
use crate::event::GameEvent;
use core::types::EntityId;
use std::collections::HashMap;

#[derive(Debug, Default)]
pub struct GameState {
    pub entities:      HashMap<EntityId, Entity>,
    pub local_player:  Option<EntityId>,
    pub zone_id:       Option<u32>,
    pub zone_name:     Option<String>,
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
                if let Some(e) = self.entities.get_mut(id) {
                    e.is_local = true;
                }
            }
            GameEvent::ZoneChange { zone_id, zone_name } => {
                self.zone_id   = Some(*zone_id);
                self.zone_name = Some(zone_name.clone());
            }
            GameEvent::Combat(_) | GameEvent::Unknown { .. } => {}
        }
    }

    pub fn entity_name(&self, id: EntityId) -> &str {
        self.entities
            .get(&id)
            .map(|e| e.name.as_str())
            .unwrap_or("Unknown")
    }
}
