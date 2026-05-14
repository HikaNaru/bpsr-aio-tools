use core::types::EntityId;
use game::event::CombatEvent;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::time::Instant;

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
    pub hit_count:       u64,
    pub crit_count:      u64,
    pub skill_breakdown: IndexMap<u32, SkillStat>,
    /// (elapsed_seconds, dps_at_that_second)
    pub dps_timeline:    Vec<(f64, f64)>,

    #[serde(skip)]
    last_window_damage: u64,
    #[serde(skip)]
    window_start: Option<Instant>,
}

impl PlayerMeter {
    pub fn new(entity_id: EntityId, player_name: String) -> Self {
        Self {
            entity_id,
            player_name,
            total_damage: 0,
            hit_count: 0,
            crit_count: 0,
            skill_breakdown: IndexMap::new(),
            dps_timeline: Vec::new(),
            last_window_damage: 0,
            window_start: None,
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

    pub fn current_dps(&self, encounter_start: Instant, window_secs: f64) -> f64 {
        let elapsed = encounter_start.elapsed().as_secs_f64();
        if elapsed < 0.1 {
            return 0.0;
        }
        self.total_damage as f64 / elapsed.min(window_secs).max(1.0)
    }

    pub fn crit_rate(&self) -> f64 {
        if self.hit_count == 0 {
            return 0.0;
        }
        self.crit_count as f64 / self.hit_count as f64
    }
}
