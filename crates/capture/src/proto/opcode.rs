use crate::proto::packets::pb;
use bpsr_core::types::EntityId;
use game::entity::CharStats;
use game::event::{
    ChatChannel, CombatEvent, DungeonStateKind, DungeonTargetProgress, EquipSlot, GameEvent,
    MatchAlertKind, ModuleEffect, PlayerModule, ShieldEntry, SkillLoadoutEntry,
};
use prost::Message;
use std::time::Instant;
use tracing::debug;

// Service UUID for World/Activity notifications
pub const SERVICE_UUID: u64 = 0x63335342;

pub const METHOD_SYNC_NEAR_ENTITIES:   u32 = 0x00000006;
pub const METHOD_SYNC_CONTAINER_DATA:  u32 = 0x00000015;
pub const METHOD_SYNC_NEAR_DELTA_INFO: u32 = 0x0000002D;
pub const METHOD_SYNC_TO_ME_DELTA:     u32 = 0x0000002E;
pub const METHOD_SYNC_DUNGEON_DATA:    u32 = 0x00000017;
pub const METHOD_SYNC_DUNGEON_DIRTY_DATA: u32 = 0x00000018;

// Social notification service (scene/zone data)
pub const SOCIAL_NTF_SERVICE_UUID: u64 = 0x254C89A3;
pub const SOCIAL_NTF_METHOD_ID:    u32 = 1;

// Chat service
pub const CHIT_CHAT_NTF_UUID:       u64 = 164_931_432;
pub const METHOD_NOTIFY_NEWEST_MSGS: u32 = 0x01;

// Matchmaking service
pub const MATCH_NTF_UUID:              u64 = 822_849_903;
pub const METHOD_ENTER_MATCH_RESULT:   u32 = 0x04;
pub const METHOD_MATCH_READY_STATUS:   u32 = 0x06;

/// Dispatch a Notify payload to game events.
pub fn dispatch(service_uuid: u64, method_id: u32, data: &[u8]) -> Vec<GameEvent> {
    if service_uuid == SOCIAL_NTF_SERVICE_UUID && method_id == SOCIAL_NTF_METHOD_ID {
        return match pb::NotifySocialData::decode(data) {
            Ok(msg) => parse_notify_social_data(msg),
            Err(e)  => { debug!("NotifySocialData decode failed: {e}"); vec![] }
        };
    }

    if service_uuid == CHIT_CHAT_NTF_UUID {
        if method_id == METHOD_NOTIFY_NEWEST_MSGS {
            return match pb::NotifyNewestChitChatMsgs::decode(data) {
                Ok(msg) => parse_chat(msg),
                Err(e)  => { debug!("NotifyNewestChitChatMsgs decode failed: {e}"); vec![] }
            };
        }
        return vec![];
    }

    if service_uuid == MATCH_NTF_UUID {
        return match method_id {
            METHOD_ENTER_MATCH_RESULT => {
                match pb::EnterMatchResultNtf::decode(data) {
                    Ok(msg) => parse_enter_match_result(msg),
                    Err(e)  => { debug!("EnterMatchResultNtf decode failed: {e}"); vec![] }
                }
            }
            METHOD_MATCH_READY_STATUS => {
                match pb::MatchReadyStatusNtf::decode(data) {
                    Ok(_)  => vec![GameEvent::MatchmakingAlert { kind: MatchAlertKind::ReadyCheck }],
                    Err(e) => { debug!("MatchReadyStatusNtf decode failed: {e}"); vec![] }
                }
            }
            _ => vec![],
        };
    }

    if service_uuid != SERVICE_UUID {
        crate::proto::packets::unknown::log_unknown_service(service_uuid, method_id, data.len());
        return vec![];
    }

    match method_id {
        METHOD_SYNC_NEAR_ENTITIES => {
            match pb::SyncNearEntities::decode(data) {
                Ok(msg) => parse_sync_near_entities(msg),
                Err(e) => { debug!("SyncNearEntities decode failed: {e}"); vec![] }
            }
        }
        METHOD_SYNC_CONTAINER_DATA => {
            match pb::SyncContainerData::decode(data) {
                Ok(msg) => parse_sync_container_data(msg),
                Err(e) => { debug!("SyncContainerData decode failed: {e}"); vec![] }
            }
        }
        METHOD_SYNC_NEAR_DELTA_INFO => {
            match pb::SyncNearDeltaInfo::decode(data) {
                Ok(msg) => msg.delta_infos.into_iter().flat_map(parse_aoi_sync_delta).collect(),
                Err(e) => { debug!("SyncNearDeltaInfo decode failed: {e}"); vec![] }
            }
        }
        METHOD_SYNC_TO_ME_DELTA => {
            match pb::SyncToMeDeltaInfo::decode(data) {
                Ok(msg) => {
                    let mut events = Vec::new();
                    if let Some(delta_info) = msg.delta_info {
                        let outer_uuid = delta_info.uuid;
                        if outer_uuid != 0 {
                            events.push(GameEvent::LocalPlayer {
                                id: EntityId((outer_uuid >> 16) as u64),
                            });
                        }
                        if let Some(mut base) = delta_info.base_delta {
                            // base.uuid may be 0 in SyncToMeDeltaInfo; use outer uuid
                            if base.uuid == 0 { base.uuid = outer_uuid; }
                            // Own player — always parse as player regardless of ent_type bits
                            events.extend(parse_aoi_sync_delta_as_player(base));
                        }
                    }
                    events
                }
                Err(e) => { debug!("SyncToMeDeltaInfo decode failed: {e}"); vec![] }
            }
        }
        METHOD_SYNC_DUNGEON_DATA => {
            match pb::SyncDungeonData::decode(data) {
                Ok(msg) => parse_sync_dungeon_data(msg),
                Err(e) => { debug!("SyncDungeonData decode failed: {e}"); vec![] }
            }
        }
        METHOD_SYNC_DUNGEON_DIRTY_DATA => {
            match pb::SyncDungeonDirtyData::decode(data) {
                Ok(msg) => parse_sync_dungeon_dirty_data(msg),
                Err(e) => { debug!("SyncDungeonDirtyData decode failed: {e}"); vec![] }
            }
        }
        _ => {
            crate::proto::packets::unknown::log_unknown(service_uuid, method_id, data);
            vec![]
        }
    }
}

fn parse_sync_near_entities(msg: pb::SyncNearEntities) -> Vec<GameEvent> {
    let mut events = Vec::new();
    for entity in msg.appear {
        if entity.uuid == 0 { continue; }
        let entity_id = EntityId((entity.uuid >> 16) as u64);

        // ent_type 10 = player, 1 = monster, 11 = dummy
        if entity.ent_type == 10 {
            if let Some(attrs) = entity.attrs {
                events.extend(parse_player_attrs(entity_id, attrs.attrs));
            }
        } else if entity.ent_type == 1 || entity.ent_type == 11 {
            if let Some(attrs) = entity.attrs {
                events.extend(parse_npc_attrs(entity_id, entity.ent_type, attrs.attrs));
            }
        }
    }
    for gone in msg.disappear {
        if gone.uuid == 0 { continue; }
        events.push(GameEvent::EntityDespawn {
            id: EntityId((gone.uuid >> 16) as u64),
        });
    }
    events
}

fn parse_notify_social_data(msg: pb::NotifySocialData) -> Vec<GameEvent> {
    let scene = msg.v_request
        .and_then(|r| r.data)
        .and_then(|s| s.scene_data);
    if let Some(s) = scene {
        if s.level_map_id != 0 {
            let name = bpsr_core::DATA.dungeon_name(&s.level_map_id.to_string())
                .or_else(|| zone_name(s.level_map_id))
                .map(|n| n.to_string())
                .unwrap_or_else(|| format!("Zone {}", s.level_map_id));
            return vec![GameEvent::ZoneChange {
                zone_id:   s.level_map_id,
                zone_name: name,
            }];
        }
    }
    vec![]
}

fn zone_name(id: u32) -> Option<&'static str> {
    static ZONES: &[(u32, &str)] = &[
        (7, "Asteria Plains"),
        (8, "Asterleeds"),
        (9, "Bahamar Highlands"),
        (10, "Valley"),
        (11, "Starland"),
        (71, "Duskdye Woods"),
        (72, "Everfall Forest"),
        (73, "Windhowl Canyon"),
        (74, "Underground District"),
        (75, "Skimmer's Lair"),
        (76, "Land of Crimson Illusion"),
        (91, "Sunken Corridor"),
        (92, "Gloomy Depths"),
        (93, "Moonshadow Wilds"),
        (94, "Stray Starway"),
        (95, "Sunset Wilds"),
        (1001, "Tina's Mindrealm"),
        (1002, "Tina's Mindrealm"),
        (1011, "Unstable - Tina's Mindrealm"),
        (1021, "Unstable - Tina's Mindrealm"),
        (1031, "Chaotic - Tina's Mindrealm"),
        (1032, "Chaotic - Tina's Mindrealm"),
        (1033, "Chaotic - Tina's Mindrealm"),
        (1101, "Towering Ruin"),
        (1102, "Towering Ruin"),
        (1111, "Unstable - Towering Ruin"),
        (1112, "Unstable - Towering Ruin"),
        (1121, "Chaotic - Towering Ruin"),
        (1122, "Chaotic - Towering Ruin"),
        (1123, "Chaotic - Towering Ruin"),
        (1201, "Dragon Claw Valley"),
        (1202, "Dragon Claw Valley"),
        (1211, "Unstable - Dragon Claw Valley"),
        (1212, "Unstable - Dragon Claw Valley"),
        (1221, "Chaotic - Dragon Claw Valley"),
        (1222, "Chaotic - Dragon Claw Valley"),
        (1223, "Chaotic - Dragon Claw Valley"),
        (1301, "Dark Mist Fortress"),
        (1302, "Dark Mist Fortress"),
        (1311, "Unstable - Dark Mist Fortress"),
        (1321, "Unstable - Dark Mist Fortress"),
        (1331, "Chaotic - Dark Mist Fortress"),
        (1332, "Chaotic - Dark Mist Fortress"),
        (1333, "Chaotic - Dark Mist Fortress"),
        (1401, "Illusory Moonveil Wilds"),
        (1402, "Illusory Moonveil Wilds"),
        (1403, "Illusory Moonveil Wilds"),
        (1411, "Unstable - Illusory Moonveil Wilds"),
        (1412, "Unstable - Illusory Moonveil Wilds"),
        (1421, "Chaotic - Illusory Moonveil Wilds"),
        (1422, "Chaotic - Illusory Moonveil Wilds"),
        (1423, "Chaotic - Illusory Moonveil Wilds"),
        (1501, "Rock Serpent Lair"),
        (1502, "Rock Serpent Lair"),
        (1511, "Unstable - Rock Serpent Lair"),
        (1521, "Unstable - Rock Serpent Lair"),
        (1531, "Chaotic - Rock Serpent Lair"),
        (1532, "Chaotic - Rock Serpent Lair"),
        (1533, "Chaotic - Rock Serpent Lair"),
        (5002, "Sea of Origin"),
        (5033, "Tower Ruins"),
        (5200, "Lamond's Office"),
        (5206, "Asterleeds"),
        (5211, "Lamond's Office"),
        (5220, "Sea Cruise"),
        (5401, "Kanamia Forest"),
        (5403, "Kanamia Brave Trial"),
        (5501, "Dragon Claw Valley"),
        (5602, "Fafala's Home"),
        (5603, "Fafala's Home"),
        (5811, "Smelting Furnace Pipes"),
        (5812, "Frostjade Staff"),
        (6005, "Unstable - Goblin Lair"),
        (6006, "Unstable - Goblin Lair"),
        (6007, "Chaotic - Goblin Lair"),
        (6008, "Chaotic - Goblin Lair"),
        (6009, "Chaotic - Goblin Lair"),
        (6011, "Unstable - Kanamia Trial"),
        (6012, "Unstable - Kanamia Trial"),
        (6021, "Chaotic - Kanamia Trial"),
        (6022, "Chaotic - Kanamia Trial"),
        (6023, "Chaotic - Kanamia Trial"),
        (6031, "Unstable - Kanamia Trial"),
        (6032, "Unstable - Kanamia Trial"),
        (6040, "Verdant Embrace of Soundless City"),
        (6041, "Unstable - Soundless City"),
        (6042, "Unstable - Soundless City"),
        (6043, "Chaotic - Soundless City"),
        (6044, "Chaotic - Soundless City"),
        (6045, "Chaotic - Soundless City"),
        (6111, "Unstable - Wraith Shrine"),
        (6112, "Unstable - Wraith Shrine"),
        (6121, "Chaotic - Wraith Shrine"),
        (6122, "Chaotic - Wraith Shrine"),
        (6123, "Chaotic - Wraith Shrine"),
        (6211, "Unstable - Depths of Decay"),
        (6212, "Unstable - Depths of Decay"),
        (6221, "Chaotic - Depths of Decay"),
        (6222, "Chaotic - Depths of Decay"),
        (6223, "Chaotic - Depths of Decay"),
        (6311, "Unstable - Devourer Cage"),
        (6321, "Unstable - Devourer Cage"),
        (6331, "Chaotic - Devourer Cage"),
        (6332, "Chaotic - Devourer Cage"),
        (6333, "Chaotic - Devourer Cage"),
        (6411, "Unstable - Soundless City"),
        (6412, "Unstable - Soundless City"),
        (6421, "Chaotic - Soundless City"),
        (6422, "Chaotic - Soundless City"),
        (6423, "Chaotic - Soundless City"),
        (7001, "Parkour"),
        (7002, "Parkour"),
        (7003, "Wasteland Race"),
        (7004, "City Rally"),
        (7050, "Goblin Rampage"),
        (7051, "Goblin Rampage"),
        (7052, "Goblin Rampage"),
        (7053, "Goblin Rampage"),
        (7054, "Goblin Rampage"),
        (7055, "Defeat the Goblin Mage"),
        (7056, "Defeat the Goblin Mage"),
        (7057, "Defeat the Goblin Mage"),
        (7058, "Defeat the Goblin Mage"),
        (7060, "Canyon Battle"),
        (7061, "Canyon Battle"),
        (7065, "Urgent Delivery"),
        (7070, "Light Bridge"),
        (7071, "Light Bridge"),
        (7072, "Light Bridge"),
        (7073, "Light Bridge"),
        (7075, "Demi-Human Q&A"),
        (7076, "Demi-Human Q&A"),
        (7077, "Demi-Human Q&A"),
        (7080, "Wind Turbulence"),
        (7085, "Boars Run"),
        (7090, "Suppress the Eruption"),
        (7095, "Spider Mine"),
        (7101, "Aerial Patrol"),
        (7102, "Aerial Patrol"),
        (7103, "Aerial Patrol"),
        (7150, "World Dominator"),
        (7151, "World Dominator"),
        (8001, "Village Defense Battle"),
        (8002, "Golem Arena"),
        (8005, "Urgent Delivery"),
        (8006, "Urgent Delivery"),
        (8020, "Demi-Human Duel"),
        (8021, "Demi-Human Duel"),
        (9000, "Spider Mine"),
        (9001, "Balloon Capture"),
        (9002, "Defeat the Year Demon"),
        (9200, "Dragon Shackles: Adept's Trial"),
        (9201, "Loom of Dreams\u{200b}"),
        (9202, "Slumberdream Valley"),
        (9203, "Slumberdream Valley"),
        (9204, "Slumberdream Valley"),
        (10001, "Square Practice Field 1"),
        (10002, "Square Practice Field 2"),
        (11001, "Wondrous Tag"),
        (12000, "Guild Center"),
        (12011, "Guild Hunt - Hard"),
        (12012, "Guild Hunt - Normal"),
        (12013, "Guild Hunt - Easy"),
        (12014, "Guild Hunt - Normal"),
        (12015, "Guild Hunt - Hard"),
        (12016, "Devourer Cage Sharing"),
        (12017, "Devourer Cage Sharing"),
        (12018, "Guild Hunt - Normal"),
        (12019, "Guild Hunt - Hard"),
        (12030, "Ee-chan's Story"),
        (12040, "Guild Party"),
        (12050, "Giant Golem Crusade"),
        (12051, "Giant Golem Crusade"),
        (12100, "Echoing Dungeon"),
        (13001, "Clash! Floating Island - Dragon Shackles"),
        (13002, "Brutal! Floating Island - Dragon Shackles"),
        (13003, "Purge! Floating Island - Dragon Shackles"),
        (13011, "Clash! Dreambloom Ruins"),
        (13012, "Brutal! Dreambloom Ruins"),
        (13013, "Purge! Dreambloom Ruins"),
        (20101, "Guardian 1"),
        (20102, "Guardian 10"),
        (20201, "Guardian 2"),
        (20202, "Guardian 22"),
        (20301, "Guardian 4"),
        (20302, "Guardian 13"),
        (20303, "Guardian 25"),
        (20401, "Guardian 5"),
        (20402, "Guardian 16"),
        (20501, "Guardian 7"),
        (20502, "Guardian 19"),
        (20701, "Guardian 14"),
        (20702, "Guardian 20"),
        (20801, "Guardian 8"),
        (20802, "Guardian 17"),
        (20803, "Guardian 26"),
        (21201, "Guardian 23"),
        (21202, "Guardian 29"),
        (21301, "Guardian 6"),
        (21302, "Guardian 12"),
        (21303, "Guardian 18"),
        (21304, "Guardian 24"),
        (21305, "Guardian 30"),
        (21306, "Guardian 3"),
        (21307, "Guardian 9"),
        (21308, "Guardian 15"),
        (21309, "Guardian 21"),
        (21310, "Guardian 27"),
        (21401, "Guardian 11"),
        (21402, "Guardian 28"),
        (22101, "Attack 17"),
        (22201, "Attack 10"),
        (22202, "Attack 23"),
        (22301, "Attack 11"),
        (22401, "Attack 13"),
        (22402, "Attack 19"),
        (22501, "Attack 6"),
        (22502, "Attack 12"),
        (22503, "Attack 18"),
        (22504, "Attack 24"),
        (22505, "Attack 30"),
        (22601, "Attack 7"),
        (22602, "Attack 14"),
        (22701, "Attack 8"),
        (22702, "Attack 26"),
        (23101, "Attack 3"),
        (23102, "Attack 9"),
        (23103, "Attack 15"),
        (23104, "Attack 21"),
        (23105, "Attack 27"),
        (23201, "Attack 4"),
        (23301, "Attack 1"),
        (23302, "Attack 20"),
        (23401, "Attack 2"),
        (23501, "Attack 5"),
        (23502, "Attack 25"),
        (23601, "Attack 16"),
        (23602, "Attack 28"),
        (23701, "Attack 22"),
        (23702, "Attack 29"),
        (24101, "Support 3"),
        (24102, "Support 6"),
        (24103, "Support 9"),
        (24104, "Support 12"),
        (24105, "Support 15"),
        (24106, "Support 18"),
        (24107, "Support 21"),
        (24108, "Support 24"),
        (24109, "Support 27"),
        (24110, "Support 30"),
        (24201, "Support 2"),
        (24202, "Support 10"),
        (24203, "Support 17"),
        (24204, "Support 25"),
        (24301, "Support 5"),
        (24302, "Support 14"),
        (24303, "Support 20"),
        (24304, "Support 29"),
        (24401, "Support 4"),
        (24402, "Support 11"),
        (24403, "Support 19"),
        (24404, "Support 23"),
        (24501, "Support 7"),
        (24502, "Support 16"),
        (24503, "Support 22"),
        (24504, "Support 28"),
        (24601, "Support 1"),
        (24602, "Support 8"),
        (24603, "Support 13"),
        (24604, "Support 26"),
        (30001, "Homestead Courtyard"),
        (30101, "Stimen Vaults Floor 1"),
        (30102, "Stimen Vaults Floor 2"),
        (30103, "Stimen Vaults Floor 3"),
        (30104, "Stimen Vaults Floor 4"),
        (30105, "Stimen Vaults Floor 5"),
        (30106, "Stimen Vaults Floor 6"),
        (30107, "Stimen Vaults Floor 7"),
        (30108, "Stimen Vaults Floor 8"),
        (30109, "Stimen Vaults Floor 9"),
        (30110, "Stimen Vaults Floor 10"),
        (30111, "Stimen Vaults Floor 11"),
        (30112, "Stimen Vaults Floor 12"),
        (30113, "Stimen Vaults Floor 13"),
        (30114, "Stimen Vaults Floor 14"),
        (30115, "Stimen Vaults Floor 15"),
        (30116, "Stimen Vaults Floor 16"),
        (30117, "Stimen Vaults Floor 17"),
        (30118, "Stimen Vaults Floor 18"),
        (30119, "Stimen Vaults Floor 19"),
        (30120, "Stimen Vaults Floor 20"),
        (30121, "Stimen Vaults Floor 21"),
        (30122, "Stimen Vaults Floor 22"),
        (30123, "Stimen Vaults Floor 23"),
        (30124, "Stimen Vaults Floor 24"),
        (30125, "Stimen Vaults Floor 25"),
        (30126, "Stimen Vaults Floor 26"),
        (30127, "Stimen Vaults Floor 27"),
        (30128, "Stimen Vaults Floor 28"),
        (30129, "Stimen Vaults Floor 29"),
        (30130, "Stimen Vaults Floor 30"),
        (30131, "Stimen Vaults Floor 31"),
        (30132, "Stimen Vaults Floor 32"),
        (30133, "Stimen Vaults Floor 33"),
        (30134, "Stimen Vaults Floor 34"),
        (30135, "Stimen Vaults Floor 35"),
        (30136, "Stimen Vaults Floor 36"),
        (30137, "Stimen Vaults Floor 37"),
        (30138, "Stimen Vaults Floor 38"),
        (30139, "Stimen Vaults Floor 39"),
        (30140, "Stimen Vaults Floor 40"),
        (30141, "Stimen Vaults Floor 41"),
        (30142, "Stimen Vaults Floor 42"),
        (30143, "Stimen Vaults Floor 43"),
        (30144, "Stimen Vaults Floor 44"),
        (30145, "Stimen Vaults Floor 45"),
        (30146, "Stimen Vaults Floor 46"),
        (30147, "Stimen Vaults Floor 47"),
        (30148, "Stimen Vaults Floor 48"),
        (30149, "Stimen Vaults Floor 49"),
        (30150, "Stimen Vaults Floor 50"),
        (30151, "Stimen Vaults Floor 51"),
        (30152, "Stimen Vaults Floor 52"),
        (30153, "Stimen Vaults Floor 53"),
        (30154, "Stimen Vaults Floor 54"),
        (30155, "Stimen Vaults Floor 55"),
        (30156, "Stimen Vaults Floor 56"),
        (30157, "Stimen Vaults Floor 57"),
        (30158, "Stimen Vaults Floor 58"),
        (30159, "Stimen Vaults Floor 59"),
        (30160, "Stimen Vaults Floor 60"),
        (30161, "Stimen Vaults Floor 61"),
        (30162, "Stimen Vaults Floor 62"),
        (30163, "Stimen Vaults Floor 63"),
        (30164, "Stimen Vaults Floor 64"),
        (30165, "Stimen Vaults Floor 65"),
        (30166, "Stimen Vaults Floor 66"),
        (30167, "Stimen Vaults Floor 67"),
        (30168, "Stimen Vaults Floor 68"),
        (30169, "Stimen Vaults Floor 69"),
        (30170, "Stimen Vaults Floor 70"),
        (30171, "Stimen Vaults Floor 71"),
        (30172, "Stimen Vaults Floor 72"),
        (30173, "Stimen Vaults Floor 73"),
        (30174, "Stimen Vaults Floor 74"),
        (30175, "Stimen Vaults Floor 75"),
        (30200, "Stimen Vaults Floor 75"),
        (31101, "Stimen Vaults Floor 1"),
        (31102, "Stimen Vaults Floor 2"),
        (31103, "Stimen Vaults Floor 3"),
        (31104, "Stimen Vaults Floor 4"),
        (31105, "Stimen Vaults Floor 5"),
        (31106, "Stimen Vaults Floor 6"),
        (31107, "Stimen Vaults Floor 7"),
        (31108, "Stimen Vaults Floor 8"),
        (31109, "Stimen Vaults Floor 9"),
        (31110, "Stimen Vaults Floor 10"),
        (31111, "Stimen Vaults Floor 11"),
        (31112, "Stimen Vaults Floor 12"),
        (31113, "Stimen Vaults Floor 13"),
        (31114, "Stimen Vaults Floor 14"),
        (31115, "Stimen Vaults Floor 15"),
        (31116, "Stimen Vaults Floor 16"),
        (31117, "Stimen Vaults Floor 17"),
        (31118, "Stimen Vaults Floor 18"),
        (31119, "Stimen Vaults Floor 19"),
        (31120, "Stimen Vaults Floor 20"),
        (31121, "Stimen Vaults Floor 21"),
        (31122, "Stimen Vaults Floor 22"),
        (31123, "Stimen Vaults Floor 23"),
        (31124, "Stimen Vaults Floor 24"),
        (31125, "Stimen Vaults Floor 25"),
        (31126, "Stimen Vaults Floor 26"),
        (31127, "Stimen Vaults Floor 27"),
        (31128, "Stimen Vaults Floor 28"),
        (31129, "Stimen Vaults Floor 29"),
        (31130, "Stimen Vaults Floor 30"),
        (31131, "Stimen Vaults Floor 31"),
        (31132, "Stimen Vaults Floor 32"),
        (31133, "Stimen Vaults Floor 33"),
        (31134, "Stimen Vaults Floor 34"),
        (31135, "Stimen Vaults Floor 35"),
        (31136, "Stimen Vaults Floor 36"),
        (31137, "Stimen Vaults Floor 37"),
        (31138, "Stimen Vaults Floor 38"),
        (31139, "Stimen Vaults Floor 39"),
        (31140, "Stimen Vaults Floor 40"),
        (31141, "Stimen Vaults Floor 41"),
        (31142, "Stimen Vaults Floor 42"),
        (31143, "Stimen Vaults Floor 43"),
        (31144, "Stimen Vaults Floor 44"),
        (31145, "Stimen Vaults Floor 45"),
        (31146, "Stimen Vaults Floor 46"),
        (31147, "Stimen Vaults Floor 47"),
        (31148, "Stimen Vaults Floor 48"),
        (31149, "Stimen Vaults Floor 49"),
        (31150, "Stimen Vaults Floor 50"),
        (31151, "Stimen Vaults Floor 51"),
        (31152, "Stimen Vaults Floor 52"),
        (31153, "Stimen Vaults Floor 53"),
        (31154, "Stimen Vaults Floor 54"),
        (31155, "Stimen Vaults Floor 55"),
        (31156, "Stimen Vaults Floor 56"),
        (31157, "Stimen Vaults Floor 57"),
        (31158, "Stimen Vaults Floor 58"),
        (31159, "Stimen Vaults Floor 59"),
        (31160, "Stimen Vaults Floor 60"),
        (31161, "Stimen Vaults Floor 61"),
        (31162, "Stimen Vaults Floor 62"),
        (31163, "Stimen Vaults Floor 63"),
        (31164, "Stimen Vaults Floor 64"),
        (31165, "Stimen Vaults Floor 65"),
        (31166, "Stimen Vaults Floor 66"),
        (31167, "Stimen Vaults Floor 67"),
        (31168, "Stimen Vaults Floor 68"),
        (31169, "Stimen Vaults Floor 69"),
        (31170, "Stimen Vaults Floor 70"),
        (31171, "Stimen Vaults Floor 71"),
        (31172, "Stimen Vaults Floor 72"),
        (31173, "Stimen Vaults Floor 73"),
        (31174, "Stimen Vaults Floor 74"),
        (31175, "Stimen Vaults Floor 75"),
        (40001, "Homestead House"),
        (171001, "Isolated Island"),
    ];
    ZONES.binary_search_by_key(&id, |&(k, _)| k)
         .ok()
         .map(|i| ZONES[i].1)
}

fn parse_sync_container_data(msg: pb::SyncContainerData) -> Vec<GameEvent> {
    let Some(v_data) = msg.v_data else { return vec![] };
    if v_data.char_id == 0 { return vec![] }

    let entity_id = EntityId(v_data.char_id as u64);
    let mut events = Vec::new();

    let name  = v_data.char_base.as_ref().map(|b| b.name.clone()).unwrap_or_default();
    let class = v_data.profession_list.map(|p| p.cur_profession_id as u32);

    if !name.is_empty() || class.is_some() {
        events.push(GameEvent::EntityName { id: entity_id, name, class, monster_type: None, is_player: true });
    }
    events.push(GameEvent::LocalPlayer { id: entity_id });

    // AS (fight_point) is in char_base for own character only
    if let Some(base) = &v_data.char_base {
        if base.fight_point > 0 {
            let mut stats = CharStats::default();
            stats.ability_score = Some(base.fight_point as u32);
            events.push(GameEvent::EntityStats { id: entity_id, stats });
        }
    }

    // Extract player module inventory
    let modules = extract_modules(v_data.item_package.as_ref(), v_data.r#mod.as_ref());
    if !modules.is_empty() {
        events.push(GameEvent::PlayerInventory { id: entity_id, modules });
    }

    // Extract equipped gear: cross-reference each equip slot's item_uuid
    // against the gear bag (package index 2) to resolve the item's ConfigId.
    if let (Some(equip), Some(item_package)) = (&v_data.equip, &v_data.item_package) {
        if let Some(gear_bag) = item_package.packages.get(&2) {
            let slots: Vec<EquipSlot> = equip.equip_list.values()
                .filter_map(|info| {
                    gear_bag.items.get(&(info.item_uuid as i64))
                        .map(|item| EquipSlot { slot: info.equip_slot, item_id: item.config_id })
                })
                .collect();
            if !slots.is_empty() {
                events.push(GameEvent::EquipData { id: entity_id, slots });
            }
        }
    }

    events
}

/// Extract PlayerModule list from ItemPackage + Mod data.
/// Mirrors bpsr-logs/src-tauri/src/utils/modules.rs extract_modules().
fn extract_modules(
    item_package: Option<&pb::ItemPackage>,
    mod_data: Option<&pb::Mod>,
) -> Vec<PlayerModule> {
    let Some(pkg) = item_package else { return vec![] };
    let mod_infos = mod_data.map(|m| &m.mod_infos);

    let mut modules = Vec::new();
    for package in pkg.packages.values() {
        for (item_key, item) in &package.items {
            let Some(mod_new_attr) = &item.mod_new_attr else { continue };
            if mod_new_attr.mod_parts.is_empty() { continue }

            let init_link_nums: &[i32] = mod_infos
                .and_then(|m| m.get(item_key))
                .map(|mi| mi.init_link_nums.as_slice())
                .unwrap_or(&[]);

            let n = mod_new_attr.mod_parts.len().min(init_link_nums.len().max(mod_new_attr.mod_parts.len()));
            let mut effects = Vec::new();
            for i in 0..n {
                let effect_id = mod_new_attr.mod_parts[i];
                let level = init_link_nums.get(i).copied().unwrap_or(0);
                if is_valid_effect_id(effect_id) {
                    effects.push(ModuleEffect { effect_id, level });
                }
            }
            if !effects.is_empty() {
                modules.push(PlayerModule { config_id: item.config_id, effects });
            }
        }
    }
    modules
}

fn is_valid_effect_id(id: i32) -> bool {
    matches!(id,
        1110 | 1111 | 1112 | 1113 | 1114 |
        1205 | 1206 |
        1307 | 1308 |
        1407 | 1408 | 1409 | 1410 |
        2104 | 2105 | 2204 | 2205 | 2304 |
        2404 | 2405 | 2406
    )
}

/// Same as parse_aoi_sync_delta but always treats entity as a player.
/// Used for SyncToMeDeltaInfo (own character) where ent_type bits may not encode 10.
fn parse_aoi_sync_delta_as_player(delta: pb::AoiSyncDelta) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let target_uuid = delta.uuid;
    if target_uuid == 0 { return events; }
    let target_id = EntityId((target_uuid >> 16) as u64);

    if let Some(attrs) = delta.attrs {
        events.extend(parse_player_attrs(target_id, attrs.attrs));
    }
    if let Some(skill_effects) = delta.skill_effects {
        for dmg in skill_effects.damages {
            if dmg.r#type == 2 {
                if let Some(event) = parse_heal(target_uuid, &dmg) {
                    events.push(GameEvent::Heal(event));
                }
            } else if let Some(event) = parse_damage(target_uuid, &dmg) {
                events.push(GameEvent::Combat(event));
            }
        }
    }
    events
}

fn parse_aoi_sync_delta(delta: pb::AoiSyncDelta) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let target_uuid = delta.uuid;
    if target_uuid == 0 { return events; }
    let target_id = EntityId((target_uuid >> 16) as u64);
    // Entity type is embedded in UUID bits [10:6] — same encoding as game client
    let ent_type = ((target_uuid >> 6) & 31) as i32;

    if let Some(attrs) = delta.attrs {
        if ent_type == 10 {
            events.extend(parse_player_attrs(target_id, attrs.attrs));
        } else {
            events.extend(parse_npc_attrs(target_id, ent_type, attrs.attrs));
        }
    }

    // Damage + heal events
    if let Some(skill_effects) = delta.skill_effects {
        for dmg in skill_effects.damages {
            if dmg.r#type == 2 {
                if let Some(event) = parse_heal(target_uuid, &dmg) {
                    events.push(GameEvent::Heal(event));
                }
            } else if let Some(event) = parse_damage(target_uuid, &dmg) {
                events.push(GameEvent::Combat(event));
            }
        }
    }

    events
}

/// Resolve name for non-player entities (monsters, dummies) from their attrs.
/// AttrId (id=10) contains the config ID used to look up the name in data tables.
/// AttrName (id=1) is a direct name string (fallback).
fn parse_npc_attrs(entity_id: EntityId, ent_type: i32, attrs: Vec<pb::Attr>) -> Vec<GameEvent> {
    let mut name_direct = None::<String>;
    let mut config_id   = None::<u32>;
    let mut stats       = CharStats::default();

    for attr in &attrs {
        match attr.id {
            1 => {
                if attr.raw_data.len() > 1 {
                    if let Ok(s) = std::str::from_utf8(&attr.raw_data[1..]) {
                        let s = s.trim_matches('\0').to_string();
                        if !s.is_empty() { name_direct = Some(s); }
                    }
                }
            }
            10 => {
                if let Some(id) = decode_varint(&attr.raw_data) {
                    config_id = Some(id as u32);
                }
            }
            // HP/MaxHP/State — needed to cross-check boss reset/wipe state.
            11310 => { stats.hp          = decode_varint(&attr.raw_data).map(|v| v as i64); }
            11320 => { stats.max_hp      = decode_varint(&attr.raw_data).map(|v| v as i64); }
            11      => { stats.actor_state = decode_varint(&attr.raw_data).map(|v| v as i32); }
            // SeasonStrength — sent by the server for some bosses/elites, mirrors player attr 11440.
            11440 => { stats.season_strength = decode_varint(&attr.raw_data).map(|v| v as u32); }
            _ => {}
        }
    }

    let resolved = config_id.and_then(|id| {
        match ent_type {
            1  => bpsr_core::DATA.monster_name(id),
            11 => bpsr_core::DATA.dummy_name(id),
            _  => None,
        }
    }).map(|s| s.to_string())
    .or(name_direct);

    let monster_type = if ent_type == 1 { config_id.and_then(|id| bpsr_core::DATA.monster_type(id)) } else { None };

    let mut events = Vec::new();
    if let Some(name) = resolved {
        events.push(GameEvent::EntityName { id: entity_id, name, class: None, monster_type, is_player: false });
    }
    if stats.hp.is_some() || stats.max_hp.is_some() || stats.actor_state.is_some() || stats.season_strength.is_some() {
        events.push(GameEvent::EntityStats { id: entity_id, stats });
    }
    events
}

/// Hex-dumps up to the first 64 bytes of a payload, for protocol-framing debugging.
fn hex_preview(data: &[u8]) -> String {
    let take = data.len().min(64);
    let mut s = String::with_capacity(take * 3);
    for b in &data[..take] {
        s.push_str(&format!("{b:02x} "));
    }
    if data.len() > take { s.push_str("..."); }
    s
}

fn parse_player_attrs(entity_id: EntityId, attrs: Vec<pb::Attr>) -> Vec<GameEvent> {
    let mut events = Vec::new();
    let mut name  = None::<String>;
    let mut class = None::<u32>;
    let mut stats = CharStats::default();

    for attr in &attrs {
        if attr.raw_data.is_empty() { continue; }
        match attr.id {
            // AttrShieldList = 60050
            60050 => {
                let shields: Vec<ShieldEntry> = pb::ShieldInfoList::decode(attr.raw_data.as_slice())
                    .map(|w| w.items).unwrap_or_default()
                    .into_iter()
                    .map(|s| ShieldEntry { uuid: s.uuid, value: s.value, initial_value: s.initial_value })
                    .collect();
                events.push(GameEvent::ShieldList { id: entity_id, shields });
            }
            // AttrEquipData = 200
            200 => {
                let decoded = pb::EquipNineList::decode(attr.raw_data.as_slice())
                    .map(|w| w.items).unwrap_or_default();
                debug!(
                    "AttrEquipData raw_len={} decoded_count={} raw_hex={}",
                    attr.raw_data.len(), decoded.len(), hex_preview(&attr.raw_data)
                );
                let slots: Vec<EquipSlot> = decoded.into_iter()
                    .map(|e| EquipSlot { slot: e.slot, item_id: e.equip_id })
                    .collect();
                events.push(GameEvent::EquipData { id: entity_id, slots });
            }
            // AttrSkillLevelIdList = 116
            116 => {
                let decoded = pb::SkillLevelInfoList::decode(attr.raw_data.as_slice())
                    .map(|w| w.items).unwrap_or_default();
                debug!(
                    "AttrSkillLevelIdList raw_len={} decoded_count={} raw_hex={}",
                    attr.raw_data.len(), decoded.len(), hex_preview(&attr.raw_data)
                );
                let skills: Vec<SkillLoadoutEntry> = decoded.into_iter()
                    .map(|s| SkillLoadoutEntry { skill_id: s.skill_id, current_level: s.current_level, tier: s.remodel_level })
                    .collect();
                events.push(GameEvent::SkillLoadout { id: entity_id, skills });
            }
            _ => {}
        }
    }

    for attr in attrs {
        if attr.raw_data.is_empty() { continue; }
        match attr.id {
            // ATTR_NAME = 1
            1 => {
                let raw = &attr.raw_data;
                if raw.len() > 1 {
                    if let Ok(s) = std::str::from_utf8(&raw[1..]) {
                        let s = s.trim_matches('\0').to_string();
                        if !s.is_empty() { name = Some(s); }
                    }
                }
            }
            // ATTR_PROFESSION_ID = 220
            220 => {
                if let Some(id) = decode_varint(&attr.raw_data) {
                    if id > 0 { class = Some(id as u32); }
                }
            }
            // --- Stats ---
            // Level = 10000, AbilityScore (FightPoint) = 10030, SeasonStrength = 11440
            10000 => { stats.level           = decode_varint(&attr.raw_data).map(|v| v as u32); }
            10030 => { stats.ability_score   = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11440 => { stats.season_strength = decode_varint(&attr.raw_data).map(|v| v as u32); }
            // Base stats (i32)
            11010 => { stats.strength   = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11040 => { stats.endurance  = decode_varint(&attr.raw_data).map(|v| v as u32); }
            // Secondary (i32)
            11110 => { stats.crit       = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11120 => { stats.haste      = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11130 => { stats.luck       = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11140 => { stats.mastery    = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11150 => { stats.versatility= decode_varint(&attr.raw_data).map(|v| v as u32); }
            11170 => { stats.block      = decode_varint(&attr.raw_data).map(|v| v as u32); }
            // Combat stats (i64)
            11310 => { stats.hp     = decode_varint(&attr.raw_data).map(|v| v as i64); }
            11320 => { stats.max_hp = decode_varint(&attr.raw_data).map(|v| v as i64); }
            11330 => { stats.attack = decode_varint(&attr.raw_data).map(|v| v as i64); }
            11350 => { stats.armor  = decode_varint(&attr.raw_data).map(|v| v as i64); }
            // Pct stats (i32)
            11710 => { stats.crit_pct        = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11780 => { stats.luck_pct        = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11930 => { stats.haste_pct       = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11940 => { stats.mastery_pct     = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11950 => { stats.versatility_pct = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11970 => { stats.block_pct       = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11720 => { stats.atk_speed_pct  = decode_varint(&attr.raw_data).map(|v| v as u32); }
            11730 => { stats.cast_speed_pct = decode_varint(&attr.raw_data).map(|v| v as u32); }
            12510 => { stats.crit_damage    = decode_varint(&attr.raw_data).map(|v| v as u32); }
            // ATTR_STATE = 11 (EActorState; 9 = Dead)
            11 => { stats.actor_state = decode_varint(&attr.raw_data).map(|v| v as i32); }
            _ => {}
        }
    }

    if name.is_some() || class.is_some() {
        events.push(GameEvent::EntityName {
            id: entity_id,
            name: name.unwrap_or_default(),
            class,
            monster_type: None,
            is_player: true,
        });
    }
    let has_stats = stats.level.is_some() || stats.ability_score.is_some() || stats.season_strength.is_some()
        || stats.hp.is_some() || stats.max_hp.is_some() || stats.attack.is_some()
        || stats.strength.is_some() || stats.endurance.is_some() || stats.armor.is_some()
        || stats.crit.is_some() || stats.haste.is_some() || stats.luck.is_some()
        || stats.mastery.is_some() || stats.versatility.is_some() || stats.block.is_some()
        || stats.crit_pct.is_some() || stats.luck_pct.is_some() || stats.haste_pct.is_some()
        || stats.mastery_pct.is_some() || stats.versatility_pct.is_some() || stats.block_pct.is_some()
        || stats.atk_speed_pct.is_some() || stats.cast_speed_pct.is_some() || stats.crit_damage.is_some()
        || stats.actor_state.is_some();
    if has_stats {
        events.push(GameEvent::EntityStats { id: entity_id, stats });
    }
    events
}

fn parse_damage(target_uuid: i64, dmg: &pb::SyncDamageInfo) -> Option<CombatEvent> {
    // Skip: miss, heal (type 2), no attacker, no skill
    if dmg.is_miss { return None; }
    if dmg.r#type == 2 { return None; } // heal
    if dmg.owner_id == 0 { return None; }

    let attacker_uuid = if dmg.top_summoner_id != 0 {
        dmg.top_summoner_id
    } else if dmg.attacker_uuid != 0 {
        dmg.attacker_uuid
    } else {
        return None;
    };

    // Matches ZDPS precedence: Value first, LuckyValue (crit/lucky damage) fallback.
    let damage = if dmg.value != 0 { dmg.value } else { dmg.lucky_value };
    if damage <= 0 { return None; }

    let is_crit  = (dmg.type_flag & 0b0000_0001) != 0;
    let is_lucky = dmg.lucky_value != 0;

    Some(CombatEvent {
        timestamp:  Instant::now(),
        source_id:  EntityId((attacker_uuid >> 16) as u64),
        target_id:  EntityId((target_uuid >> 16) as u64),
        skill_id:   dmg.owner_id as u32,
        damage:     damage as u64,
        is_crit,
        is_lucky,
        is_dot:     false,
        element:    None,
    })
}

fn parse_heal(target_uuid: i64, dmg: &pb::SyncDamageInfo) -> Option<CombatEvent> {
    if dmg.owner_id == 0 { return None; }
    let healer_uuid = if dmg.top_summoner_id != 0 {
        dmg.top_summoner_id
    } else if dmg.attacker_uuid != 0 {
        dmg.attacker_uuid
    } else {
        return None;
    };
    let amount = if dmg.value != 0 { dmg.value } else { dmg.lucky_value };
    if amount <= 0 { return None; }
    let is_crit  = (dmg.type_flag & 0b0000_0001) != 0;
    let is_lucky = dmg.lucky_value != 0;
    Some(CombatEvent {
        timestamp: Instant::now(),
        source_id: EntityId((healer_uuid >> 16) as u64),
        target_id: EntityId((target_uuid >> 16) as u64),
        skill_id:  dmg.owner_id as u32,
        damage:    amount as u64,
        is_crit,
        is_lucky,
        is_dot:    false,
        element:   None,
    })
}

fn parse_chat(msg: pb::NotifyNewestChitChatMsgs) -> Vec<GameEvent> {
    let Some(req) = msg.v_request else { return vec![] };
    let Some(chat) = req.chat_msg else { return vec![] };
    let Some(info) = chat.msg_info else { return vec![] };
    if info.msg_text.is_empty() { return vec![]; }

    let (sender_name, sender_uid) = chat.send_char_info
        .map(|i| (i.name, i.char_id as u64))
        .unwrap_or_default();

    vec![GameEvent::Chat {
        channel:     ChatChannel::from_i32(req.channel_type),
        sender_name,
        sender_uid,
        text:        info.msg_text,
    }]
}

fn parse_enter_match_result(msg: pb::EnterMatchResultNtf) -> Vec<GameEvent> {
    let Some(req) = msg.v_request else { return vec![] };
    if req.err_code != 0 { return vec![]; }
    let status = req.match_info.map(|i| i.match_status).unwrap_or(0);
    // WaitReady (2) = queue popped, need to ready
    if status == 2 {
        return vec![GameEvent::MatchmakingAlert { kind: MatchAlertKind::QueuePop }];
    }
    vec![]
}

fn parse_sync_dungeon_data(msg: pb::SyncDungeonData) -> Vec<GameEvent> {
    let Some(data) = msg.v_data else { return vec![] };
    let mut events = Vec::new();

    if let Some(flow) = &data.flow_info {
        events.push(GameEvent::DungeonState {
            state: DungeonStateKind::from_i32(flow.state),
        });
    }

    let targets: Vec<DungeonTargetProgress> = data.target
        .map(|t| t.target_data.into_values().map(|d| DungeonTargetProgress {
            target_id: d.target_id, nums: d.nums, complete: d.complete,
        }).collect())
        .unwrap_or_default();
    let phase_id = data.dungeon_phase_data.map(|p| p.phase_id);
    if !targets.is_empty() || phase_id.is_some() {
        events.push(GameEvent::DungeonPhaseSignal { targets, phase_id });
    }

    events
}

/// WorldNtf 0x18 — mid-run incremental dungeon state (see `packets::blob`
/// for the custom non-protobuf delta format `v_data.buffer` decodes to).
/// Unlike SyncDungeonData (0x17, enter/exit only), this fires throughout
/// the run and is where DungeonTarget progress transitions actually show up.
fn parse_sync_dungeon_dirty_data(msg: pb::SyncDungeonDirtyData) -> Vec<GameEvent> {
    let Some(v_data) = msg.v_data else { return vec![] };
    if v_data.buffer.is_empty() { return vec![]; }

    let stream_safe = v_data.stream_type == 0; // EStreamType::StreamTypeDeltaDirtySafe
    let dirty = crate::proto::packets::blob::DungeonDirtyData::parse(&v_data.buffer, stream_safe);
    if dirty.state.is_some() || !dirty.targets.is_empty() {
        debug!(
            "SyncDungeonDirtyData state={:?} targets={:?}",
            dirty.state,
            dirty.targets.iter().map(|(k, d)| (*k, d.target_id, d.nums, d.complete)).collect::<Vec<_>>()
        );
    }

    let mut events = Vec::new();
    if let Some(state) = dirty.state {
        events.push(GameEvent::DungeonState { state: DungeonStateKind::from_i32(state) });
    }
    if !dirty.targets.is_empty() {
        let targets: Vec<DungeonTargetProgress> = dirty.targets.into_iter()
            .map(|(_, d)| DungeonTargetProgress { target_id: d.target_id, nums: d.nums, complete: d.complete })
            .collect();
        events.push(GameEvent::DungeonPhaseSignal { targets, phase_id: None });
    }
    events
}

/// Standard protobuf varint decoder.
fn decode_varint(data: &[u8]) -> Option<u64> {
    let mut result = 0u64;
    let mut shift = 0u32;
    for &byte in data {
        result |= ((byte & 0x7F) as u64) << shift;
        if (byte & 0x80) == 0 {
            return Some(result);
        }
        shift += 7;
        if shift >= 64 { return None; }
    }
    None
}
