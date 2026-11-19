//! Optional character-name telemetry. Never used to authorize automation.
use std::io;

use super::{invalid, plausible_ptr, read_player_id, Probe};
use crate::entity::resolve_component;
use crate::process::Process;

// PlayerOffsets.Name in the archived reference and the supplied 2026-11
// GameOffsets.dll metadata. Read a bounded MSVC std::wstring, never scan offsets.
const PLAYER_NAME: usize = 0x168;
const MAX_NAME_UNITS: usize = 64;

pub fn name(process: &Process, probe: &Probe) -> io::Result<String> {
    let player = probe.local_player;
    if read_player_id(process, player)? != probe.player_id {
        return Err(invalid("character changed"));
    }
    let component = resolve_component(process, player, "Player")
        .ok_or_else(|| invalid("Player component unavailable"))?;
    if process.read_u64(component + 8)? as usize != player {
        return Err(invalid("Player component owner changed"));
    }
    let address = component + PLAYER_NAME;
    let descriptor = process.read_bytes(address, 32)?;
    let (text_address, length) = string_layout(&descriptor, address)?;
    let text = process.read_utf16_len(text_address, length)?;
    validate_name(&text)?;
    if process.read_bytes(address, 32)? != descriptor
        || process.read_u64(component + 8)? as usize != player
        || read_player_id(process, player)? != probe.player_id
    {
        return Err(invalid("character changed during name read"));
    }
    Ok(text)
}

fn string_layout(bytes: &[u8], inline_address: usize) -> io::Result<(usize, usize)> {
    if bytes.len() != 32 {
        return Err(invalid("invalid character name descriptor"));
    }
    let length = u64::from_le_bytes(bytes[16..24].try_into().expect("wstring length"));
    let capacity = u64::from_le_bytes(bytes[24..32].try_into().expect("wstring capacity"));
    if length == 0 || length > MAX_NAME_UNITS as u64 || length > capacity || capacity > 4096 {
        return Err(invalid("invalid character name length"));
    }
    let address = if capacity < 8 {
        inline_address
    } else {
        u64::from_le_bytes(bytes[..8].try_into().expect("wstring pointer")) as usize
    };
    if !plausible_ptr(address) {
        return Err(invalid("invalid character name pointer"));
    }
    Ok((address, length as usize))
}

fn validate_name(text: &str) -> io::Result<()> {
    if text.is_empty()
        || text.chars().any(|c| {
            c.is_control() || c.is_whitespace() || matches!(c, '\u{fffd}' | '/' | '\\' | '<' | '>')
        })
    {
        return Err(invalid("invalid character name text"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_descriptor_bounds_inline_and_heap_reads() {
        let mut descriptor = [0u8; 32];
        descriptor[16..24].copy_from_slice(&3u64.to_le_bytes());
        descriptor[24..32].copy_from_slice(&7u64.to_le_bytes());
        assert_eq!(string_layout(&descriptor, 0x20000).unwrap(), (0x20000, 3));
        descriptor[..8].copy_from_slice(&0x30000u64.to_le_bytes());
        descriptor[16..24].copy_from_slice(&20u64.to_le_bytes());
        descriptor[24..32].copy_from_slice(&31u64.to_le_bytes());
        assert_eq!(string_layout(&descriptor, 0x20000).unwrap(), (0x30000, 20));
        descriptor[16..24].copy_from_slice(&65u64.to_le_bytes());
        assert!(string_layout(&descriptor, 0x20000).is_err());
        descriptor[16..24].copy_from_slice(&32u64.to_le_bytes());
        assert!(string_layout(&descriptor, 0x20000).is_err());
        descriptor[16..24].copy_from_slice(&0u64.to_le_bytes());
        assert!(string_layout(&descriptor, 0x20000).is_err());
    }

    #[test]
    fn name_accepts_unicode_and_rejects_invalid_or_metadata_text() {
        for name in ["Test_Character", "Игрок", "캐릭터"] {
            assert!(validate_name(name).is_ok());
        }
        for name in [
            "",
            "Test\nName",
            " Name",
            "Metadata/Characters/Witch",
            "\u{fffd}",
        ] {
            assert!(validate_name(name).is_err());
        }
    }
}
