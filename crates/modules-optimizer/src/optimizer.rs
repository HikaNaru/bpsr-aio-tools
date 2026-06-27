use game::event::PlayerModule;
use std::collections::HashMap;

pub const MAX_STAT_VALUE: u8 = 20;
pub const MAX_STAT_SLOTS: usize = 32; // matches ZDPS Vector<byte>.Count
/// Default beam width — trade-off between quality and speed.
pub const DEFAULT_BEAM_WIDTH: usize = 8000;
/// Boost multiplier per breakpoint tier: [>=1, >=4, >=8, >=12, >=16, >=20].
pub const LINK_LEVEL_BONUS: [i32; 6] = [1, 2, 4, 8, 16, 32];

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum StatMode {
    AtLeast,
    Exactly,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StatPriority {
    /// Raw game effect ID (e.g. 1110 = HP Boost I, 1205 = ATK Boost I).
    pub effect_id: i32,
    /// Minimum total level required across chosen modules (0 = no requirement).
    pub req_level: u8,
    pub mode: StatMode,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SolverConfig {
    /// Ordered list of stat priorities (highest priority first).
    pub priorities: Vec<StatPriority>,
    /// Number of module slots to fill (4 or 5).
    pub num_modules: usize,
    /// Give unspecified stats a small weight of 1.
    pub value_all_stats: bool,
    pub beam_width: usize,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            priorities: vec![],
            num_modules: 4,
            value_all_stats: true,
            beam_width: DEFAULT_BEAM_WIDTH,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModComboResult {
    /// Indices into the original `modules` slice.
    pub module_indices: Vec<usize>,
    /// (effect_id, capped_total) for each priority stat.
    pub priority_stats: Vec<(i32, u8)>,
    /// All stats (effect_id, total) sorted by effect_id.
    pub all_stats: Vec<(i32, u8)>,
    pub score: i32,
}

// ── Internal ─────────────────────────────────────────────────────────────────

/// Compact beam node — no heap allocations.
#[derive(Clone)]
struct BeamNode {
    /// Module indices chosen so far (sorted ascending, -1 = empty slot).
    mods: [i16; 5],
    /// Aggregated stat totals indexed by dense stat index, capped at MAX_STAT_VALUE.
    totals: [u8; MAX_STAT_SLOTS],
    score: i32,
    req_met: u8,
}

impl BeamNode {
    fn empty(stat_count: usize) -> Self {
        let _ = stat_count;
        Self {
            mods: [-1; 5],
            totals: [0u8; MAX_STAT_SLOTS],
            score: 0,
            req_met: 0,
        }
    }

    fn contains_mod(&self, idx: i16) -> bool {
        self.mods.iter().any(|&m| m == idx)
    }
}

struct ScoreTables {
    score:   Vec<i32>,  // [dense_idx * 21 + value]
    req_met: Vec<bool>, // same layout
    len_v:   usize,     // MAX_STAT_VALUE + 1 = 21
}

impl ScoreTables {
    fn score(&self, stat_idx: usize, value: u8) -> i32 {
        self.score[stat_idx * self.len_v + value as usize]
    }

    fn req_met(&self, stat_idx: usize, value: u8) -> bool {
        self.req_met[stat_idx * self.len_v + value as usize]
    }
}

// ── Entry point ──────────────────────────────────────────────────────────────

pub fn optimize(modules: &[PlayerModule], config: &SolverConfig) -> Vec<ModComboResult> {
    if modules.is_empty() || config.num_modules == 0 {
        return vec![];
    }

    let num_slots = config.num_modules.min(5);

    // Dense index: collect all effect_ids across inventory, sort, map to 0..n
    let mut effect_ids: Vec<i32> = modules.iter()
        .flat_map(|m| m.effects.iter().map(|e| e.effect_id))
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    effect_ids.sort_unstable();

    let stat_count = effect_ids.len().min(MAX_STAT_SLOTS);
    if stat_count == 0 { return vec![]; }

    let to_dense: HashMap<i32, usize> = effect_ids.iter()
        .take(stat_count)
        .enumerate()
        .map(|(i, &id)| (id, i))
        .collect();

    // Convert each PlayerModule to a dense [u8; MAX_STAT_SLOTS]
    let dense: Vec<[u8; MAX_STAT_SLOTS]> = modules.iter().map(|m| {
        let mut v = [0u8; MAX_STAT_SLOTS];
        for e in &m.effects {
            if let Some(&idx) = to_dense.get(&e.effect_id) {
                let add = e.level.max(0) as u8;
                v[idx] = v[idx].saturating_add(add);
            }
        }
        v
    }).collect();

    // Build normalized priorities
    let prio_count = config.priorities.len();
    struct NormPrio { dense_idx: usize, req_level: u8, mode: StatMode, weight: i32 }
    let norm_prios: Vec<NormPrio> = config.priorities.iter().enumerate()
        .filter_map(|(order, p)| {
            to_dense.get(&p.effect_id).map(|&di| NormPrio {
                dense_idx: di,
                req_level: p.req_level,
                mode: p.mode.clone(),
                weight: (prio_count - order + 1) as i32,
            })
        })
        .collect();

    let lv = MAX_STAT_VALUE as usize + 1; // 21
    let mut score_tbl = vec![0i32;  stat_count * lv];
    let mut req_tbl   = vec![false; stat_count * lv];

    // Fill table for prioritized stats
    let prio_idxs: std::collections::HashSet<usize> = norm_prios.iter().map(|np| np.dense_idx).collect();
    for np in &norm_prios {
        for v in 0u8..=MAX_STAT_VALUE {
            let tidx = np.dense_idx * lv + v as usize;
            let boost = link_level_boost(v);
            match np.mode {
                StatMode::AtLeast => {
                    score_tbl[tidx] = v.min(MAX_STAT_VALUE) as i32 * np.weight * boost;
                    req_tbl[tidx]   = v >= np.req_level;
                }
                StatMode::Exactly => {
                    let max_s = MAX_STAT_VALUE as i32 * np.weight * boost;
                    if v == np.req_level {
                        score_tbl[tidx] = max_s;
                        req_tbl[tidx]   = true;
                    } else {
                        let req = np.req_level.max(1) as f32;
                        score_tbl[tidx] = (max_s as f32 * (v as f32 / req)) as i32;
                    }
                }
            }
        }
    }
    // Fill table for non-prioritized stats (weight 1, no req)
    if config.value_all_stats {
        for di in 0..stat_count {
            if prio_idxs.contains(&di) { continue; }
            for v in 0u8..=MAX_STAT_VALUE {
                let tidx = di * lv + v as usize;
                score_tbl[tidx] = v.min(MAX_STAT_VALUE) as i32 * link_level_boost(v);
            }
        }
    }

    let tables = ScoreTables { score: score_tbl, req_met: req_tbl, len_v: lv };
    let num_reqs = norm_prios.iter().filter(|np| np.req_level > 0).count();

    // Beam search
    let raw = beam_search(&dense, num_slots, config.beam_width, &tables, stat_count, num_reqs);

    // Convert to ModComboResult
    raw.into_iter().map(|node| {
        let priority_stats: Vec<(i32, u8)> = norm_prios.iter().map(|np| {
            (effect_ids[np.dense_idx], node.totals[np.dense_idx].min(MAX_STAT_VALUE))
        }).collect();

        let all_stats: Vec<(i32, u8)> = effect_ids.iter().enumerate()
            .take(stat_count)
            .filter_map(|(di, &eid)| {
                let v = node.totals[di];
                if v > 0 { Some((eid, v.min(MAX_STAT_VALUE))) } else { None }
            })
            .collect();

        let module_indices: Vec<usize> = node.mods.iter()
            .filter(|&&m| m >= 0)
            .map(|&m| m as usize)
            .collect();

        ModComboResult { module_indices, priority_stats, all_stats, score: node.score }
    }).collect()
}

// ── Beam search ───────────────────────────────────────────────────────────────

fn beam_search(
    dense: &[[u8; MAX_STAT_SLOTS]],
    num_slots: usize,
    beam_width: usize,
    tables: &ScoreTables,
    stat_count: usize,
    num_reqs: usize,
) -> Vec<BeamNode> {
    use rayon::prelude::*;

    let mut beam = vec![BeamNode::empty(stat_count)];

    for depth in 0..num_slots {
        // Parallel expansion of all beam nodes
        let mut candidates: Vec<BeamNode> = beam.par_iter().flat_map_iter(|node| {
            let start = if depth == 0 {
                0
            } else {
                // enforce ascending index order for combination semantics
                let last = node.mods[..depth].iter()
                    .filter(|&&m| m >= 0)
                    .map(|&m| m as usize)
                    .max()
                    .unwrap_or(0);
                last + 1
            };

            (start..dense.len()).filter_map(move |mod_idx| {
                if node.contains_mod(mod_idx as i16) { return None; }
                let m = &dense[mod_idx];
                let mut new_totals = node.totals;
                let mut score = 0i32;
                let mut req_met = 0u8;

                for di in 0..stat_count {
                    new_totals[di] = new_totals[di].saturating_add(m[di]).min(MAX_STAT_VALUE);
                    let v = new_totals[di];
                    score   += tables.score(di, v);
                    if tables.req_met(di, v) { req_met += 1; }
                }

                let mut new_mods = node.mods;
                new_mods[depth] = mod_idx as i16;

                Some(BeamNode {
                    mods: new_mods,
                    totals: new_totals,
                    score,
                    req_met,
                })
            })
        }).collect();

        top_k_inplace(&mut candidates, beam_width);
        beam = candidates;

        if beam.is_empty() { return vec![]; }
    }

    // Final: prefer those meeting all requirements
    let meets: Vec<BeamNode> = beam.into_iter()
        .filter(|n| n.req_met as usize >= num_reqs)
        .collect();

    // If none meet requirements, return top 10 from beam anyway
    let mut final_beam = meets;
    top_k_inplace(&mut final_beam, 10);
    final_beam
}

/// Partial sort in-place: keep best `k` elements (by req_met desc, score desc).
fn top_k_inplace(v: &mut Vec<BeamNode>, k: usize) {
    if v.len() <= k { return; }
    // Partial sort using select_nth_unstable for O(n) average complexity
    v.select_nth_unstable_by(k, |a, b| {
        b.req_met.cmp(&a.req_met).then(b.score.cmp(&a.score))
    });
    v.truncate(k);
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn link_level_boost(total: u8) -> i32 {
    let idx = match total {
        t if t >= 20 => 5,
        t if t >= 16 => 4,
        t if t >= 12 => 3,
        t if t >= 8  => 2,
        t if t >= 4  => 1,
        t if t >= 1  => 0,
        _ => return 0,
    };
    LINK_LEVEL_BONUS[idx]
}
