use core::types::EntityId;
use game::event::CombatEvent;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Reusable per-category combat counter block — applied identically to
/// damage-dealt, damage-taken, and healing-given (and each per-skill entry).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CombatStats {
    pub total:            u64,
    pub hits:              u64,
    pub crit_hits:         u64,
    pub lucky_hits:        u64,
    pub crit_lucky_hits:   u64,
    pub crit_total:        u64,
    pub lucky_total:       u64,
    pub crit_lucky_total:  u64,
    pub max_hit:           u64,
}

impl CombatStats {
    pub fn apply(&mut self, amount: u64, is_crit: bool, is_lucky: bool) {
        self.total += amount;
        self.hits += 1;
        match (is_crit, is_lucky) {
            (true, true)  => { self.crit_lucky_hits += 1; self.crit_lucky_total += amount; }
            (true, false) => { self.crit_hits += 1; self.crit_total += amount; }
            (false, true) => { self.lucky_hits += 1; self.lucky_total += amount; }
            (false, false) => {}
        }
        if amount > self.max_hit { self.max_hit = amount; }
    }

    pub fn crit_rate(&self) -> f64 {
        if self.hits == 0 { return 0.0; }
        (self.crit_hits + self.crit_lucky_hits) as f64 / self.hits as f64
    }

    pub fn lucky_rate(&self) -> f64 {
        if self.hits == 0 { return 0.0; }
        (self.lucky_hits + self.crit_lucky_hits) as f64 / self.hits as f64
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SkillStat {
    pub skill_id: u32,
    pub stats:    CombatStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HitKind { Damage, Heal, Taken }

/// One timestamped hit, kept only on the live meter — bucketed into
/// per-phase stats at save time, not persisted itself (avoids unbounded
/// growth in the saved file for long fights).
#[derive(Debug, Clone)]
pub struct HitRecord {
    pub time_secs: f64,
    pub skill_id:  u32,
    pub amount:    u64,
    pub is_crit:   bool,
    pub is_lucky:  bool,
    pub kind:      HitKind,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerMeter {
    pub entity_id:       EntityId,
    pub player_name:     String,
    pub is_player:       bool,
    pub damage_stats:    CombatStats,
    pub taken_stats:     CombatStats,
    pub heal_stats:      CombatStats,
    pub overheal:        u64,
    pub shield_gain:     u64,
    pub deaths:          u32,
    pub skill_breakdown: IndexMap<u32, SkillStat>,
    /// (elapsed_seconds, dps_at_that_second)
    pub dps_timeline:    Vec<(f64, f64)>,
    #[serde(skip)]
    pub hit_log:         Vec<HitRecord>,
}

impl PlayerMeter {
    pub fn new(entity_id: EntityId, player_name: String) -> Self {
        Self {
            entity_id,
            player_name,
            is_player:       false,
            damage_stats:    CombatStats::default(),
            taken_stats:     CombatStats::default(),
            heal_stats:      CombatStats::default(),
            overheal:        0,
            shield_gain:     0,
            deaths:          0,
            skill_breakdown: IndexMap::new(),
            dps_timeline:    Vec::new(),
            hit_log:         Vec::new(),
        }
    }

    pub fn apply(&mut self, event: &CombatEvent, time_secs: f64) {
        self.damage_stats.apply(event.damage, event.is_crit, event.is_lucky);
        let skill = self.skill_breakdown.entry(event.skill_id).or_insert_with(|| SkillStat {
            skill_id: event.skill_id,
            ..Default::default()
        });
        skill.stats.apply(event.damage, event.is_crit, event.is_lucky);
        self.hit_log.push(HitRecord {
            time_secs, skill_id: event.skill_id, amount: event.damage,
            is_crit: event.is_crit, is_lucky: event.is_lucky, kind: HitKind::Damage,
        });
    }

    pub fn apply_heal(&mut self, event: &CombatEvent, overheal: u64, time_secs: f64) {
        self.heal_stats.apply(event.damage, event.is_crit, event.is_lucky);
        self.overheal += overheal;
        self.hit_log.push(HitRecord {
            time_secs, skill_id: event.skill_id, amount: event.damage,
            is_crit: event.is_crit, is_lucky: event.is_lucky, kind: HitKind::Heal,
        });
    }

    pub fn apply_taken(&mut self, event: &CombatEvent, time_secs: f64) {
        self.taken_stats.apply(event.damage, event.is_crit, event.is_lucky);
        self.hit_log.push(HitRecord {
            time_secs, skill_id: event.skill_id, amount: event.damage,
            is_crit: event.is_crit, is_lucky: event.is_lucky, kind: HitKind::Taken,
        });
    }

    pub fn total_damage(&self) -> u64 { self.damage_stats.total }
    pub fn hit_count(&self)    -> u64 { self.damage_stats.hits }
    pub fn crit_count(&self)   -> u64 { self.damage_stats.crit_hits + self.damage_stats.crit_lucky_hits }
    pub fn damage_taken(&self) -> u64 { self.taken_stats.total }
    pub fn total_healing(&self) -> u64 { self.heal_stats.total }

    /// Average DPS over the encounter duration. Same formula as DPS tile — never 0 after first hit.
    pub fn avg_dps(&self, elapsed: f64) -> f64 {
        if elapsed <= 0.0 { return 0.0; }
        self.damage_stats.total as f64 / elapsed
    }

    pub fn crit_rate(&self)  -> f64 { self.damage_stats.crit_rate() }
    pub fn lucky_rate(&self) -> f64 { self.damage_stats.lucky_rate() }

    pub fn max_dps(&self) -> f64 {
        self.dps_timeline.iter().map(|(_, d)| *d).fold(0.0, f64::max)
    }
}
