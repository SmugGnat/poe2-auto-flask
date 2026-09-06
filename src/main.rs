mod area;
mod automation;
mod bindings;
mod buffs;
mod config;
mod debug;
mod entity;
mod file_watch;
mod flask_inventory;
mod input;
mod platform;
mod poe2;
mod process;

use std::io;
use std::thread;
use std::time::Duration;

use automation::run_auto;
use config::{Config, ConfigManager, ConfigStartup};
use debug::run_debug;
use platform::running_under_wine;
use poe2::{probe_vitals, VitalsProbe};
use process::Process;

const PROCESS_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const GAME_STATE_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const SIGNATURE_RETRY_LIMIT: u32 = 3;

#[derive(Clone, Copy)]
enum Mode {
    Auto,
    Debug,
}

fn main() {
    let mode = parse_mode();
    println!("poe2-auto-flask v{}", env!("CARGO_PKG_VERSION"));

    if let Err(error) = run(mode) {
        eprintln!("\nFAIL: {error}");
        std::process::exit(1);
    }
}

fn parse_mode() -> Mode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [] => Mode::Auto,
        [arg] if arg == "--auto" => Mode::Auto,
        [arg] if arg == "--debug" => Mode::Debug,
        [arg] if arg == "--help" || arg == "-h" => print_help_and_exit(),
        _ => fail_usage(),
    }
}

fn print_help_and_exit() -> ! {
    println!("Usage: poe2-auto-flask.exe [--auto | --debug]");
    println!("  --auto   Run the auto-flask engine (default; starts disarmed)");
    println!("  --debug  Read-only combined diagnostics for vitals, area, flask state and charges");
    std::process::exit(0);
}

fn fail_usage() -> ! {
    eprintln!("ERROR: unknown or incompatible arguments");
    eprintln!("Usage: poe2-auto-flask.exe [--auto | --debug]");
    std::process::exit(2);
}

fn run(mode: Mode) -> io::Result<()> {
    let mut config_manager = ConfigManager::load_next_to_exe()?;
    print_config_startup(config_manager.startup());
    let under_wine = running_under_wine();

    loop {
        let Some(process) = wait_for_process(under_wine)? else {
            return Ok(());
        };

        if let Err(error) = process.validate_pe_headers() {
            if !process.is_alive().unwrap_or(false) {
                if under_wine {
                    print_wine_process_exit_notice();
                    return Ok(());
                }
                continue;
            }
            return Err(error);
        }

        let Some(probe) = wait_for_game_state(&process)? else {
            if under_wine {
                print_wine_process_exit_notice();
                return Ok(());
            }
            continue;
        };

        match mode {
            Mode::Auto => {
                print_auto_session_header(config_manager.current());
                run_auto(&process, probe, &mut config_manager)?;
            }
            Mode::Debug => run_debug(
                &process,
                probe,
                config_manager.current(),
                config_manager.current_from_file(),
            )?,
        }

        // Native Windows can attach to a restarted game process. Under Proton/Wine the helper must
        // exit with the game so Steam does not keep the AppID alive through the helper process.
        if under_wine {
            print_wine_process_exit_notice();
            return Ok(());
        }
    }
}

fn wait_for_process(under_wine: bool) -> io::Result<Option<Process>> {
    let mut notice_shown = false;

    loop {
        match Process::attach_to_poe2() {
            Ok(process) => return Ok(Some(process)),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => return Err(error),
            Err(error) if under_wine && error.kind() == io::ErrorKind::NotFound => {
                println!("Path of Exile 2 is not running.");
                println!("On Proton/Wine, start PoE2 first, then relaunch poe2-auto-flask.");
                return Ok(None);
            }
            Err(_) => {
                if !notice_shown {
                    println!("Status: WAITING FOR PATH OF EXILE 2");
                    notice_shown = true;
                }
                thread::sleep(PROCESS_RETRY_INTERVAL);
            }
        }
    }
}

fn wait_for_game_state(process: &Process) -> io::Result<Option<VitalsProbe>> {
    let mut notice_shown = false;
    let mut signature_failures = 0u32;

    loop {
        if !process.is_alive().unwrap_or(false) {
            return Ok(None);
        }

        match probe_vitals(process) {
            Ok(probe) => return Ok(Some(probe)),
            Err(error) => {
                if error.kind() == io::ErrorKind::NotFound {
                    signature_failures = signature_failures.saturating_add(1);
                    if signature_failures >= SIGNATURE_RETRY_LIMIT {
                        return Err(error);
                    }
                } else {
                    signature_failures = 0;
                }

                if !notice_shown {
                    println!("Status: WAITING FOR GAME STATE");
                    notice_shown = true;
                }
                thread::sleep(GAME_STATE_RETRY_INTERVAL);
            }
        }
    }
}

fn print_config_startup(startup: &ConfigStartup) {
    match startup {
        ConfigStartup::Loaded => {}
        ConfigStartup::Created => {
            println!("Config: created config.toml with default settings.");
        }
        ConfigStartup::BuiltInFallback(error) => {
            println!("Config: could not create config.toml ({error}); using built-in defaults.");
        }
    }
}

fn print_wine_process_exit_notice() {
    println!("Path of Exile 2 exited.");
    println!("On Proton/Wine, restart PoE2 first, then relaunch poe2-auto-flask.");
}

fn print_auto_session_header(config: &Config) {
    println!("Connected to Path of Exile 2.");
    println!("{}", config.summary());
}
