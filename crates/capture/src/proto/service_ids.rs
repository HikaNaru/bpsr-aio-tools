// Service UUIDs observed in the game protocol.
// Source: BPSR-ZDPS/BPSR-ZDPSLib/ServiceIds.cs
//
// NOTE: The value for WORLD_NTF differs slightly from what aio-tools captures
// (opcode.rs uses 0x63335342). The constants here are ZDPS reference values
// for documentation; the actual captured UUID may vary by game client version.

pub const WORLD_NTF:          u64 = 1664308034; // 0x63436362
pub const SOCIAL_NTF:         u64 = 625772963;  // 0x254C89A3
pub const CHIT_CHAT_NTF:      u64 = 164931432;  // 0x09CFC878
pub const UNION_NTF:          u64 = 504281929;
pub const LEVEL_NTF:          u64 = 656251580;
pub const FRIEND_NTF:         u64 = 1994100518;
pub const ACE_SDK_NTF:        u64 = 1239299317;
pub const GRPC_CHARACTOR_NTF: u64 = 1953836151;
pub const GRPC_COMMUNITY_NTF: u64 = 1453563045;
pub const MATCH_NTF:          u64 = 822849903;  // 0x310F5ACF
pub const MAIL_NTF:           u64 = 1753654261;
pub const GRPC_TEAM_NTF:      u64 = 966773353;
pub const GRPC_WAREHOUSE_NTF: u64 = 1406513963;
pub const WORLD_ACTIVITY_NTF: u64 = 936649811;
pub const WORLD_LOGIN_NTF:    u64 = 78136601;
pub const QUESTIONNAIRE_NTF:  u64 = 194476713;
pub const PHOTOGRAPH_NTF:     u64 = 829259716;
pub const GRPC_CHARACTOR:     u64 = 1232729813;
pub const WORLD_ACT_NTF:      u64 = 548813240;
