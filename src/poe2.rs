use std::collections::HashSet;
use std::io;

use crate::entity::resolve_component;
use crate::process::{PeSection, Process};

// GameStates global-slot signature. Immediate operands after the RIP-relative reference are
// wildcarded; the stable post-call instructions keep the pattern unique.
const GAME_STATE_PATTERN: [Option<u8>; 31] = [
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

const GAME_STATE_CURRENT_STATE_PTR: usize = 0x08;
const GAME_STATE_STATES: usize = 0x48;
const GAME_STATE_SLOT_STRIDE: usize = 0x10;
const GAME_STATE_SLOT_COUNT: usize = 12;

// Primary offsets are validated before use. Narrow fallback searches tolerate minor layout drift.
const IN_GAME_STATE_AREA_INSTANCE: usize = 0x2A0;
const AREA_INSTANCE_LOCAL_PLAYER: usize = 0x5D0;

const ENTITY_DETAILS_PTR: usize = 0x08;
const ENTITY_DETAILS_NAME: usize = 0x08;

const LIFE_HEALTH: usize = 0x1B0;
const LIFE_MANA: usize = 0x208;
const LIFE_ENERGY_SHIELD: usize = 0x248;

const VITAL_RESERVED_FLAT: usize = 0x10;
const VITAL_RESERVED_FRACTION: usize = 0x14;
const VITAL_TOTAL: usize = 0x2C;
const VITAL_CURRENT: usize = 0x30;
const VITAL_READ_LEN: usize = VITAL_CURRENT + 4 - VITAL_RESERVED_FLAT;

const MAX_METADATA_CHARS: usize = 256;
const MAX_VITAL_TOTAL: i32 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VitalPool {
    pub current: i32,
    /// Raw VitalStruct total before reservations are removed.
    pub total: i32,
    pub reserved_flat: i32,
    /// Reservation fraction in basis points: 10_000 = 100%.
    pub reserved_fraction: i32,
}

impl VitalPool {
    pub fn usable_max(self) -> i32 {
        let reserved_percent =
            ((self.total as i64 * self.reserved_fraction as i64) + 9_999) / 10_000;
        (self.total as i64 - reserved_percent - self.reserved_flat as i64).clamp(0, i32::MAX as i64)
            as i32
    }

    pub fn percentage(self) -> Option<f32> {
        let usable_max = self.usable_max();
        (usable_max > 0).then_some(100.0 * self.current as f32 / usable_max as f32)
    }

    fn looks_valid(self) -> bool {
        if self.total <= 0 || self.total > MAX_VITAL_TOTAL {
            return false;
        }
        if self.current < -self.total || self.current > self.total + 1 {
            return false;
        }
        if self.reserved_flat < 0 || self.reserved_flat > self.total {
            return false;
        }
        (0..=10_000).contains(&self.reserved_fraction)
    }

    fn looks_valid_or_zero(self) -> bool {
        (self.total == 0
            && self.current == 0
            && self.reserved_flat == 0
            && (0..=10_000).contains(&self.reserved_fraction))
            || self.looks_valid()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveVitals {
    pub health: VitalPool,
    pub mana: Option<VitalPool>,
    pub energy_shield: Option<VitalPool>,
}

#[derive(Debug)]
pub struct VitalsProbe {
    pub text_section: PeSection,
    pub aob_match_count: usize,
    pub game_state_slot: usize,
    pub game_state: usize,
    pub in_game_state: usize,
    pub area_instance: usize,
    pub area_instance_offset: usize,
    pub local_player: usize,
    pub local_player_offset: usize,
    pub metadata: String,
    pub life_component: usize,
    pub health_offset: usize,
    pub mana_offset: Option<usize>,
    pub energy_shield_offset: Option<usize>,
    pub health: VitalPool,
    pub mana: Option<VitalPool>,
    pub energy_shield: Option<VitalPool>,
}

pub fn probe_vitals(process: &Process) -> io::Result<VitalsProbe> {
    let text_section = process.find_pe_section(".text")?;
    let matches =
        process.scan_pattern(text_section.address, text_section.size, &GAME_STATE_PATTERN)?;

    if matches.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "current GameState AOB signature was not found in .text",
        ));
    }

    let mut last_error = None;
    for &instruction in &matches {
        match probe_from_game_state_match(process, instruction, matches.len(), text_section.clone())
        {
            Ok(probe) => return Ok(probe),
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "GameState signature matched, but no valid in-game player chain was found",
        )
    }))
}

// Re-resolve after an area or player transition without rescanning the executable section.
pub fn refresh_vitals(process: &Process, previous: &VitalsProbe) -> io::Result<VitalsProbe> {
    let Some(game_state) = read_ptr(process, previous.game_state_slot) else {
        return probe_vitals(process);
    };

    match probe_from_game_state_root(
        process,
        previous.game_state_slot,
        game_state,
        previous.aob_match_count,
        previous.text_section.clone(),
    ) {
        Ok(probe) => Ok(probe),
        Err(error) => {
            if read_ptr(process, previous.game_state_slot).is_none() {
                probe_vitals(process)
            } else {
                Err(error)
            }
        }
    }
}

// Area and player identity checks make stale pointers fail closed. Each VitalStruct is read in one
// compact block instead of several separate process reads.
pub fn sample_vitals(process: &Process, probe: &VitalsProbe) -> io::Result<LiveVitals> {
    let area_instance = read_ptr(process, probe.in_game_state + probe.area_instance_offset)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "AreaInstance is unavailable"))?;
    if area_instance != probe.area_instance {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "AreaInstance changed",
        ));
    }

    let local_player = read_ptr(process, area_instance + probe.local_player_offset)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "LocalPlayer is unavailable"))?;
    if local_player != probe.local_player {
        return Err(io::Error::new(
            io::ErrorKind::Interrupted,
            "LocalPlayer changed",
        ));
    }

    let health = read_vital(process, probe.life_component + probe.health_offset)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "health pool is unreadable"))?;
    if !health.looks_valid() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "health pool failed sanity validation",
        ));
    }

    let mana = match probe.mana_offset {
        Some(offset) => {
            let pool = read_vital(process, probe.life_component + offset).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "mana pool is unreadable")
            })?;
            if !pool.looks_valid() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "mana pool failed sanity validation",
                ));
            }
            Some(pool)
        }
        None => None,
    };

    let energy_shield = match probe.energy_shield_offset {
        Some(offset) => {
            let pool = read_vital(process, probe.life_component + offset).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "energy-shield pool is unreadable",
                )
            })?;
            if !pool.looks_valid_or_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "energy-shield pool failed sanity validation",
                ));
            }
            Some(pool)
        }
        None => None,
    };

    Ok(LiveVitals {
        health,
        mana,
        energy_shield,
    })
}

fn probe_from_game_state_match(
    process: &Process,
    instruction: usize,
    aob_match_count: usize,
    text_section: PeSection,
) -> io::Result<VitalsProbe> {
    let displacement = process.read_i32(instruction + 3)? as i64;
    let game_state_slot = add_signed(instruction + 7, displacement)?;
    let game_state = read_ptr(process, game_state_slot).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("GameState slot 0x{game_state_slot:016X} was null or unreadable"),
        )
    })?;

    probe_from_game_state_root(
        process,
        game_state_slot,
        game_state,
        aob_match_count,
        text_section,
    )
}

fn probe_from_game_state_root(
    process: &Process,
    game_state_slot: usize,
    game_state: usize,
    aob_match_count: usize,
    text_section: PeSection,
) -> io::Result<VitalsProbe> {
    let states = in_game_state_candidates(process, game_state);
    if states.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "GameState was readable, but it did not expose any candidate game states",
        ));
    }

    for &in_game_state in &states {
        if let Some(probe) = try_player_chain(
            process,
            &text_section,
            aob_match_count,
            game_state_slot,
            game_state,
            in_game_state,
            IN_GAME_STATE_AREA_INSTANCE,
            AREA_INSTANCE_LOCAL_PLAYER,
        ) {
            return Ok(probe);
        }
    }

    let player_offsets = prioritized_offsets(AREA_INSTANCE_LOCAL_PLAYER, 0x560, 0x640, 8);
    for &in_game_state in &states {
        for &player_offset in &player_offsets {
            if player_offset == AREA_INSTANCE_LOCAL_PLAYER {
                continue;
            }
            if let Some(probe) = try_player_chain(
                process,
                &text_section,
                aob_match_count,
                game_state_slot,
                game_state,
                in_game_state,
                IN_GAME_STATE_AREA_INSTANCE,
                player_offset,
            ) {
                return Ok(probe);
            }
        }
    }

    let area_offsets = prioritized_offsets(IN_GAME_STATE_AREA_INSTANCE, 0x250, 0x2D0, 8);
    for &in_game_state in &states {
        for &area_offset in &area_offsets {
            if area_offset == IN_GAME_STATE_AREA_INSTANCE {
                continue;
            }
            for &player_offset in &player_offsets {
                if let Some(probe) = try_player_chain(
                    process,
                    &text_section,
                    aob_match_count,
                    game_state_slot,
                    game_state,
                    in_game_state,
                    area_offset,
                    player_offset,
                ) {
                    return Ok(probe);
                }
            }
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        format!(
            "GameState resolved at 0x{game_state:016X}, but no validated local player/Life chain was found"
        ),
    ))
}

#[allow(clippy::too_many_arguments)]
fn try_player_chain(
    process: &Process,
    text_section: &PeSection,
    aob_match_count: usize,
    game_state_slot: usize,
    game_state: usize,
    in_game_state: usize,
    area_instance_offset: usize,
    local_player_offset: usize,
) -> Option<VitalsProbe> {
    let area_instance = read_ptr(process, in_game_state + area_instance_offset)?;
    let local_player = read_ptr(process, area_instance + local_player_offset)?;
    let metadata = read_metadata(process, local_player)?;

    if !metadata.starts_with("Metadata/Characters/") {
        return None;
    }

    let life_component = resolve_component(process, local_player, "Life")?;
    let (health_offset, health) = resolve_vital_offset(process, life_component, LIFE_HEALTH, true)?;
    let mana = resolve_vital_offset(process, life_component, LIFE_MANA, false);
    let energy_shield =
        resolve_vital_offset_allow_zero(process, life_component, LIFE_ENERGY_SHIELD);

    Some(VitalsProbe {
        text_section: text_section.clone(),
        aob_match_count,
        game_state_slot,
        game_state,
        in_game_state,
        area_instance,
        area_instance_offset,
        local_player,
        local_player_offset,
        metadata,
        life_component,
        health_offset,
        mana_offset: mana.map(|(offset, _)| offset),
        energy_shield_offset: energy_shield.map(|(offset, _)| offset),
        health,
        mana: mana.map(|(_, pool)| pool),
        energy_shield: energy_shield.map(|(_, pool)| pool),
    })
}

fn in_game_state_candidates(process: &Process, game_state: usize) -> Vec<usize> {
    let mut result = Vec::with_capacity(GAME_STATE_SLOT_COUNT + 1);
    let mut seen = HashSet::with_capacity(GAME_STATE_SLOT_COUNT + 1);

    if let Some(vector_first) = read_ptr(process, game_state + GAME_STATE_CURRENT_STATE_PTR) {
        if let Some(active_state) = read_ptr(process, vector_first) {
            if seen.insert(active_state) {
                result.push(active_state);
            }
        }
    }

    for index in 0..GAME_STATE_SLOT_COUNT {
        let address = game_state + GAME_STATE_STATES + index * GAME_STATE_SLOT_STRIDE;
        if let Some(state) = read_ptr(process, address) {
            if seen.insert(state) {
                result.push(state);
            }
        }
    }

    result
}

fn read_metadata(process: &Process, entity: usize) -> Option<String> {
    let details = read_ptr(process, entity + ENTITY_DETAILS_PTR)?;
    read_std_wstring(process, details + ENTITY_DETAILS_NAME)
}

fn read_std_wstring(process: &Process, address: usize) -> Option<String> {
    let len = process.read_i32(address + 0x10).ok()?;
    if len <= 0 || len as usize > MAX_METADATA_CHARS {
        return None;
    }

    let string_address = if len < 8 {
        address
    } else {
        read_ptr(process, address)?
    };

    process.read_utf16_len(string_address, len as usize).ok()
}

fn resolve_vital_offset(
    process: &Process,
    life_component: usize,
    configured: usize,
    require_positive_current: bool,
) -> Option<(usize, VitalPool)> {
    if let Some(pool) = read_vital(process, life_component + configured) {
        if pool.looks_valid() && (!require_positive_current || pool.current > 0) {
            return Some((configured, pool));
        }
    }

    let offsets = prioritized_offsets(
        configured,
        configured.saturating_sub(0x20),
        configured + 0x40,
        4,
    );
    for offset in offsets {
        if offset == configured {
            continue;
        }
        let Some(pool) = read_vital(process, life_component + offset) else {
            continue;
        };
        if pool.looks_valid() && (!require_positive_current || pool.current > 0) {
            return Some((offset, pool));
        }
    }

    None
}

fn resolve_vital_offset_allow_zero(
    process: &Process,
    life_component: usize,
    configured: usize,
) -> Option<(usize, VitalPool)> {
    let offsets = prioritized_offsets(
        configured,
        configured.saturating_sub(0x20),
        configured + 0x40,
        4,
    );
    for offset in offsets {
        let Some(pool) = read_vital(process, life_component + offset) else {
            continue;
        };
        if pool.looks_valid_or_zero() {
            return Some((offset, pool));
        }
    }
    None
}

fn read_vital(process: &Process, address: usize) -> Option<VitalPool> {
    let bytes = process
        .read_bytes(address + VITAL_RESERVED_FLAT, VITAL_READ_LEN)
        .ok()?;
    Some(VitalPool {
        reserved_flat: read_i32_from(&bytes, 0),
        reserved_fraction: read_i32_from(&bytes, VITAL_RESERVED_FRACTION - VITAL_RESERVED_FLAT),
        total: read_i32_from(&bytes, VITAL_TOTAL - VITAL_RESERVED_FLAT),
        current: read_i32_from(&bytes, VITAL_CURRENT - VITAL_RESERVED_FLAT),
    })
}

fn read_i32_from(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes(
        bytes[offset..offset + 4]
            .try_into()
            .expect("validated 4-byte vital field"),
    )
}

fn read_ptr(process: &Process, address: usize) -> Option<usize> {
    let value = process.read_u64(address).ok()? as usize;
    (value >= 0x1_0000).then_some(value)
}

fn add_signed(base: usize, displacement: i64) -> io::Result<usize> {
    let value = base as i128 + displacement as i128;
    if !(0..=usize::MAX as i128).contains(&value) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "RIP-relative address overflow",
        ));
    }
    Ok(value as usize)
}

fn prioritized_offsets(center: usize, start: usize, end: usize, step: usize) -> Vec<usize> {
    let mut offsets = Vec::new();
    if center >= start && center <= end {
        offsets.push(center);
    }

    let mut distance = step;
    while center.saturating_sub(distance) >= start || center.saturating_add(distance) <= end {
        if let Some(lower) = center.checked_sub(distance) {
            if lower >= start && lower <= end {
                offsets.push(lower);
            }
        }
        if let Some(upper) = center.checked_add(distance) {
            if upper >= start && upper <= end {
                offsets.push(upper);
            }
        }
        distance = match distance.checked_add(step) {
            Some(next) => next,
            None => break,
        };
    }

    offsets
}

#[cfg(test)]
mod tests {
    use super::{prioritized_offsets, VitalPool};

    #[test]
    fn offsets_start_at_center_and_expand_outward() {
        assert_eq!(
            prioritized_offsets(0x20, 0x10, 0x30, 8),
            vec![0x20, 0x18, 0x28, 0x10, 0x30]
        );
    }

    #[test]
    fn usable_max_applies_percent_and_flat_reservation() {
        let pool = VitalPool {
            current: 700,
            total: 1_000,
            reserved_flat: 50,
            reserved_fraction: 2_023,
        };
        assert_eq!(pool.usable_max(), 747);
    }

    #[test]
    fn reservation_rounds_up() {
        let pool = VitalPool {
            current: 99,
            total: 101,
            reserved_flat: 0,
            reserved_fraction: 100,
        };
        assert_eq!(pool.usable_max(), 99);
    }
}
