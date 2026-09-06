use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::area::{resolve_area_info, AreaInfo};
use crate::buffs::{
    resolve_flask_lock_probe, resolve_flask_probe, sample_flask_effects, sample_flask_locks,
    FlaskLockProbe, FlaskLocks, FlaskProbe,
};
use crate::config::Config;
use crate::flask_inventory::{
    resolve_flask_inventory_probe, sample_flask_charges, FlaskChargeSnapshot, FlaskInventoryProbe,
};
use crate::poe2::{refresh_vitals, sample_vitals, LiveVitals, VitalPool, VitalsProbe};
use crate::process::Process;

const DEBUG_POLL_INTERVAL: Duration = Duration::from_millis(250);

struct DebugRuntime {
    vitals: VitalsProbe,
    area: AreaInfo,
    raw_flasks: FlaskProbe,
    locks: FlaskLockProbe,
    charges: FlaskInventoryProbe,
}

impl DebugRuntime {
    fn resolve(process: &Process, vitals: VitalsProbe) -> io::Result<Self> {
        let area = resolve_area_info(process, &vitals)?;
        let raw_flasks = resolve_flask_probe(process, &vitals)?;
        let locks = resolve_flask_lock_probe(process, &vitals)?;
        let charges = resolve_flask_inventory_probe(process, &vitals)?;
        Ok(Self {
            vitals,
            area,
            raw_flasks,
            locks,
            charges,
        })
    }

    fn refresh(&mut self, process: &Process) -> io::Result<()> {
        let vitals = refresh_vitals(process, &self.vitals)?;
        *self = Self::resolve(process, vitals)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
struct CandidateEffect {
    slot: usize,
    name: String,
    buff_type: Option<u8>,
    total_time: f32,
    time_left: f32,
}

#[derive(Debug, Clone, PartialEq)]
struct DebugSnapshot {
    vitals: LiveVitals,
    locks: FlaskLocks,
    life_charges: Option<(i32, i32)>,
    mana_charges: Option<(i32, i32)>,
    status_effect_count: usize,
    candidates: Vec<CandidateEffect>,
}

pub fn run_debug(
    process: &Process,
    vitals: VitalsProbe,
    config: &Config,
    config_loaded: bool,
) -> io::Result<()> {
    let mut runtime = DebugRuntime::resolve(process, vitals)?;
    print_debug_header(process, &runtime, config, config_loaded);

    let mut previous: Option<DebugSnapshot> = None;
    let mut refresh_notice_shown = false;

    loop {
        let tick_started = Instant::now();

        let sample = sample_vitals(process, &runtime.vitals).and_then(|vitals| {
            let locks = sample_flask_locks(process, &mut runtime.locks)?;
            let charges = sample_flask_charges(process, &runtime.charges)?;
            let effects = sample_flask_effects(process, &runtime.raw_flasks)?;
            Ok((vitals, locks, charges, effects))
        });

        match sample {
            Ok((vitals, locks, charges, effects)) => {
                refresh_notice_shown = false;
                let snapshot = DebugSnapshot {
                    vitals,
                    locks,
                    life_charges: charge_pair(charges, 0),
                    mana_charges: charge_pair(charges, 1),
                    status_effect_count: effects.status_effect_count,
                    candidates: effects
                        .raw
                        .iter()
                        .filter_map(|effect| {
                            if effect.source_entity_id != Some(runtime.raw_flasks.player_id) {
                                return None;
                            }
                            let slot = effect.flask_slot?;
                            if !(0..2).contains(&slot) {
                                return None;
                            }
                            let total_time = effect.total_time?;
                            let time_left = effect.time_left?;
                            if !total_time.is_finite()
                                || !time_left.is_finite()
                                || total_time <= 0.0
                                || time_left <= 0.0
                            {
                                return None;
                            }
                            Some(CandidateEffect {
                                slot: slot as usize,
                                name: effect
                                    .definition_name
                                    .clone()
                                    .unwrap_or_else(|| "?".to_string()),
                                buff_type: effect.buff_type,
                                total_time,
                                time_left,
                            })
                        })
                        .collect(),
                };

                if previous.as_ref() != Some(&snapshot) {
                    print_debug_snapshot(&runtime.area, &snapshot);
                    previous = Some(snapshot);
                }
            }
            Err(error) => {
                if !process.is_alive().unwrap_or(false) {
                    return Ok(());
                }
                if !refresh_notice_shown {
                    println!("DEBUG: state changed/unavailable ({error}); re-resolving...");
                    refresh_notice_shown = true;
                }
                match runtime.refresh(process) {
                    Ok(()) => {
                        println!(
                            "DEBUG: area -> {} ({}, {})",
                            runtime.area.name,
                            runtime.area.id,
                            runtime.area.kind()
                        );
                        previous = None;
                        refresh_notice_shown = false;
                    }
                    Err(_) => thread::sleep(Duration::from_millis(250)),
                }
            }
        }

        let elapsed = tick_started.elapsed();
        if elapsed < DEBUG_POLL_INTERVAL {
            thread::sleep(DEBUG_POLL_INTERVAL - elapsed);
        }
    }
}

fn print_debug_header(
    process: &Process,
    runtime: &DebugRuntime,
    config: &Config,
    config_loaded: bool,
) {
    let module = process.main_module();
    println!("Mode: DEBUG (read-only)\n");
    println!("Process");
    println!("  PID       : {}", process.pid());
    println!("  Module    : {}", module.name);
    println!("  Base      : 0x{:016X}", module.base_address);
    println!("  Image size: {} bytes", module.size);
    println!(
        "  Section   : {} @ 0x{:016X} + 0x{:X}",
        runtime.vitals.text_section.name,
        runtime.vitals.text_section.address,
        runtime.vitals.text_section.size
    );

    println!("\nResolved state");
    println!("  GameState : 0x{:016X}", runtime.vitals.game_state);
    println!("  InGame    : 0x{:016X}", runtime.vitals.in_game_state);
    println!("  Area      : 0x{:016X}", runtime.vitals.area_instance);
    println!("  Player    : 0x{:016X}", runtime.vitals.local_player);
    println!("  Life comp : 0x{:016X}", runtime.vitals.life_component);
    println!("  Metadata  : {}", runtime.vitals.metadata);
    println!(
        "  Initial   : HP {} | Mana {} | ES {}",
        vital_text(runtime.vitals.health),
        runtime
            .vitals
            .mana
            .map(vital_text)
            .unwrap_or_else(|| "n/a".to_string()),
        runtime
            .vitals
            .energy_shield
            .map(vital_text)
            .unwrap_or_else(|| "n/a".to_string())
    );

    println!("\nArea metadata");
    println!("  Name      : {}", runtime.area.name);
    println!("  Id        : {}", runtime.area.id);
    println!("  Act       : {}", runtime.area.act);
    println!("  Type      : {}", runtime.area.kind());
    println!(
        "  WorldData : InGame+0x{:X} -> 0x{:016X}",
        runtime.area.world_data_offset, runtime.area.world_data
    );
    println!("  Area row  : 0x{:016X}", runtime.area.area_row);

    println!("\nFlask inventory");
    println!("  PlayerData: 0x{:016X}", runtime.charges.player_data);
    println!("  Inventory : 0x{:016X}", runtime.charges.flask_inventory);
    println!(
        "  Grid      : {}x{} ({} cells)",
        runtime.charges.width, runtime.charges.height, runtime.charges.item_cells
    );

    println!("\nConfig");
    println!(
        "  Source    : {}",
        if config_loaded {
            "config.toml"
        } else {
            "built-in defaults"
        }
    );
    println!(
        "  Life      : {} <= {:.1}%",
        if config.health.enabled { "ON" } else { "OFF" },
        config.health.threshold_percent
    );
    println!(
        "  Mana      : {} <= {:.1}%",
        if config.mana.enabled { "ON" } else { "OFF" },
        config.mana.threshold_percent
    );
    println!(
        "  Hideout   : {}",
        if config.general.enable_in_hideout {
            "enabled"
        } else {
            "disabled"
        }
    );
    println!("\nLive read-only state follows. Ctrl+C exits.\n");
}

fn print_debug_snapshot(area: &AreaInfo, snapshot: &DebugSnapshot) {
    println!(
        "HP {} | Mana {} | ES {} | Area: {} ({})",
        vital_text(snapshot.vitals.health),
        snapshot
            .vitals
            .mana
            .map(vital_text)
            .unwrap_or_else(|| "n/a".to_string()),
        snapshot
            .vitals
            .energy_shield
            .map(vital_text)
            .unwrap_or_else(|| "n/a".to_string()),
        area.name,
        area.kind()
    );
    println!(
        "  Life flask: lock={} charges={} | Mana flask: lock={} charges={} | status effects={}",
        yes_no(snapshot.locks.slots[0]),
        charge_text(snapshot.life_charges),
        yes_no(snapshot.locks.slots[1]),
        charge_text(snapshot.mana_charges),
        snapshot.status_effect_count
    );

    for effect in &snapshot.candidates {
        println!(
            "  Timed slot {}: {} type={} {:.3}/{:.3}s",
            effect.slot + 1,
            effect.name,
            effect
                .buff_type
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            effect.time_left,
            effect.total_time
        );
    }
}

fn charge_pair(snapshot: FlaskChargeSnapshot, slot: usize) -> Option<(i32, i32)> {
    snapshot.slots[slot].map(|state| (state.current, state.per_use))
}

fn vital_text(pool: VitalPool) -> String {
    let percent = pool
        .percentage()
        .map(|value| format!("{value:.1}%"))
        .unwrap_or_else(|| "?".to_string());
    format!("{}/{} ({percent})", pool.current, pool.usable_max())
}

fn charge_text(charges: Option<(i32, i32)>) -> String {
    charges
        .map(|(current, per_use)| format!("{current}/{per_use}"))
        .unwrap_or_else(|| "empty/unreadable".to_string())
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "YES"
    } else {
        "NO"
    }
}
