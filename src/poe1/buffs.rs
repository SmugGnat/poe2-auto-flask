//! Player-only PoE1 status effects, with a bounded vector and cached definitions.
use std::collections::HashMap;
use std::io;

use crate::entity::resolve_component;
use crate::process::Process;
use super::{invalid, plausible_ptr, read_player_id, Probe};
use super::offsets::*;

const MAX_EFFECTS: usize = 512;
const MAX_DEFINITIONS: usize = 2048;
const MAX_NAME_UNITS: usize = 128;
const SLOT_COUNT: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct Effect {
    pub name: String,
    pub flask_slot: Option<usize>,
    pub buff_type: u8,
    pub time_left: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EffectsSnapshot {
    pub slots_active: [bool; SLOT_COUNT],
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone)]
struct Definition {
    name: String,
    kind: u8,
}

#[derive(Debug, Default)]
pub struct EffectCache {
    component: Option<usize>,
    definitions: HashMap<usize, Definition>,
}

impl EffectCache {
    pub fn clear(&mut self) {
        self.component = None;
        self.definitions.clear();
    }
}

pub fn sample(process: &Process, probe: &Probe, cache: &mut EffectCache) -> io::Result<EffectsSnapshot> {
    if read_player_id(process, probe.local_player)? != probe.player_id {
        cache.clear();
        return Err(io::Error::new(io::ErrorKind::Interrupted, "PoE1 player identity changed"));
    }
    let component = match cache.component {
        Some(value) => value,
        None => {
            let value = resolve_component(process, probe.local_player, "Buffs")
                .ok_or_else(|| invalid("PoE1 player Buffs component unavailable"))?;
            cache.component = Some(value);
            value
        }
    };
    let header = process.read_bytes(component + BUFFS_EFFECTS, 16)?;
    let first = u64_at(&header, 0) as usize;
    let last = u64_at(&header, 8) as usize;
    let count = effect_count(first, last)?;
    let pointers = if count == 0 {
        Vec::new()
    } else {
        process.read_bytes(first, count * 8)?
    };

    let mut result = EffectsSnapshot {
        slots_active: [false; SLOT_COUNT],
        effects: Vec::with_capacity(count.min(32)),
    };
    for pointer in pointers.chunks_exact(8) {
        let at = u64_at(pointer, 0) as usize;
        if !plausible_ptr(at) {
            return Err(invalid("PoE1 effect vector contains invalid entry"));
        }
        let fields = process.read_bytes(at + STATUS_DEFINITION, STATUS_FLASK_SLOT + 2 - STATUS_DEFINITION)?;
        let definition = u64_at(&fields, 0) as usize;
        if definition == 0 {
            continue;
        }
        if !plausible_ptr(definition) {
            return Err(invalid("PoE1 effect definition pointer invalid"));
        }
        let time_left = f32_at(&fields, STATUS_TIME_LEFT - STATUS_DEFINITION);
        let total_time = f32_at(&fields, STATUS_TOTAL_TIME - STATUS_DEFINITION);
        let source_id = u32::from_le_bytes(
            fields[STATUS_SOURCE_ID - STATUS_DEFINITION..STATUS_SOURCE_ID - STATUS_DEFINITION + 4]
                .try_into().expect("effect source"),
        );
        let slot = i16::from_le_bytes(
            fields[STATUS_FLASK_SLOT - STATUS_DEFINITION..STATUS_FLASK_SLOT - STATUS_DEFINITION + 2]
                .try_into().expect("effect slot"),
        );
        if cache.definitions.len() >= MAX_DEFINITIONS {
            cache.definitions.clear();
        }
        if !cache.definitions.contains_key(&definition) {
            cache.definitions.insert(definition, read_definition(process, definition)?);
        }
        let detail = &cache.definitions[&definition];
        let active_slot = flask_slot(
            detail.kind,
            source_id,
            probe.player_id,
            slot,
            time_left,
            total_time,
        );
        if let Some(index) = active_slot {
            result.slots_active[index] = true;
        }
        result.effects.push(Effect {
            name: detail.name.clone(),
            flask_slot: active_slot,
            buff_type: detail.kind,
            time_left,
        });
    }
    Ok(result)
}

fn flask_slot(
    kind: u8,
    source: u32,
    player: u32,
    slot: i16,
    remaining: f32,
    total: f32,
) -> Option<usize> {
    (kind == 4
        && source == player
        && (0..SLOT_COUNT as i16).contains(&slot)
        && total.is_finite()
        && remaining.is_finite()
        && total > 0.0
        && remaining > 0.0
        && remaining <= total + 0.25)
        .then_some(slot as usize)
}

fn read_definition(process: &Process, at: usize) -> io::Result<Definition> {
    let fields = process.read_bytes(at, DEFINITION_BUFF_TYPE + 1)?;
    let pointer = u64_at(&fields, 0) as usize;
    if !plausible_ptr(pointer) {
        return Err(invalid("PoE1 buff definition name pointer invalid"));
    }
    let utf16 = process.read_bytes(pointer, MAX_NAME_UNITS * 2)?;
    let mut chars = Vec::with_capacity(32);
    let mut terminated = false;
    for pair in utf16.chunks_exact(2) {
        let code = u16::from_le_bytes([pair[0], pair[1]]);
        if code == 0 {
            terminated = true;
            break;
        }
        chars.push(code);
    }
    if !terminated {
        return Err(invalid("unterminated PoE1 buff name"));
    }
    let name = String::from_utf16(&chars).map_err(|_| invalid("invalid PoE1 buff name"))?;
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(invalid("invalid PoE1 buff definition name"));
    }
    Ok(Definition {
        name,
        kind: fields[DEFINITION_BUFF_TYPE],
    })
}

fn effect_count(first: usize, last: usize) -> io::Result<usize> {
    if first == 0 && last == 0 {
        return Ok(0);
    }
    if !plausible_ptr(first) || last < first || (last - first) % 8 != 0 {
        return Err(invalid("PoE1 status-effect vector malformed"));
    }
    let count = (last - first) / 8;
    if count > MAX_EFFECTS {
        return Err(invalid("PoE1 status-effect vector exceeds limit"));
    }
    Ok(count)
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("bounded pointer"))
}

fn f32_at(bytes: &[u8], at: usize) -> f32 {
    f32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded float"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flask_effect_requires_player_and_valid_slot() {
        assert_eq!(flask_slot(4, 18, 18, 4, 3.0, 5.0), Some(4));
        assert_eq!(flask_slot(4, 19, 18, 4, 3.0, 5.0), None);
        assert_eq!(flask_slot(3, 18, 18, 4, 3.0, 5.0), None);
        assert_eq!(flask_slot(4, 18, 18, 5, 3.0, 5.0), None);
        assert_eq!(flask_slot(4, 18, 18, -1, 3.0, 5.0), None);
        assert_eq!(flask_slot(4, 18, 18, 4, f32::NAN, 5.0), None);
        assert_eq!(flask_slot(4, 18, 18, 4, 6.0, 5.0), None);
    }

    #[test]
    fn effect_vector_is_bounded() {
        assert_eq!(effect_count(0, 0).unwrap(), 0);
        assert_eq!(effect_count(0x10000, 0x10028).unwrap(), 5);
        assert!(effect_count(0x10000, 0x11008).is_err());
        assert!(effect_count(0x10000, 0x10009).is_err());
    }
}
