//! Experimental read-only PoE1 awake-monster census.
//! Proximity-gated utility flasks and skill presets reuse the same throttled
//! snapshot in automation. --debug --nearby can also inspect it; policies
//! without an eligible proximity gate never initiate a scan.
//! Bounded std::map walk; cache entity component pointers within an area.
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use crate::entity::resolve_component;
use crate::process::Process;

use super::buffs::{MonsterEffectCache, MonsterEffects};
use super::nearby_filters::{is_excluded_monster_path, is_harbinger, Encounter, MonsterMods};
use super::offsets::nearby::*;
use super::offsets::{MOD_DATA_PTR, MOD_STRIDE};
use super::{invalid, plausible_ptr, read_metadata, read_ptr, Probe};

#[path = "nearby_debug.rs"]
mod diagnostic;
pub(crate) use diagnostic::{BossReporter, NearbyDiagnostic};

const MAX_MAP_NODES: usize = 1500;
const MAX_NEARBY_MONSTERS: usize = 512;
const MAX_NEARBY_CORPSES: usize = 512;
const MAX_STATIC_CACHE: usize = 2500;
const MAX_COMPONENTS: usize = 50;
const MAX_GRID_DISTANCE: f32 = 150.0;
const WORLD_TO_GRID: f32 = 250.0 / 23.0;
const REFRESH_DELAY: Duration = Duration::from_millis(750);
const MAX_MONSTER_MODS: usize = 128;
const MAX_MONSTER_EFFECTS_PER_SCAN: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rarity {
    Normal,
    Magic,
    Rare,
    Unique,
}

impl Rarity {
    fn parse(value: i32) -> Option<Self> {
        Some(match value {
            0 => Self::Normal,
            1 => Self::Magic,
            2 => Self::Rare,
            3 => Self::Unique,
            _ => return None,
        })
    }

    fn index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Magic => 1,
            Self::Rare => 2,
            Self::Unique => 3,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NearbyMonster {
    pub rarity: Rarity,
    /// Squared distance, in the same grid units used by GameHelper's circles.
    pub distance_squared: f32,
}

/// A dead, hostile monster entity that still appears targetable. This is a
/// conservative corpse *candidate*, not a confirmed Spirit Offering target.
#[derive(Debug, Clone)]
pub struct NearbyCorpse {
    /// Squared grid distance, identical scale to nearby monster rules.
    pub distance_squared: f32,
}

#[derive(Debug, Clone)]
enum NearbyEntity {
    Monster(NearbyMonster),
    Corpse(NearbyCorpse),
}

#[derive(Debug, Clone, Default)]
pub struct NearbySnapshot {
    pub awake_nodes: usize,
    pub candidate_monsters: usize,
    pub nearby: Vec<NearbyMonster>,
    pub corpses: Vec<NearbyCorpse>,
    /// Wall time for the most recent full scan (not CPU time). Includes
    /// Wine/ReadProcessMemory latency; sampling requires debug --nearby or
    /// an explicitly enabled proximity-gated PoE1 utility flask.
    pub scan_wall: Duration,
}

impl NearbySnapshot {
    pub fn counts_at(&self, radius: f32) -> [usize; 4] {
        let mut counts = [0; 4];
        if radius <= 0.0 || !radius.is_finite() {
            return counts;
        }
        let distance_squared = radius * radius;
        for monster in &self.nearby {
            if monster.distance_squared < distance_squared {
                counts[monster.rarity.index()] += 1;
            }
        }
        counts
    }

    /// Counts corpse candidates from the same cached bounded entity scan.
    /// Dead monsters without the observed targetable flag are excluded.
    pub fn corpses_at(&self, radius: f32) -> usize {
        if radius <= 0.0 || !radius.is_finite() {
            return 0;
        }
        let distance_squared = radius * radius;
        self.corpses
            .iter()
            .filter(|corpse| corpse.distance_squared < distance_squared)
            .count()
    }
}

#[derive(Debug, Clone, Copy)]
struct Components {
    render: usize,
    life: usize,
    positioned: usize,
    properties: usize,
    targetable: usize,
    buffs: Option<usize>,
    has_npc: bool,
    encounter: Encounter,
}

#[derive(Debug, Clone, Copy)]
enum CachedEntity {
    Ignored,
    Monster(Components),
}

#[derive(Debug, Default)]
pub struct NearbyCache {
    player_render: Option<usize>,
    static_entities: HashMap<(u32, usize), CachedEntity>,
    mod_definitions: HashMap<usize, MonsterMods>,
    monster_effects: MonsterEffectCache,
    last_poll: Option<Instant>,
    last_result: Option<Result<NearbySnapshot, String>>,
}

impl NearbyCache {
    pub fn clear(&mut self) {
        self.player_render = None;
        self.static_entities.clear();
        self.mod_definitions.clear();
        self.monster_effects.clear();
        self.last_poll = None;
        self.last_result = None;
    }

    pub fn sample(&mut self, process: &Process, probe: &Probe) -> io::Result<NearbySnapshot> {
        let now = Instant::now();
        if self
            .last_poll
            .is_some_and(|previous| now.saturating_duration_since(previous) < REFRESH_DELAY)
        {
            if let Some(previous) = &self.last_result {
                return previous.clone().map_err(invalid);
            }
        }
        self.last_poll = Some(now);
        let scan_started = Instant::now();
        let read = self
            .read_fresh(process, probe, None)
            .map(|mut result| {
                result.scan_wall = scan_started.elapsed();
                result
            })
            .map_err(|error| error.to_string());
        self.last_result = Some(read.clone());
        read.map_err(invalid)
    }

    fn read_fresh(
        &mut self,
        process: &Process,
        probe: &Probe,
        mut debug_targets: Option<&mut Vec<diagnostic::Target>>,
    ) -> io::Result<NearbySnapshot> {
        // The caller normally samples vitals first; check player identity anyway
        // so no stale pointers are accepted across character transitions.
        if super::read_player_id(process, probe.local_player)? != probe.player_id {
            return Err(invalid("PoE1 nearby reader player changed"));
        }

        let player_render = match self.player_render {
            Some(address) => address,
            None => {
                let address = resolve_component(process, probe.local_player, "Render")
                    .ok_or_else(|| invalid("PoE1 player Render unavailable"))?;
                self.player_render = Some(address);
                address
            }
        };
        let player = read_xy(process, player_render)?;

        let map = process.read_bytes(probe.area_instance + AREA_AWAKE_ENTITIES, 16)?;
        let head = get_u64(&map, 0) as usize;
        let size = get_i32(&map, 8);
        if !(0..=MAX_MAP_NODES as i32).contains(&size) {
            return Err(invalid("awake entity count exceeds bounded reader budget"));
        }
        let mut result = NearbySnapshot::default();
        let mut remaining_effects = MAX_MONSTER_EFFECTS_PER_SCAN;
        if size == 0 {
            return Ok(result);
        }
        if !plausible_ptr(head) {
            return Err(invalid("PoE1 awake entity map has invalid head"));
        }
        let root = read_ptr(process, head + 8)?;
        let mut stack = vec![root];
        let mut seen = HashSet::<usize>::with_capacity((size as usize).min(128));
        while let Some(at) = stack.pop() {
            if at == head || !plausible_ptr(at) || !seen.insert(at) {
                continue;
            }
            if seen.len() > MAX_MAP_NODES {
                return Err(invalid("PoE1 entity tree exceeded traversal limit"));
            }
            let node = process.read_bytes(at, MAP_NODE_SIZE)?;
            if node[MAP_IS_NIL] != 0 {
                continue;
            }
            for offset in [0, 16] {
                let child = get_u64(&node, offset) as usize;
                if child != head && plausible_ptr(child) && !seen.contains(&child) {
                    stack.push(child);
                }
            }
            result.awake_nodes += 1;
            let id = get_u32(&node, MAP_KEY_ID);
            if id == 0 || id >= 0x4000_0000 || id == probe.player_id {
                continue;
            }
            let entity = get_u64(&node, MAP_ENTITY_PTR) as usize;
            if !plausible_ptr(entity) {
                continue;
            }
            let cache_key = (id, entity);
            let kind = match self.static_entities.get(&cache_key) {
                Some(item) => *item,
                None => {
                    let item = classify(process, entity, &mut self.mod_definitions)?;
                    if self.static_entities.len() >= MAX_STATIC_CACHE {
                        self.static_entities.clear();
                    }
                    self.static_entities.insert(cache_key, item);
                    item
                }
            };
            let CachedEntity::Monster(components) = kind else {
                continue;
            };
            // Only the read-only diagnostic path requests these targets. Normal
            // automation does no additional reads or diagnostic allocation.
            if let Some(targets) = debug_targets.as_deref_mut() {
                if process.read_i32(components.properties + MONSTER_RARITY)? == 3 {
                    targets.push(diagnostic::Target {
                        id,
                        entity,
                        components,
                    });
                }
            }
            result.candidate_monsters += 1;
            match read_nearby_entity(
                process,
                components,
                player,
                &mut self.monster_effects,
                &mut remaining_effects,
            )? {
                Some(NearbyEntity::Monster(monster)) => {
                    result.nearby.push(monster);
                    if result.nearby.len() > MAX_NEARBY_MONSTERS {
                        return Err(invalid("PoE1 nearby monster count exceeds budget"));
                    }
                }
                Some(NearbyEntity::Corpse(corpse)) => {
                    result.corpses.push(corpse);
                    if result.corpses.len() > MAX_NEARBY_CORPSES {
                        return Err(invalid("PoE1 nearby corpse count exceeds budget"));
                    }
                }
                None => {}
            }
        }

        // A changing map may have been traversed partly. Never use a partial
        // result to authorize anything in the future.
        if result.awake_nodes != size as usize {
            return Err(invalid("PoE1 awake entity tree changed during scan"));
        }
        Ok(result)
    }
}

fn classify(
    process: &Process,
    entity: usize,
    definitions: &mut HashMap<usize, MonsterMods>,
) -> io::Result<CachedEntity> {
    let path = match read_metadata(process, entity) {
        Ok(path) => path,
        Err(_) => return Ok(CachedEntity::Ignored),
    };
    if !path.starts_with("Metadata/Monsters/") || is_excluded_monster_path(&path) {
        return Ok(CachedEntity::Ignored);
    }
    let Some(mut components) = find_monster_components(process, entity)? else {
        return Ok(CachedEntity::Ignored);
    };
    // GameHelper classifies pinnacle bosses by their modifiers, not their name
    // or rarity. This is sampled only once per cached entity in the area.
    let mods = read_monster_mods(process, components.properties, definitions)?;
    if is_harbinger(&path) && mods.cannot_be_damaged {
        return Ok(CachedEntity::Ignored);
    }
    components.encounter = Encounter::classify(&path, components.has_npc, mods);
    if components.buffs.is_none()
        && (components.encounter != Encounter::Ordinary
            || (process.read_i32(components.properties + MONSTER_RARITY)? == 3
                && process.read_i32(components.life + LIFE_CURRENT)? > 0))
    {
        // Do not cache an incomplete encounter classification while it spawns.
        return Err(invalid("PoE1 encounter Buffs component unavailable"));
    }
    Ok(CachedEntity::Monster(components))
}

fn read_monster_mods(
    process: &Process,
    properties: usize,
    definitions: &mut HashMap<usize, MonsterMods>,
) -> io::Result<MonsterMods> {
    let headers = process.read_bytes(
        properties + MONSTER_MODS,
        MONSTER_MOD_VECTOR_COUNT * MOD_VECTOR_STRIDE,
    )?;
    let mut result = MonsterMods::default();
    for header in headers.chunks_exact(MOD_VECTOR_STRIDE) {
        let first = get_u64(header, 0) as usize;
        let last = get_u64(header, 8) as usize;
        let count = monster_mod_count(first, last)?;
        let entries = if count == 0 {
            Vec::new()
        } else {
            process.read_bytes(first, count * MOD_STRIDE)?
        };
        for entry in entries.chunks_exact(MOD_STRIDE) {
            let row = get_u64(entry, MOD_DATA_PTR) as usize;
            if !plausible_ptr(row) {
                return Err(invalid("PoE1 monster mod definition invalid"));
            }
            if definitions.len() >= MAX_STATIC_CACHE {
                definitions.clear();
            }
            let flags = match definitions.get(&row) {
                Some(value) => *value,
                None => {
                    let name =
                        super::charge_cost::read_utf16_z(process, read_ptr(process, row)?, 128)?;
                    if name.is_empty() || name.chars().any(char::is_control) {
                        return Err(invalid("invalid PoE1 monster mod name"));
                    }
                    let flags = MonsterMods::from_name(&name);
                    definitions.insert(row, flags);
                    flags
                }
            };
            result.include(flags);
        }
    }
    Ok(result)
}

fn monster_mod_count(first: usize, last: usize) -> io::Result<usize> {
    if first == 0 && last == 0 {
        Ok(0)
    } else {
        vector_count(first, last, MOD_STRIDE, MAX_MONSTER_MODS)
    }
}

fn find_monster_components(process: &Process, entity: usize) -> io::Result<Option<Components>> {
    let details = read_ptr(process, entity + 8)?;
    let lookup = read_ptr(process, details + 0x28)?;
    let list = process.read_bytes(entity + 0x10, 16)?;
    let first = get_u64(&list, 0) as usize;
    let component_count = vector_count(first, get_u64(&list, 8) as usize, 8, MAX_COMPONENTS)?;
    let bucket = process.read_bytes(lookup + 0x28, 16)?;
    let name_first = get_u64(&bucket, 0) as usize;
    let name_count = vector_count(name_first, get_u64(&bucket, 8) as usize, 16, MAX_COMPONENTS)?;
    if component_count == 0 || name_count == 0 {
        return Ok(None);
    }
    let component_pointers = process.read_bytes(first, component_count * 8)?;
    let names = process.read_bytes(name_first, name_count * 16)?;
    let mut selected = [0usize; 7];
    let mut has_npc = false;
    for entry in names.chunks_exact(16) {
        let name_at = get_u64(entry, 0) as usize;
        let index = get_i32(entry, 8);
        if !plausible_ptr(name_at) || index < 0 || index as usize >= component_count {
            continue;
        }
        let name = match process.read_utf8_z(name_at, 32) {
            Ok(value) => value,
            Err(_) => continue,
        };
        let selected_index = match name.as_str() {
            "Render" => Some(0),
            "Life" => Some(1),
            "Positioned" => Some(2),
            "ObjectMagicProperties" => Some(3),
            "Targetable" => Some(4),
            "Stats" => Some(5),
            "Buffs" => Some(6),
            "NPC" => {
                has_npc = plausible_ptr(get_u64(&component_pointers, index as usize * 8) as usize);
                None
            }
            _ => None,
        };
        if let Some(idx) = selected_index {
            let address = get_u64(&component_pointers, index as usize * 8) as usize;
            if plausible_ptr(address) {
                selected[idx] = address;
            }
        }
    }
    if selected[..6].contains(&0) {
        return Ok(None);
    }
    Ok(Some(Components {
        render: selected[0],
        life: selected[1],
        positioned: selected[2],
        properties: selected[3],
        targetable: selected[4],
        buffs: (selected[6] != 0).then_some(selected[6]),
        has_npc,
        encounter: Encounter::Ordinary,
    }))
}

fn vector_count(first: usize, last: usize, stride: usize, limit: usize) -> io::Result<usize> {
    if !plausible_ptr(first) || last < first || (last - first) % stride != 0 {
        return Err(invalid("invalid PoE1 monster component vector"));
    }
    let count = (last - first) / stride;
    if count > limit {
        return Err(invalid("too many PoE1 monster components"));
    }
    Ok(count)
}

fn read_nearby_entity(
    process: &Process,
    components: Components,
    player: (f32, f32),
    effects: &mut MonsterEffectCache,
    remaining_effects: &mut usize,
) -> io::Result<Option<NearbyEntity>> {
    let current_life = process.read_i32(components.life + LIFE_CURRENT)?;
    if current_life < 0 {
        return Ok(None);
    }
    let reaction = process.read_bytes(components.positioned + POSITIONED_REACTION, 1)?[0];
    let targetable = process.read_i32(components.targetable + TARGETABLE_FLAG)? as u32;
    if reaction & 0x7f == 1 || targetable & 1 == 0 {
        return Ok(None);
    }
    // A zero-Life targetable hostile monster is a candidate for corpse skills.
    // TargetTypes=Corpse is a SKILL target filter, not an entity-memory field;
    // this heuristic needs live verification (shattered/consumed bodies differ).
    let rarity = if current_life > 0 {
        Rarity::parse(process.read_i32(components.properties + MONSTER_RARITY)?)
    } else {
        None
    };
    if current_life > 0 && rarity.is_none() {
        return Ok(None);
    }
    let (x, y) = read_xy(process, components.render)?;
    let dx = (x - player.0) / WORLD_TO_GRID;
    let dy = (y - player.1) / WORLD_TO_GRID;
    let squared = dx * dx + dy * dy;
    let candidate = classify_nearby(current_life, rarity, squared);
    if matches!(candidate, Some(NearbyEntity::Monster(_)))
        && (components.encounter != Encounter::Ordinary
            || matches!(rarity, Some(Rarity::Rare | Rarity::Unique)))
    {
        let observed = match components.buffs {
            Some(at) => effects.sample(process, at, remaining_effects)?,
            None if rarity == Some(Rarity::Unique) => {
                return Err(invalid("PoE1 boss Buffs component unavailable"));
            }
            None => MonsterEffects::default(),
        };
        return Ok(filter_encounter(candidate, components.encounter, observed));
    }
    Ok(candidate)
}

fn filter_encounter(
    candidate: Option<NearbyEntity>,
    encounter: Encounter,
    effects: MonsterEffects,
) -> Option<NearbyEntity> {
    if let Some(NearbyEntity::Monster(monster)) = &candidate {
        if !encounter.allows(effects, monster.rarity == Rarity::Unique) {
            return None;
        }
    }
    candidate
}

/// Pure final classification keeps the existing living-monster counts separate
/// from dead, targetable corpse candidates.
fn classify_nearby(life: i32, rarity: Option<Rarity>, squared: f32) -> Option<NearbyEntity> {
    if !squared.is_finite() || squared > MAX_GRID_DISTANCE * MAX_GRID_DISTANCE {
        return None;
    }
    match life {
        0 => Some(NearbyEntity::Corpse(NearbyCorpse {
            distance_squared: squared,
        })),
        1.. => rarity.map(|rarity| {
            NearbyEntity::Monster(NearbyMonster {
                rarity,
                distance_squared: squared,
            })
        }),
        _ => None,
    }
}

fn read_xy(process: &Process, render: usize) -> io::Result<(f32, f32)> {
    let values = process.read_bytes(render + RENDER_POSITION, 8)?;
    let x = f32::from_le_bytes(values[0..4].try_into().expect("x"));
    let y = f32::from_le_bytes(values[4..8].try_into().expect("y"));
    if !x.is_finite() || !y.is_finite() || x.abs() > 100_000_000.0 || y.abs() > 100_000_000.0 {
        return Err(invalid("implausible monster or player position"));
    }
    Ok((x, y))
}

fn get_u64(bytes: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(bytes[at..at + 8].try_into().expect("bounded u64"))
}
fn get_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded u32"))
}
fn get_i32(bytes: &[u8], at: usize) -> i32 {
    i32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded i32"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rarity_distance_counts_are_single_pass_and_cumulative() {
        let snapshot = NearbySnapshot {
            awake_nodes: 4,
            candidate_monsters: 4,
            scan_wall: Duration::ZERO,
            corpses: Vec::new(),
            nearby: vec![
                NearbyMonster {
                    rarity: Rarity::Normal,
                    distance_squared: 20.0 * 20.0,
                },
                NearbyMonster {
                    rarity: Rarity::Magic,
                    distance_squared: 60.0 * 60.0,
                },
                NearbyMonster {
                    rarity: Rarity::Rare,
                    distance_squared: 125.0 * 125.0,
                },
                NearbyMonster {
                    rarity: Rarity::Unique,
                    distance_squared: 100.0 * 100.0,
                },
            ],
        };
        assert_eq!(snapshot.counts_at(50.0), [1, 0, 0, 0]);
        assert_eq!(snapshot.counts_at(75.0), [1, 1, 0, 0]);
        assert_eq!(snapshot.counts_at(150.0), [1, 1, 1, 1]);
    }

    #[test]
    fn living_monsters_and_dead_corpse_candidates_stay_separate() {
        let living = classify_nearby(10, Some(Rarity::Magic), 20.0 * 20.0);
        assert!(matches!(living, Some(NearbyEntity::Monster(_))));
        let corpse = classify_nearby(0, None, 20.0 * 20.0);
        assert!(matches!(corpse, Some(NearbyEntity::Corpse(_))));
        assert!(classify_nearby(-1, None, 20.0 * 20.0).is_none());
        assert!(classify_nearby(0, None, 200.0 * 200.0).is_none());

        let snapshot = NearbySnapshot {
            corpses: vec![
                NearbyCorpse {
                    distance_squared: 20.0 * 20.0,
                },
                NearbyCorpse {
                    distance_squared: 60.0 * 60.0,
                },
            ],
            ..NearbySnapshot::default()
        };
        assert_eq!(snapshot.corpses_at(30.0), 1);
        assert_eq!(snapshot.corpses_at(60.0), 1); // Strict radius
        assert_eq!(snapshot.corpses_at(61.0), 2);
        assert_eq!(snapshot.corpses_at(-1.0), 0);
        assert_eq!(snapshot.corpses_at(f32::NAN), 0);
        assert_eq!(snapshot.counts_at(150.0), [0; 4]);
    }

    #[test]
    fn invalid_radius_and_bad_rarity_are_rejected() {
        assert_eq!(NearbySnapshot::default().counts_at(-1.0), [0, 0, 0, 0]);
        assert_eq!(NearbySnapshot::default().counts_at(f32::NAN), [0, 0, 0, 0]);
        assert_eq!(Rarity::parse(-1), None);
        assert_eq!(Rarity::parse(4), None);
    }

    #[test]
    fn special_encounter_entities_are_ignored() {
        assert!(is_excluded_monster_path(
            "Metadata/Monsters/Spirit/TormentedSomething"
        ));
        assert!(!is_excluded_monster_path(
            "Metadata/Monsters/Skeletons/Skeleton"
        ));
    }

    #[test]
    fn hidden_boss_counts_do_not_mask_active_adds_or_change_corpse_candidates() {
        let mut snapshot = NearbySnapshot::default();
        let hidden = MonsterEffects::from_name("hidden_monster");
        let frozen = MonsterEffects::from_name("augury_boss_frozen");
        for (rarity, encounter, effects) in [
            (Rarity::Unique, Encounter::Pinnacle, hidden),
            (Rarity::Unique, Encounter::Ordinary, hidden),
            (Rarity::Unique, Encounter::Ordinary, frozen),
            (
                Rarity::Unique,
                Encounter::Ordinary,
                MonsterEffects::default(),
            ),
            (
                Rarity::Normal,
                Encounter::Ordinary,
                MonsterEffects::default(),
            ),
        ] {
            if let Some(NearbyEntity::Monster(monster)) = filter_encounter(
                classify_nearby(100, Some(rarity), 20.0 * 20.0),
                encounter,
                effects,
            ) {
                snapshot.nearby.push(monster);
            }
        }
        assert_eq!(snapshot.counts_at(60.0), [1, 0, 0, 1]);
        let corpse = classify_nearby(0, None, 20.0 * 20.0);
        assert!(matches!(
            filter_encounter(corpse.clone(), Encounter::Pinnacle, hidden),
            Some(NearbyEntity::Corpse(_))
        ));
        assert!(matches!(
            filter_encounter(corpse.clone(), Encounter::Ordinary, frozen),
            Some(NearbyEntity::Corpse(_))
        ));
        assert!(matches!(
            filter_encounter(corpse, Encounter::Ordinary, hidden),
            Some(NearbyEntity::Corpse(_))
        ));
    }

    #[test]
    fn monster_mod_vectors_are_bounded_and_static_filters_reset_with_the_area() {
        assert_eq!(monster_mod_count(0, 0).unwrap(), 0);
        assert_eq!(monster_mod_count(0x10000, 0x10000 + MOD_STRIDE).unwrap(), 1);
        assert!(monster_mod_count(0, 8).is_err());
        assert!(monster_mod_count(0x10000, 0x10001).is_err());
        assert!(monster_mod_count(0x10000, 0x10000 + (MAX_MONSTER_MODS + 1) * MOD_STRIDE).is_err());
        let mut cache = NearbyCache::default();
        cache
            .mod_definitions
            .insert(0x10000, MonsterMods::from_name("PinnacleAtlasBoss"));
        cache.clear();
        assert!(cache.mod_definitions.is_empty());
        assert!(cache.static_entities.is_empty());
        assert!(cache.last_poll.is_none());
    }

    #[test]
    fn monster_mod_reader_handles_all_five_vectors_and_rejects_partial_entries() {
        let process = Process::current_for_tests().unwrap();
        let mut name = vec![0u16; 128];
        let text: Vec<_> = "PinnacleAtlasBoss".encode_utf16().collect();
        name[..text.len()].copy_from_slice(&text);
        let row = (name.as_ptr() as u64).to_le_bytes();
        let mut entry = [0u8; MOD_STRIDE];
        entry[MOD_DATA_PTR..MOD_DATA_PTR + 8].copy_from_slice(&(row.as_ptr() as u64).to_le_bytes());
        let mut properties = vec![0u8; MONSTER_MODS + MONSTER_MOD_VECTOR_COUNT * MOD_VECTOR_STRIDE];
        let mut definitions = HashMap::new();
        for index in 0..MONSTER_MOD_VECTOR_COUNT {
            properties.fill(0);
            let start = MONSTER_MODS + index * MOD_VECTOR_STRIDE;
            properties[start..start + 8].copy_from_slice(&(entry.as_ptr() as u64).to_le_bytes());
            properties[start + 8..start + 16]
                .copy_from_slice(&((entry.as_ptr() as u64) + MOD_STRIDE as u64).to_le_bytes());
            let mods = read_monster_mods(&process, properties.as_ptr() as usize, &mut definitions)
                .unwrap();
            assert!(mods.pinnacle, "vector {index}");
            assert!(!mods.cannot_be_damaged);
        }
        let last = MONSTER_MODS + (MONSTER_MOD_VECTOR_COUNT - 1) * MOD_VECTOR_STRIDE + 8;
        properties[last..last + 8].copy_from_slice(&((entry.as_ptr() as u64) + 1).to_le_bytes());
        assert!(
            read_monster_mods(&process, properties.as_ptr() as usize, &mut definitions).is_err()
        );
    }

    #[test]
    fn live_encounter_effects_are_reread_while_definition_flags_stay_cached() {
        use super::super::offsets::{BUFFS_EFFECTS, DEFINITION_BUFF_TYPE, STATUS_DEFINITION};
        let process = Process::current_for_tests().unwrap();
        for (effect_name, encounter) in [
            ("hidden_monster", Encounter::Pinnacle),
            ("hidden_monster", Encounter::Ordinary),
            ("augury_boss_frozen", Encounter::Ordinary),
        ] {
            let mut name = vec![0u16; 128];
            let text: Vec<_> = effect_name.encode_utf16().collect();
            name[..text.len()].copy_from_slice(&text);
            let mut definition = [0u8; DEFINITION_BUFF_TYPE + 1];
            definition[..8].copy_from_slice(&(name.as_ptr() as u64).to_le_bytes());
            let mut entry = [0u8; STATUS_DEFINITION + 8];
            entry[STATUS_DEFINITION..STATUS_DEFINITION + 8]
                .copy_from_slice(&(definition.as_ptr() as u64).to_le_bytes());
            let pointers = [entry.as_ptr() as u64];
            let mut buffs = vec![0u8; BUFFS_EFFECTS + 16];
            let header = [pointers.as_ptr() as u64, pointers.as_ptr() as u64 + 8];
            for (index, value) in header.into_iter().enumerate() {
                buffs[BUFFS_EFFECTS + index * 8..BUFFS_EFFECTS + (index + 1) * 8]
                    .copy_from_slice(&value.to_le_bytes());
            }
            let blocked_header = buffs[BUFFS_EFFECTS..].to_vec();
            let render = vec![0u8; RENDER_POSITION + 8];
            let mut life = vec![0u8; LIFE_CURRENT + 4];
            life[LIFE_CURRENT..].copy_from_slice(&100i32.to_le_bytes());
            let positioned = vec![0u8; POSITIONED_REACTION + 1];
            let mut properties = vec![0u8; MONSTER_RARITY + 4];
            properties[MONSTER_RARITY..].copy_from_slice(&3i32.to_le_bytes());
            let mut targetable = [0u8; TARGETABLE_FLAG + 4];
            targetable[TARGETABLE_FLAG..].copy_from_slice(&1u32.to_le_bytes());
            let components = Components {
                render: render.as_ptr() as usize,
                life: life.as_ptr() as usize,
                positioned: positioned.as_ptr() as usize,
                properties: properties.as_ptr() as usize,
                targetable: targetable.as_ptr() as usize,
                buffs: Some(buffs.as_ptr() as usize),
                has_npc: false,
                encounter,
            };
            let mut effects = MonsterEffectCache::default();
            let mut read = |observed| {
                let mut remaining = MAX_MONSTER_EFFECTS_PER_SCAN;
                read_nearby_entity(&process, observed, (0.0, 0.0), &mut effects, &mut remaining)
            };
            assert!(read(components).unwrap().is_none());
            assert_eq!(
                crate::poe1::buffs::debug_monster_names(&process, buffs.as_ptr() as usize).unwrap(),
                [effect_name]
            );
            // No timer is required. Removing the status entry must immediately allow it.
            buffs[BUFFS_EFFECTS..].fill(0);
            assert!(
                crate::poe1::buffs::debug_monster_names(&process, buffs.as_ptr() as usize)
                    .unwrap()
                    .is_empty()
            );
            assert!(matches!(
                read(components).unwrap(),
                Some(NearbyEntity::Monster(_))
            ));
            buffs[BUFFS_EFFECTS..].copy_from_slice(&blocked_header);
            assert!(read(components).unwrap().is_none());
            // Living-enemy filters must not silently remove existing corpse candidates.
            life[LIFE_CURRENT..].copy_from_slice(&0i32.to_le_bytes());
            assert!(matches!(
                read(components).unwrap(),
                Some(NearbyEntity::Corpse(_))
            ));
            assert!(matches!(
                read(Components {
                    buffs: None,
                    ..components
                })
                .unwrap(),
                Some(NearbyEntity::Corpse(_))
            ));
            life[LIFE_CURRENT..].copy_from_slice(&100i32.to_le_bytes());
            buffs[BUFFS_EFFECTS..BUFFS_EFFECTS + 8].copy_from_slice(&1u64.to_le_bytes());
            assert!(read(components).is_err());
            assert!(read(Components {
                buffs: None,
                ..components
            })
            .is_err());
        }
    }
}
