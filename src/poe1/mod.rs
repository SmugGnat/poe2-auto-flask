//! Validated read-only PoE1 memory reader. No input, injection or memory writes.
pub mod activation;
pub mod area;
pub mod automation;
pub mod bindings;
pub mod buffs;
pub mod charge_cost;
pub mod chat;
pub mod debug;
pub mod decision;
pub mod flasks;
pub mod movement;
pub mod nearby;
mod offsets;
pub mod rotation;
pub mod types;

use std::io;

use crate::entity::resolve_component;
use crate::poe2::{LiveVitals, VitalPool};
use crate::process::Process;
use offsets::*;

const MAX_METADATA_CHARS: usize = 256;
const MAX_VITAL_TOTAL: i32 = 10_000_000;

#[derive(Debug)]
pub struct Probe {
    game_state_slot: usize,
    game_state: usize,
    pub in_game_state: usize,
    pub area_instance: usize,
    pub local_player: usize,
    pub player_id: u32,
    pub server_data: usize,
    pub world_data: usize,
    life_component: usize,
    pub metadata: String,
}

pub fn probe(process: &Process) -> io::Result<Probe> {
    let section = process.find_pe_section(".text")?;
    let matches = process.scan_pattern(section.address, section.size, &GAME_STATE_PATTERN)?;
    if matches.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "PoE1 Game States signature not found (possibly wrong game or outdated offsets)",
        ));
    }

    let mut failure = None;
    for instruction in matches {
        let attempt = (|| {
            let displacement = process.read_i32(instruction + GAME_STATE_DISPLACEMENT)? as i64;
            let address =
                (instruction as i128) + (GAME_STATE_PATTERN.len() as i128) + (displacement as i128);
            if !(0x1_0000..=usize::MAX as i128).contains(&address) {
                return Err(invalid("invalid RIP-relative GameState address"));
            }
            let slot = address as usize;
            let state = read_ptr(process, slot)?;
            from_root(process, slot, state)
        })();
        match attempt {
            Ok(found) => return Ok(found),
            Err(error) => failure = Some(error),
        }
    }
    Err(failure.unwrap_or_else(|| invalid("PoE1 local player could not be validated")))
}

pub fn refresh(process: &Process, previous: &Probe) -> io::Result<Probe> {
    // Reuse the executable signature's static slot; don't rescan on ordinary area changes.
    match read_ptr(process, previous.game_state_slot) {
        Ok(state) => from_root(process, previous.game_state_slot, state),
        Err(_) => probe(process),
    }
}

fn from_root(process: &Process, slot: usize, state: usize) -> io::Result<Probe> {
    let in_game_state = read_ptr(
        process,
        state + GAME_STATE_STATES + IN_GAME_STATE_INDEX * GAME_STATE_SLOT_STRIDE,
    )?;
    if current_state(process, state)? != in_game_state {
        return Err(interrupted("PoE1 is not in the active InGameState"));
    }

    let area_instance = read_ptr(process, in_game_state + IN_GAME_AREA)?;
    let world_data = read_ptr(process, in_game_state + IN_GAME_WORLD)?;
    let player_info = area_instance + AREA_PLAYER_INFO;
    let server_data = read_ptr(process, player_info)?;
    let local_player = read_ptr(process, player_info + PLAYER_INFO_LOCAL_PLAYER)?;
    let metadata = read_metadata(process, local_player)?;
    if !metadata.starts_with("Metadata/Characters/") {
        return Err(invalid("PoE1 player metadata is not a character"));
    }
    let player_id = read_player_id(process, local_player)?;
    let life_component = resolve_component(process, local_player, "Life")
        .ok_or_else(|| invalid("PoE1 local player Life component was not found"))?;

    let result = Probe {
        game_state_slot: slot,
        game_state: state,
        in_game_state,
        area_instance,
        local_player,
        player_id,
        server_data,
        world_data,
        life_component,
        metadata,
    };
    sample_vitals(process, &result)?;
    Ok(result)
}

/// Requires the active-state vector to identify the InGameState, not just an old state pointer.
fn current_state(process: &Process, state: usize) -> io::Result<usize> {
    let first = read_ptr(process, state + GAME_STATE_CURRENT_VECTOR)?;
    let last = read_ptr(process, state + GAME_STATE_CURRENT_VECTOR + 8)?;
    if last < first || last - first < 16 || last - first > 1024 || (last - first) % 8 != 0 {
        return Err(invalid("PoE1 current GameState vector is invalid"));
    }
    read_ptr(process, last - 16)
}

pub fn sample_vitals(process: &Process, probe: &Probe) -> io::Result<LiveVitals> {
    if read_ptr(process, probe.game_state_slot)? != probe.game_state
        || current_state(process, probe.game_state)? != probe.in_game_state
        || read_ptr(process, probe.in_game_state + IN_GAME_AREA)? != probe.area_instance
        || read_ptr(process, probe.area_instance + AREA_PLAYER_INFO)? != probe.server_data
        || read_ptr(
            process,
            probe.area_instance + AREA_PLAYER_INFO + PLAYER_INFO_LOCAL_PLAYER,
        )? != probe.local_player
        || read_player_id(process, probe.local_player)? != probe.player_id
    {
        return Err(interrupted("PoE1 player, area, or game state changed"));
    }

    // Three pools span 0xBC bytes. Read them in a single cross-process call.
    let first = LIFE_HEALTH + VITAL_FLAT;
    let last = LIFE_ES + VITAL_CURRENT + 4;
    let bytes = process.read_bytes(probe.life_component + first, last - first)?;
    let health = pool_at(&bytes, 0);
    let mana = pool_at(&bytes, LIFE_MANA - LIFE_HEALTH);
    let es = pool_at(&bytes, LIFE_ES - LIFE_HEALTH);

    if !valid_pool(health, false) || !valid_pool(mana, true) || !valid_pool(es, true) {
        return Err(invalid("PoE1 vital fields failed range validation"));
    }

    Ok(LiveVitals {
        health,
        mana: (mana.total > 0).then_some(mana),
        energy_shield: (es.total > 0).then_some(es),
    })
}

fn pool_at(bytes: &[u8], offset: usize) -> VitalPool {
    VitalPool {
        reserved_flat: i32_at(bytes, offset),
        reserved_fraction: i32_at(bytes, offset + VITAL_PERCENT - VITAL_FLAT),
        total: i32_at(bytes, offset + VITAL_TOTAL - VITAL_FLAT),
        current: i32_at(bytes, offset + VITAL_CURRENT - VITAL_FLAT),
    }
}

fn i32_at(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("bounded vital field"),
    )
}

fn valid_pool(pool: VitalPool, allow_zero: bool) -> bool {
    if allow_zero && pool.total == 0 {
        return pool.current == 0
            && pool.reserved_flat == 0
            && (0..=10_000).contains(&pool.reserved_fraction);
    }
    (1..=MAX_VITAL_TOTAL).contains(&pool.total)
        && (0..=pool.total + 1).contains(&pool.current)
        && (0..=pool.total).contains(&pool.reserved_flat)
        && (0..=10_000).contains(&pool.reserved_fraction)
}

pub fn read_player_id(process: &Process, player: usize) -> io::Result<u32> {
    let value = process.read_bytes(player + ENTITY_ID, 4)?;
    let id = u32::from_le_bytes(value.try_into().expect("four byte ID"));
    if id == 0 || id == u32::MAX {
        return Err(invalid("invalid PoE1 local player entity ID"));
    }
    Ok(id)
}

pub fn read_metadata(process: &Process, entity: usize) -> io::Result<String> {
    let details = read_ptr(process, entity + ENTITY_DETAILS)?;
    let at = details + ENTITY_METADATA;
    let len = process.read_i32(at + 0x10)?;
    if !(1..=MAX_METADATA_CHARS as i32).contains(&len) {
        return Err(invalid("invalid PoE1 entity metadata length"));
    }
    let location = if len < 8 { at } else { read_ptr(process, at)? };
    let text = process.read_utf16_len(location, len as usize)?;
    if text.chars().any(char::is_control) {
        return Err(invalid("invalid PoE1 metadata characters"));
    }
    Ok(text)
}

pub fn read_ptr(process: &Process, address: usize) -> io::Result<usize> {
    let value = process.read_u64(address)? as usize;
    if plausible_ptr(value) {
        Ok(value)
    } else {
        Err(invalid(format!("invalid pointer at 0x{address:016X}")))
    }
}

pub fn plausible_ptr(value: usize) -> bool {
    value >= 0x1_0000
}

pub fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn interrupted(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_read_covers_three_pools() {
        assert_eq!(LIFE_ES + VITAL_CURRENT + 4 - LIFE_HEALTH - VITAL_FLAT, 0xBC);
    }

    #[test]
    fn valid_pool_fails_closed_on_corruption() {
        let normal = VitalPool {
            current: 45,
            total: 100,
            reserved_flat: 0,
            reserved_fraction: 0,
        };
        assert!(valid_pool(normal, false));
        assert!(!valid_pool(
            VitalPool {
                current: 102,
                ..normal
            },
            false
        ));
        assert!(!valid_pool(
            VitalPool {
                reserved_fraction: 10001,
                ..normal
            },
            false
        ));
        assert!(!valid_pool(VitalPool { total: 0, ..normal }, false));
        assert!(valid_pool(
            VitalPool {
                current: 0,
                total: 0,
                ..normal
            },
            true
        ));
    }

    #[test]
    fn pool_byte_layout_matches_gamehelper() {
        let mut bytes = [0u8; 0xBC];
        let o = LIFE_ES - LIFE_HEALTH;
        bytes[o..o + 4].copy_from_slice(&12i32.to_le_bytes());
        bytes[o + 4..o + 8].copy_from_slice(&3000i32.to_le_bytes());
        bytes[o + 0x1C..o + 0x20].copy_from_slice(&300i32.to_le_bytes());
        bytes[o + 0x20..o + 0x24].copy_from_slice(&50i32.to_le_bytes());
        let pool = pool_at(&bytes, o);
        assert_eq!(pool.current, 50);
        assert_eq!(pool.total, 300);
        assert_eq!(pool.reserved_flat, 12);
        assert_eq!(pool.reserved_fraction, 3000);
    }
}
