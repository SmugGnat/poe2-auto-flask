use crate::runtime::{BackendControl, Connection};
use crate::{poe1, poe2};
use std::io::{self, IsTerminal, Write};
use std::time::Duration;

use crate::config::{ConfigManager, ConfigStartup};
use crate::diagnostics::FailureMonitor;
use crate::game::Game;
use crate::platform::running_under_wine;
use crate::process::Process;
use crate::session::GameSession;

const PROCESS_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const GAME_STATE_RETRY_INTERVAL: Duration = Duration::from_secs(1);
const SIGNATURE_RETRY_LIMIT: u32 = 3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Auto,
    Debug,
}

pub struct Options {
    pub mode: Mode,
    pub game: Option<Game>,
    pub nearby_debug: bool,
    pub config_dir: Option<std::path::PathBuf>,
}

pub fn run(options: Options, mut control: BackendControl) -> io::Result<()> {
    let result = run_inner(options, &mut control);
    control.finish(&result);
    result
}

fn run_inner(options: Options, control: &mut BackendControl) -> io::Result<()> {
    let Options {
        mode,
        game,
        nearby_debug,
        config_dir,
    } = options;
    let game = match game {
        Some(game) => Some(game),
        None => Game::from_steam_context()?,
    };
    println!("poe2-auto-flask v{}", env!("CARGO_PKG_VERSION"));
    let mut config_manager = match config_dir {
        Some(root) => ConfigManager::load_profiles(root.clone(), &root.join("config.toml"))?,
        None => ConfigManager::load_default()?,
    };
    print_config_startup(config_manager.startup());
    let under_wine = running_under_wine();

    loop {
        let Some((process, game)) = wait_for_process(under_wine, game, control)? else {
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
        config_manager.select_game(game)?;
        control.applied(&config_manager);
        control.attached(process.pid());
        control.connection(Connection::Validating, Some(game), "Waiting for game state");

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

        let Some(probe) = wait_for_game_state(&process, game, control)? else {
            if under_wine {
                print_wine_process_exit_notice(game);
                return Ok(());
            }
            continue;
        };

        if control.is_stopping() {
            return Ok(());
        }
        control.connection(Connection::Connected, Some(game), "DISABLED");
        match (mode, game) {
            (Mode::Auto, Game::Poe2) => {
                poe2::automation::run(&process, probe, &mut config_manager, control)?;
            }
            (Mode::Debug, Game::Poe2) => poe2::debug::run(
                &process,
                probe.into_poe2_probe()?,
                config_manager.current(),
                config_manager.current_from_file(),
            )?,
            (Mode::Debug, Game::Poe1) => poe1::debug::run(&process, probe, nearby_debug)?,
            (Mode::Auto, Game::Poe1) => {
                poe1::automation::run(&process, probe, &mut config_manager, control)?
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
    control: &mut BackendControl,
) -> io::Result<Option<(Process, Game)>> {
    // run_inner resolves Steam IDs/prefix context before reaching this point.
    // Wine retains the legacy PoE2 fallback when no game context is available.
    let selected_game = requested_game.or(if under_wine { Some(Game::Poe2) } else { None });
    let mut notice_shown = false;

    loop {
        if control.is_stopping() {
            return Ok(None);
        }
        control.connection(Connection::Waiting, selected_game, "Waiting for game");
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
                            None => choose_game(control)?,
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
                control.wait(PROCESS_RETRY_INTERVAL);
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

fn choose_game(control: &mut BackendControl) -> io::Result<Game> {
    if !control.is_supervised() {
        return prompt_for_game();
    }
    control.connection(Connection::ChoosingGame, None, "Choose the running game");
    loop {
        if control.is_stopping() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "Game selection cancelled",
            ));
        }
        if let Some(game) = control.take_selected_game() {
            return Ok(game);
        }
        control.wait(Duration::from_millis(100));
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

fn wait_for_game_state(
    process: &Process,
    game: Game,
    control: &mut BackendControl,
) -> io::Result<Option<GameSession>> {
    let mut notice_shown = false;
    let mut signature_failures = 0u32;
    let mut failures = FailureMonitor::default();

    loop {
        if control.is_stopping() || !process.is_alive().unwrap_or(false) {
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
                control.wait(GAME_STATE_RETRY_INTERVAL);
            }
        }
    }
}

fn print_config_startup(startup: &ConfigStartup) {
    match startup {
        ConfigStartup::Loaded => {}
        ConfigStartup::Created => {
            println!("Config: created settings.toml and default PoE1/PoE2 profiles.");
        }
        ConfigStartup::Migrated => {
            println!(
                "Config: migrated existing config.toml into game profiles; original preserved."
            );
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
        assert_eq!(
            game_from_install_path(r"D:\Games\PoE\PathOfExileSteam.exe"),
            None
        );
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
