use bytes::Bytes;
use core::types::{Element, EntityId};
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct CombatEvent {
    pub timestamp: Instant,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub skill_id:  u32,
    pub damage:    u64,
    pub is_crit:   bool,
    pub is_dot:    bool,
    pub element:   Option<Element>,
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    Combat(CombatEvent),
    EntityName {
        id:    EntityId,
        name:  String,
        class: Option<u32>,
    },
    LocalPlayer {
        id: EntityId,
    },
    ZoneChange {
        zone_id:   u32,
        zone_name: String,
    },
    Unknown {
        opcode:  u32,
        payload: Bytes,
    },
}
