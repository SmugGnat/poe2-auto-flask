//! Reads charge-cost modifiers from the local player's Stats and each flask's
//! Mods component, following the GameHelper reference. Fails closed on bad data.
use std::io;

use crate::entity::resolve_component;
use crate::process::Process;

use super::offsets::*;
use super::recovery::RecoveryKind;
use super::types::UniqueFlask;
use super::{invalid, plausible_ptr, read_ptr};

const FLASK_CHARGES_USED_STAT: i32 = 0x2E5;
const MAX_STATS: usize = 8192;

// ModsOffsets.Details0.Mods.ExplicitMods = 0xC0 + 0x18.
const MAX_MODS: usize = 128;
const MAX_NAME_UNITS: usize = 128;

pub fn player_charge_modifier(process: &Process, player: usize) -> io::Result<i32> {
    let stats = resolve_component(process, player, "Stats")
        .ok_or_else(|| invalid("PoE1 player Stats component unavailable"))?;
    let data = read_ptr(process, stats + STATS_DATA_PTR)?;
    let (start, count) = read_vector(process, data + STATS_VECTOR, STAT_STRIDE, MAX_STATS)?;
    if count == 0 {
        return Err(invalid("PoE1 player Stats vector is empty"));
    }
    let bytes = process.read_bytes(start, count * STAT_STRIDE)?;
    for entry in bytes.chunks_exact(STAT_STRIDE) {
        let id = i32_at(entry, 0);
        if id == FLASK_CHARGES_USED_STAT {
            return Ok(i32_at(entry, 4));
        }
    }
    Ok(0)
}

/// Flask charge-cost and recovery type are classified from the same existing
/// bounded Mods.dat walk, only when a new flask is equipped/area refreshed.
#[derive(Debug, Clone, Copy)]
pub struct LocalFlaskMods {
    pub charge_multiplier: f64,
    pub recovery: RecoveryKind,
    pub unique: Option<UniqueFlask>,
}

pub fn inspect_flask_modifiers(process: &Process, item: usize) -> io::Result<LocalFlaskMods> {
    let mods = resolve_component(process, item, "Mods")
        .ok_or_else(|| invalid("PoE1 flask Mods component unavailable"))?;
    let (first, count) = read_vector(process, mods + EXPLICIT_MODS_VECTOR, MOD_STRIDE, MAX_MODS)?;
    let mut multiplier = 1.0;
    let mut recovery = RecoveryKind::Gradual;
    let mut unique = None;
    for idx in 0..count {
        let entry = process.read_bytes(first + idx * MOD_STRIDE, MOD_STRIDE)?;
        let dat_row = u64_at(&entry, MOD_DATA_PTR) as usize;
        if dat_row == 0 {
            continue;
        }
        if !plausible_ptr(dat_row) {
            return Err(invalid("invalid PoE1 flask mod pointer"));
        }
        let name_ptr = read_ptr(process, dat_row)?;
        let name = read_utf16_z(process, name_ptr, MAX_NAME_UNITS)?;
        if let Some(found) = UniqueFlask::from_mod_id(&name) {
            unique = Some(found);
        }
        if let Some(found) = RecoveryKind::from_mod_name(&name) {
            if recovery != RecoveryKind::Gradual && recovery != found {
                return Err(invalid("conflicting PoE1 flask recovery modifiers"));
            }
            recovery = found;
        }
        if !name.to_ascii_lowercase().contains("flaskchargesused") {
            continue;
        }
        let (values_start, value_count) =
            bounded_range(u64_at(&entry, 0) as usize, u64_at(&entry, 8) as usize, 4, 8)?;
        let value = match value_count {
            0 => return Err(invalid("charge-use mod has no value")),
            1 => i32_at(&entry, MOD_VALUE), // GameHelper's optimized one-value field
            _ => process.read_i32(values_start)?,
        };
        multiplier *= percent_multiplier(value)?;
    }
    if !multiplier.is_finite() || multiplier <= 0.0 {
        return Err(invalid("invalid local flask charge multiplier"));
    }
    Ok(LocalFlaskMods {
        charge_multiplier: multiplier,
        recovery,
        unique,
    })
}

pub fn effective_cost(raw: i32, player_modifier: i32, local_multiplier: f64) -> io::Result<i32> {
    if !(1..=100_000).contains(&raw) || !local_multiplier.is_finite() || local_multiplier <= 0.0 {
        return Err(invalid("invalid base flask charge cost or modifier"));
    }
    let cost = (raw as f64 * percent_multiplier(player_modifier)? * local_multiplier).trunc();
    if !cost.is_finite() || !(1.0..=100_000.0).contains(&cost) {
        return Err(invalid("effective flask charge cost is out of range"));
    }
    Ok(cost as i32)
}

fn percent_multiplier(value: i32) -> io::Result<f64> {
    if !(-99..=500).contains(&value) {
        return Err(invalid("implausible flask charge modifier"));
    }
    Ok((100 + value) as f64 / 100.0)
}

fn read_vector(
    process: &Process,
    at: usize,
    stride: usize,
    limit: usize,
) -> io::Result<(usize, usize)> {
    let bytes = process.read_bytes(at, 16)?;
    let first = u64_at(&bytes, 0) as usize;
    let last = u64_at(&bytes, 8) as usize;
    Ok((first, bounded_range(first, last, stride, limit)?.1))
}

fn bounded_range(
    first: usize,
    last: usize,
    stride: usize,
    limit: usize,
) -> io::Result<(usize, usize)> {
    if first == 0 && last == 0 {
        return Ok((0, 0));
    }
    if !plausible_ptr(first) || last < first || stride == 0 || (last - first) % stride != 0 {
        return Err(invalid("invalid PoE1 modifier/stat vector"));
    }
    let count = (last - first) / stride;
    if count > limit {
        return Err(invalid("PoE1 modifier/stat vector exceeds limit"));
    }
    Ok((first, count))
}

fn read_utf16_z(process: &Process, address: usize, max: usize) -> io::Result<String> {
    let bytes = process.read_bytes(address, max * 2)?;
    let mut units = Vec::with_capacity(32);
    for pair in bytes.chunks_exact(2) {
        let code = u16::from_le_bytes([pair[0], pair[1]]);
        if code == 0 {
            return String::from_utf16(&units).map_err(|_| invalid("invalid mod name encoding"));
        }
        units.push(code);
    }
    Err(invalid("unterminated PoE1 mod name"))
}

fn u64_at(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("bounded u64"))
}
fn i32_at(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded i32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charges_match_observed_reduced_cost() {
        assert_eq!(effective_cost(15, 0, 0.75).unwrap(), 11);
        assert_eq!(effective_cost(15, -25, 1.0).unwrap(), 11);
        assert_eq!(effective_cost(30, -10, 0.8).unwrap(), 21);
    }

    #[test]
    fn invalid_costs_and_vectors_fail_closed() {
        assert!(effective_cost(0, 0, 1.0).is_err());
        assert!(effective_cost(15, -100, 1.0).is_err());
        assert!(effective_cost(15, 0, f64::NAN).is_err());
        assert!(bounded_range(0x10000, 0x10021, 8, 10).is_err());
        assert!(bounded_range(0x10000, 0x20000, 8, 10).is_err());
    }
}
