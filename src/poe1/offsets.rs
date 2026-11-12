//! PoE1 offsets transcribed from references/GameHelper/GameOffsets.
//! Source comments identify the 2026-09-05 executable; live verification is required.
pub(super) const GAME_STATE_PATTERN: [Option<u8>; 16] = [
    Some(0x48),
    Some(0x83),
    Some(0xEC),
    None,
    Some(0x48),
    Some(0x8B),
    Some(0xF1),
    Some(0x33),
    Some(0xED),
    Some(0x48),
    Some(0x39),
    Some(0x2D),
    None,
    None,
    None,
    None,
];
pub(super) const GAME_STATE_DISPLACEMENT: usize = 12;
pub(super) const GAME_STATE_CURRENT_VECTOR: usize = 0x08;
pub(super) const GAME_STATE_STATES: usize = 0x48;
pub(super) const IN_GAME_STATE_INDEX: usize = 4;
pub(super) const GAME_STATE_SLOT_STRIDE: usize = 0x10;
pub(super) const IN_GAME_AREA: usize = 0x218;
pub(super) const IN_GAME_WORLD: usize = 0x270;
pub(super) const AREA_PLAYER_INFO: usize = 0x968;
pub(super) const PLAYER_INFO_LOCAL_PLAYER: usize = 0x08;
pub(super) const ENTITY_DETAILS: usize = 0x08;
pub(super) const ENTITY_METADATA: usize = 0x08;
pub(super) const ENTITY_ID: usize = 0x88;
pub(super) const LIFE_HEALTH: usize = 0x178;
pub(super) const LIFE_MANA: usize = 0x1C8;
pub(super) const LIFE_ES: usize = 0x210;
pub(super) const VITAL_FLAT: usize = 0x10;
pub(super) const VITAL_PERCENT: usize = 0x14;
pub(super) const VITAL_TOTAL: usize = 0x2C;
pub(super) const VITAL_CURRENT: usize = 0x30;
pub(super) const WORLD_AREA_DETAILS: usize = 0xA0;
pub(super) const WORLD_DETAILS_ROW: usize = 0xB0;
pub(super) const SERVER_PLAYER_INVENTORIES: usize = 0xC9F8;
pub(super) const INVENTORY_DESCRIPTOR_STRIDE: usize = 0x18;
pub(super) const FLASK_INVENTORY_ID: i32 = 12;
pub(super) const INVENTORY_SIZE: usize = 0x150;
pub(super) const INVENTORY_ITEMS: usize = 0x170;
pub(super) const CHARGES_INTERNAL: usize = 0x10;
pub(super) const CHARGES_CURRENT: usize = 0x18;
pub(super) const CHARGES_PER_USE: usize = 0x18;
pub(super) const BUFFS_EFFECTS: usize = 0x160;
pub(super) const STATUS_DEFINITION: usize = 0x08;
pub(super) const STATUS_TOTAL_TIME: usize = 0x18;
pub(super) const STATUS_TIME_LEFT: usize = 0x1C;
pub(super) const STATUS_SOURCE_ID: usize = 0x28;
pub(super) const STATUS_FLASK_SLOT: usize = 0x42;
pub(super) const DEFINITION_BUFF_TYPE: usize = 0x67;

// From chat.rs:
pub(super) const IN_GAME_UI: usize = 0x978;
pub(super) const CHAT_PARENT: usize = 0x540;
pub(super) const CHAT_BACKGROUND_COLOR: usize = 0x288;

// From movement.rs:
pub(super) const ACTOR_ANIMATION_ID: usize = 0x248;

// Actor active-skill and cooldown structures (2026-09 GameHelper candidate layout).
pub(super) const ACTOR_SKILLS: usize = 0x6F0;
pub(super) const ACTOR_COOLDOWNS: usize = 0x708;
pub(super) const SKILL_DETAIL_ID: usize = 0x40;
pub(super) const SKILL_DETAIL_GRANTED_ROW: usize = 0x48;
pub(super) const SKILL_DETAIL_CANNOT_USE: usize = 0xB1;
pub(super) const COOLDOWN_SKILL_ID: usize = 0x3C;
pub(super) const COOLDOWN_USES: usize = 0x30;
pub(super) const COOLDOWN_VECTOR: usize = 0x10;

// From charge_cost.rs:
pub(super) const STATS_DATA_PTR: usize = 0x20;
pub(super) const STATS_VECTOR: usize = 0xF0;
pub(super) const STAT_STRIDE: usize = 8;
pub(super) const EXPLICIT_MODS_VECTOR: usize = 0xD8;
pub(super) const MOD_STRIDE: usize = 0x38;
pub(super) const MOD_VALUE: usize = 0x18;
pub(super) const MOD_DATA_PTR: usize = 0x28;

// Monster-side offsets. ObjectMagicProperties has a different base from
// the inventory item's Mods component.
pub(super) mod nearby {
    pub(crate) const AREA_AWAKE_ENTITIES: usize = 0xA28;
    pub(crate) const MAP_NODE_SIZE: usize = 0x30;
    pub(crate) const MAP_KEY_ID: usize = 0x20;
    pub(crate) const MAP_ENTITY_PTR: usize = 0x28;
    pub(crate) const MAP_IS_NIL: usize = 0x19;
    pub(crate) const RENDER_POSITION: usize = 0x120;
    pub(crate) const LIFE_CURRENT: usize = 0x178 + 0x30;
    pub(crate) const POSITIONED_REACTION: usize = 0x1E0;
    pub(crate) const TARGETABLE_FLAG: usize = 0x50;
    pub(crate) const MONSTER_RARITY: usize = 0x90 + 0xB4;
}
