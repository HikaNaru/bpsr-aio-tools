// Protobuf message structs for Blue Protocol: Star Resonance
// Ported from bpsr-logs/src-tauri/src/protocol/pb.rs (without specta derives)

#[derive(Clone, PartialEq, Eq, Hash, ::prost::Message)]
pub struct Attr {
    #[prost(int32, tag = "1")]
    pub id: i32,
    #[prost(bytes = "vec", tag = "2")]
    pub raw_data: Vec<u8>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AttrCollection {
    #[prost(int64, tag = "1")]
    pub uuid: i64,
    #[prost(message, repeated, tag = "2")]
    pub attrs: Vec<Attr>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AoiSyncDelta {
    #[prost(int64, tag = "1")]
    pub uuid: i64,
    #[prost(message, optional, tag = "2")]
    pub attrs: Option<AttrCollection>,
    #[prost(message, optional, tag = "7")]
    pub skill_effects: Option<SkillEffect>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct AoiSyncToMeDelta {
    #[prost(message, optional, tag = "1")]
    pub base_delta: Option<AoiSyncDelta>,
    #[prost(int64, tag = "5")]
    pub uuid: i64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CharBaseInfo {
    #[prost(int64, tag = "1")]
    pub char_id: i64,
    #[prost(string, tag = "2")]
    pub account_id: String,
    #[prost(string, tag = "5")]
    pub name: String,
    #[prost(int32, tag = "35")]
    pub fight_point: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct CharSerialize {
    #[prost(int64, tag = "1")]
    pub char_id: i64,
    #[prost(message, optional, tag = "2")]
    pub char_base: Option<CharBaseInfo>,
    #[prost(message, optional, tag = "7")]
    pub item_package: Option<ItemPackage>,
    #[prost(message, optional, tag = "57")]
    pub r#mod: Option<Mod>,
    #[prost(message, optional, tag = "61")]
    pub profession_list: Option<ProfessionList>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModInfo {
    #[prost(int32, repeated, tag = "4")]
    pub init_link_nums: Vec<i32>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Mod {
    #[prost(map = "int64, message", tag = "2")]
    pub mod_infos: std::collections::HashMap<i64, ModInfo>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ModNewAttr {
    #[prost(int32, repeated, tag = "1")]
    pub mod_parts: Vec<i32>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Item {
    #[prost(message, optional, tag = "13")]
    pub mod_new_attr: Option<ModNewAttr>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Package {
    #[prost(map = "int64, message", tag = "4")]
    pub items: std::collections::HashMap<i64, Item>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ItemPackage {
    #[prost(map = "int32, message", tag = "1")]
    pub packages: std::collections::HashMap<i32, Package>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct DisappearEntity {
    #[prost(int64, tag = "1")]
    pub uuid: i64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct Entity {
    #[prost(int64, tag = "1")]
    pub uuid: i64,
    #[prost(int32, tag = "2")]
    pub ent_type: i32,
    #[prost(message, optional, tag = "3")]
    pub attrs: Option<AttrCollection>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct ProfessionList {
    #[prost(int32, tag = "1")]
    pub cur_profession_id: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SkillEffect {
    #[prost(message, repeated, tag = "2")]
    pub damages: Vec<SyncDamageInfo>,
}

#[derive(Clone, Copy, PartialEq, ::prost::Message)]
pub struct SyncDamageInfo {
    #[prost(bool, tag = "2")]
    pub is_miss: bool,
    #[prost(int32, tag = "4")]
    pub r#type: i32,
    #[prost(int32, tag = "5")]
    pub type_flag: i32,
    #[prost(int64, tag = "6")]
    pub value: i64,
    #[prost(int64, tag = "8")]
    pub lucky_value: i64,
    #[prost(int64, tag = "9")]
    pub hp_lessen_value: i64,
    #[prost(int64, tag = "11")]
    pub attacker_uuid: i64,
    #[prost(int32, tag = "12")]
    pub owner_id: i32,
    #[prost(bool, tag = "17")]
    pub is_dead: bool,
    #[prost(int64, tag = "21")]
    pub top_summoner_id: i64,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncNearDeltaInfo {
    #[prost(message, repeated, tag = "1")]
    pub delta_infos: Vec<AoiSyncDelta>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncNearEntities {
    #[prost(message, repeated, tag = "1")]
    pub appear: Vec<Entity>,
    #[prost(message, repeated, tag = "2")]
    pub disappear: Vec<DisappearEntity>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncContainerData {
    #[prost(message, optional, tag = "1")]
    pub v_data: Option<CharSerialize>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncToMeDeltaInfo {
    #[prost(message, optional, tag = "1")]
    pub delta_info: Option<AoiSyncToMeDelta>,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, ::prost::Message)]
pub struct SceneData {
    #[prost(uint32, tag = "6")]
    pub level_map_id: u32,
    #[prost(uint32, tag = "15")]
    pub line_id: u32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SocialData {
    #[prost(message, optional, tag = "10")]
    pub scene_data: Option<SceneData>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NotifySocialDataRequest {
    #[prost(message, optional, tag = "1")]
    pub data: Option<SocialData>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NotifySocialData {
    #[prost(message, optional, tag = "1")]
    pub v_request: Option<NotifySocialDataRequest>,
}

// ─── Chat (ChitChatNtf, method 0x01) ──────────────────────────────────────

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct BasicShowInfo {
    #[prost(int64, tag = "1")]
    pub char_id: i64,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(int32, tag = "5")]
    pub level: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChatMsgInfo {
    #[prost(int32, tag = "1")]
    pub msg_type: i32,
    #[prost(int64, tag = "2")]
    pub target_id: i64,
    #[prost(string, tag = "3")]
    pub msg_text: String,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct ChitChatMsg {
    #[prost(int64, tag = "1")]
    pub msg_id: i64,
    #[prost(message, optional, tag = "2")]
    pub send_char_info: Option<BasicShowInfo>,
    #[prost(int64, tag = "3")]
    pub timestamp: i64,
    #[prost(message, optional, tag = "4")]
    pub msg_info: Option<ChatMsgInfo>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NotifyNewestChitChatMsgsRequest {
    #[prost(int32, tag = "1")]
    pub channel_type: i32,
    #[prost(message, optional, tag = "2")]
    pub chat_msg: Option<ChitChatMsg>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct NotifyNewestChitChatMsgs {
    #[prost(message, optional, tag = "1")]
    pub v_request: Option<NotifyNewestChitChatMsgsRequest>,
}

// ─── Matchmaking (MatchNtf, methods 0x04 / 0x06) ─────────────────────────

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MatchInfo {
    #[prost(int32, tag = "2")]
    pub match_status: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EnterMatchResultNtfRequest {
    #[prost(int32, tag = "1")]
    pub err_code: i32,
    #[prost(message, optional, tag = "2")]
    pub match_info: Option<MatchInfo>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct EnterMatchResultNtf {
    #[prost(message, optional, tag = "1")]
    pub v_request: Option<EnterMatchResultNtfRequest>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MatchPlayerInfo {
    #[prost(int64, tag = "1")]
    pub char_id: i64,
    #[prost(int32, tag = "2")]
    pub ready_status: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MatchReadyStatusNtfRequest {
    #[prost(int32, tag = "1")]
    pub err_code: i32,
    #[prost(int64, tag = "2")]
    pub leader_char_id: i64,
    #[prost(message, repeated, tag = "5")]
    pub match_player_info: Vec<MatchPlayerInfo>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MatchReadyStatusNtf {
    #[prost(message, optional, tag = "1")]
    pub v_request: Option<MatchReadyStatusNtfRequest>,
}

// ─── Dungeon (WorldNtf, method 0x17 SYNC_DUNGEON_DATA) ───────────────────

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DungeonFlowInfo {
    #[prost(int32, tag = "1")]
    pub state: i32,
    #[prost(int32, tag = "2")]
    pub active_time: i32,
    #[prost(int32, tag = "3")]
    pub ready_time: i32,
    #[prost(int32, tag = "4")]
    pub play_time: i32,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct DungeonSyncData {
    #[prost(int64, tag = "1")]
    pub scene_uuid: i64,
    #[prost(message, optional, tag = "2")]
    pub flow_info: Option<DungeonFlowInfo>,
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct SyncDungeonData {
    #[prost(message, optional, tag = "1")]
    pub v_data: Option<DungeonSyncData>,
}
