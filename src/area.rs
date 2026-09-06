use std::io;

use crate::poe2::VitalsProbe;
use crate::process::Process;

// Primary WorldData offset with a narrow pointer-aligned fallback window for layout drift.
const IN_GAME_WORLD_DATA: usize = 0x368;
const WORLD_DATA_SEARCH_START: usize = 0x340;
const WORLD_DATA_SEARCH_END: usize = 0x390;
const WORLD_DATA_SEARCH_STEP: usize = 8;

const WORLD_DATA_AREA_DETAILS: usize = 0x98;
const WORLD_AREA_DETAILS_ROW: usize = 0x98;

const AREA_ROW_ID_PTR: usize = 0x00;
const AREA_ROW_NAME_PTR: usize = 0x08;
const AREA_ROW_ACT: usize = 0x10;
const AREA_ROW_IS_TOWN: usize = 0x14;
const AREA_ROW_HAS_WAYPOINT: usize = 0x15;
const AREA_ROW_READ_LEN: usize = 0x16;

const MAX_AREA_ID_CHARS: usize = 128;
const MAX_AREA_NAME_CHARS: usize = 192;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AreaInfo {
    pub id: String,
    pub name: String,
    pub act: i32,
    pub is_town: bool,
    pub is_hideout: bool,
    pub world_data_offset: usize,
    pub world_data: usize,
    pub area_row: usize,
}

impl AreaInfo {
    pub fn allows_auto_flask(&self, enable_in_hideout: bool) -> bool {
        !self.is_town && (!self.is_hideout || enable_in_hideout)
    }

    pub fn kind(&self) -> &'static str {
        if self.is_town {
            "town"
        } else if self.is_hideout {
            "hideout"
        } else {
            "combat area"
        }
    }
}

pub fn resolve_area_info(process: &Process, vitals: &VitalsProbe) -> io::Result<AreaInfo> {
    for offset in prioritized_offsets(
        IN_GAME_WORLD_DATA,
        WORLD_DATA_SEARCH_START,
        WORLD_DATA_SEARCH_END,
        WORLD_DATA_SEARCH_STEP,
    ) {
        if let Some(info) = try_area_info(process, vitals.in_game_state, offset) {
            return Ok(info);
        }
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "current PoE2 area metadata could not be resolved",
    ))
}

fn try_area_info(
    process: &Process,
    in_game_state: usize,
    world_data_offset: usize,
) -> Option<AreaInfo> {
    let world_data = read_ptr(process, in_game_state + world_data_offset)?;
    let area_details = read_ptr(process, world_data + WORLD_DATA_AREA_DETAILS)?;
    let area_row = read_ptr(process, area_details + WORLD_AREA_DETAILS_ROW)?;

    let row = process.read_bytes(area_row, AREA_ROW_READ_LEN).ok()?;
    let id_ptr = read_u64_from(&row, AREA_ROW_ID_PTR) as usize;
    let name_ptr = read_u64_from(&row, AREA_ROW_NAME_PTR) as usize;
    if !plausible_ptr(id_ptr) || !plausible_ptr(name_ptr) {
        return None;
    }

    let act = i32::from_le_bytes(
        row[AREA_ROW_ACT..AREA_ROW_ACT + 4]
            .try_into()
            .expect("fixed area act field"),
    );
    if !(-1..=20).contains(&act) {
        return None;
    }

    let raw_town = row[AREA_ROW_IS_TOWN];
    let has_waypoint = row[AREA_ROW_HAS_WAYPOINT];
    if raw_town > 1 || has_waypoint > 1 {
        return None;
    }

    let id = read_utf16_z(process, id_ptr, MAX_AREA_ID_CHARS).ok()?;
    let name = read_utf16_z(process, name_ptr, MAX_AREA_NAME_CHARS).ok()?;
    if !valid_text(&id) || !valid_text(&name) {
        return None;
    }

    // A small number of hub rows behave as towns without setting the raw IsTown byte.
    let is_town = raw_town != 0 || matches!(id.as_str(), "HeistHub" | "KalguuranSettlersLeague");
    let id_lower = id.to_ascii_lowercase();
    let is_hideout = id_lower.contains("hideout") && !id_lower.contains("map");

    Some(AreaInfo {
        id,
        name,
        act,
        is_town,
        is_hideout,
        world_data_offset,
        world_data,
        area_row,
    })
}

fn read_utf16_z(process: &Process, address: usize, max_units: usize) -> io::Result<String> {
    let bytes = process.read_bytes(address, max_units.saturating_mul(2))?;
    let mut units = Vec::with_capacity(max_units);
    for pair in bytes.chunks_exact(2) {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            return String::from_utf16(&units).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "invalid UTF-16 area text")
            });
        }
        units.push(unit);
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "unterminated UTF-16 area text",
    ))
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}

fn read_ptr(process: &Process, address: usize) -> Option<usize> {
    let value = process.read_u64(address).ok()? as usize;
    plausible_ptr(value).then_some(value)
}

fn plausible_ptr(value: usize) -> bool {
    value >= 0x1_0000
}

fn read_u64_from(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        bytes[offset..offset + 8]
            .try_into()
            .expect("validated 8-byte field"),
    )
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
    use super::prioritized_offsets;

    #[test]
    fn world_data_offsets_start_at_known_field() {
        let offsets = prioritized_offsets(0x368, 0x340, 0x390, 8);
        assert_eq!(offsets.first(), Some(&0x368));
        assert!(offsets.contains(&0x360));
        assert!(offsets.contains(&0x370));
    }
}
