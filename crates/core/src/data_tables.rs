use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::warn;

fn data_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let d = exe.parent().unwrap_or(exe.as_path()).join("data");
        if d.exists() {
            return d;
        }
    }
    PathBuf::from("data")
}

fn load_table<T: for<'de> Deserialize<'de>>(filename: &str) -> HashMap<String, T> {
    let path = data_dir().join(filename);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            warn!("data table missing: {}", path.display());
            return HashMap::new();
        }
    };
    match serde_json::from_str(&content) {
        Ok(map) => map,
        Err(e) => {
            warn!("data table parse error [{filename}]: {e}");
            HashMap::new()
        }
    }
}

fn load_flat(filename: &str) -> HashMap<String, String> {
    let path = data_dir().join(filename);
    let content = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            warn!("data table missing: {}", path.display());
            return HashMap::new();
        }
    };
    match serde_json::from_str(&content) {
        Ok(map) => map,
        Err(e) => {
            warn!("data table parse error [{filename}]: {e}");
            HashMap::new()
        }
    }
}

// ── Mod Tables ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "ModType", default)]
    pub mod_type: i32,
    #[serde(rename = "EffectLibId", default)]
    pub effect_lib_ids: Vec<i32>,
    #[serde(rename = "IsCanLink", default)]
    pub is_can_link: bool,
    #[serde(rename = "SimilarId", default)]
    pub similar_id: i32,
    #[serde(rename = "InitializationId", default)]
    pub initialization_id: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModEffectEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "EffectID", default)]
    pub effect_id: i32,
    #[serde(rename = "EffectName", default)]
    pub effect_name: String,
    #[serde(rename = "Level", default)]
    pub level: i32,
    #[serde(rename = "EffectType", default)]
    pub effect_type: i32,
    #[serde(rename = "IsNegative", default)]
    pub is_negative: bool,
    #[serde(rename = "FightValue", default)]
    pub fight_value: i32,
}

// ── Skill Overrides ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillOverrideEntry {
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Icon", default)]
    pub icon: String,
}

// ── Skill Table ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SkillEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "NameDesign", default)]
    pub name_design: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Icon", default)]
    pub icon: String,
    #[serde(rename = "CoolTime", default)]
    pub cool_time: f32,
    #[serde(rename = "SkillType", default)]
    pub skill_type: i32,
    /// Slot positions this skill can occupy in the loadout bar. Positions 7/8 = Imagines.
    #[serde(rename = "SlotPositionId", default)]
    pub slot_position_id: Vec<i32>,
}

// ── Equip / Item Tables ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EquipEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "EquipPart", default)]
    pub equip_part: i32,
    #[serde(rename = "EquipGs", default)]
    pub equip_gs: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ItemEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "Icon", default)]
    pub icon: String,
    #[serde(rename = "Quality", default)]
    pub quality: i32,
}

// ── Monster Table ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct MonsterEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "MonsterType", default)]
    pub monster_type: i32,
    // MonsterRank is "" or a string in JSON — not an integer
    #[serde(rename = "MonsterRank", default)]
    pub monster_rank: String,
}

// ── Dummy Table ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DummyEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name", default)]
    pub name: String,
}

// ── Scene Table ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SceneEntry {
    #[serde(rename = "Id")]
    pub id: String,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "SceneType", default)]
    pub scene_type: i32,
}

// ── DataTables ───────────────────────────────────────────────────────────────

pub struct DataTables {
    pub mods: HashMap<String, ModEntry>,
    pub mod_effects: HashMap<String, ModEffectEntry>,
    pub skills: HashMap<String, SkillEntry>,
    pub skill_overrides: HashMap<String, SkillOverrideEntry>,
    pub monsters: HashMap<String, MonsterEntry>,
    pub dummies: HashMap<String, DummyEntry>,
    pub scenes: HashMap<String, SceneEntry>,
    pub equips: HashMap<String, EquipEntry>,
    pub items: HashMap<String, ItemEntry>,
    /// AppStrings.en.json — flat key→value localization map
    pub strings: HashMap<String, String>,
}

impl DataTables {
    pub fn skill_name(&self, id: u32) -> Option<&str> {
        // English override takes priority over the base table
        if let Some(ov) = self.skill_overrides.get(&id.to_string()) {
            if !ov.name.is_empty() {
                return Some(ov.name.as_str());
            }
        }
        let s = self.skills.get(&id.to_string())?;
        let name = if !s.name_design.is_empty() { &s.name_design } else { &s.name };
        if name.is_empty() { None } else { Some(name.as_str()) }
    }

    pub fn skill_cooldown_secs(&self, id: u32) -> Option<f32> {
        let s = self.skills.get(&id.to_string())?;
        if s.cool_time > 0.0 { Some(s.cool_time / 1000.0) } else { None }
    }

    pub fn monster_name(&self, id: u32) -> Option<&str> {
        let m = self.monsters.get(&id.to_string())?;
        if m.name.is_empty() { None } else { Some(m.name.as_str()) }
    }

    pub fn monster_type(&self, id: u32) -> Option<i32> {
        self.monsters.get(&id.to_string()).map(|m| m.monster_type)
    }

    pub fn dummy_name(&self, id: u32) -> Option<&str> {
        let d = self.dummies.get(&id.to_string())?;
        if d.name.is_empty() { None } else { Some(d.name.as_str()) }
    }

    pub fn scene_name(&self, id: &str) -> Option<&str> {
        let s = self.scenes.get(id)?;
        if s.name.is_empty() { None } else { Some(s.name.as_str()) }
    }

    pub fn mod_name(&self, id: i32) -> Option<&str> {
        let m = self.mods.get(&id.to_string())?;
        if m.name.is_empty() { None } else { Some(m.name.as_str()) }
    }

    pub fn effect_name(&self, id: i32) -> Option<&str> {
        let e = self.mod_effects.get(&id.to_string())?;
        if e.effect_name.is_empty() { None } else { Some(e.effect_name.as_str()) }
    }

    pub fn item_name(&self, id: i32) -> Option<&str> {
        let it = self.items.get(&id.to_string())?;
        if it.name.is_empty() { None } else { Some(it.name.as_str()) }
    }

    pub fn item_icon(&self, id: i32) -> Option<&str> {
        let it = self.items.get(&id.to_string())?;
        if it.icon.is_empty() { None } else { Some(it.icon.as_str()) }
    }

    pub fn equip_part(&self, id: i32) -> Option<i32> {
        self.equips.get(&id.to_string()).map(|e| e.equip_part)
    }

    /// True if the skill occupies an "Imagine" slot (positions 7/8 in the loadout bar).
    pub fn skill_is_imagine(&self, id: u32) -> bool {
        self.skills.get(&id.to_string())
            .map(|s| s.slot_position_id.iter().any(|&p| p == 7 || p == 8))
            .unwrap_or(false)
    }

    pub fn string<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map(|s| s.as_str()).unwrap_or(key)
    }
}

pub static DATA: Lazy<DataTables> = Lazy::new(|| {
    tracing::info!("loading data tables from {}", data_dir().display());
    DataTables {
        mods:            load_table("ModTable.json"),
        mod_effects:     load_table("ModEffectTable.json"),
        skills:          load_table("SkillTable.json"),
        skill_overrides: load_table("SkillOverrides.en.json"),
        monsters:        load_table("MonsterTable.json"),
        dummies:         load_table("DummyTable.json"),
        scenes:          load_table("SceneTable.json"),
        equips:          load_table("EquipTable.json"),
        items:           load_table("ItemTable.json"),
        strings:         load_flat("AppStrings.en.json"),
    }
});
