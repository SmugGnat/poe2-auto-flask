//! Read-only PoE1 Actor skill availability probe. Offsets are candidates from
//! the archived 2026-09 GameHelper snapshot and require live validation.
use std::collections::HashMap;
use std::io;

use crate::entity::resolve_component;
use crate::process::Process;

use super::offsets::{
    ACTOR_COOLDOWNS, ACTOR_SKILLS, COOLDOWN_SKILL_ID, COOLDOWN_USES, COOLDOWN_VECTOR,
    SKILL_DETAIL_CANNOT_USE, SKILL_DETAIL_GRANTED_ROW, SKILL_DETAIL_ID,
};
use super::{invalid, plausible_ptr, read_player_id, read_ptr, Probe};

const MAX_SKILLS: usize = 128;
const MAX_NAMES: usize = 256;
const NAME_UNITS: usize = 128;

/// Stable PoE1 skill identifier. Adding a skill doesn't add a new scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SkillId {
    MoltenShell,
}

impl SkillId {
    pub fn label(self) -> &'static str {
        match self {
            Self::MoltenShell => "Molten Shell",
        }
    }

    fn matches_internal_name(self, name: &str) -> bool {
        match self {
            Self::MoltenShell => {
                name.eq_ignore_ascii_case("MoltenShell")
                    || name.eq_ignore_ascii_case("molten_shell_barrier")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillReadiness {
    NotFound,
    NotUsable,
    Usable,
}

#[derive(Default)]
pub struct SkillCache {
    actor: Option<usize>,
    names: HashMap<usize, String>,
}

impl SkillCache {
    pub fn clear(&mut self) {
        self.actor = None;
        self.names.clear();
    }

    pub fn sample(&mut self, process: &Process, probe: &Probe, skill: SkillId) -> io::Result<SkillReadiness> {
        if read_player_id(process, probe.local_player)? != probe.player_id {
            self.clear();
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "PoE1 player identity changed",
            ));
        }
        let actor = match self.actor {
            Some(addr) => addr,
            None => {
                let addr = resolve_component(process, probe.local_player, "Actor")
                    .ok_or_else(|| invalid("PoE1 player Actor component unavailable"))?;
                self.actor = Some(addr);
                addr
            }
        };
        let skill_header = process.read_bytes(actor + ACTOR_SKILLS, 16)?;
        let first = u64_at(&skill_header, 0) as usize;
        let count = vector_count(first, u64_at(&skill_header, 8) as usize, 16, MAX_SKILLS)?;
        let list = if count == 0 {
            Vec::new()
        } else {
            process.read_bytes(first, count * 16)?
        };
        let mut found = None;
        for entry in list.chunks_exact(16) {
            let details_ptr = u64_at(entry, 0) as usize;
            if !plausible_ptr(details_ptr) {
                return Err(invalid("PoE1 ActiveSkill pointer invalid"));
            }
            let details = process.read_bytes(details_ptr, SKILL_DETAIL_CANNOT_USE + 1)?;
            let row = u64_at(&details, SKILL_DETAIL_GRANTED_ROW) as usize;
            if row == 0 {
                continue;
            }
            if !plausible_ptr(row) {
                return Err(invalid("PoE1 ActiveSkill definition pointer invalid"));
            }
            let name = if let Some(cached) = self.names.get(&row) {
                cached.clone()
            } else {
                if self.names.len() >= MAX_NAMES {
                    self.names.clear();
                }
                let p1 = read_ptr(process, row)?;
                let name_ptr = read_ptr(process, p1)?;
                let name = read_name(process, name_ptr)?;
                self.names.insert(row, name.clone());
                name
            };
            if !skill.matches_internal_name(&name) {
                continue;
            }
            let id = u32_at(&details, SKILL_DETAIL_ID);
            let unavailable = details[SKILL_DETAIL_CANNOT_USE] != 0;
            if found.is_some() {
                // Ambiguous skills cannot authorize a key press.
                return Err(invalid(format!("multiple {} skill entries found", skill.label())));
            }
            found = Some((id, unavailable));
        }
        let Some((id, unavailable)) = found else {
            return Ok(SkillReadiness::NotFound);
        };
        if unavailable {
            return Ok(SkillReadiness::NotUsable);
        }
        let header = process.read_bytes(actor + ACTOR_COOLDOWNS, 16)?;
        let first = u64_at(&header, 0) as usize;
        let count = vector_count(first, u64_at(&header, 8) as usize, 0x48, MAX_SKILLS)?;
        let cooldowns = if count == 0 {
            Vec::new()
        } else {
            process.read_bytes(first, count * 0x48)?
        };
        for record in cooldowns.chunks_exact(0x48) {
            if u32_at(record, COOLDOWN_SKILL_ID) != id {
                continue;
            }
            let max_uses = u32_at(record, COOLDOWN_USES);
            let active_first = u64_at(record, COOLDOWN_VECTOR) as usize;
            let active_last = u64_at(record, COOLDOWN_VECTOR + 8) as usize;
            let active = vector_count(active_first, active_last, 16, MAX_SKILLS)?;
            if max_uses == 0 || active >= max_uses as usize {
                return Ok(SkillReadiness::NotUsable);
            }
        }
        Ok(SkillReadiness::Usable)
    }
}

fn read_name(process: &Process, address: usize) -> io::Result<String> {
    let bytes = process.read_bytes(address, NAME_UNITS * 2)?;
    let mut units = Vec::with_capacity(32);
    let mut terminated = false;
    for pair in bytes.chunks_exact(2) {
        let ch = u16::from_le_bytes([pair[0], pair[1]]);
        if ch == 0 {
            terminated = true;
            break;
        }
        units.push(ch);
    }
    if !terminated {
        return Err(invalid("PoE1 skill name is unterminated"));
    }
    let name = String::from_utf16(&units).map_err(|_| invalid("PoE1 skill name invalid UTF-16"))?;
    if name.is_empty() || name.chars().any(char::is_control) {
        return Err(invalid("PoE1 skill name is invalid"));
    }
    Ok(name)
}

fn vector_count(first: usize, last: usize, stride: usize, max: usize) -> io::Result<usize> {
    if first == 0 && last == 0 {
        return Ok(0);
    }
    if !plausible_ptr(first) || last < first || (last - first) % stride != 0 {
        return Err(invalid("PoE1 skill vector malformed"));
    }
    let count = (last - first) / stride;
    if count > max {
        return Err(invalid("PoE1 skill vector exceeds limit"));
    }
    Ok(count)
}

fn u64_at(data: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(
        data[offset..offset + 8]
            .try_into()
            .expect("bounded pointer"),
    )
}
fn u32_at(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(
        data[offset..offset + 4]
            .try_into()
            .expect("bounded integer"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn molten_shell_is_matched_exactly_not_vaal_variant() {
        assert!(SkillId::MoltenShell.matches_internal_name("MoltenShell"));
        assert!(SkillId::MoltenShell.matches_internal_name("molten_shell_barrier"));
        assert!(!SkillId::MoltenShell.matches_internal_name("VaalMoltenShell"));
        assert!(!SkillId::MoltenShell.matches_internal_name("molten_shell_shield"));
    }

    #[test]
    fn skill_vectors_are_bounded_and_aligned() {
        assert_eq!(vector_count(0, 0, 16, 128).unwrap(), 0);
        assert_eq!(vector_count(0x10000, 0x10030, 16, 128).unwrap(), 3);
        assert!(vector_count(0x10000, 0x10031, 16, 128).is_err());
        assert!(vector_count(0x10000, 0x20000, 16, 128).is_err());
    }
}
