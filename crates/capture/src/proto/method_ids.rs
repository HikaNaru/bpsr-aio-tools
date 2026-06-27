// Game protocol method IDs.
// Source: BPSR-ZDPS/BPSR-ZDPSLib/ServiceMethods/{WorldNtf,MatchNtf,ChitChatNtf}.cs

pub mod world_ntf {
    pub const NOTIFY_SWITCH_SCENE:               u32 = 0x01;
    pub const NOTIFY_SWITCH_SCENE_END:           u32 = 0x02;
    pub const ENTER_SCENE:                       u32 = 0x03;
    pub const NOTIFY_LOAD_SCENE_END:             u32 = 0x04;
    pub const TELEPORT:                          u32 = 0x05;
    pub const SYNC_NEAR_ENTITIES:                u32 = 0x06;
    pub const SYNC_SCENE_ATTRS:                  u32 = 0x07;
    pub const SYNC_SCENE_EVENTS:                 u32 = 0x08;
    pub const SYNC_ENTITY_BEHAVIOR_TREE:         u32 = 0x09;
    pub const SYNC_PLAY_CAMERA_ANIMATION:        u32 = 0x0A;
    pub const SYNC_FIELD_OF_VIEW:                u32 = 0x0B;
    pub const SYNC_LOG:                          u32 = 0x0C;
    pub const SYNC_PATH_NODE:                    u32 = 0x0D;
    pub const SYNC_PIONEER_INFO:                 u32 = 0x0E;
    pub const SYNC_SERVER_DATA:                  u32 = 0x0F;
    pub const FORCED_PULL_BACK:                  u32 = 0x10;
    pub const LINE_DRAWING:                      u32 = 0x11;
    pub const SYNC_SWITCH_CHANGE:                u32 = 0x12;
    pub const SYNC_SWITCH_INFO:                  u32 = 0x13;
    pub const ENTER_GAME:                        u32 = 0x14;
    pub const SYNC_CONTAINER_DATA:               u32 = 0x15;
    pub const SYNC_CONTAINER_DIRTY_DATA:         u32 = 0x16;
    pub const SYNC_DUNGEON_DATA:                 u32 = 0x17;
    pub const SYNC_DUNGEON_DIRTY_DATA:           u32 = 0x18;
    pub const AWARD_NOTIFY:                      u32 = 0x19;
    pub const CARD_INFO_ACK:                     u32 = 0x1A;
    pub const SYNC_SEASON:                       u32 = 0x1B;
    pub const USER_ACTION:                       u32 = 0x1C;
    pub const NOTIFY_DISPLAY_PLAY_HELP:          u32 = 0x1D;
    pub const NOTIFY_APPLICATION_INTERACTION:    u32 = 0x1E;
    pub const NOTIFY_IS_AGREE:                   u32 = 0x1F;
    pub const NOTIFY_CANCEL_ACTION:              u32 = 0x20;
    pub const NOTIFY_UPLOAD_PICTURE_RESULT:      u32 = 0x21;
    pub const SYNC_PERSONAL_OBJECT:              u32 = 0x22;
    pub const PERSONAL_OBJECT_UPDATE:            u32 = 0x23;
    pub const SYNC_INVITE:                       u32 = 0x24;
    pub const NOTIFY_RED_DOT_CHANGE:             u32 = 0x25;
    pub const CHANGE_NAME_RESULT_NTF:            u32 = 0x26;
    pub const NOTIFY_REVIVE_USER:                u32 = 0x27;
    pub const NOTIFY_PARKOUR_RANK_INFO:          u32 = 0x28;
    pub const NOTIFY_PARKOUR_RECORD_INFO:        u32 = 0x29;
    pub const NOTIFY_SHOW_TIPS:                  u32 = 0x2A;
    pub const SYNC_SERVER_TIME:                  u32 = 0x2B;
    pub const NOTIFY_NOTICE_INFO:                u32 = 0x2C;
    pub const SYNC_NEAR_DELTA_INFO:              u32 = 0x2D;
    pub const SYNC_TO_ME_DELTA_INFO:             u32 = 0x2E;
    pub const NOTIFY_CLIENT_KICK_OFF:            u32 = 0x31;
    pub const PAYMENT_RESPONSE:                  u32 = 0x33;
    pub const NOTIFY_UNLOCK_COOK_BOOK:           u32 = 0x35;
    pub const NOTIFY_CUSTOM_EVENT:               u32 = 0x36;
    pub const NOTIFY_START_PLAYING_DUNGEON:      u32 = 0x37;
    pub const CHANGE_SHOW_ID_RESULT_NTF:         u32 = 0x38;
    pub const NOTIFY_SHOW_ITEMS:                 u32 = 0x39;
    pub const NOTIFY_SEASON_ACTIVATION_TARGET:   u32 = 0x3A;
    pub const NOTIFY_TEXT_CHECK_RESULT:          u32 = 0x3B;
    pub const PERSONAL_GROUP_OBJECT_UPDATE:      u32 = 0x3C;
    pub const NOTIFY_DEBUG_MESSAGE_TIP:          u32 = 0x3D;
    pub const NOTIFY_USER_CLOSE_FUNCTION:        u32 = 0x3E;
    pub const NOTIFY_SERVER_CLOSE_FUNCTION:      u32 = 0x3F;
    pub const BOUNCE_JUMP:                       u32 = 0x42;
    pub const SERVER_STATE_OBJECT_UPDATE:        u32 = 0x43;
    pub const SYNC_ALL_SERVER_STATE_OBJECT:      u32 = 0x44;
    pub const NOTIFY_AWARD_ALL_ITEMS:            u32 = 0x45;
    pub const NOTIFY_ALL_MEMBER_READY:           u32 = 0x46;
    pub const NOTIFY_CAPTAIN_READY:              u32 = 0x47;
    pub const NOTIFY_TIMER_LIST:                 u32 = 0x48;
    pub const NOTIFY_TIMER_UPDATE:               u32 = 0x49;
    pub const NOTIFY_USER_ALL_SOURCE_PRIV_DATA:  u32 = 0x4A;
    pub const NOTIFY_QUEST_ACCEPT:               u32 = 0x4B;
    pub const NOTIFY_QUEST_CHANGE_STEP:          u32 = 0x4C;
    pub const NOTIFY_QUEST_GIVE_UP:              u32 = 0x4D;
    pub const NOTIFY_QUEST_COMPLETE:             u32 = 0x4E;
    pub const NOTIFY_USER_ALL_BATTLE_PASS:       u32 = 0x4F;
    pub const NOTIFY_PLAYER_BEGIN_INTERACTION:   u32 = 0x51;
    pub const SYNC_SUB_SCENE_ATTRS:              u32 = 0x52;
    pub const QTE_BEGIN:                         u32 = 0x3001;
    pub const SYNC_CLIENT_USE_SKILL:             u32 = 0x3002;
    pub const NOTIFY_BUFF_CHANGE:                u32 = 0x3003;
    pub const SYNC_SERVER_SKILL_STAGE_END:       u32 = 0x3004;
    pub const SYNC_SERVER_SKILL_END:             u32 = 0x3005;
    pub const SYNC_SERVER_SKILL_SINGING_TIME:    u32 = 0x3006;
    pub const QUEST_ABORT:                       u32 = 0x6001;
    pub const NOTIFY_BUY_SHOP_RESULT:            u32 = 0x29001;
    pub const NOTIFY_SHOP_ITEM_CAN_BUY:          u32 = 0x29002;
    pub const WORLD_BOSS_RANK_INFO:              u32 = 0x46001;
    pub const ENTER_MATCH_RESULT_NTF:            u32 = 0x48001;
    pub const NOTIFY_DRIVER_APPLY_RIDE:          u32 = 0x4D001;
    pub const NOTIFY_INVITE_APPLY_RIDE:          u32 = 0x4D002;
    pub const NOTIFY_RIDE_IS_AGREE:              u32 = 0x4D003;
    pub const NOTIFY_PAY_INFO:                   u32 = 0x51001;
    pub const NOTIFY_LIFE_PROFESSION_WORK:       u32 = 0x52001;
    pub const NOTIFY_LIFE_PROFESSION_RECIPE:     u32 = 0x52002;
    pub const SIGN_REWARD_NOTIFY:                u32 = 0x5E001;
}

pub mod match_ntf {
    pub const ENTER_MATCH_RESULT:  u32 = 0x04;
    pub const CANCEL_MATCH_RESULT: u32 = 0x05;
    pub const MATCH_READY_STATUS:  u32 = 0x06;
}

pub mod chit_chat_ntf {
    pub const NOTIFY_NEWEST_MSGS:      u32 = 0x01;
    pub const NOTIFY_BE_MUTED:         u32 = 0x02;
    pub const NOTIFY_ADD_PRIVATE_CHAT: u32 = 0x03;
    pub const NOTIFY_CLEAR_HISTORY:    u32 = 0x04;
}
