//! Read-only PoE1 Actor skill availability probe. Offsets derive from the
//! archived 2026-09 GameHelper snapshot; Molten Shell and Blood Rage were
//! verified in live PoE1 testing, but future game updates may change them.
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
}

impl SkillId {
    pub fn label(self) -> &'static str {
        match self {
            Self::MoltenShell => "Molten Shell",
            Self::BloodRage => "Blood Rage",
        }
    }

    fn matches_internal_name(self, name: &str) -> bool {
        match self {
            Self::MoltenShell => {
                name.eq_ignore_ascii_case("MoltenShell")
                    || name.eq_ignore_ascii_case("molten_shell_barrier")
            }
            Self::BloodRage => {
                name.eq_ignore_ascii_case("BloodRage") || name.eq_ignore_ascii_case("blood_rage")
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillReadiness {
    NotFound,
    NotUsable,
    /// Multiple Actor entries match the same preset. Refuse to guess which
    /// key/cooldown entry belongs to the configured action.
    Ambiguous,
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
        let mut ambiguous = BTreeSet::new();
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
                record_match(
                    &mut found,
                    &mut ambiguous,
                    skill,
                    (
                        u32_at(&details, SKILL_DETAIL_ID),
                        details[SKILL_DETAIL_CANNOT_USE] != 0,
                    ),
                );
            }
        }

        // No cooldown read is necessary when every requested skill is
        // absent or the Actor already reports it unusable.
        if !found
            .iter()
            .any(|(id, (_, unavailable))| !unavailable && !ambiguous.contains(id))
        {
            return Ok(classify(&wanted, &found, &ambiguous, &HashSet::new()));
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
            .iter()
            .filter(|(skill, (_, unavailable))| !unavailable && !ambiguous.contains(skill))
            .map(|(_, (game_id, _))| *game_id)
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
        Ok(classify(&wanted, &found, &ambiguous, &blocked))
    }
}

/// Keep duplicate detection local to a preset: a duplicated Blood Rage
/// must never suppress a distinct, unambiguous Molten Shell.
fn record_match(
    found: &mut BTreeMap<SkillId, (u32, bool)>,
    ambiguous: &mut BTreeSet<SkillId>,
    skill: SkillId,
    observed: (u32, bool),
) {
    if found.insert(skill, observed).is_some() {
        ambiguous.insert(skill);
    }
}

/// Small pure resolution function: matches and cooldowns are collected
/// once; every requested preset receives its independent status.
fn classify(
    wanted: &BTreeSet<SkillId>,
    found: &BTreeMap<SkillId, (u32, bool)>,
    ambiguous: &BTreeSet<SkillId>,
    blocked: &HashSet<u32>,
) -> BTreeMap<SkillId, SkillReadiness> {
    wanted
        .iter()
        .map(|id| {
            let status = match found.get(id) {
                _ if ambiguous.contains(id) => SkillReadiness::Ambiguous,
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
        let requested = BTreeSet::from([SkillId::MoltenShell, SkillId::BloodRage]);
        let found = BTreeMap::from([
            (SkillId::MoltenShell, (3u32, false)),
            (SkillId::BloodRage, (8u32, false)),
        ]);
        let snapshot = classify(&requested, &found, &BTreeSet::new(), &HashSet::from([3u32]));
        assert_eq!(snapshot[&SkillId::MoltenShell], SkillReadiness::NotUsable);
        assert_eq!(snapshot[&SkillId::BloodRage], SkillReadiness::Usable);
        let absent = classify(&requested, &BTreeMap::new(), &BTreeSet::new(), &HashSet::new());
        assert_eq!(absent[&SkillId::MoltenShell], SkillReadiness::NotFound);
        assert_eq!(absent[&SkillId::BloodRage], SkillReadiness::NotFound);
    }

    #[test]
    fn duplicated_skill_fails_closed_without_poisoning_other_requested_skills() {
        let wanted = BTreeSet::from([SkillId::MoltenShell, SkillId::BloodRage]);
        let mut found = BTreeMap::new();
        let mut ambiguous = BTreeSet::new();
        record_match(&mut found, &mut ambiguous, SkillId::MoltenShell, (3, false));
        record_match(&mut found, &mut ambiguous, SkillId::BloodRage, (8, false));
        record_match(&mut found, &mut ambiguous, SkillId::BloodRage, (9, true));

        let statuses = classify(&wanted, &found, &ambiguous, &HashSet::new());
        assert_eq!(statuses[&SkillId::MoltenShell], SkillReadiness::Usable);
        assert_eq!(statuses[&SkillId::BloodRage], SkillReadiness::Ambiguous);
        // Even if cooldown entries use different IDs, ambiguous entries are
        // denied, and the unambiguous skill retains its own cooldown state.
        let cooling = classify(&wanted, &found, &ambiguous, &HashSet::from([3u32]));
        assert_eq!(cooling[&SkillId::MoltenShell], SkillReadiness::NotUsable);
        assert_eq!(cooling[&SkillId::BloodRage], SkillReadiness::Ambiguous);
    }

    #[test]
    fn skill_vectors_are_bounded_and_aligned() {
        assert_eq!(vector_count(0, 0, 16, 128).unwrap(), 0);
        assert_eq!(vector_count(0x10000, 0x10030, 16, 128).unwrap(), 3);
        assert!(vector_count(0x10000, 0x10031, 16, 128).is_err());
        assert!(vector_count(0x10000, 0x20000, 16, 128).is_err());
    }
}
