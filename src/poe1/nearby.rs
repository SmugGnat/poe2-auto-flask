//! EXPERIMENTAL read-only PoE1 awake-monster census.
//! No automation currently consumes this reader. An explicit diagnostic flag
//! enables it so the candidate filters and cost can be verified in-game.
//! Bounded std::map walk; cache entity component pointers within an area.
use std::collections::{HashMap, HashSet};
use std::io;
use std::time::{Duration, Instant};

use crate::entity::resolve_component;
use crate::process::Process;

use super::{invalid, plausible_ptr, read_metadata, read_ptr, Probe};

const AREA_AWAKE_ENTITIES: usize = 0xA28;
const MAP_NODE_SIZE: usize = 0x30;
const MAP_KEY_ID: usize = 0x20;
const MAP_ENTITY_PTR: usize = 0x28;
const MAP_IS_NIL: usize = 0x19;
const MAX_MAP_NODES: usize = 1500;
const MAX_NEARBY_MONSTERS: usize = 512;
const MAX_STATIC_CACHE: usize = 2500;
const MAX_COMPONENTS: usize = 50;
const MAX_GRID_DISTANCE: f32 = 150.0;
const WORLD_TO_GRID: f32 = 250.0 / 23.0;
const REFRESH_DELAY: Duration = Duration::from_millis(750);

// Monster-side offsets. ObjectMagicProperties has a different base from
// the inventory item's Mods component.
const RENDER_POSITION: usize = 0x120;
const LIFE_CURRENT: usize = 0x178 + 0x30;
const POSITIONED_REACTION: usize = 0x1E0;
const TARGETABLE_FLAG: usize = 0x50;
const MONSTER_RARITY: usize = 0x90 + 0xB4;

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

#[derive(Debug, Clone, Default)]
pub struct NearbySnapshot {
    pub awake_nodes: usize,
    pub candidate_monsters: usize,
    pub nearby: Vec<NearbyMonster>,
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
}

#[derive(Debug, Clone, Copy)]
struct Components {
    render: usize,
    life: usize,
    positioned: usize,
    properties: usize,
    targetable: usize,
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
    last_poll: Option<Instant>,
    last_result: Option<Result<NearbySnapshot, String>>,
}

impl NearbyCache {
    pub fn clear(&mut self) {
        self.player_render = None;
        self.static_entities.clear();
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
        let read = self
            .read_fresh(process, probe)
            .map_err(|error| error.to_string());
        self.last_result = Some(read.clone());
        read.map_err(invalid)
    }

    fn read_fresh(&mut self, process: &Process, probe: &Probe) -> io::Result<NearbySnapshot> {
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
                    let item = classify(process, entity)?;
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
            result.candidate_monsters += 1;
            if let Some(monster) = read_live_monster(process, components, player)? {
                result.nearby.push(monster);
                if result.nearby.len() > MAX_NEARBY_MONSTERS {
                    return Err(invalid("PoE1 nearby monster count exceeds budget"));
                }
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

fn classify(process: &Process, entity: usize) -> io::Result<CachedEntity> {
    let path = match read_metadata(process, entity) {
        Ok(path) => path,
        Err(_) => return Ok(CachedEntity::Ignored),
    };
    if !path.starts_with("Metadata/Monsters/") || is_excluded_monster_path(&path) {
        return Ok(CachedEntity::Ignored);
    }
    let Some(components) = find_monster_components(process, entity)? else {
        return Ok(CachedEntity::Ignored);
    };
    Ok(CachedEntity::Monster(components))
}

fn is_excluded_monster_path(path: &str) -> bool {
    path.starts_with("Metadata/Monsters/Avatar/")
        || path.starts_with("Metadata/Monsters/Spirit/Tormented")
        || path.starts_with("Metadata/Monsters/LeagueAffliction/DoodadDaemons/")
        || path.starts_with("Metadata/Monsters/LeagueAffliction/Volatile/")
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
    let mut selected = [0usize; 5];
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
            _ => None,
        };
        if let Some(idx) = selected_index {
            let address = get_u64(&component_pointers, index as usize * 8) as usize;
            if plausible_ptr(address) {
                selected[idx] = address;
            }
        }
    }
    if selected.contains(&0) {
        return Ok(None);
    }
    Ok(Some(Components {
        render: selected[0],
        life: selected[1],
        positioned: selected[2],
        properties: selected[3],
        targetable: selected[4],
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

fn read_live_monster(
    process: &Process,
    components: Components,
    player: (f32, f32),
) -> io::Result<Option<NearbyMonster>> {
    let current_life = process.read_i32(components.life + LIFE_CURRENT)?;
    if current_life <= 0 {
        return Ok(None);
    }
    let reaction = process.read_bytes(components.positioned + POSITIONED_REACTION, 1)?[0];
    if reaction & 0x7f == 1 {
        return Ok(None);
    }
    let targetable = process.read_i32(components.targetable + TARGETABLE_FLAG)? as u32;
    if targetable & 1 == 0 {
        return Ok(None);
    }
    let rarity = process.read_i32(components.properties + MONSTER_RARITY)?;
    let Some(rarity) = Rarity::parse(rarity) else {
        return Ok(None);
    };
    let (x, y) = read_xy(process, components.render)?;
    let dx = (x - player.0) / WORLD_TO_GRID;
    let dy = (y - player.1) / WORLD_TO_GRID;
    let squared = dx * dx + dy * dy;
    if !squared.is_finite() || squared > MAX_GRID_DISTANCE * MAX_GRID_DISTANCE {
        return Ok(None);
    }
    Ok(Some(NearbyMonster {
        rarity,
        distance_squared: squared,
    }))
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
    fn invalid_radius_and_bad_rarity_are_rejected() {
        assert_eq!(NearbySnapshot::default().counts_at(-1.0), [0, 0, 0, 0]);
        assert_eq!(NearbySnapshot::default().counts_at(f32::NAN), [0, 0, 0, 0]);
        assert_eq!(Rarity::parse(-1), None);
        assert_eq!(Rarity::parse(4), None);
    }

    #[test]
    fn special_encounter_entities_are_ignored() {
        assert!(is_excluded_monster_path(
            "Metadata/Monsters/Avatar/Harbinger"
        ));
        assert!(is_excluded_monster_path(
            "Metadata/Monsters/Spirit/TormentedSomething"
        ));
        assert!(!is_excluded_monster_path(
            "Metadata/Monsters/Skeletons/Skeleton"
        ));
    }
}
