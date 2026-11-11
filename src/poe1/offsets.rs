//! PoE1 offsets transcribed from references/GameHelper/GameOffsets.
//! Source comments identify the 2026-09-05 executable; live verification is required.
pub const GAME_STATE_PATTERN: [Option<u8>; 16] = [
    Some(0x48), Some(0x83), Some(0xEC), None,
    Some(0x48), Some(0x8B), Some(0xF1), Some(0x33),
    Some(0xED), Some(0x48), Some(0x39), Some(0x2D),
    None, None, None, None,
];
pub const GAME_STATE_DISPLACEMENT: usize = 12;
pub const GAME_STATE_CURRENT_VECTOR: usize = 0x08;
pub const GAME_STATE_STATES: usize = 0x48;
pub const IN_GAME_STATE_INDEX: usize = 4;
pub const GAME_STATE_SLOT_STRIDE: usize = 0x10;
pub const IN_GAME_AREA: usize = 0x218;
pub const IN_GAME_WORLD: usize = 0x270;
pub const AREA_PLAYER_INFO: usize = 0x968;
pub const PLAYER_INFO_LOCAL_PLAYER: usize = 0x08;
pub const ENTITY_DETAILS: usize = 0x08;
pub const ENTITY_METADATA: usize = 0x08;
pub const ENTITY_ID: usize = 0x88;
pub const LIFE_HEALTH: usize = 0x178;
pub const LIFE_MANA: usize = 0x1C8;
pub const LIFE_ES: usize = 0x210;
pub const VITAL_FLAT: usize = 0x10;
pub const VITAL_PERCENT: usize = 0x14;
pub const VITAL_TOTAL: usize = 0x2C;
pub const VITAL_CURRENT: usize = 0x30;
pub const WORLD_AREA_DETAILS: usize = 0xA0;
pub const WORLD_DETAILS_ROW: usize = 0xB0;
pub const SERVER_PLAYER_INVENTORIES: usize = 0xC9F8;
pub const INVENTORY_DESCRIPTOR_STRIDE: usize = 0x18;
pub const FLASK_INVENTORY_ID: i32 = 12;
pub const INVENTORY_SIZE: usize = 0x150;
pub const INVENTORY_ITEMS: usize = 0x170;
pub const CHARGES_INTERNAL: usize = 0x10;
pub const CHARGES_CURRENT: usize = 0x18;
pub const CHARGES_PER_USE: usize = 0x18;
pub const BUFFS_EFFECTS: usize = 0x160;
pub const STATUS_DEFINITION: usize = 0x08;
pub const STATUS_TOTAL_TIME: usize = 0x18;
pub const STATUS_TIME_LEFT: usize = 0x1C;
pub const STATUS_SOURCE_ID: usize = 0x28;
pub const STATUS_FLASK_SLOT: usize = 0x42;
pub const DEFINITION_BUFF_TYPE: usize = 0x67;
