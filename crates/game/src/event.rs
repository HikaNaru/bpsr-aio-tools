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

/// One entry from an entity's AttrShieldList snapshot.
#[derive(Debug, Clone, Copy)]
pub struct ShieldEntry {
    pub uuid:          i64,
    pub value:         i64,
    pub initial_value: i64,
}

/// One equipped item slot from an entity's AttrEquipData snapshot.
#[derive(Debug, Clone, Copy)]
pub struct EquipSlot {
    pub slot:     i32,
    pub item_id:  i32,
}

/// One learned skill entry from an entity's AttrSkillLevelIdList snapshot.
#[derive(Debug, Clone, Copy)]
pub struct SkillLoadoutEntry {
    pub skill_id:      i32,
    pub current_level: i32,
    pub tier:          i32,
}

/// One dungeon objective's progress, from DungeonSyncData.Target.TargetData.
#[derive(Debug, Clone, Copy)]
pub struct DungeonTargetProgress {
    pub target_id: i32,
    pub nums:      i32,
    pub complete:  i32,
}

#[derive(Debug, Clone)]
pub struct CombatEvent {
    pub timestamp: Instant,
    pub source_id: EntityId,
    pub target_id: EntityId,
    pub skill_id:  u32,
    pub damage:    u64,
    pub is_crit:   bool,
    pub is_lucky:  bool,
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
        id:           EntityId,
        name:         String,
        class:        Option<u32>,
        monster_type: Option<i32>,
        is_player:    bool,
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
    /// Snapshot of an entity's currently-active shields (AttrShieldList).
    /// Emitted for any player entity, not just the local one.
    ShieldList {
        id:      EntityId,
        shields: Vec<ShieldEntry>,
    },
    /// Snapshot of an entity's equipped gear (AttrEquipData).
    /// Emitted for any player entity, not just the local one.
    EquipData {
        id:    EntityId,
        slots: Vec<EquipSlot>,
    },
    /// Snapshot of an entity's learned skills/Imagines (AttrSkillLevelIdList).
    /// Emitted for any player entity, not just the local one.
    SkillLoadout {
        id:     EntityId,
        skills: Vec<SkillLoadoutEntry>,
    },
    EntityStats {
        id:    EntityId,
        stats: CharStats,
    },
    DungeonState {
        state: DungeonStateKind,
    },
    /// Dungeon objective/phase progress — candidate signals for detecting a
    /// phase transition within one continuous encounter. `phase_id` is an
    /// explicit field the reference tool decodes but never actually uses, so
    /// its reliability here is unverified; `targets` carries the
    /// complete/nums-based signal the reference tool's phase logic runs on.
    DungeonPhaseSignal {
        targets:  Vec<DungeonTargetProgress>,
        phase_id: Option<i32>,
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
