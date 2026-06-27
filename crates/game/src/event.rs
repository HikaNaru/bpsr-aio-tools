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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DungeonStateKind {
    Null,
    Active,
    Ready,
    Playing,
    End,
    Settlement,
    Vote,
}

impl DungeonStateKind {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Active,
            2 => Self::Ready,
            3 => Self::Playing,
            4 => Self::End,
            5 => Self::Settlement,
            6 => Self::Vote,
            _ => Self::Null,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatChannel {
    World,
    Scene,
    Team,
    Union,
    Private,
    Group,
    Other(i32),
}

impl ChatChannel {
    pub fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::World,
            2 => Self::Scene,
            3 => Self::Team,
            4 => Self::Union,
            5 => Self::Private,
            6 => Self::Group,
            _ => Self::Other(v),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchAlertKind {
    QueuePop,
    ReadyCheck,
}

#[derive(Debug, Clone)]
pub enum GameEvent {
    Combat(CombatEvent),
    Heal(CombatEvent),
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
    DungeonState {
        state: DungeonStateKind,
    },
    Chat {
        channel:     ChatChannel,
        sender_name: String,
        sender_uid:  u64,
        text:        String,
    },
    MatchmakingAlert {
        kind: MatchAlertKind,
    },
    ThreatUpdate {
        /// The entity (boss/mob) whose aggro table changed
        target_id: EntityId,
        /// The player whose threat changed
        entity_id: EntityId,
        /// Absolute threat value
        threat:    u64,
    },
    Unknown {
        opcode:  u32,
        payload: Bytes,
    },
}
