use core::types::EntityId;
use game::event::CombatEvent;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillStat {
    pub skill_id:  u32,
    pub total_dmg: u64,
    pub hits:      u64,
    pub crits:     u64,
    pub max_hit:   u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerMeter {
    pub entity_id:       EntityId,
    pub player_name:     String,
    pub total_damage:    u64,
    pub damage_taken:    u64,
    pub total_healing:   u64,
    pub hit_count:       u64,
    pub crit_count:      u64,
    pub skill_breakdown: IndexMap<u32, SkillStat>,
    /// (elapsed_seconds, dps_at_that_second)
    pub dps_timeline:    Vec<(f64, f64)>,
}

impl PlayerMeter {
    pub fn new(entity_id: EntityId, player_name: String) -> Self {
        Self {
            entity_id,
            player_name,
            total_damage:  0,
            damage_taken:  0,
            total_healing: 0,
            hit_count:     0,
            crit_count:    0,
            skill_breakdown: IndexMap::new(),
            dps_timeline:  Vec::new(),
        }
    }

    pub fn apply(&mut self, event: &CombatEvent) {
        self.total_damage += event.damage;
        self.hit_count += 1;
        if event.is_crit {
            self.crit_count += 1;
        }
        let skill = self.skill_breakdown.entry(event.skill_id).or_insert_with(|| SkillStat {
            skill_id: event.skill_id,
            ..Default::default()
        });
        skill.total_dmg += event.damage;
        skill.hits += 1;
        if event.is_crit {
            skill.crits += 1;
        }
        if event.damage > skill.max_hit {
            skill.max_hit = event.damage;
        }
    }

    pub fn apply_heal(&mut self, amount: u64) {
        self.total_healing += amount;
    }

    /// Average DPS over the encounter duration. Same formula as DPS tile — never 0 after first hit.
    pub fn avg_dps(&self, elapsed: f64) -> f64 {
        if elapsed <= 0.0 { return 0.0; }
        self.total_damage as f64 / elapsed
    }

    pub fn crit_rate(&self) -> f64 {
        if self.hit_count == 0 { return 0.0; }
        self.crit_count as f64 / self.hit_count as f64
    }
}
