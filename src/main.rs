mod area;
mod automation;
mod bindings;
mod buffs;
mod config;
mod debug;
mod entity;
mod file_watch;
mod flask_inventory;
mod game;
mod input;
mod platform;
mod poe1;
mod poe2;
mod process;
mod session;

use std::io;
use std::thread;
use std::time::Duration;

use automation::run_auto;
use config::{Config, ConfigManager, ConfigStartup};
use debug::run_debug;
use game::Game;
use platform::running_under_wine;
use process::Process;
use session::GameSession;

const PROCESS_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const GAME_STATE_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const SIGNATURE_RETRY_LIMIT: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Auto,
    Debug,
}

struct Options {
    mode: Mode,
    game: Game,
}

fn main() {
    let options = parse_options();
    println!("poe2-auto-flask v{}", env!("CARGO_PKG_VERSION"));

    if let Err(error) = run(options) {
        eprintln!("\nFAIL: {error}");
        std::process::exit(1);
    }
}

fn parse_options() -> Options {
    let mut mode = Mode::Auto;
    let mut game = Game::Poe2;
    let mut mode_seen = false;
    let mut game_seen = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--auto" | "--debug" if !mode_seen => {
                mode_seen = true;
                mode = if arg == "--debug" {
                    Mode::Debug
                } else {
                    Mode::Auto
                };
            }
            "--game" if !game_seen => {
                game_seen = true;
                let value = args.next().unwrap_or_else(|| fail_usage());
                game = value.parse().unwrap_or_else(|_| fail_usage());
            }
            "--help" | "-h" => print_help_and_exit(),
            _ => fail_usage(),
        }
    }
    Options { mode, game }
}

fn print_help_and_exit() -> ! {
    println!("Usage: poe2-auto-flask.exe [--auto | --debug] [--game poe1|poe2]");
    println!("  --game   Select game (default: poe2)");
    println!("  --auto   Run PoE2 automation (default; starts disarmed)");
    println!("  --debug  Read-only diagnostics for the selected game");
    println!("  PoE1 automation is not available in milestone 1.");
    std::process::exit(0);
}

fn fail_usage() -> ! {
    eprintln!("ERROR: unknown or incompatible arguments");
    eprintln!("Usage: poe2-auto-flask.exe [--auto | --debug] [--game poe1|poe2]");
    std::process::exit(2);
}

fn run(options: Options) -> io::Result<()> {
    let Options { mode, game } = options;
    println!(
        "Selected {} (Steam AppID {}).",
        game.name(),
        game.steam_app_id()
    );
    if game == Game::Poe1 && mode == Mode::Auto {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "PoE1 is read-only in milestone 1; run --game poe1 --debug",
        ));
    }
    let mut config_manager = ConfigManager::load_default()?;
    print_config_startup(config_manager.startup());
    let under_wine = running_under_wine();

    loop {
        let Some(process) = wait_for_process(under_wine, game)? else {
            return Ok(());
        };

        if let Err(error) = process.validate_pe_headers() {
            if !process.is_alive().unwrap_or(false) {
                if under_wine {
                    print_wine_process_exit_notice(game);
                    return Ok(());
                }
                continue;
            }
            return Err(error);
        }

        let Some(probe) = wait_for_game_state(&process, game)? else {
            if under_wine {
                print_wine_process_exit_notice(game);
                return Ok(());
            }
            continue;
        };

        match (mode, game) {
            (Mode::Auto, Game::Poe2) => {
                print_auto_session_header(config_manager.current());
                run_auto(&process, probe, &mut config_manager)?;
            }
            (Mode::Debug, Game::Poe2) => run_debug(
                &process,
                probe.into_poe2_probe()?,
                config_manager.current(),
                config_manager.current_from_file(),
            )?,
            (Mode::Debug, Game::Poe1) => poe1::debug::run(&process, probe)?,
            (Mode::Auto, Game::Poe1) => unreachable!("PoE1 auto blocked at startup"),
        }

        // Native Windows can attach to a restarted game process. Under Proton/Wine the helper must
        // exit with the game so Steam does not keep the AppID alive through the helper process.
        if under_wine {
            print_wine_process_exit_notice(game);
            return Ok(());
        }
    }
}

fn wait_for_process(under_wine: bool, game: Game) -> io::Result<Option<Process>> {
    let mut notice_shown = false;

    loop {
        match Process::attach_to_game(game) {
            Ok(process) => return Ok(Some(process)),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists
                ) =>
            {
                return Err(error)
            }
            Err(error) if under_wine && error.kind() == io::ErrorKind::NotFound => {
                println!("{} is not running.", game.name());
                println!(
                    "On Proton/Wine, start the selected game first, then relaunch poe2-auto-flask."
                );
                return Ok(None);
            }
            Err(_) => {
                if !notice_shown {
                    println!("Status: WAITING FOR {}", game.name().to_ascii_uppercase());
                    notice_shown = true;
                }
                thread::sleep(PROCESS_RETRY_INTERVAL);
            }
        }
    }
}

fn wait_for_game_state(process: &Process, game: Game) -> io::Result<Option<GameSession>> {
    let mut notice_shown = false;
    let mut signature_failures = 0u32;

    loop {
        if !process.is_alive().unwrap_or(false) {
            return Ok(None);
        }

        match GameSession::probe(process, game) {
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

fn print_wine_process_exit_notice(game: Game) {
    println!("{} exited.", game.name());
    println!("On Proton/Wine, restart the game first, then relaunch poe2-auto-flask.");
}

fn print_auto_session_header(config: &Config) {
    println!("Connected to Path of Exile 2.");
    println!("{}", config.summary());
}
