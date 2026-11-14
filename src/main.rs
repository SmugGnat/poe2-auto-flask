mod action;
mod area;
mod bindings;
mod config;
mod diagnostics;
mod engine;
mod entity;
mod file_watch;
mod game;
mod game_state;
mod input;
mod platform;
mod poe1;
mod poe2;
mod process;
mod session;

use std::io::{self, IsTerminal, Write};
use std::thread;
use std::time::Duration;

use config::{ConfigManager, ConfigStartup};
use diagnostics::FailureMonitor;
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
    game: Option<Game>,
    nearby_debug: bool,
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
    let mut game = None;
    let mut mode_seen = false;
    let mut nearby_debug = false;
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
            "--nearby" if !nearby_debug => nearby_debug = true,
            "--game" if !game_seen => {
                game_seen = true;
                let value = args.next().unwrap_or_else(|| fail_usage());
                game = Some(value.parse().unwrap_or_else(|_| fail_usage()));
            }
            "--help" | "-h" => print_help_and_exit(),
            _ => fail_usage(),
        }
    }
    if nearby_debug && (mode != Mode::Debug || game == Some(Game::Poe2)) {
        fail_usage();
    }
    Options {
        mode,
        game,
        nearby_debug,
    }
}

fn print_help_and_exit() -> ! {
    println!("Usage: poe2-auto-flask.exe [--auto | --debug] [--game poe1|poe2] [--nearby]");
    println!("  --game   Explicit game selection (Windows detects automatically if omitted)");
    println!("  --auto   Run selected game's automation (default; starts disarmed)");
    println!("  --debug  Read-only diagnostics for the selected game");
    println!("  --nearby Experimental PoE1 monster census in --debug only (may cost CPU)");
    println!("  PoE1 uses type-aware flask rules and starts disarmed.");
    std::process::exit(0);
}

fn fail_usage() -> ! {
    eprintln!("ERROR: unknown or incompatible arguments");
    eprintln!("Usage: poe2-auto-flask.exe [--auto | --debug] [--game poe1|poe2] [--nearby]");
    std::process::exit(2);
}

fn run(options: Options) -> io::Result<()> {
    let Options {
        mode,
        game,
        nearby_debug,
    } = options;
    let mut config_manager = ConfigManager::load_default()?;
    print_config_startup(config_manager.startup());
    let under_wine = running_under_wine();

    loop {
        let Some((process, game)) = wait_for_process(under_wine, game)? else {
            return Ok(());
        };
        if nearby_debug && game != Game::Poe1 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "--nearby is only available for PoE1; use --game poe1 to select explicitly",
            ));
        }
        println!(
            "Selected {} (Steam AppID {}).",
            game.name(),
            game.steam_app_id()
        );

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
                poe2::automation::run(&process, probe, &mut config_manager)?;
            }
            (Mode::Debug, Game::Poe2) => poe2::debug::run(
                &process,
                probe.into_poe2_probe()?,
                config_manager.current(),
                config_manager.current_from_file(),
            )?,
            (Mode::Debug, Game::Poe1) => poe1::debug::run(&process, probe, nearby_debug)?,
            (Mode::Auto, Game::Poe1) => {
                poe1::automation::run(&process, probe, &mut config_manager)?
            }
        }

        // Native Windows can attach to a restarted game process. Under Proton/Wine the helper must
        // exit with the game so Steam does not keep the AppID alive through the helper process.
        if under_wine {
            print_wine_process_exit_notice(game);
            return Ok(());
        }
    }
}

fn wait_for_process(
    under_wine: bool,
    requested_game: Option<Game>,
) -> io::Result<Option<(Process, Game)>> {
    // The Linux launcher already supplies --game. A bare Wine invocation keeps
    // the existing PoE2 default, never entering the Windows console fallback.
    let selected_game = requested_game.or(if under_wine { Some(Game::Poe2) } else { None });
    let mut notice_shown = false;

    loop {
        let attached = match selected_game {
            Some(game) => Process::attach_to_game(game),
            None => Process::attach_to_any_game(),
        };
        match attached {
            Ok(process) => {
                let game = match selected_game {
                    Some(game) => game,
                    None => match process.executable_path() {
                        Ok(path) => match game_from_install_path(&path) {
                            Some(game) => game,
                            None => prompt_for_game()?,
                        },
                        Err(_) if !process.is_alive().unwrap_or(false) => continue,
                        Err(error) => return Err(error),
                    },
                };
                // A process may exit while a user is answering the fallback.
                if !process.is_alive().unwrap_or(false) {
                    continue;
                }
                return Ok(Some((process, game)));
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::PermissionDenied | io::ErrorKind::AlreadyExists
                ) =>
            {
                return Err(error)
            }
            Err(error) if under_wine && error.kind() == io::ErrorKind::NotFound => {
                let game = selected_game.expect("Wine always has a selected game");
                println!("{} is not running.", game.name());
                println!(
                    "On Proton/Wine, start the selected game first, then relaunch poe2-auto-flask."
                );
                return Ok(None);
            }
            Err(_) => {
                if !notice_shown {
                    let name = selected_game.map_or("Path of Exile", Game::name);
                    println!("Status: WAITING FOR {}", name.to_ascii_uppercase());
                    notice_shown = true;
                }
                thread::sleep(PROCESS_RETRY_INTERVAL);
            }
        }
    }
}

/// Only exact installation-folder names qualify for automatic identification.
fn game_from_install_path(path: &str) -> Option<Game> {
    let directory = path.rsplit(['\\', '/']).nth(1)?;
    if directory.eq_ignore_ascii_case("Path of Exile") {
        Some(Game::Poe1)
    } else if directory.eq_ignore_ascii_case("Path of Exile 2") {
        Some(Game::Poe2)
    } else {
        None
    }
}

fn parse_game_choice(input: &str) -> Option<Game> {
    match input.trim() {
        "1" => Some(Game::Poe1),
        "2" => Some(Game::Poe2),
        _ => None,
    }
}

/// An unfamiliar native Windows directory requires an explicit user choice;
/// do not scan both game memory parsers or silently guess.
fn prompt_for_game() -> io::Result<Game> {
    println!("Automatic game detection failed.");
    if !io::stdin().is_terminal() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "no interactive console; launch with --game poe1 or --game poe2",
        ));
    }
    println!("Select game: 1) PoE1  2) PoE2");
    loop {
        print!("> ");
        io::stdout().flush()?;
        let mut input = String::new();
        if io::stdin().read_line(&mut input)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "game selection cancelled; use --game poe1 or --game poe2",
            ));
        }
        if let Some(game) = parse_game_choice(&input) {
            return Ok(game);
        }
        println!("Please enter 1 or 2.");
    }
}

fn wait_for_game_state(process: &Process, game: Game) -> io::Result<Option<GameSession>> {
    let mut notice_shown = false;
    let mut signature_failures = 0u32;
    let mut failures = FailureMonitor::default();

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
                        return Err(io::Error::new(
                            error.kind(),
                            format!("{error} (game update may require a new GameState signature)"),
                        ));
                    }
                } else {
                    signature_failures = 0;
                }

                if !notice_shown {
                    println!("Status: WAITING FOR GAME STATE");
                    notice_shown = true;
                }
                failures.warn("Startup", &error.to_string());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installation_directories_identify_only_the_matching_game() {
        assert_eq!(
            game_from_install_path(r"C:\Steam\steamapps\common\Path of Exile\PathOfExileSteam.exe"),
            Some(Game::Poe1)
        );
        assert_eq!(
            game_from_install_path(r"D:\Games\PATH OF EXILE 2\PathOfExile.exe"),
            Some(Game::Poe2)
        );
        assert_eq!(
            game_from_install_path("D:/SteamLibrary/Path of Exile 2/PathOfExileSteam.exe"),
            Some(Game::Poe2)
        );
    }

    #[test]
    fn unknown_or_misleading_directory_names_require_manual_selection() {
        assert_eq!(game_from_install_path(r"D:\Games\PoE\PathOfExileSteam.exe"), None);
        assert_eq!(
            game_from_install_path(r"C:\Games\Path of Exile 2 Backup\PathOfExile.exe"),
            None
        );
        assert_eq!(
            game_from_install_path(r"C:\Games\Path of Exile Mods\PathOfExile.exe"),
            None
        );
        assert_eq!(game_from_install_path("PathOfExileSteam.exe"), None);
    }

    #[test]
    fn manual_selection_accepts_only_one_or_two() {
        assert_eq!(parse_game_choice("1\r\n"), Some(Game::Poe1));
        assert_eq!(parse_game_choice(" 2 "), Some(Game::Poe2));
        assert_eq!(parse_game_choice("poe1"), None);
        assert_eq!(parse_game_choice("3"), None);
        assert_eq!(parse_game_choice(""), None);
    }
}
