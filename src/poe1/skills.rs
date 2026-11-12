//! Read-only PoE1 Actor skill availability probe. Offsets are candidates from
//! the archived 2026-09 GameHelper snapshot and require live validation.
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
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
    BloodRage,
    #[cfg(test)]
    EnduringCry,
}

impl SkillId {
    pub fn label(self) -> &'static str {
        match self {
            Self::MoltenShell => "Molten Shell",
            Self::BloodRage => "Blood Rage",
            #[cfg(test)]
            Self::EnduringCry => "Enduring Cry",
        }
    }

    fn matches_internal_name(self, name: &str) -> bool {
        match self {
            Self::MoltenShell => {
                name.eq_ignore_ascii_case("MoltenShell")
                    || name.eq_ignore_ascii_case("molten_shell_barrier")
            }
            Self::BloodRage => {
                name.eq_ignore_ascii_case("BloodRage")
                    || name.eq_ignore_ascii_case("blood_rage")
            }
            #[cfg(test)]
            Self::EnduringCry => name.eq_ignore_ascii_case("EnduringCry"),
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

    /// Inspect the character's Actor ActiveSkills vector exactly once for
    /// every requested preset, then inspect cooldowns once. No game-wide
    /// skill database enumeration occurs here.
    pub fn sample_many(
        &mut self,
        process: &Process,
        probe: &Probe,
        requested: &[SkillId],
    ) -> io::Result<BTreeMap<SkillId, SkillReadiness>> {
        let wanted: BTreeSet<SkillId> = requested.iter().copied().collect();
        if wanted.is_empty() {
            return Ok(BTreeMap::new());
        }
        if wanted.len() > MAX_SKILLS {
            return Err(invalid("PoE1 requested skill count exceeds limit"));
        }
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
        let header = process.read_bytes(actor + ACTOR_SKILLS, 16)?;
        let first = u64_at(&header, 0) as usize;
        let count = vector_count(first, u64_at(&header, 8) as usize, 16, MAX_SKILLS)?;
        let list = if count == 0 {
            Vec::new()
        } else {
            process.read_bytes(first, count * 16)?
        };

        // Game skill ID and the Actor's base unusable flag. The static
        // definition name is cached across ticks, not the live usability.
        let mut found: BTreeMap<SkillId, (u32, bool)> = BTreeMap::new();
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
            if !self.names.contains_key(&row) {
                if self.names.len() >= MAX_NAMES {
                    self.names.clear();
                }
                let p1 = read_ptr(process, row)?;
                let name_ptr = read_ptr(process, p1)?;
                let name = read_name(process, name_ptr)?;
                self.names.insert(row, name);
            }
            let name = self.names.get(&row).expect("skill name was resolved");
            for &skill in &wanted {
                if !skill.matches_internal_name(name) {
                    continue;
                }
                if found
                    .insert(
                        skill,
                        (
                            u32_at(&details, SKILL_DETAIL_ID),
                            details[SKILL_DETAIL_CANNOT_USE] != 0,
                        ),
                    )
                    .is_some()
                {
                    // Duplicated matching entries may have different
                    // cooldowns/bindings. Unknown must never mean usable.
                    return Err(invalid(format!(
                        "multiple {} skill entries found",
                        skill.label()
                    )));
                }
            }
        }

        // No cooldown read is necessary when every requested skill is
        // absent or the Actor already reports it unusable.
        if !found.values().any(|(_, unavailable)| !unavailable) {
            return Ok(classify(&wanted, &found, &HashSet::new()));
        }

        let header = process.read_bytes(actor + ACTOR_COOLDOWNS, 16)?;
        let first = u64_at(&header, 0) as usize;
        let count = vector_count(first, u64_at(&header, 8) as usize, 0x48, MAX_SKILLS)?;
        let records = if count == 0 {
            Vec::new()
        } else {
            process.read_bytes(first, count * 0x48)?
        };
        let relevant_ids: HashSet<u32> = found
            .values()
            .filter(|(_, unavailable)| !unavailable)
            .map(|(id, _)| *id)
            .collect();
        let mut blocked = HashSet::new();
        for record in records.chunks_exact(0x48) {
            let id = u32_at(record, COOLDOWN_SKILL_ID);
            if !relevant_ids.contains(&id) {
                continue;
            }
            let max_uses = u32_at(record, COOLDOWN_USES);
            let first = u64_at(record, COOLDOWN_VECTOR) as usize;
            let last = u64_at(record, COOLDOWN_VECTOR + 8) as usize;
            let active = vector_count(first, last, 16, MAX_SKILLS)?;
            if max_uses == 0 || active >= max_uses as usize {
                blocked.insert(id);
            }
        }
        Ok(classify(&wanted, &found, &blocked))
    }
}

/// Small pure resolution function: matches and cooldowns are collected
/// once; every requested preset receives its independent status.
fn classify(
    wanted: &BTreeSet<SkillId>,
    found: &BTreeMap<SkillId, (u32, bool)>,
    blocked: &HashSet<u32>,
) -> BTreeMap<SkillId, SkillReadiness> {
    wanted
        .iter()
        .map(|id| {
            let status = match found.get(id) {
                None => SkillReadiness::NotFound,
                Some((_, true)) => SkillReadiness::NotUsable,
                Some((game_id, false)) if blocked.contains(game_id) => SkillReadiness::NotUsable,
                Some(_) => SkillReadiness::Usable,
            };
            (*id, status)
        })
        .collect()
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
    fn blood_rage_uses_an_exact_name_not_a_similar_skill() {
        assert!(SkillId::BloodRage.matches_internal_name("BloodRage"));
        assert!(SkillId::BloodRage.matches_internal_name("blood_rage"));
        assert!(!SkillId::BloodRage.matches_internal_name("blood_rage_leech"));
    }

    #[test]
    fn one_snapshot_classifies_multiple_requested_skills() {
        let requested = BTreeSet::from([SkillId::MoltenShell, SkillId::EnduringCry]);
        let found = BTreeMap::from([
            (SkillId::MoltenShell, (3u32, false)),
            (SkillId::EnduringCry, (8u32, false)),
        ]);
        let snapshot = classify(&requested, &found, &HashSet::from([3u32]));
        assert_eq!(snapshot[&SkillId::MoltenShell], SkillReadiness::NotUsable);
        assert_eq!(snapshot[&SkillId::EnduringCry], SkillReadiness::Usable);
        let absent = classify(&requested, &BTreeMap::new(), &HashSet::new());
        assert_eq!(absent[&SkillId::MoltenShell], SkillReadiness::NotFound);
        assert_eq!(absent[&SkillId::EnduringCry], SkillReadiness::NotFound);
    }

    #[test]
    fn skill_vectors_are_bounded_and_aligned() {
        assert_eq!(vector_count(0, 0, 16, 128).unwrap(), 0);
        assert_eq!(vector_count(0x10000, 0x10030, 16, 128).unwrap(), 3);
        assert!(vector_count(0x10000, 0x10031, 16, 128).is_err());
        assert!(vector_count(0x10000, 0x20000, 16, 128).is_err());
    }
}
