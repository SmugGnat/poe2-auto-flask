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

pub(super) const MAX_METADATA_CHARS: usize = 256;

