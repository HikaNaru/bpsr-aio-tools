use bytes::Bytes;
use core::types::{Element, EntityId};
use std::time::Instant;

use crate::entity::CharStats;

#[derive(Debug, Clone)]
pub struct PlayerModule {
    pub effects: Vec<ModuleEffect>,
}

#[derive(Debug, Clone)]
pub struct ModuleEffect {
    pub effect_id: i32,
    pub level:     i32,
}

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
    EntityDespawn {
        id: EntityId,
    },
    PlayerInventory {
        id:      EntityId,
        modules: Vec<PlayerModule>,
    },
    EntityStats {
        id:    EntityId,
        stats: CharStats,
    },
    Unknown {
        opcode:  u32,
        payload: Bytes,
    },
}
