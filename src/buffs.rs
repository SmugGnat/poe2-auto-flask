use std::collections::HashMap;
use std::io;

use crate::entity::resolve_component;
use crate::poe2::VitalsProbe;
use crate::process::Process;

const ENTITY_ID: usize = 0x88;

const BUFFS_STATUS_EFFECT_VECTOR: usize = 0x160;
const MAX_STATUS_EFFECTS: usize = 512;

const STATUS_BUFF_DEFINITION_PTR: usize = 0x08;
const STATUS_TOTAL_TIME: usize = 0x18;
const STATUS_TIME_LEFT: usize = 0x1C;
const STATUS_SOURCE_ENTITY_ID: usize = 0x28;
const STATUS_FLASK_SLOT: usize = 0x42;

const BUFF_DEFINITION_NAME_PTR: usize = 0x00;
const BUFF_DEFINITION_TYPE: usize = 0x67;
const MAX_BUFF_NAME_CHARS: usize = 128;
const POE2_FLASK_SLOT_COUNT: usize = 2;

// Auto mode reads one contiguous block containing every status field it needs.
const FAST_STATUS_START: usize = STATUS_BUFF_DEFINITION_PTR;
const FAST_STATUS_LEN: usize = STATUS_FLASK_SLOT + 2 - FAST_STATUS_START;

const KNOWN_FLASK_BUFFS: &[&str] = &[
    "flask_effect_life",
    "flask_effect_life_not_removed_when_full",
    "flask_effect_mana",
    "flask_effect_mana_not_removed_when_full",
    "flask_instant_mana_recovery_at_end_of_effect",
];

#[derive(Debug, Clone, PartialEq)]
pub struct DebugStatusEffect {
    pub definition_name: Option<String>,
    pub source_entity_id: Option<u32>,
    pub flask_slot: Option<i16>,
    pub buff_type: Option<u8>,
    pub total_time: Option<f32>,
    pub time_left: Option<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugFlaskEffects {
    pub status_effect_count: usize,
    pub raw: Vec<DebugStatusEffect>,
}

#[derive(Debug, Clone, Copy)]
pub struct FlaskProbe {
    pub local_player: usize,
    pub player_id: u32,
    pub buffs_component: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlaskLocks {
    pub slots: [bool; POE2_FLASK_SLOT_COUNT],
}

/// Caches definition classifications for the current player chain.
pub struct FlaskLockProbe {
    pub local_player: usize,
    pub player_id: u32,
    pub buffs_component: usize,
    definition_cache: HashMap<usize, bool>,
}

pub fn resolve_flask_probe(process: &Process, vitals: &VitalsProbe) -> io::Result<FlaskProbe> {
    let (player_id, buffs_component) = resolve_player_buffs(process, vitals)?;
    let probe = FlaskProbe {
        local_player: vitals.local_player,
        player_id,
        buffs_component,
    };

    // An empty status vector is valid; a malformed one is not.
    let _ = sample_flask_effects(process, &probe)?;
    Ok(probe)
}

pub fn resolve_flask_lock_probe(
    process: &Process,
    vitals: &VitalsProbe,
) -> io::Result<FlaskLockProbe> {
    let (player_id, buffs_component) = resolve_player_buffs(process, vitals)?;
    let mut probe = FlaskLockProbe {
        local_player: vitals.local_player,
        player_id,
        buffs_component,
        definition_cache: HashMap::new(),
    };

    // Validate the vector and cache definitions already active at startup.
    let _ = sample_flask_locks(process, &mut probe)?;
    Ok(probe)
}

fn resolve_player_buffs(process: &Process, vitals: &VitalsProbe) -> io::Result<(u32, usize)> {
    let player_id = read_u32(process, vitals.local_player + ENTITY_ID)?;
    if player_id == 0 || player_id == u32::MAX {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("implausible player entity id: {player_id}"),
        ));
    }

    let buffs_component =
        resolve_component(process, vitals.local_player, "Buffs").ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "named Buffs component could not be resolved on the local player",
            )
        })?;
    Ok((player_id, buffs_component))
}

/// Reads active flask-slot status with the minimum data needed by auto mode.
pub fn sample_flask_locks(process: &Process, probe: &mut FlaskLockProbe) -> io::Result<FlaskLocks> {
    let current_player_id = read_u32(process, probe.local_player + ENTITY_ID)?;
    if current_player_id != probe.player_id {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "local player entity id changed",
        ));
    }

    let (first, count) = read_status_vector(process, probe.buffs_component)?;
    if count == 0 {
        return Ok(FlaskLocks { slots: [false; 2] });
    }

    let pointer_bytes = process.read_bytes(first, count * 8)?;
    let mut slots = [false; POE2_FLASK_SLOT_COUNT];

    for pointer in pointer_bytes.chunks_exact(8) {
        let effect_ptr =
            u64::from_le_bytes(pointer.try_into().expect("8-byte pointer chunk")) as usize;
        if !plausible_ptr(effect_ptr) {
            return Err(invalid(
                "Buffs status-effect vector contains an invalid pointer",
            ));
        }

        let bytes = process.read_bytes(effect_ptr + FAST_STATUS_START, FAST_STATUS_LEN)?;
        let definition =
            read_u64_from(&bytes, STATUS_BUFF_DEFINITION_PTR - FAST_STATUS_START) as usize;
        let total_time = read_f32_from(&bytes, STATUS_TOTAL_TIME - FAST_STATUS_START);
        let time_left = read_f32_from(&bytes, STATUS_TIME_LEFT - FAST_STATUS_START);
        let source = read_u32_from(&bytes, STATUS_SOURCE_ENTITY_ID - FAST_STATUS_START);
        let slot = read_i16_from(&bytes, STATUS_FLASK_SLOT - FAST_STATUS_START);

        if source != probe.player_id
            || !(0..POE2_FLASK_SLOT_COUNT as i16).contains(&slot)
            || !total_time.is_finite()
            || !time_left.is_finite()
            || !(0.05..=60.0).contains(&total_time)
            || time_left <= 0.0
            || time_left > total_time + 0.25
            || !plausible_ptr(definition)
        {
            continue;
        }

        let is_lock = match probe.definition_cache.get(&definition) {
            Some(&cached) => cached,
            None => {
                let classified = classify_flask_lock_definition(process, definition)?;
                probe.definition_cache.insert(definition, classified);
                classified
            }
        };

        if is_lock {
            slots[slot as usize] = true;
            if slots.iter().all(|&locked| locked) {
                break;
            }
        }
    }

    Ok(FlaskLocks { slots })
}

fn classify_flask_lock_definition(process: &Process, definition: usize) -> io::Result<bool> {
    // Common timed flask effects use type 4 and need no name lookup.
    let definition_bytes = process.read_bytes(definition, BUFF_DEFINITION_TYPE + 1)?;
    if definition_bytes[BUFF_DEFINITION_TYPE] == 4 {
        return Ok(true);
    }

    let name_ptr = read_u64_from(&definition_bytes, BUFF_DEFINITION_NAME_PTR) as usize;
    if !plausible_ptr(name_ptr) {
        return Ok(false);
    }
    let name = read_utf16_z(process, name_ptr, MAX_BUFF_NAME_CHARS)?;

    // Some instant-recovery modifiers expose a short visual_display_buff slot state.
    Ok(KNOWN_FLASK_BUFFS.contains(&name.as_str()) || name == "visual_display_buff")
}

/// Full status reader used by --debug.
pub fn sample_flask_effects(
    process: &Process,
    probe: &FlaskProbe,
) -> io::Result<DebugFlaskEffects> {
    let current_player_id = read_u32(process, probe.local_player + ENTITY_ID)?;
    if current_player_id != probe.player_id {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "local player entity id changed",
        ));
    }

    let (first, count) = read_status_vector(process, probe.buffs_component)?;
    let mut raw = Vec::with_capacity(count);

    if count != 0 {
        let pointer_bytes = process.read_bytes(first, count * 8)?;
        for pointer in pointer_bytes.chunks_exact(8) {
            let effect_ptr =
                u64::from_le_bytes(pointer.try_into().expect("8-byte pointer chunk")) as usize;
            if plausible_ptr(effect_ptr) {
                raw.push(read_debug_status_effect(process, effect_ptr));
            }
        }
    }

    Ok(DebugFlaskEffects {
        status_effect_count: count,
        raw,
    })
}

fn read_debug_status_effect(process: &Process, address: usize) -> DebugStatusEffect {
    let bytes = process
        .read_bytes(address + FAST_STATUS_START, FAST_STATUS_LEN)
        .ok();

    let definition = bytes
        .as_ref()
        .map(|data| read_u64_from(data, STATUS_BUFF_DEFINITION_PTR - FAST_STATUS_START) as usize)
        .filter(|&value| plausible_ptr(value));

    let definition_name = definition.and_then(|definition| {
        read_definition_name(process, definition + BUFF_DEFINITION_NAME_PTR)
    });
    let buff_type =
        definition.and_then(|definition| read_u8(process, definition + BUFF_DEFINITION_TYPE).ok());

    DebugStatusEffect {
        definition_name,
        source_entity_id: bytes
            .as_ref()
            .map(|data| read_u32_from(data, STATUS_SOURCE_ENTITY_ID - FAST_STATUS_START)),
        flask_slot: bytes
            .as_ref()
            .map(|data| read_i16_from(data, STATUS_FLASK_SLOT - FAST_STATUS_START)),
        buff_type,
        total_time: bytes
            .as_ref()
            .map(|data| read_f32_from(data, STATUS_TOTAL_TIME - FAST_STATUS_START))
            .filter(|value| value.is_finite()),
        time_left: bytes
            .as_ref()
            .map(|data| read_f32_from(data, STATUS_TIME_LEFT - FAST_STATUS_START))
            .filter(|value| value.is_finite()),
    }
}

fn read_status_vector(process: &Process, buffs_component: usize) -> io::Result<(usize, usize)> {
    let header = process.read_bytes(buffs_component + BUFFS_STATUS_EFFECT_VECTOR, 16)?;
    let first = read_u64_from(&header, 0) as usize;
    let last = read_u64_from(&header, 8) as usize;

    if first == 0 && last == 0 {
        return Ok((0, 0));
    }
    if !plausible_ptr(first) || last < first {
        return Err(invalid("Buffs status-effect vector has an invalid range"));
    }
    let bytes = last - first;
    if bytes % 8 != 0 {
        return Err(invalid("Buffs status-effect vector is not pointer-aligned"));
    }
    let count = bytes / 8;
    if count > MAX_STATUS_EFFECTS {
        return Err(invalid(format!("implausible status-effect count: {count}")));
    }
    Ok((first, count))
}

fn read_definition_name(process: &Process, name_ptr_field: usize) -> Option<String> {
    let name_ptr = read_ptr(process, name_ptr_field)?;
    let name = read_utf16_z(process, name_ptr, MAX_BUFF_NAME_CHARS).ok()?;
    if name.is_empty() || name.chars().any(char::is_control) {
        return None;
    }
    Some(name)
}

fn read_ptr(process: &Process, address: usize) -> Option<usize> {
    let value = process.read_u64(address).ok()? as usize;
    plausible_ptr(value).then_some(value)
}

fn plausible_ptr(value: usize) -> bool {
    value >= 0x1_0000
}

fn read_utf16_z(process: &Process, address: usize, max_units: usize) -> io::Result<String> {
    let bytes = process.read_bytes(address, max_units.saturating_mul(2))?;
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect::<Vec<_>>();
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "unterminated UTF-16 string"))?;
    Ok(String::from_utf16_lossy(&units[..end]))
}

fn read_u8(process: &Process, address: usize) -> io::Result<u8> {
    Ok(process.read_bytes(address, 1)?[0])
}

fn read_u32(process: &Process, address: usize) -> io::Result<u32> {
    let bytes = process.read_bytes(address, 4)?;
    Ok(u32::from_le_bytes(
        bytes.try_into().expect("fixed 4-byte read"),
    ))
}

fn read_u64_from(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated 8-byte field"),
    )
}

fn read_u32_from(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated 4-byte field"),
    )
}

fn read_i16_from(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes(
        bytes[offset..offset + 2]
            .try_into()
            .expect("validated 2-byte field"),
    )
}

fn read_f32_from(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated 4-byte field"),
    )
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
