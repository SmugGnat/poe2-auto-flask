//! Validated read-only PoE1 memory reader. No input, injection or memory writes.
pub mod activation;
pub mod adapter;
pub mod area;
pub mod automation;
pub mod bindings;
pub mod buffs;
pub mod charge_cost;
pub mod chat;
pub mod debug;
pub mod decision;
pub mod flasks;
mod layout;
pub mod movement;
pub mod nearby;
mod offsets;
pub mod recovery;
pub mod rotation;
pub mod types;

use std::io;

use crate::entity::resolve_component;
use crate::game_state::{LiveVitals, VitalPool};
use crate::process::Process;
use layout::Layout;
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
    layout: Layout,
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
            from_root(process, slot, state, None)
        })();
        match attempt {
            Ok(found) => {
                report_recovery(&found, None);
                return Ok(found);
            }
            Err(error) => failure = Some(error),
        }
    }
    Err(failure.unwrap_or_else(|| invalid("PoE1 local player could not be validated")))
}

pub fn refresh(process: &Process, previous: &Probe) -> io::Result<Probe> {
    // Reuse the stable global-slot pointer and previously validated offsets
    // first. A fresh signature scan only happens if that pointer disappeared.
    match read_ptr(process, previous.game_state_slot) {
        Ok(state) => {
            let found = from_root(
                process,
                previous.game_state_slot,
                state,
                Some(previous.layout),
            )?;
            report_recovery(&found, Some(previous.layout));
            Ok(found)
        }
        Err(_) => probe(process),
    }
}

fn report_recovery(probe: &Probe, previous: Option<Layout>) {
    if probe.layout != Layout::default() && previous != Some(probe.layout) {
        println!(
            "PoE1 validated recovered offsets: area 0x{:X}, WorldData 0x{:X}, PlayerInfo 0x{:X}, Life delta {:+#X} (session only)",
            probe.layout.area,
            probe.layout.world,
            probe.layout.player_info,
            probe.layout.vital_delta,
        );
    }
}

fn from_root(
    process: &Process,
    slot: usize,
    state: usize,
    previous: Option<Layout>,
) -> io::Result<Probe> {
    // The GameStates root/index is *not* guessed. Recovery is limited to
    // player and area chains with independently verifiable structures.
    let in_game_state = read_ptr(
        process,
        state + GAME_STATE_STATES + IN_GAME_STATE_INDEX * GAME_STATE_SLOT_STRIDE,
    )?;
    if current_state(process, state)? != in_game_state {
        return Err(interrupted("PoE1 is not in the active InGameState"));
    }

    // Unchanged known offsets take the fast path: no candidate enumeration.
    if let Some(layout) = previous {
        if let Ok(probe) = try_layout(process, slot, state, in_game_state, layout) {
            return Ok(probe);
        }
    }
    if previous != Some(Layout::default()) {
        if let Ok(probe) = try_layout(process, slot, state, in_game_state, Layout::default()) {
            return Ok(probe);
        }
    }

    // Only attach/refresh can reach this branch. Each candidate must have a
    // real character entity, stable player identity, a Life component with
    // three plausible pools, and an independently validated WorldArea row.
    // Reject ambiguity instead of taking the first pointer that looks valid.
    recover_layout(process, slot, state, in_game_state)
}

fn try_layout(
    process: &Process,
    slot: usize,
    state: usize,
    in_game_state: usize,
    layout: Layout,
) -> io::Result<Probe> {
    let area_instance = read_ptr(process, in_game_state + layout.area)?;
    let (server_data, local_player) = player_info(process, area_instance, layout.player_info)?;
    let metadata = character_metadata(process, local_player)?;
    let player_id = read_player_id(process, local_player)?;
    let life_component = resolve_component(process, local_player, "Life")
        .ok_or_else(|| invalid("PoE1 local player Life component was not found"))?;
    let world_data = read_ptr(process, in_game_state + layout.world)?;
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
        layout,
        metadata,
    };
    // Validate *both* the player vital chain and the independently parsed area
    // before granting an offset repair to the automation session.
    sample_vitals(process, &result)?;
    area::read(process, &result)?;
    Ok(result)
}

fn player_info(
    process: &Process,
    area_instance: usize,
    offset: usize,
) -> io::Result<(usize, usize)> {
    let bytes = process.read_bytes(area_instance + offset, 16)?;
    let server = u64::from_le_bytes(bytes[0..8].try_into().expect("server pointer")) as usize;
    let player = u64::from_le_bytes(bytes[8..16].try_into().expect("player pointer")) as usize;
    if !plausible_ptr(server) || !plausible_ptr(player) || server == player {
        return Err(invalid("invalid PoE1 PlayerInfo pair"));
    }
    Ok((server, player))
}

fn character_metadata(process: &Process, player: usize) -> io::Result<String> {
    let metadata = read_metadata(process, player)?;
    if !metadata.starts_with("Metadata/Characters/") {
        return Err(invalid("PoE1 player metadata is not a character"));
    }
    Ok(metadata)
}

fn recover_layout(
    process: &Process,
    slot: usize,
    state: usize,
    in_game_state: usize,
) -> io::Result<Probe> {
    let mut found: Option<Probe> = None;
    for area_offset in layout::area_offsets() {
        let Ok(area_instance) = read_ptr(process, in_game_state + area_offset) else {
            continue;
        };
        for info_offset in layout::player_info_offsets() {
            let Ok((server_data, local_player)) = player_info(process, area_instance, info_offset)
            else {
                continue;
            };
            let Ok(metadata) = character_metadata(process, local_player) else {
                continue;
            };
            let Ok(player_id) = read_player_id(process, local_player) else {
                continue;
            };
            let Some(life_component) = resolve_component(process, local_player, "Life") else {
                continue;
            };
            for vital_delta in layout::vital_deltas() {
                if read_vital_pools(process, life_component, vital_delta).is_err() {
                    continue;
                }
                for world_offset in layout::world_offsets() {
                    let Ok(world_data) = read_ptr(process, in_game_state + world_offset) else {
                        continue;
                    };
                    let candidate = Probe {
                        game_state_slot: slot,
                        game_state: state,
                        in_game_state,
                        area_instance,
                        local_player,
                        player_id,
                        server_data,
                        world_data,
                        life_component,
                        layout: Layout {
                            area: area_offset,
                            world: world_offset,
                            player_info: info_offset,
                            vital_delta,
                        },
                        metadata: metadata.clone(),
                    };
                    if sample_vitals(process, &candidate).is_err()
                        || area::read(process, &candidate).is_err()
                    {
                        continue;
                    }
                    if found.is_some() {
                        return Err(invalid("ambiguous PoE1 recovered offsets; refusing input"));
                    }
                    found = Some(candidate);
                }
            }
        }
    }
    found.ok_or_else(|| {
        invalid("PoE1 layout recovery found no fully validated player/area chain (offset update required)")
    })
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
        || read_ptr(process, probe.in_game_state + probe.layout.area)? != probe.area_instance
        || read_ptr(process, probe.area_instance + probe.layout.player_info)? != probe.server_data
        || read_ptr(
            process,
            probe.area_instance + probe.layout.player_info + PLAYER_INFO_LOCAL_PLAYER,
        )? != probe.local_player
        || read_player_id(process, probe.local_player)? != probe.player_id
    {
        return Err(interrupted("PoE1 player, area, or game state changed"));
    }
    read_vital_pools(process, probe.life_component, probe.layout.vital_delta)
}

/// The three adjacent pools retain their known internal stride. A small
/// signed shift of their common start is resolved only at attach/refresh.
fn read_vital_pools(
    process: &Process,
    life_component: usize,
    delta: isize,
) -> io::Result<LiveVitals> {
    let base = (LIFE_HEALTH + VITAL_FLAT)
        .checked_add_signed(delta)
        .ok_or_else(|| invalid("invalid PoE1 shifted vital offset"))?;
    let len = LIFE_ES + VITAL_CURRENT + 4 - (LIFE_HEALTH + VITAL_FLAT);
    let bytes = process.read_bytes(life_component + base, len)?;
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
