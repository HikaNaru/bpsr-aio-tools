use core::types::EntityId;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CharStats {
    pub level:           Option<u32>,
    pub ability_score:   Option<u32>,
    pub hp:              Option<i64>,
    pub max_hp:          Option<i64>,
    pub attack:          Option<i64>,
    pub strength:        Option<u32>,
    pub endurance:       Option<u32>,
    pub armor:           Option<i64>,
    pub crit:            Option<u32>,
    pub crit_pct:        Option<u32>,
    pub haste:           Option<u32>,
    pub haste_pct:       Option<u32>,
    pub luck:            Option<u32>,
    pub luck_pct:        Option<u32>,
    pub mastery:         Option<u32>,
    pub mastery_pct:     Option<u32>,
    pub versatility:     Option<u32>,
    pub versatility_pct: Option<u32>,
    pub block:           Option<u32>,
    pub block_pct:       Option<u32>,
}

impl CharStats {
    pub fn merge(&mut self, other: &CharStats) {
        macro_rules! m { ($f:ident) => { if other.$f.is_some() { self.$f = other.$f; } }; }
        m!(level); m!(ability_score); m!(hp); m!(max_hp); m!(attack); m!(strength);
        m!(endurance); m!(armor); m!(crit); m!(crit_pct); m!(haste); m!(haste_pct);
        m!(luck); m!(luck_pct); m!(mastery); m!(mastery_pct); m!(versatility);
        m!(versatility_pct); m!(block); m!(block_pct);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Entity {
    pub id:       EntityId,
    pub name:     String,
    pub class_id: Option<u32>,
    pub is_local: bool,
    pub stats:    CharStats,
}

impl Entity {
    pub fn new(id: EntityId) -> Self {
        Self {
            id,
            name: format!("{id}"),
            class_id: None,
            is_local: false,
            stats:    CharStats::default(),
        }
    }
}
