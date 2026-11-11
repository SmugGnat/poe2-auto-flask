//! PoE1 WorldData → WorldArea.dat lookup. Does not enumerate game entities.
use std::io;

use crate::area::AreaInfo;
use crate::process::Process;
use super::{invalid, read_ptr, Probe};
use super::offsets::*;

const MAX_ID_UNITS: usize = 128;
const MAX_NAME_UNITS: usize = 192;

pub fn read(process: &Process, probe: &Probe) -> io::Result<AreaInfo> {
    if read_ptr(process, probe.in_game_state + IN_GAME_WORLD)? != probe.world_data {
        return Err(io::Error::new(io::ErrorKind::Interrupted, "PoE1 WorldData changed"));
    }
    let details = read_ptr(process, probe.world_data + WORLD_AREA_DETAILS)?;
    let row = read_ptr(process, details + WORLD_DETAILS_ROW)?;
    let fields = process.read_bytes(row, 0x16)?;

    let id_ptr = u64_at(&fields, 0) as usize;
    let name_ptr = u64_at(&fields, 8) as usize;
    let act = i32::from_le_bytes(fields[0x10..0x14].try_into().expect("act field"));
    if !(-1..=20).contains(&act) || fields[0x14] > 1 || fields[0x15] > 1 {
        return Err(invalid("invalid PoE1 WorldArea.dat row"));
    }
    let id = read_utf16_z(process, id_ptr, MAX_ID_UNITS)?;
    let name = read_utf16_z(process, name_ptr, MAX_NAME_UNITS)?;
    if id.is_empty() || name.is_empty()
        || id.chars().any(char::is_control)
        || name.chars().any(char::is_control)
    {
        return Err(invalid("invalid PoE1 area text"));
    }
    let lower = id.to_ascii_lowercase();
    Ok(AreaInfo {
        is_town: fields[0x14] == 1 || matches!(id.as_str(), "HeistHub" | "KalguuranSettlersLeague"),
        is_hideout: lower.contains("hideout"),
        id,
        name,
        act,
        world_data_offset: IN_GAME_WORLD,
        world_data: probe.world_data,
        area_row: row,
    })
}

fn u64_at(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("bounded pointer"))
}

fn read_utf16_z(process: &Process, ptr: usize, max_units: usize) -> io::Result<String> {
    if !super::plausible_ptr(ptr) {
        return Err(invalid("invalid PoE1 area string pointer"));
    }
    let bytes = process.read_bytes(ptr, max_units * 2)?;
    let mut units = Vec::with_capacity(32);
    for pair in bytes.chunks_exact(2) {
        let unit = u16::from_le_bytes([pair[0], pair[1]]);
        if unit == 0 {
            return String::from_utf16(&units).map_err(|_| invalid("invalid UTF-16 area string"));
        }
        units.push(unit);
    }
    Err(invalid("unterminated PoE1 area string"))
}

#[cfg(test)]
mod tests {
    #[test]
    fn source_world_offsets_are_independent_of_poe2() {
        assert_eq!(super::IN_GAME_WORLD, 0x270);
        assert_eq!(super::WORLD_AREA_DETAILS, 0xA0);
        assert_eq!(super::WORLD_DETAILS_ROW, 0xB0);
    }
}
