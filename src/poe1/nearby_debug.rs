//! Opt-in boss diagnostics. These observations never authorize automation.
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use super::*;
use crate::poe1::offsets::{STATS_DATA_PTR, STATS_VECTOR, STAT_STRIDE};

const MAX_BOSSES: usize = 16;
const MAX_STATS: usize = 8192;
const MAX_STATES: usize = 256;
const MAX_STATE_NAME: usize = 128;
type Identity = (usize, u32, usize);

pub(super) struct Target {
    pub id: u32,
    pub entity: usize,
    pub components: Components,
}

pub(crate) struct NearbyDiagnostic {
    pub census: NearbySnapshot,
    pub bosses: Vec<BossSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BossSnapshot {
    identity: Identity,
    metadata: String,
    detail: Result<BossDetail, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BossDetail {
    distance: u16,
    life: i32,
    total_life: i32,
    targetable: u32,
    targetable_extra: Result<TargetableExtra, String>,
    friendly: bool,
    encounter: Encounter,
    eligible: Option<bool>,
    state_machine: Result<Option<Vec<StateValue>>, String>,
    buffs: Result<Vec<String>, String>,
    mods: Result<Vec<String>, String>,
    stats: Result<BTreeMap<i32, i32>, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetableExtra {
    quest_state: u8,
    hidden_flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StateValue {
    name: String,
    value: i64,
}

impl NearbyCache {
    pub(crate) fn sample_debug(
        &mut self,
        process: &Process,
        probe: &Probe,
    ) -> io::Result<NearbyDiagnostic> {
        let mut targets = Vec::new();
        let started = Instant::now();
        let mut census = self.read_fresh(process, probe, Some(&mut targets))?;
        census.scan_wall = started.elapsed();
        let player = read_xy(
            process,
            self.player_render
                .ok_or_else(|| invalid("player Render unavailable"))?,
        )?;
        let mut candidates: Vec<_> = targets
            .into_iter()
            .filter_map(|target| {
                let (x, y) = read_xy(process, target.components.render).ok()?;
                let distance = ((x - player.0).hypot(y - player.1)) / WORLD_TO_GRID;
                (distance <= MAX_GRID_DISTANCE).then_some((distance.round() as u16, target))
            })
            .collect();
        candidates.sort_by_key(|(distance, target)| (*distance, target.id));
        let bosses = candidates
            .into_iter()
            .take(MAX_BOSSES)
            .map(|(distance, target)| {
                let identity = (probe.area_instance, target.id, target.entity);
                let metadata = read_metadata(process, target.entity)
                    .unwrap_or_else(|error| format!("unavailable ({error})"));
                let detail =
                    read_detail(process, &target, distance).map_err(|error| error.to_string());
                BossSnapshot {
                    identity,
                    metadata,
                    detail,
                }
            })
            .collect();
        Ok(NearbyDiagnostic { census, bosses })
    }
}

fn read_detail(process: &Process, target: &Target, distance: u16) -> io::Result<BossDetail> {
    if super::super::read_player_id(process, target.entity)? != target.id {
        return Err(invalid("boss identity changed"));
    }
    let components = target.components;
    let life = process.read_i32(components.life + LIFE_CURRENT)?;
    let total_life = process.read_i32(components.life + LIFE_CURRENT - 4)?;
    let targetable = process.read_i32(components.targetable + TARGETABLE_FLAG)? as u32;
    let targetable_extra =
        read_targetable_extra(process, components.targetable).map_err(|error| error.to_string());
    let reaction = process.read_bytes(components.positioned + POSITIONED_REACTION, 1)?[0];
    let friendly = reaction & 0x7f == 1;
    let buffs = components.buffs.map_or_else(
        || Err("Buffs component absent".to_string()),
        |at| {
            crate::poe1::buffs::debug_monster_names(process, at).map_err(|error| error.to_string())
        },
    );
    let eligible = if life <= 0 || friendly || targetable & 1 == 0 {
        Some(false)
    } else {
        buffs.as_ref().ok().map(|names| {
            components.encounter.allows(
                MonsterEffects {
                    hidden: names.iter().any(|name| name == "hidden_monster"),
                    captured: names.iter().any(|name| name == "capture_monster_trapped"),
                    betrayal_safe: names
                        .iter()
                        .any(|name| name == "betrayal_target_safety_aura"),
                    augury_frozen: names.iter().any(|name| name == "augury_boss_frozen"),
                },
                true,
            )
        })
    };
    let mods = read_mod_names(process, components.properties).map_err(|error| error.to_string());
    let stats = read_stats(process, target.entity).map_err(|error| error.to_string());
    let state_machine = resolve_component(process, target.entity, "StateMachine")
        .map(|component| read_state_machine(process, component))
        .transpose()
        .map_err(|error| error.to_string());
    if super::super::read_player_id(process, target.entity)? != target.id {
        return Err(invalid("boss identity changed during diagnostic read"));
    }
    Ok(BossDetail {
        distance,
        life,
        total_life,
        targetable,
        targetable_extra,
        friendly,
        encounter: components.encounter,
        eligible,
        state_machine,
        buffs,
        mods,
        stats,
    })
}

fn read_targetable_extra(process: &Process, component: usize) -> io::Result<TargetableExtra> {
    let bytes = process.read_bytes(component + TARGETABLE_EXTRA, 8)?;
    Ok(TargetableExtra {
        quest_state: bytes[2],
        hidden_flags: get_u32(&bytes, 4),
    })
}

/// Current GameHelper StateMachine layout, used only for observations. Never
/// truncate an invalid vector or interpret a state value as input permission.
fn read_state_machine(process: &Process, component: usize) -> io::Result<Vec<StateValue>> {
    let first_header = process.read_bytes(component + STATE_MACHINE_STATES_PTR, 8)?;
    let header = process.read_bytes(component + STATE_MACHINE_VALUES, 16)?;
    let first = get_u64(&header, 0) as usize;
    let last = get_u64(&header, 8) as usize;
    let count = if first == 0 && last == 0 {
        0
    } else {
        vector_count(first, last, 8, MAX_STATES)?
    };
    if count == 0 {
        return Ok(Vec::new());
    }
    let definitions = get_u64(&first_header, 0) as usize;
    if !plausible_ptr(definitions) {
        return Err(invalid("invalid boss StateMachine definition pointer"));
    }
    let names = read_ptr(process, definitions + STATE_MACHINE_NAMES)?;
    let descriptors = process.read_bytes(names, (count - 1) * STATE_MACHINE_NAME_STRIDE + 32)?;
    let values = process.read_bytes(first, count * 8)?;
    let mut states = Vec::with_capacity(count);
    for (index, value) in values.chunks_exact(8).enumerate() {
        let start = index * STATE_MACHINE_NAME_STRIDE;
        let name = read_state_name(process, &descriptors[start..start + 32])?;
        states.push(StateValue {
            name,
            value: i64::from_le_bytes(value.try_into().expect("bounded state value")),
        });
    }
    if process.read_bytes(component + STATE_MACHINE_STATES_PTR, 8)? != first_header
        || process.read_bytes(component + STATE_MACHINE_VALUES, 16)? != header
        || read_ptr(process, definitions + STATE_MACHINE_NAMES)? != names
    {
        return Err(invalid("boss StateMachine changed during diagnostic read"));
    }
    Ok(states)
}

fn read_state_name(process: &Process, descriptor: &[u8]) -> io::Result<String> {
    let length = get_u64(descriptor, 16) as usize;
    let capacity = get_u64(descriptor, 24) as usize;
    if length == 0 || length > MAX_STATE_NAME || length > capacity || capacity > 4096 {
        return Err(invalid("invalid boss StateMachine name length"));
    }
    let bytes = if capacity < 16 {
        descriptor[..length + 1].to_vec()
    } else {
        let pointer = get_u64(descriptor, 0) as usize;
        if !plausible_ptr(pointer) {
            return Err(invalid("invalid boss StateMachine name pointer"));
        }
        process.read_bytes(pointer, length + 1)?
    };
    if bytes[length] != 0 {
        return Err(invalid("unterminated boss StateMachine name"));
    }
    let text = std::str::from_utf8(&bytes[..length])
        .map_err(|_| invalid("invalid boss StateMachine name encoding"))?;
    if text.chars().any(char::is_control) {
        return Err(invalid("invalid boss StateMachine name text"));
    }
    Ok(text.to_owned())
}

fn read_mod_names(process: &Process, properties: usize) -> io::Result<Vec<String>> {
    let headers = process.read_bytes(
        properties + MONSTER_MODS,
        MONSTER_MOD_VECTOR_COUNT * MOD_VECTOR_STRIDE,
    )?;
    let mut names = Vec::new();
    for header in headers.chunks_exact(MOD_VECTOR_STRIDE) {
        let first = get_u64(header, 0) as usize;
        let count = monster_mod_count(first, get_u64(header, 8) as usize)?;
        let entries = if count == 0 {
            Vec::new()
        } else {
            process.read_bytes(first, count * MOD_STRIDE)?
        };
        for entry in entries.chunks_exact(MOD_STRIDE) {
            let row = get_u64(entry, MOD_DATA_PTR) as usize;
            if !plausible_ptr(row) {
                return Err(invalid("invalid boss mod definition"));
            }
            let name =
                crate::poe1::charge_cost::read_utf16_z(process, read_ptr(process, row)?, 128)?;
            if name.is_empty() || name.chars().any(char::is_control) {
                return Err(invalid("invalid boss mod name"));
            }
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

fn read_stats(process: &Process, entity: usize) -> io::Result<BTreeMap<i32, i32>> {
    let component = resolve_component(process, entity, "Stats")
        .ok_or_else(|| invalid("boss Stats unavailable"))?;
    let data = read_ptr(process, component + STATS_DATA_PTR)?;
    let header = process.read_bytes(data + STATS_VECTOR, 16)?;
    let first = get_u64(&header, 0) as usize;
    let last = get_u64(&header, 8) as usize;
    let count = if first == 0 && last == 0 {
        0
    } else {
        vector_count(first, last, STAT_STRIDE, MAX_STATS)?
    };
    let bytes = if count == 0 {
        Vec::new()
    } else {
        process.read_bytes(first, count * STAT_STRIDE)?
    };
    Ok(bytes
        .chunks_exact(STAT_STRIDE)
        .map(|entry| (get_i32(entry, 0), get_i32(entry, 4)))
        .collect())
}

#[derive(Default)]
pub(crate) struct BossReporter {
    previous: BTreeMap<Identity, BossSnapshot>,
}

impl BossReporter {
    pub fn report(&mut self, bosses: &[BossSnapshot], elapsed: Duration, area: &str) -> String {
        let mut text = String::new();
        let identities: BTreeSet<_> = bosses.iter().map(|boss| boss.identity).collect();
        self.previous.retain(|identity, _| {
            if identities.contains(identity) {
                true
            } else {
                let _ = writeln!(
                    text,
                    "  Boss {} no longer in diagnostic view (gone, outside range, or beyond the 16-boss limit)",
                    identity.1
                );
                false
            }
        });
        for boss in bosses {
            let previous = self.previous.get(&boss.identity);
            if previous == Some(boss) {
                continue;
            }
            let _ = writeln!(
                text,
                "  Boss {}: {}{}",
                boss.identity.1,
                boss.metadata,
                if previous.is_none() {
                    " [new observation]"
                } else {
                    ""
                }
            );
            let previous_detail = previous.and_then(|previous| previous.detail.as_ref().ok());
            match &boss.detail {
                Err(error) => {
                    let _ = writeln!(text, "    Details unavailable: {error}");
                }
                Ok(detail) => {
                    let eligible = match detail.eligible {
                        Some(true) => "yes",
                        Some(false) => "no",
                        None => "unknown",
                    };
                    let percent = if detail.total_life > 0 {
                        format!(
                            "{:.3}%",
                            100.0 * f64::from(detail.life) / f64::from(detail.total_life)
                        )
                    } else {
                        "unknown %".into()
                    };
                    let _ = writeln!(text, "    Distance {} | Life {}/{} ({}) | Targetable {} (0x{:x}) | Friendly {} | Filter {:?} | Nearby eligible {}",
                        detail.distance, detail.life, detail.total_life, percent, detail.targetable & 1 != 0,
                        detail.targetable, detail.friendly, detail.encounter, eligible);
                    if previous_detail.is_none_or(|old| old.targetable != detail.targetable) {
                        let _ = writeln!(text, "    Targetable bytes (diagnostic): targetable={} | highlightable={} | targeted_by_player={}",
                            (detail.targetable >> 8) as u8, (detail.targetable >> 16) as u8,
                            (detail.targetable >> 24) as u8);
                    }
                    report_targetable_extra(
                        &mut text,
                        &detail.targetable_extra,
                        previous_detail.map(|d| &d.targetable_extra),
                    );
                    report_states(
                        &mut text,
                        &detail.state_machine,
                        previous_detail.map(|d| &d.state_machine),
                    );
                    report_names(
                        &mut text,
                        "Boss buffs",
                        &detail.buffs,
                        previous_detail.map(|d| &d.buffs),
                    );
                    report_names(
                        &mut text,
                        "Boss mods",
                        &detail.mods,
                        previous_detail.map(|d| &d.mods),
                    );
                    report_stats(&mut text, &detail.stats, previous_detail.map(|d| &d.stats));
                }
            }
            self.previous.insert(boss.identity, boss.clone());
        }
        if text.is_empty() {
            text
        } else {
            format!(
                "Boss observations [{}] | Area: {area}\n{text}",
                timestamp(elapsed)
            )
        }
    }

    pub fn interrupted(&mut self, elapsed: Duration, reason: &str) -> String {
        if self.previous.is_empty() {
            return String::new();
        }
        self.previous.clear();
        format!(
            "Boss observations [{}]: interrupted ({reason}); next observation starts fresh\n",
            timestamp(elapsed)
        )
    }
}

fn timestamp(elapsed: Duration) -> String {
    format!("+{}.{:03}s", elapsed.as_secs(), elapsed.subsec_millis())
}

fn report_targetable_extra(
    text: &mut String,
    extra: &Result<TargetableExtra, String>,
    previous: Option<&Result<TargetableExtra, String>>,
) {
    if previous == Some(extra) {
        return;
    }
    match extra {
        Err(error) => {
            let _ = writeln!(text, "    Hidden flags: unavailable ({error})");
        }
        Ok(extra) => {
            let raw = extra.hidden_flags;
            let _ = writeln!(text, "    Hidden flags 0x{raw:x} (diagnostic bytes: needs_true={} | hidden_from_player={} | needs_false={}) | quest_state={}",
                raw as u8, (raw >> 8) as u8, (raw >> 16) as u8, extra.quest_state);
        }
    }
}

fn report_states(
    text: &mut String,
    states: &Result<Option<Vec<StateValue>>, String>,
    previous: Option<&Result<Option<Vec<StateValue>>, String>>,
) {
    if previous == Some(states) {
        return;
    }
    match (states, previous) {
        (Err(error), _) => {
            let _ = writeln!(text, "    Boss states: unavailable ({error})");
        }
        (Ok(None), _) => {
            let _ = writeln!(
                text,
                "    Boss states: StateMachine component not found/unresolved"
            );
        }
        (Ok(Some(states)), Some(Ok(Some(old)))) => {
            for index in 0..states.len().max(old.len()) {
                if states.get(index) != old.get(index) {
                    let describe = |state: Option<&StateValue>| {
                        state.map_or_else(
                            || "absent".into(),
                            |state| format!("{}={}", state.name, state.value),
                        )
                    };
                    let _ = writeln!(
                        text,
                        "    Boss state[{index}]: {} -> {}",
                        describe(old.get(index)),
                        describe(states.get(index))
                    );
                }
            }
        }
        (Ok(Some(states)), _) => {
            let _ = writeln!(text, "    Boss states ({}):", states.len());
            for (index, state) in states.iter().enumerate() {
                let _ = writeln!(text, "      [{index}] {}={}", state.name, state.value);
            }
        }
    }
}

fn report_names(
    text: &mut String,
    label: &str,
    names: &Result<Vec<String>, String>,
    previous: Option<&Result<Vec<String>, String>>,
) {
    if previous == Some(names) {
        return;
    }
    match (names, previous) {
        (Err(error), _) => {
            let _ = writeln!(text, "    {label}: unavailable ({error})");
        }
        (Ok(names), Some(Ok(old))) => {
            let added: Vec<_> = names
                .iter()
                .filter(|name| !old.contains(name))
                .cloned()
                .collect();
            let removed: Vec<_> = old
                .iter()
                .filter(|name| !names.contains(name))
                .cloned()
                .collect();
            let _ = writeln!(
                text,
                "    {label} changed: added [{}] | removed [{}]",
                added.join(", "),
                removed.join(", ")
            );
        }
        (Ok(names), _) => {
            let _ = writeln!(
                text,
                "    {label} ({}): {}",
                names.len(),
                if names.is_empty() {
                    "(none)".into()
                } else {
                    names.join(", ")
                }
            );
        }
    }
}

fn report_stats(
    text: &mut String,
    stats: &Result<BTreeMap<i32, i32>, String>,
    previous: Option<&Result<BTreeMap<i32, i32>, String>>,
) {
    if previous == Some(stats) {
        return;
    }
    match (stats, previous) {
        (Err(error), _) => {
            let _ = writeln!(text, "    Boss stats: unavailable ({error})");
        }
        (Ok(stats), Some(Ok(old))) => {
            let keys: BTreeSet<_> = old.keys().chain(stats.keys()).collect();
            for id in keys {
                if old.get(id) != stats.get(id) {
                    let value = |entry: Option<&i32>| {
                        entry.map_or_else(|| "absent".into(), |value| value.to_string())
                    };
                    let _ = writeln!(
                        text,
                        "    Boss stat {id} (0x{id:x}): {} -> {}",
                        value(old.get(id)),
                        value(stats.get(id))
                    );
                }
            }
        }
        (Ok(stats), _) => {
            let entries: Vec<_> = stats
                .iter()
                .map(|(id, value)| format!("{id}(0x{id:x})={value}"))
                .collect();
            let _ = writeln!(text, "    Boss stats ({}; numeric IDs):", stats.len());
            for chunk in entries.chunks(8) {
                let _ = writeln!(text, "      {}", chunk.join(", "));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> BossSnapshot {
        BossSnapshot {
            identity: (1, 42, 1000),
            metadata: "Metadata/Monsters/TestBoss".into(),
            detail: Ok(BossDetail {
                distance: 20,
                life: 100,
                total_life: 100,
                targetable: 1,
                targetable_extra: Ok(TargetableExtra {
                    quest_state: 4,
                    hidden_flags: 0x101,
                }),
                friendly: false,
                encounter: Encounter::Ordinary,
                eligible: Some(true),
                state_machine: Ok(Some(vec![StateValue {
                    name: "phase".into(),
                    value: 1,
                }])),
                buffs: Ok(vec!["test_invulnerability".into()]),
                mods: Ok(vec!["test_mod".into()]),
                stats: Ok(BTreeMap::from([(101, 1), (202, 5)])),
            }),
        }
    }

    #[test]
    fn boss_reporter_prints_first_snapshot_then_only_changes_and_handles_departures() {
        let mut reporter = BossReporter::default();
        let mut boss = snapshot();
        let first = reporter.report(&[boss.clone()], Duration::from_millis(1234), "Test area");
        assert!(first.contains("[+1.234s] | Area: Test area"));
        assert!(first.contains("[new observation]"));
        assert!(first.contains("(100.000%)"));
        assert!(first.contains("test_invulnerability"));
        assert!(first.contains("101(0x65)=1"));
        assert!(first.contains("[0] phase=1"));
        assert!(reporter
            .report(&[boss.clone()], Duration::from_secs(2), "Test area")
            .is_empty());
        let detail = boss.detail.as_mut().unwrap();
        detail.buffs = Ok(Vec::new());
        detail.stats.as_mut().unwrap().insert(101, 0);
        detail.state_machine.as_mut().unwrap().as_mut().unwrap()[0].value = 2;
        detail.targetable_extra.as_mut().unwrap().hidden_flags = 0;
        let changed = reporter.report(&[boss.clone()], Duration::from_millis(2250), "Test area");
        assert!(changed.contains("removed [test_invulnerability]"));
        assert!(changed.contains("101 (0x65): 1 -> 0"));
        assert!(changed.contains("phase=1 -> phase=2"));
        assert!(changed.contains("Hidden flags 0x0"));
        assert!(!changed.contains("test_mod"));
        assert!(reporter
            .report(&[], Duration::from_secs(3), "Test area")
            .contains("Boss 42 no longer"));
        assert!(reporter
            .report(&[boss], Duration::from_secs(4), "Test area")
            .contains("Boss stats (2"));
    }

    #[test]
    fn failed_boss_fields_are_distinct_from_verified_empty_lists() {
        let mut boss = snapshot();
        boss.detail.as_mut().unwrap().buffs = Err("read failed".into());
        boss.detail.as_mut().unwrap().eligible = None;
        boss.detail.as_mut().unwrap().targetable_extra = Err("read failed".into());
        boss.detail.as_mut().unwrap().state_machine = Err("read failed".into());
        let text = BossReporter::default().report(&[boss], Duration::ZERO, "Test area");
        assert!(text.contains("Nearby eligible unknown"));
        assert!(text.contains("Boss buffs: unavailable (read failed)"));
        assert!(text.contains("Boss states: unavailable (read failed)"));
        assert!(text.contains("Hidden flags: unavailable (read failed)"));
        assert!(!text.contains("Boss buffs (0)"));
    }

    #[test]
    fn interrupted_observations_reset_state_and_distinguish_empty_or_unresolved_components() {
        let mut reporter = BossReporter::default();
        let mut boss = snapshot();
        reporter.report(&[boss.clone()], Duration::ZERO, "Test area");
        assert!(reporter
            .interrupted(Duration::from_secs(1), "area changed")
            .contains("interrupted (area changed)"));
        assert!(reporter
            .interrupted(Duration::from_secs(2), "still waiting")
            .is_empty());
        let detail = boss.detail.as_mut().unwrap();
        detail.total_life = 0;
        detail.state_machine = Ok(None);
        let resumed = reporter.report(&[boss.clone()], Duration::from_secs(3), "New area");
        assert!(resumed.contains("[new observation]"));
        assert!(resumed.contains("StateMachine component not found/unresolved"));
        assert!(resumed.contains("unknown %"));
        boss.detail.as_mut().unwrap().state_machine = Ok(Some(Vec::new()));
        let empty = reporter.report(&[boss], Duration::from_secs(4), "New area");
        assert!(empty.contains("Boss states (0)"));
    }

    fn state_name_descriptor(name: &[u8]) -> [u8; 32] {
        assert_eq!(name.last(), Some(&0));
        let length = name.len() - 1;
        let mut descriptor = [0u8; 32];
        let capacity = if length < 16 {
            descriptor[..name.len()].copy_from_slice(name);
            15
        } else {
            descriptor[..8].copy_from_slice(&(name.as_ptr() as u64).to_le_bytes());
            length
        };
        descriptor[16..24].copy_from_slice(&(length as u64).to_le_bytes());
        descriptor[24..32].copy_from_slice(&(capacity as u64).to_le_bytes());
        descriptor
    }

    #[test]
    fn state_machine_reads_current_signed_values_and_inline_or_heap_names_with_bounds() {
        let process = Process::current_for_tests().unwrap();
        let mut names = vec![0u8; STATE_MACHINE_NAME_STRIDE + 32];
        names[..32].copy_from_slice(&state_name_descriptor(b"phase\0"));
        names[STATE_MACHINE_NAME_STRIDE..]
            .copy_from_slice(&state_name_descriptor(b"boss_vulnerability_state\0"));
        let mut definitions = [0u8; STATE_MACHINE_NAMES + 8];
        definitions[STATE_MACHINE_NAMES..].copy_from_slice(&(names.as_ptr() as u64).to_le_bytes());
        let mut values = [1i64, -9_007_199_254_740_993];
        let mut component = [0u8; STATE_MACHINE_VALUES + 16];
        component[STATE_MACHINE_STATES_PTR..STATE_MACHINE_STATES_PTR + 8]
            .copy_from_slice(&(definitions.as_ptr() as u64).to_le_bytes());
        let first = values.as_ptr() as u64;
        component[STATE_MACHINE_VALUES..STATE_MACHINE_VALUES + 8]
            .copy_from_slice(&first.to_le_bytes());
        component[STATE_MACHINE_VALUES + 8..].copy_from_slice(&(first + 16).to_le_bytes());
        let states = read_state_machine(&process, component.as_ptr() as usize).unwrap();
        assert_eq!(
            states[0],
            StateValue {
                name: "phase".into(),
                value: 1
            }
        );
        assert_eq!(
            states[1],
            StateValue {
                name: "boss_vulnerability_state".into(),
                value: values[1]
            }
        );
        values[0] = 2;
        assert_eq!(
            read_state_machine(&process, component.as_ptr() as usize).unwrap()[0].value,
            values[0]
        );
        component[STATE_MACHINE_VALUES + 8..]
            .copy_from_slice(&(first + ((MAX_STATES + 1) * 8) as u64).to_le_bytes());
        assert!(read_state_machine(&process, component.as_ptr() as usize).is_err());
        component[STATE_MACHINE_VALUES + 8..].copy_from_slice(&(first + 1).to_le_bytes());
        assert!(read_state_machine(&process, component.as_ptr() as usize).is_err());
        component[STATE_MACHINE_VALUES..].fill(0);
        assert!(read_state_machine(&process, component.as_ptr() as usize)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn state_names_reject_invalid_lengths_pointers_encoding_and_terminators() {
        let process = Process::current_for_tests().unwrap();
        for name in [
            b"\xff\0".as_slice(),
            b"phase\n\0".as_slice(),
            b"\0".as_slice(),
        ] {
            assert!(read_state_name(&process, &state_name_descriptor(name)).is_err());
        }
        let mut descriptor = state_name_descriptor(b"phase\0");
        descriptor[5] = b'x';
        assert!(read_state_name(&process, &descriptor).is_err());
        descriptor[16..24].copy_from_slice(&((MAX_STATE_NAME + 1) as u64).to_le_bytes());
        assert!(read_state_name(&process, &descriptor).is_err());
        descriptor[..8].copy_from_slice(&1u64.to_le_bytes());
        descriptor[16..24].copy_from_slice(&20u64.to_le_bytes());
        descriptor[24..32].copy_from_slice(&31u64.to_le_bytes());
        assert!(read_state_name(&process, &descriptor).is_err());
        descriptor[24..32].copy_from_slice(&19u64.to_le_bytes());
        assert!(read_state_name(&process, &descriptor).is_err());
    }

    #[test]
    fn hidden_fields_are_read_separately_from_the_targetability_word() {
        let process = Process::current_for_tests().unwrap();
        let mut component = [0u8; TARGETABLE_EXTRA + 8];
        component[TARGETABLE_FLAG..TARGETABLE_FLAG + 4].copy_from_slice(&0x10101u32.to_le_bytes());
        component[TARGETABLE_EXTRA + 2] = 4;
        component[TARGETABLE_EXTRA + 4..].copy_from_slice(&0x10203u32.to_le_bytes());
        assert_eq!(
            read_targetable_extra(&process, component.as_ptr() as usize).unwrap(),
            TargetableExtra {
                quest_state: 4,
                hidden_flags: 0x10203,
            }
        );
    }
}
