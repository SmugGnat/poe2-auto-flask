//! PoE2 executable structure offsets and signature, isolated from the reader.
//! Changes to these offsets require independent live validation.

// GameStates global-slot signature. Immediate operands after the RIP-relative reference are
// wildcarded; the stable post-call instructions keep the pattern unique.
pub(super) const GAME_STATE_PATTERN: [Option<u8>; 31] = [
    Some(0x48),
    Some(0x39),
    Some(0x2D),
    None,
    None,
    None,
    None,
    Some(0x0F),
    Some(0x85),
    None,
    None,
    None,
    None,
    Some(0xB9),
    None,
    None,
    None,
    None,
    Some(0xE8),
    None,
    None,
    None,
    None,
    Some(0x48),
    Some(0x8B),
    Some(0xF8),
    Some(0x48),
    Some(0x89),
    Some(0x44),
    Some(0x24),
    Some(0x50),
];

pub(super) const GAME_STATE_CURRENT_STATE_PTR: usize = 0x08;
pub(super) const GAME_STATE_STATES: usize = 0x48;
pub(super) const GAME_STATE_SLOT_STRIDE: usize = 0x10;
pub(super) const GAME_STATE_SLOT_COUNT: usize = 12;

// Primary offsets are validated before use. Narrow fallback searches tolerate minor layout drift.
pub(super) const IN_GAME_STATE_AREA_INSTANCE: usize = 0x2A0;
pub(super) const AREA_INSTANCE_LOCAL_PLAYER: usize = 0x5D0;

pub(super) const ENTITY_DETAILS_PTR: usize = 0x08;
pub(super) const ENTITY_DETAILS_NAME: usize = 0x08;

pub(super) const LIFE_HEALTH: usize = 0x1B0;
pub(super) const LIFE_MANA: usize = 0x208;
pub(super) const LIFE_ENERGY_SHIELD: usize = 0x248;

pub(super) const VITAL_RESERVED_FLAT: usize = 0x10;
pub(super) const VITAL_RESERVED_FRACTION: usize = 0x14;
pub(super) const VITAL_TOTAL: usize = 0x2C;
pub(super) const VITAL_CURRENT: usize = 0x30;
pub(super) const VITAL_READ_LEN: usize = VITAL_CURRENT + 4 - VITAL_RESERVED_FLAT;

/// Fixed PoE2 WorldData and WorldArea offsets.
pub(super) mod area {
    pub(crate) const IN_GAME_WORLD_DATA: usize = 0x368;
    pub(crate) const WORLD_DATA_SEARCH_START: usize = 0x340;
    pub(crate) const WORLD_DATA_SEARCH_END: usize = 0x390;
    pub(crate) const WORLD_DATA_SEARCH_STEP: usize = 8;
    pub(crate) const WORLD_DATA_AREA_DETAILS: usize = 0x98;
    pub(crate) const WORLD_AREA_DETAILS_ROW: usize = 0x98;
    pub(crate) const AREA_ROW_ID_PTR: usize = 0x00;
    pub(crate) const AREA_ROW_NAME_PTR: usize = 0x08;
    pub(crate) const AREA_ROW_ACT: usize = 0x10;
    pub(crate) const AREA_ROW_IS_TOWN: usize = 0x14;
    pub(crate) const AREA_ROW_HAS_WAYPOINT: usize = 0x15;
    pub(crate) const AREA_ROW_READ_LEN: usize = 0x16;
}

/// Fixed PoE2 buff/status structures.
pub(super) mod buffs {
    pub(crate) const ENTITY_ID: usize = 0x88;
    pub(crate) const BUFFS_STATUS_EFFECT_VECTOR: usize = 0x160;
    pub(crate) const STATUS_BUFF_DEFINITION_PTR: usize = 0x08;
    pub(crate) const STATUS_TOTAL_TIME: usize = 0x18;
    pub(crate) const STATUS_TIME_LEFT: usize = 0x1C;
    pub(crate) const STATUS_SOURCE_ENTITY_ID: usize = 0x28;
    pub(crate) const STATUS_FLASK_SLOT: usize = 0x42;
    pub(crate) const BUFF_DEFINITION_NAME_PTR: usize = 0x00;
    pub(crate) const BUFF_DEFINITION_TYPE: usize = 0x67;
}

/// Fixed PoE2 flask inventory and charge structures.
pub(super) mod flasks {
    pub(crate) const PLAYER_INFO_LOCAL_PLAYER: usize = 0x20;
    pub(crate) const SERVER_DATA_PLAYER_DATA_VECTOR: usize = 0x48;
    pub(crate) const PLAYER_DATA_INVENTORIES_VECTOR: usize = 0x320;
    pub(crate) const INVENTORY_DESCRIPTOR_STRIDE: usize = 0x18;
    pub(crate) const INVENTORY_TOTAL_BOXES: usize = 0x150;
    pub(crate) const INVENTORY_ITEM_LIST: usize = 0x170;
    pub(crate) const INVENTORY_ITEM_ENTITY: usize = 0x00;
    pub(crate) const CHARGES_INTERNAL_PTR: usize = 0x10;
    pub(crate) const CHARGES_CURRENT: usize = 0x18;
    pub(crate) const CHARGES_INTERNAL_PER_USE: usize = 0x18;
}
