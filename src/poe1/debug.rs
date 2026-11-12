//! Read-only PoE1 diagnostics for flasks, status effects and skill presets.
//! This module NEVER sends input to the game.
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use super::config::Poe1SkillsConfig;
use super::movement::RUN_ANIMATION_ID;
use super::skill_rules::{configured_rules, evaluate, BLOOD_RAGE_BUFF, MOLTEN_SHELL_BUFF};
use super::skills::SkillId;
use crate::process::Process;
use crate::session::GameSession;
use windows_sys::Win32::Foundation::FILETIME;
use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const REPORT_INTERVAL: Duration = Duration::from_secs(1);
const CPU_REPORT_INTERVAL: Duration = Duration::from_secs(5);

pub fn run(process: &Process, mut session: GameSession, nearby_debug: bool) -> io::Result<()> {
    println!("Connected to Path of Exile 1 (read-only diagnostic mode).");
    if let Some(metadata) = session.poe1_player_metadata() {
        println!("Player metadata: {metadata}");
    }
    println!(
        "Effective flask costs and chat UI state require live verification before auto input."
    );
    println!("Press Ctrl+C to exit. No automation or keyboard input is enabled.\n");
    if nearby_debug {
        println!("Experimental nearby census ENABLED (read-only, 750ms cache, 1500-node limit).");
    }

    let mut diagnostic_config = Poe1SkillsConfig::default();
    diagnostic_config.molten_shell.enabled = true;
    let diagnostic_rule = configured_rules(&diagnostic_config)?
        .into_iter()
        .next()
        .expect("diagnostic Molten Shell preset exists");
    let mut report_at = Instant::now();
    let mut refresh_at = Instant::now();
    let mut waiting = false;
    // Diagnostic-only Win32 process CPU timer. Reads the current helper's CPU
    // time, not game CPU or Wine server CPU, and never runs in --auto.
    let mut cpu_meter = CpuMeter::new();
    loop {
        let started = Instant::now();
        if !process.is_alive().unwrap_or(false) {
            return Ok(());
        }

        session.begin_tick();
        let sampled = (|| {
            let vitals = session.vitals(process)?;
            let area = session.poe1_area(process)?.clone();
            let flasks = session.poe1_flasks(process)?.clone();
            let buffs = session.poe1_effects(process)?.clone();
            let animation = session.poe1_animation(process).ok();
            let chat = match session.poe1_chat_active(process) {
                Ok(true) => "OPEN",
                Ok(false) => "closed",
                Err(_) => "unavailable (automation blocked)",
            };
            Ok::<_, io::Error>((vitals, area, flasks, buffs, chat, animation))
        })();

        match sampled {
            Ok((vitals, area, flasks, buffs, chat, animation)) => {
                waiting = false;
                if started >= report_at {
                    println!(
                        "Area: {} [{}] | Life: {}/{} | Mana: {} | ES: {}",
                        area.name,
                        area.kind(),
                        vitals.health.current,
                        vitals.health.usable_max(),
                        format_pool(vitals.mana),
                        format_pool(vitals.energy_shield),
                    );
                    println!("  Chat: {chat}");
                    if nearby_debug && !area.is_town && !area.is_hideout {
                        match session.poe1_nearby(process) {
                            Ok(census) => {
                                println!(
                                    "  Nearby nodes {} | candidate monsters {} | within 60 grid units (N/M/R/U): {:?} | within 100: {:?}",
                                    census.awake_nodes,
                                    census.candidate_monsters,
                                    census.counts_at(60.0),
                                    census.counts_at(100.0)
                                );
                                println!(
                                    "  Nearby last full scan: {:.2} ms wall (includes cross-process read latency)",
                                    census.scan_wall.as_secs_f64() * 1000.0
                                );
                            }
                            Err(error) => {
                                println!("  Nearby: unavailable ({error}); never authorizes input")
                            }
                        }
                    }
                    match animation {
                        Some(id) => println!(
                            "  Movement: {} (Actor animation ID {})",
                            if id == RUN_ANIMATION_ID {
                                "Run"
                            } else {
                                "not running"
                            },
                            id
                        ),
                        None => {
                            println!("  Movement: unavailable (optional Quicksilver gate blocked)")
                        }
                    }
                    for (index, slot) in flasks.slots.iter().enumerate() {
                        match slot {
                            Some(flask) => println!(
                                "  Flask {}: {} ({}) | charges {} | raw cost {} | effective cost {} | effect {} | recovery {}",
                                index + 1,
                                flask.item_path,
                                flask.flask_type.name(),
                                flask.current_charges,
                                flask.raw_per_use,
                                flask.effective_per_use.map_or_else(|| "unavailable".into(), |v| v.to_string()),
                                if buffs.slots_active[index] {
                                    "active"
                                } else {
                                    "inactive"
                                },
                                match flask.flask_type {
                                    crate::poe1::types::FlaskType::Life
                                    | crate::poe1::types::FlaskType::Mana => flask.recovery.label(),
                                    _ => "n/a",
                                },
                            ),
                            None => println!("  Flask {}: empty", index + 1),
                        }
                    }
                    println!(
                        "  Effects ({}): {}",
                        buffs.effects.len(),
                        buffs
                            .effects
                            .iter()
                            .map(|e| e.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                    );
                    // The debug reader shares one Actor pass for both actual
                    // presets, even when only one is equipped.
                    let skills =
                        session.poe1_skills(process, &[SkillId::MoltenShell, SkillId::BloodRage]);
                    let skill = skills
                        .as_ref()
                        .map(|snapshot| snapshot[&SkillId::MoltenShell]);
                    let blood_rage = skills
                        .as_ref()
                        .map(|snapshot| snapshot[&SkillId::BloodRage]);
                    let rule = evaluate(
                        &diagnostic_rule,
                        vitals,
                        &buffs,
                        None,
                        skill.as_ref().ok().copied(),
                    );
                    println!(
                        "  Molten Shell (read-only): buff {} | skill {} | rule at {}% Life {:?}",
                        if buffs.effects.iter().any(|effect| effect.name == MOLTEN_SHELL_BUFF && effect.time_left > 0.0) {
                            "active"
                        } else {
                            "absent"
                        },
                        match &skill {
                            Ok(state) => format!("{state:?}"),
                            Err(error) => format!("unavailable ({error})"),
                        },
                        diagnostic_config.molten_shell.threshold_percent,
                        rule
                    );
                    println!(
                        "  Blood Rage (read-only): buff {} | skill {}",
                        if buffs
                            .effects
                            .iter()
                            .any(|effect| effect.name == BLOOD_RAGE_BUFF && effect.time_left > 0.0)
                        {
                            "active"
                        } else {
                            "absent"
                        },
                        match &blood_rage {
                            Ok(state) => format!("{state:?}"),
                            Err(error) => format!("unavailable ({error})"),
                        }
                    );
                    if let Some(line) = cpu_meter.report() {
                        println!("  {line}");
                    }
                    report_at = started + REPORT_INTERVAL;
                }
            }
            Err(error) => {
                if !waiting {
                    eprintln!("WAITING FOR POE1 GAME STATE: {error}");
                    waiting = true;
                }
                if started >= refresh_at {
                    match session.refresh(process) {
                        Ok(()) => {
                            waiting = false;
                        }
                        Err(_) => {
                            refresh_at = started + POLL_INTERVAL;
                        }
                    }
                }
            }
        }

        if started.elapsed() < POLL_INTERVAL {
            thread::sleep(POLL_INTERVAL - started.elapsed());
        }
    }
}

fn format_pool(pool: Option<crate::game_state::VitalPool>) -> String {
    match pool {
        Some(value) => format!("{}/{}", value.current, value.usable_max()),
        None => "N/A".into(),
    }
}

#[derive(Debug, Clone, Copy)]
struct CpuSample {
    instant: Instant,
    process_ticks_100ns: u64,
}

/// Samples the helper's whole-process CPU usage, not a tight-loop cycle counter.
/// Five-second windows make low fractions of a CPU core easier to compare.
struct CpuMeter {
    previous: Option<CpuSample>,
    failed: bool,
}

impl CpuMeter {
    fn new() -> Self {
        Self {
            previous: read_process_cpu_ticks()
                .ok()
                .map(|process_ticks_100ns| CpuSample {
                    instant: Instant::now(),
                    process_ticks_100ns,
                }),
            failed: false,
        }
    }

    fn report(&mut self) -> Option<String> {
        if self.failed {
            return None;
        }
        let Some(previous) = self.previous else {
            self.failed = true;
            return Some("Helper CPU: unavailable (GetProcessTimes failed under Wine)".into());
        };
        let now = Instant::now();
        let wall = now.saturating_duration_since(previous.instant);
        if wall < CPU_REPORT_INTERVAL {
            return None;
        }
        match read_process_cpu_ticks() {
            Ok(current) => {
                self.previous = Some(CpuSample {
                    instant: now,
                    process_ticks_100ns: current,
                });
                let ticks = current.saturating_sub(previous.process_ticks_100ns);
                let cpu_seconds = ticks as f64 / 10_000_000.0;
                let core_percent = cpu_seconds / wall.as_secs_f64() * 100.0;
                Some(format!(
                    "Helper CPU: {:.2}% of one core ({:.1} ms CPU / {:.1} s wall; 5s average)",
                    core_percent,
                    cpu_seconds * 1000.0,
                    wall.as_secs_f64()
                ))
            }
            Err(_) => {
                self.failed = true;
                Some("Helper CPU: unavailable (GetProcessTimes failed under Wine)".into())
            }
        }
    }
}

fn read_process_cpu_ticks() -> io::Result<u64> {
    let mut created: FILETIME = unsafe { std::mem::zeroed() };
    let mut exited: FILETIME = unsafe { std::mem::zeroed() };
    let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
    let mut user: FILETIME = unsafe { std::mem::zeroed() };
    let ok = unsafe {
        GetProcessTimes(
            GetCurrentProcess(),
            &mut created,
            &mut exited,
            &mut kernel,
            &mut user,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(filetime_ticks(kernel).saturating_add(filetime_ticks(user)))
}

fn filetime_ticks(time: FILETIME) -> u64 {
    (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
}

#[cfg(test)]
mod cpu_tests {
    use super::*;

    #[test]
    fn filetime_high_and_low_halves_are_combined_without_sign_loss() {
        let time = FILETIME {
            dwLowDateTime: 0x1122_3344,
            dwHighDateTime: 0x5566_7788,
        };
        assert_eq!(filetime_ticks(time), 0x5566_7788_1122_3344);
    }
}
