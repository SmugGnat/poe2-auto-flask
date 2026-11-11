//! PoE1 milestone-1 diagnostics. This module NEVER sends input to the game.
use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::process::Process;
use crate::session::GameSession;
use super::movement::RUN_ANIMATION_ID;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const REPORT_INTERVAL: Duration = Duration::from_secs(1);

pub fn run(process: &Process, mut session: GameSession) -> io::Result<()> {
    println!("Connected to Path of Exile 1 (read-only diagnostic mode).");
    if let Some(metadata) = session.poe1_player_metadata() {
        println!("Player metadata: {metadata}");
    }
    println!(
        "Effective flask costs and chat UI state require live verification before auto input."
    );
    println!("Press Ctrl+C to exit. No automation or keyboard input is enabled.\n");

    let mut report_at = Instant::now();
    let mut refresh_at = Instant::now();
    let mut waiting = false;
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
                    match animation {
                        Some(id) => println!(
                            "  Movement: {} (Actor animation ID {})",
                            if id == RUN_ANIMATION_ID { "Run" } else { "not running" },
                            id
                        ),
                        None => println!("  Movement: unavailable (optional Quicksilver gate blocked)"),
                    }
                    for (index, slot) in flasks.slots.iter().enumerate() {
                        match slot {
                            Some(flask) => println!(
                                "  Flask {}: {} ({}) | charges {} | raw cost {} | effective cost {} | effect {}",
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

fn format_pool(pool: Option<crate::poe2::VitalPool>) -> String {
    match pool {
        Some(value) => format!("{}/{}", value.current, value.usable_max()),
        None => "N/A".into(),
    }
}
