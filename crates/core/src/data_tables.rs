use once_cell::sync::Lazy;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::warn;

pub fn data_dir() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe.parent().unwrap_or(exe.as_path());

        // Packaged/installed layout: data/ shipped next to the executable.
        let next_to_exe = exe_dir.join("data");
        if next_to_exe.exists() {
            return next_to_exe;
        }

        // `cargo build` layout: exe sits in target/{debug,release}/, data/ is
        // at the repo root two levels up — covers running the binary by
        // absolute/relative path from anywhere (e.g. `/path/to/target/release/…`
        // without `cd`-ing into the repo root first), not just the CWD-relative
        // fallback below.
        if let Some(repo_root) = exe_dir.parent().and_then(|p| p.parent()) {
            let two_up = repo_root.join("data");
            if two_up.exists() {
                return two_up;
            }
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
    #[serde(rename = "EffectConfigIcon", default)]
    pub effect_config_icon: String,
    #[serde(rename = "EnhancementNum", default)]
    pub enhancement_num: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct ModLinkEffectEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "LinkTime", default)]
    pub link_time: i32,
    #[serde(rename = "LinkLevelEffect", default)]
    pub link_level_effect: Vec<Vec<i32>>,
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
    #[serde(rename = "BasicAttrLibId", default)]
    pub basic_attr_lib_id: Vec<i32>,
    #[serde(rename = "AdvancedAttrLibId", default)]
    pub advanced_attr_lib_id: Vec<i32>,
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

// ── Gear Attribute Roll Tables ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EquipAttrLibEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "AttrLibId", default)]
    pub attr_lib_id: i32,
    #[serde(rename = "AttrEffect", default)]
    pub attr_effect: Vec<Vec<i32>>,
    #[serde(rename = "AttrEffectConfig", default)]
    pub attr_effect_config: Vec<Vec<i32>>,
    #[serde(rename = "AllowPart", default)]
    pub allow_part: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EquipAttrSchoolLibEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "AttrLibId", default)]
    pub attr_lib_id: i32,
    #[serde(rename = "AttrEffect", default)]
    pub attr_effect: Vec<Vec<i32>>,
    #[serde(rename = "AttrEffectConfig", default)]
    pub attr_effect_config: Vec<Vec<i32>>,
    #[serde(rename = "AllowPart", default)]
    pub allow_part: Vec<i32>,
    #[serde(rename = "TalentSchoolId", default)]
    pub talent_school_id: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct EquipBreakThroughEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "EquipId", default)]
    pub equip_id: i32,
    #[serde(rename = "BreakThroughTime", default)]
    pub break_through_time: i32,
    #[serde(rename = "EquipGs", default)]
    pub equip_gs: i32,
    #[serde(rename = "BasicAttrLibId", default)]
    pub basic_attr_lib_id: Vec<i32>,
    #[serde(rename = "AdvancedAttrLibId", default)]
    pub advanced_attr_lib_id: Vec<i32>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FightAttrEntry {
    #[serde(rename = "AttrAdd", default)]
    pub attr_add: i32,
    #[serde(rename = "OfficialName", default)]
    pub official_name: String,
    #[serde(rename = "AttrNumType", default)]
    pub attr_num_type: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AttrDescriptionEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Description", default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct BuffEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Desc", default)]
    pub desc: String,
    #[serde(rename = "TipsDescription", default)]
    pub tips_description: i32,
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

// ── Dungeons Table ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DungeonEntry {
    #[serde(rename = "Id")]
    pub id: i32,
    #[serde(rename = "Name", default)]
    pub name: String,
    #[serde(rename = "SceneID", default)]
    pub scene_id: i32,
}

// ── Gear Slot Naming ────────────────────────────────────────────────────────

/// Wire slot id (`EquipSlot.slot` / `SavedGearSlot.slot`, 200-210) → display name.
/// Fixed game enum, same mapping BPSR-ZDPS hardcodes in its `GearInspector.cs`.
pub fn gear_slot_name(slot: i32) -> String {
    match slot {
        200 => "Weapon".to_string(),
        201 => "Head".to_string(),
        202 => "Chest".to_string(),
        203 => "Hands".to_string(),
        204 => "Boots".to_string(),
        205 => "Earring".to_string(),
        206 => "Necklace".to_string(),
        207 => "Ring".to_string(),
        208 => "Bracelet (L)".to_string(),
        209 => "Bracelet (R)".to_string(),
        210 => "Charm".to_string(),
        _   => format!("Slot {slot}"),
    }
}

/// Module quality tier (`DataTables::mod_quality_tier` return value, 0-3) -> display label.
pub fn mod_quality_label(tier: u8) -> &'static str {
    match tier {
        0 => "Basic",
        1 => "Advanced",
        2 => "Excellent",
        3 => "EXT",
        _ => "Unknown",
    }
}

// ── Gear Attribute Rolls ────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct AttrLine {
    pub name:       String,
    pub min:        i32,
    pub max:        i32,
    pub is_percent: bool,
}

#[derive(Debug, Clone)]
pub struct GearBreakthroughOption {
    pub level:    i32,
    pub equip_gs: i32,
    pub basic:    Vec<AttrLine>,
    pub advanced: Vec<AttrLine>,
}

#[derive(Debug, Clone)]
pub struct GearAttrRoll {
    pub equip_gs:     i32,
    pub basic:        Vec<AttrLine>,
    pub advanced:     Vec<AttrLine>,
    pub breakthroughs: Vec<GearBreakthroughOption>,
}

/// Resolves an `AttrDescriptionEntry.description` template against `[min, max]`.
/// Placeholder syntax observed in the data: `{*Decision.marknormal(N)*}` /
/// `{*Decision.unmarknormal(N)*}`, where `N` is a 1-based index into `[min, max]`.
/// Also strips the `<style="...">...</style>` / `<br>` markup down to plain text
/// since this renders as plain egui text, not HTML.
fn resolve_attr_template(desc: &str, min: i32, max: i32) -> String {
    let values = [min, max];
    let mut out = String::with_capacity(desc.len());
    let mut rest = desc;
    while let Some(start) = rest.find("{*Decision.") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        match after.find("*}") {
            Some(end) => {
                let token = &after[..end + 2];
                if let (Some(p1), Some(p2)) = (token.find('('), token.find(')')) {
                    if let Ok(idx) = token[p1 + 1..p2].trim().parse::<usize>() {
                        if idx >= 1 {
                            if let Some(v) = values.get(idx - 1) {
                                out.push_str(&v.to_string());
                            }
                        }
                    }
                }
                rest = &after[end + 2..];
            }
            None => {
                out.push_str(after);
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    strip_markup(&out.replace("<br>", " "))
}

fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '<' {
            for c2 in chars.by_ref() {
                if c2 == '>' { break; }
            }
            continue;
        }
        out.push(c);
    }
    out
}

// ── DataTables ───────────────────────────────────────────────────────────────

pub struct DataTables {
    pub mods: HashMap<String, ModEntry>,
    pub mod_effects: HashMap<String, ModEffectEntry>,
    pub mod_link_effects: HashMap<String, ModLinkEffectEntry>,
    pub skills: HashMap<String, SkillEntry>,
    pub skill_overrides: HashMap<String, SkillOverrideEntry>,
    pub monsters: HashMap<String, MonsterEntry>,
    pub dummies: HashMap<String, DummyEntry>,
    pub scenes: HashMap<String, SceneEntry>,
    pub equips: HashMap<String, EquipEntry>,
    pub items: HashMap<String, ItemEntry>,
    pub dungeons: HashMap<String, DungeonEntry>,
    pub equip_attr_libs: HashMap<String, EquipAttrLibEntry>,
    pub equip_attr_school_libs: HashMap<String, EquipAttrSchoolLibEntry>,
    pub equip_break_throughs: HashMap<String, EquipBreakThroughEntry>,
    pub fight_attrs: HashMap<String, FightAttrEntry>,
    pub attr_descriptions: HashMap<String, AttrDescriptionEntry>,
    pub buffs: HashMap<String, BuffEntry>,
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
        // `Name` is the already-translated display name (English when available);
        // `NameDesign` is the internal/design label (almost always Chinese) — prefer
        // Name, falling back to NameDesign only when Name is genuinely absent.
        let name = if !s.name.is_empty() { &s.name } else { &s.name_design };
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

    /// Dungeon/instance display name, keyed by scene id — kept current via
    /// data extraction (DungeonsTable.json), unlike the hardcoded overworld
    /// zone list in capture::proto::opcode::zone_name().
    pub fn dungeon_name(&self, id: &str) -> Option<&str> {
        let d = self.dungeons.get(id)?;
        if d.name.is_empty() { None } else { Some(d.name.as_str()) }
    }

    pub fn mod_name(&self, id: i32) -> Option<&str> {
        let m = self.mods.get(&id.to_string())?;
        if m.name.is_empty() { None } else { Some(m.name.as_str()) }
    }

    /// `id` here is the module stat's `EffectID` (e.g. 1110), not the `ModEffectEntry`
    /// row `Id` — `mod_effects` is keyed by row `Id` (unique per enhancement breakpoint),
    /// so this scans `.values()` for the first row with a matching `EffectID`, same as
    /// `effect_icon`/`effect_fight_value`.
    pub fn effect_name(&self, id: i32) -> Option<&str> {
        let e = self.mod_effects.values().find(|e| e.effect_id == id)?;
        if e.effect_name.is_empty() { None } else { Some(e.effect_name.as_str()) }
    }

    /// 0=Basic, 1=Advanced, 2=Excellent, 3=EXT — derived from `ModEntry.initialization_id`
    /// (literally 101..104 in the data, not a longer number ending in those digits).
    pub fn mod_quality_tier(&self, config_id: i32) -> Option<u8> {
        let m = self.mods.get(&config_id.to_string())?;
        match m.initialization_id {
            101 => Some(0),
            102 => Some(1),
            103 => Some(2),
            104 => Some(3),
            _ => None,
        }
    }

    pub fn mod_type(&self, config_id: i32) -> Option<i32> {
        self.mods.get(&config_id.to_string()).map(|m| m.mod_type)
    }

    /// Every tier-3 (EXT) row in `ModTable.json` is literally named
    /// "EXT - Excellent Guard Module" regardless of its actual `ModType` (a
    /// copy-paste bug in the game's own data) — reconstruct the display name
    /// from `mod_type` for tier 3 instead of trusting `Name` verbatim.
    pub fn mod_display_name(&self, config_id: i32) -> Option<String> {
        let m = self.mods.get(&config_id.to_string())?;
        if m.initialization_id == 104 {
            let kind = match m.mod_type {
                1 => "Attack",
                2 => "Support",
                3 => "Guard",
                _ => "Unknown",
            };
            Some(format!("EXT - Excellent {kind} Module"))
        } else if !m.name.is_empty() {
            Some(m.name.clone())
        } else {
            None
        }
    }

    /// Module-type-tier icon filename (no extension), e.g. "item_mod_device_attack3".
    /// Returns `None` for tier 0 (Basic) — no dedicated icon asset exists for that tier.
    pub fn mod_type_icon_name(&self, mod_type: i32, tier: u8) -> Option<String> {
        if tier == 0 { return None; }
        let n = tier + 1; // 1(Advanced)->2, 2(Excellent)->3, 3(EXT)->4
        match mod_type {
            1 => Some(format!("item_mod_device_attack{n}")),
            2 => Some(format!("item_mod_device_{n}")),
            3 => Some(format!("item_mod_device_protect{n}")),
            _ => None,
        }
    }

    /// Stat-effect icon path, e.g. "ui/atlas/mod_effect/mod_effect_icon_011" — identical
    /// across every enhancement-breakpoint row of a given effect id, so the first match wins.
    /// `mod_effects` is keyed by row `Id` (unique per breakpoint), not `EffectID`, hence the scan.
    pub fn effect_icon(&self, effect_id: i32) -> Option<&str> {
        self.mod_effects.values()
            .find(|e| e.effect_id == effect_id && !e.effect_config_icon.is_empty())
            .map(|e| e.effect_config_icon.as_str())
    }

    /// Exact-match (effect_id, enhancement_num) -> FightValue, used for the Ability Score calc.
    pub fn effect_fight_value(&self, effect_id: i32, enhancement_num: i32) -> i32 {
        self.mod_effects.values()
            .find(|e| e.effect_id == effect_id && e.enhancement_num == enhancement_num)
            .map(|e| e.fight_value)
            .unwrap_or(0)
    }

    /// Total-link-level bonus FightValue — `ModLinkEffectTable.json` is keyed by `Id`,
    /// where `Id == LinkTime + 1` for every row.
    pub fn link_level_bonus_fight_value(&self, total_link_time: i32) -> i32 {
        self.mod_link_effects.get(&(total_link_time + 1).to_string())
            .map(|e| e.fight_value)
            .unwrap_or(0)
    }

    pub fn item_name(&self, id: i32) -> Option<&str> {
        let it = self.items.get(&id.to_string())?;
        if it.name.is_empty() { None } else { Some(it.name.as_str()) }
    }

    pub fn item_icon(&self, id: i32) -> Option<&str> {
        let it = self.items.get(&id.to_string())?;
        if it.icon.is_empty() { None } else { Some(it.icon.as_str()) }
    }

    pub fn item_quality(&self, id: i32) -> Option<i32> {
        self.items.get(&id.to_string()).map(|it| it.quality)
    }

    pub fn skill_icon(&self, id: u32) -> Option<&str> {
        let s = self.skills.get(&id.to_string())?;
        if s.icon.is_empty() { None } else { Some(s.icon.as_str()) }
    }

    pub fn equip_part(&self, id: i32) -> Option<i32> {
        self.equips.get(&id.to_string()).map(|e| e.equip_part)
    }

    /// True if the skill occupies an "Imagine" slot (positions 7/8 in the loadout bar).
    pub fn skill_is_imagine(&self, id: u32) -> bool {
        self.skills.get(&id.to_string())
            .map(|s| {
                s.slot_position_id.iter().any(|&p| p == 7 || p == 8)
                    // Some talent/passive skills also list slot positions 7/8 in this
                    // data but aren't companion Imagines — exclude them by icon path.
                    && !s.icon.starts_with("ui/textures/talent_passive/")
            })
            .unwrap_or(false)
    }

    /// Basic/Advanced attribute rolls + Breakthrough options for an equipped item,
    /// ported from BPSR-ZDPS's GearInspector.ResolveAttrsForEquip.
    pub fn gear_attr_roll(&self, item_id: i32) -> Option<GearAttrRoll> {
        let equip = self.equips.get(&item_id.to_string())?;
        let basic = self.resolve_attr_lib_ids(&equip.basic_attr_lib_id, equip.equip_part);
        let advanced = self.resolve_attr_lib_ids(&equip.advanced_attr_lib_id, equip.equip_part);

        let mut breakthroughs: Vec<GearBreakthroughOption> = self.equip_break_throughs.values()
            .filter(|bt| bt.equip_id == item_id)
            .map(|bt| GearBreakthroughOption {
                level:    bt.break_through_time,
                equip_gs: bt.equip_gs,
                basic:    self.resolve_attr_lib_ids(&bt.basic_attr_lib_id, equip.equip_part),
                advanced: self.resolve_attr_lib_ids(&bt.advanced_attr_lib_id, equip.equip_part),
            })
            .collect();
        breakthroughs.sort_by_key(|bt| bt.level);

        Some(GearAttrRoll { equip_gs: equip.equip_gs, basic, advanced, breakthroughs })
    }

    /// `lib_ids[0]` is a version tag (1 = EquipAttrLibTable, 2 = EquipAttrSchoolLibTable),
    /// the rest are attr-lib ids to resolve against `equip_part`. No player spec/talent
    /// tracking exists in this app, so — matching ZDPS's own fallback for an unknown
    /// spec — the first `allow_part`-matching row is used regardless of TalentSchoolId.
    fn resolve_attr_lib_ids(&self, lib_ids: &[i32], equip_part: i32) -> Vec<AttrLine> {
        if lib_ids.len() < 2 { return Vec::new(); }
        let lib_version = lib_ids[0];
        let mut out = Vec::new();
        for &attr in &lib_ids[1..] {
            if lib_version == 1 {
                if let Some(lib) = self.equip_attr_libs.values()
                    .find(|l| l.attr_lib_id == attr && l.allow_part.contains(&equip_part))
                {
                    self.append_attr_lines(&lib.attr_effect, &lib.attr_effect_config, &mut out);
                }
            } else if lib_version == 2 {
                if let Some(lib) = self.equip_attr_school_libs.values()
                    .find(|l| l.attr_lib_id == attr && l.allow_part.contains(&equip_part))
                {
                    self.append_attr_lines(&lib.attr_effect, &lib.attr_effect_config, &mut out);
                }
            }
        }
        out
    }

    fn append_attr_lines(&self, effects: &[Vec<i32>], configs: &[Vec<i32>], out: &mut Vec<AttrLine>) {
        for (idx, effect) in effects.iter().enumerate() {
            let Some(&attr_type) = effect.first() else { continue };
            let Some(&kind_id) = effect.get(1) else { continue };
            let (min, max) = configs.get(idx)
                .map(|c| (c.first().copied().unwrap_or(0), c.get(1).copied().unwrap_or(0)))
                .unwrap_or((0, 0));

            if attr_type == 1 {
                // Raw stat effect — look up the human-readable name + percent formatting.
                if let Some(fa) = self.fight_attrs.values().find(|f| f.attr_add == kind_id) {
                    out.push(AttrLine {
                        name:       fa.official_name.clone(),
                        min, max,
                        is_percent: fa.attr_num_type == 1,
                    });
                }
            } else if attr_type == 3 {
                // Buff-based special effect — resolve via its templated description when available.
                if let Some(buff) = self.buffs.get(&kind_id.to_string()) {
                    let name = if buff.tips_description > 0 {
                        self.attr_descriptions.get(&buff.tips_description.to_string())
                            .map(|d| resolve_attr_template(&d.description, min, max))
                            .unwrap_or_else(|| buff.desc.clone())
                    } else {
                        buff.desc.clone()
                    };
                    out.push(AttrLine { name, min, max, is_percent: false });
                }
            }
        }
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
        mod_link_effects: load_table("ModLinkEffectTable.json"),
        skills:          load_table("SkillTable.json"),
        skill_overrides: load_table("SkillOverrides.en.json"),
        monsters:        load_table("MonsterTable.json"),
        dummies:         load_table("DummyTable.json"),
        scenes:          load_table("SceneTable.json"),
        equips:          load_table("EquipTable.json"),
        items:           load_table("ItemTable.json"),
        dungeons:        load_table("DungeonsTable.json"),
        equip_attr_libs:        load_table("EquipAttrLibTable.json"),
        equip_attr_school_libs: load_table("EquipAttrSchoolLibTable.json"),
        equip_break_throughs:   load_table("EquipBreakThroughTable.json"),
        fight_attrs:            load_table("FightAttrTable.json"),
        attr_descriptions:      load_table("AttrDescription.json"),
        buffs:                  load_table("BuffTable.json"),
        strings:         load_flat("AppStrings.en.json"),
    }
});
