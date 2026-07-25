use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use uuid::Uuid;

// ── Stored types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum EncounterOutcome {
    Cleared,
    Failed,
    ManualStop,
    #[default]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSkillStat {
    pub skill_id:  u32,
    pub total_dmg: u64,
    pub hits:      u64,
    pub crits:     u64,
    pub max_hit:   u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedGearSlot {
    pub slot:      i32,
    pub item_id:   i32,
    pub item_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedImagineEntry {
    pub skill_id: i32,
    pub tier:     i32,
    pub name:     String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedGearInfo {
    pub gear:     Vec<SavedGearSlot>,
    pub imagines: Vec<SavedImagineEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedPlayerPhaseStats {
    pub phase_index:   usize,
    pub total_damage:  u64,
    pub hits:          u64,
    pub damage_taken:  u64,
    pub total_healing: u64,
    pub skills:        Vec<SavedSkillStat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SavedPhaseMarker {
    pub name:              String,
    pub start_offset_secs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedPlayerMeter {
    pub entity_id:      u64,
    pub name:           String,
    pub class_id:       Option<u32>,
    #[serde(default)] pub monster_type: Option<i32>,
    pub total_damage:   u64,
    pub hit_count:      u64,
    pub crit_count:     u64,
    pub skills:         Vec<SavedSkillStat>,
    #[serde(default)] pub damage_taken:   u64,
    #[serde(default)] pub total_healing:  u64,
    #[serde(default)] pub ability_score:  Option<u32>,
    #[serde(default)] pub season_strength: Option<u32>,
    #[serde(default)] pub crit_pct:       Option<u32>,
    #[serde(default)] pub luck_pct:       Option<u32>,
    #[serde(default)] pub crit_damage:    Option<u32>,
    #[serde(default)] pub spec:           Option<String>,
    // ── Lucky / crit-lucky splits (damage) ──────────────────────────────────
    #[serde(default)] pub damage_lucky_hits:       u64,
    #[serde(default)] pub damage_crit_lucky_hits:  u64,
    #[serde(default)] pub damage_crit_total:       u64,
    #[serde(default)] pub damage_lucky_total:      u64,
    #[serde(default)] pub damage_crit_lucky_total: u64,
    #[serde(default)] pub max_hit:                 u64,
    #[serde(default)] pub max_dps:                 f64,
    // ── Healing splits ───────────────────────────────────────────────────────
    #[serde(default)] pub heal_hits:              u64,
    #[serde(default)] pub heal_crit_hits:          u64,
    #[serde(default)] pub heal_lucky_hits:         u64,
    #[serde(default)] pub heal_crit_lucky_hits:    u64,
    #[serde(default)] pub heal_crit_total:         u64,
    #[serde(default)] pub heal_lucky_total:        u64,
    #[serde(default)] pub heal_crit_lucky_total:   u64,
    #[serde(default)] pub heal_max_hit:            u64,
    #[serde(default)] pub overheal:                u64,
    // ── Misc new stats ───────────────────────────────────────────────────────
    #[serde(default)] pub shield_gain: u64,
    #[serde(default)] pub deaths:      u32,
    #[serde(default)] pub max_hp:      Option<i64>,
    #[serde(default)] pub is_player:   bool,
    #[serde(default)] pub gear:        Option<SavedGearInfo>,
    #[serde(default)] pub phase_stats: Vec<SavedPlayerPhaseStats>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedEncounter {
    pub id:           Uuid,
    pub scene_name:   String,
    pub started_at:   DateTime<Utc>,
    pub ended_at:     DateTime<Utc>,
    pub duration_secs: f64,
    pub players:      Vec<SavedPlayerMeter>,
    pub total_damage: u64,
    #[serde(default)] pub custom_name: Option<String>,
    #[serde(default)] pub outcome:     EncounterOutcome,
    #[serde(default)] pub phases:      Vec<SavedPhaseMarker>,
}

/// Lightweight summary stored alongside the compressed encounter file.
/// Lets us list all encounters without decompressing each one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncounterSummary {
    pub id:              Uuid,
    pub scene_name:      String,
    pub started_at:      DateTime<Utc>,
    pub duration_secs:   f64,
    pub player_count:    usize,
    pub total_damage:    u64,
    pub top_player_name: String,
    pub top_player_dps:  f64,
    #[serde(default)] pub custom_name: Option<String>,
    #[serde(default)] pub outcome:     EncounterOutcome,
}

// ── Store ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct EncounterStore {
    dir: PathBuf,
}

impl EncounterStore {
    pub fn open() -> Self {
        let dir = dirs_next::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("bpsr-aio-tools")
            .join("encounters");
        std::fs::create_dir_all(&dir).ok();
        Self { dir }
    }

    fn enc_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{}.enc.zst", id))
    }

    fn sum_path(&self, id: Uuid) -> PathBuf {
        self.dir.join(format!("{}.sum.json", id))
    }

    pub fn save(&self, enc: &SavedEncounter) -> anyhow::Result<()> {
        // Full encounter — zstd-compressed JSON
        let json = serde_json::to_vec(enc)?;
        let compressed = zstd::encode_all(json.as_slice(), 3)?;
        std::fs::write(self.enc_path(enc.id), compressed)?;

        // Sidecar summary — plain JSON (fast listing, no decompression)
        let sum = summary_of(enc);
        let sum_json = serde_json::to_string(&sum)?;
        std::fs::write(self.sum_path(enc.id), sum_json)?;

        Ok(())
    }

    pub fn load(&self, id: Uuid) -> anyhow::Result<SavedEncounter> {
        let data = std::fs::read(self.enc_path(id))?;
        let json = zstd::decode_all(data.as_slice())?;
        let enc  = serde_json::from_slice(&json)?;
        Ok(enc)
    }

    pub fn delete(&self, id: Uuid) -> anyhow::Result<()> {
        let _ = std::fs::remove_file(self.enc_path(id));
        let _ = std::fs::remove_file(self.sum_path(id));
        Ok(())
    }

    /// Deletes every saved encounter. Reuses `delete()` per-id so partial
    /// failures are collected rather than aborting the whole operation.
    pub fn clear_all(&self) -> anyhow::Result<()> {
        let mut first_err = None;
        for s in self.list_summaries() {
            if let Err(e) = self.delete(s.id) {
                tracing::warn!("clear_all delete {}: {e}", s.id);
                first_err.get_or_insert(e);
            }
        }
        match first_err {
            Some(e) => Err(e),
            None    => Ok(()),
        }
    }

    /// Deletes every saved encounter started before `cutoff`.
    pub fn clear_before(&self, cutoff: DateTime<Utc>) -> anyhow::Result<usize> {
        let mut count = 0;
        for s in self.list_summaries() {
            if s.started_at < cutoff {
                if let Err(e) = self.delete(s.id) {
                    tracing::warn!("clear_before delete {}: {e}", s.id);
                } else {
                    count += 1;
                }
            }
        }
        Ok(count)
    }

    /// Renames a saved encounter (sets/clears its custom display name).
    pub fn rename(&self, id: Uuid, name: Option<String>) -> anyhow::Result<()> {
        let mut enc = self.load(id)?;
        enc.custom_name = name;
        self.save(&enc)
    }

    /// Returns summaries sorted newest-first.
    /// Reads only the lightweight `.sum.json` sidecars.
    /// Falls back to decompressing `.enc.zst` if sidecar is missing.
    pub fn list_summaries(&self) -> Vec<EncounterSummary> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else { return vec![] };

        let mut summaries = Vec::new();
        let mut seen = std::collections::HashSet::new();

        for entry in entries.flatten() {
            let path = entry.path();
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();

            if let Some(id_str) = name.strip_suffix(".sum.json") {
                if let Ok(id) = Uuid::parse_str(id_str) {
                    seen.insert(id);
                    match std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<EncounterSummary>(&s).ok())
                    {
                        Some(sum) => summaries.push(sum),
                        None => {
                            // Corrupt sidecar — load from full file and rewrite
                            if let Ok(enc) = self.load(id) {
                                let sum = summary_of(&enc);
                                let _ = self.write_summary(&sum);
                                summaries.push(sum);
                            }
                        }
                    }
                }
            }
        }

        // Fallback: any .enc.zst without a sidecar
        if let Ok(entries2) = std::fs::read_dir(&self.dir) {
            for entry in entries2.flatten() {
                let path = entry.path();
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                if let Some(id_str) = name.strip_suffix(".enc.zst") {
                    if let Ok(id) = Uuid::parse_str(id_str) {
                        if !seen.contains(&id) {
                            if let Ok(enc) = self.load(id) {
                                let sum = summary_of(&enc);
                                let _ = self.write_summary(&sum);
                                summaries.push(sum);
                            }
                        }
                    }
                }
            }
        }

        summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        summaries
    }

    pub fn auto_cleanup(&self, max_count: usize, max_age_days: u64) {
        let mut summaries = self.list_summaries();
        let cutoff = Utc::now() - chrono::Duration::days(max_age_days as i64);

        summaries.retain(|s| {
            if s.started_at < cutoff {
                if let Err(e) = self.delete(s.id) {
                    tracing::warn!("encounter cleanup delete {}: {e}", s.id);
                }
                false
            } else {
                true
            }
        });

        if summaries.len() > max_count {
            for s in &summaries[max_count..] {
                let _ = self.delete(s.id);
            }
        }
    }

    fn write_summary(&self, sum: &EncounterSummary) -> anyhow::Result<()> {
        let json = serde_json::to_string(sum)?;
        std::fs::write(self.sum_path(sum.id), json)?;
        Ok(())
    }
}

fn summary_of(enc: &SavedEncounter) -> EncounterSummary {
    let top = enc.players.iter().max_by_key(|p| p.total_damage);
    let (top_name, top_dps) = top.map(|p| {
        let dps = if enc.duration_secs > 0.1 {
            p.total_damage as f64 / enc.duration_secs
        } else { 0.0 };
        (p.name.clone(), dps)
    }).unwrap_or_default();

    EncounterSummary {
        id:              enc.id,
        scene_name:      enc.scene_name.clone(),
        started_at:      enc.started_at,
        duration_secs:   enc.duration_secs,
        player_count:    enc.players.iter().filter(|p| p.is_player).count(),
        total_damage:    enc.total_damage,
        top_player_name: top_name,
        top_player_dps:  top_dps,
        custom_name:     enc.custom_name.clone(),
        outcome:         enc.outcome,
    }
}
