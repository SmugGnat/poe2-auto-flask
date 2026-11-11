use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Game {
    Poe1,
    Poe2,
}

impl Game {
    fn app_id(self) -> &'static str {
        match self {
            Self::Poe1 => "238960",
            Self::Poe2 => "2694490",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Poe1 => "PoE1",
            Self::Poe2 => "PoE2",
        }
    }

    fn exe_names(self) -> &'static [&'static [u8]] {
        match self {
            Self::Poe1 => &[
                b"PathOfExileSteam.exe",
                b"PathOfExile.exe",
                b"PathOfExile_x64.exe",
                b"PathOfExile_KG.exe",
            ],
            Self::Poe2 => &[b"PathOfExileSteam.exe", b"PathOfExile.exe"],
        }
    }
}

const BUNDLED_HELPER_ENV: &str = "POE2_AUTO_FLASK_BUNDLED_HELPER";
const HELPER_CONFIG_ENV: &str = "POE2_AUTO_FLASK_CONFIG";

struct LaunchEnvironment {
    pid: u32,
    steam_client: PathBuf,
    compat_data: PathBuf,
    proton: PathBuf,
}

struct Args {
    helper: Option<PathBuf>,
    game: Option<Game>,
    helper_args: Vec<OsString>,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("poe2-auto-flask: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let Some(args) = parse_args()? else {
        return Ok(());
    };

    // Normal automation is the helper's default for either game; this launcher
    // only selects the matching Steam/Proton environment. Arming still requires F11.
    let nearby = args.helper_args.iter().any(|arg| arg == "--nearby");
    let debug = args.helper_args.iter().any(|arg| arg == "--debug");
    if nearby && (!debug || args.game == Some(Game::Poe2)) {
        return Err("--nearby requires PoE1 --debug".into());
    }

    let (game, launch) = discover_launch_environment(args.game)?;
    if nearby && game != Game::Poe1 {
        return Err("--nearby requires PoE1 --debug".into());
    }
    let helper = resolve_helper_path(args.helper)?;
    if !helper.is_file() {
        return Err(format!(
            "helper executable was not found at {}",
            helper.display()
        ));
    }

    let config = prepare_config_path()?;
    let wine_config = host_path_to_wine_z(&config)?;

    println!(
        "Selected    : {} (Steam AppID {}, {})",
        game.label(),
        game.app_id(),
        if args.game.is_some() {
            "explicit"
        } else {
            "auto-detected"
        }
    );
    println!("{} PID    : {}", game.label(), launch.pid);
    println!("Steam       : {}", launch.steam_client.display());
    println!("Compat data : {}", launch.compat_data.display());
    println!("Proton      : {}", launch.proton.display());
    println!("Config      : {}", config.display());
    println!();

    let status = Command::new(&launch.proton)
        .arg("runinprefix")
        .arg(&helper)
        .args(&args.helper_args)
        .arg("--game")
        .arg(match game {
            Game::Poe1 => "poe1",
            Game::Poe2 => "poe2",
        })
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &launch.steam_client)
        .env("STEAM_COMPAT_DATA_PATH", &launch.compat_data)
        .env(HELPER_CONFIG_ENV, wine_config)
        .status()
        .map_err(|error| format!("failed to start Proton: {error}"))?;

    if !status.success() {
        return Err(format!("Proton exited with {status}"));
    }

    Ok(())
}

fn parse_args() -> Result<Option<Args>, String> {
    parse_args_from(env::args_os().skip(1))
}

fn parse_args_from(mut args: impl Iterator<Item = OsString>) -> Result<Option<Args>, String> {
    let mut helper = None;
    let mut helper_args = Vec::new();
    let mut game = None;
    let mut game_seen = false;

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--help") || arg == OsStr::new("-h") {
            print_help();
            return Ok(None);
        }
        if arg == OsStr::new("--game") {
            if game_seen {
                return Err("--game specified more than once".into());
            }
            game_seen = true;
            game = match args.next().as_deref() {
                Some(value) if value == OsStr::new("poe1") => Some(Game::Poe1),
                Some(value) if value == OsStr::new("poe2") => Some(Game::Poe2),
                _ => return Err("--game requires poe1 or poe2".into()),
            };
            continue;
        }
        if arg == OsStr::new("--helper") {
            helper = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| "--helper requires a path".to_string())?,
            ));
            continue;
        }
        if arg == OsStr::new("--auto")
            || arg == OsStr::new("--debug")
            || arg == OsStr::new("--nearby")
        {
            helper_args.push(arg);
            continue;
        }
        if arg == OsStr::new("--") {
            helper_args.extend(args);
            break;
        }

        return Err(format!("unknown argument: {}", arg.to_string_lossy()));
    }

    Ok(Some(Args {
        helper,
        game,
        helper_args,
    }))
}

fn resolve_helper_path(explicit: Option<PathBuf>) -> Result<PathBuf, String> {
    if let Some(helper) = explicit {
        return Ok(helper);
    }

    if let Some(source) = env::var_os(BUNDLED_HELPER_ENV) {
        return install_bundled_helper(&PathBuf::from(source));
    }

    default_helper_path()
}

fn default_helper_path() -> Result<PathBuf, String> {
    let executable = env::current_exe()
        .map_err(|error| format!("could not determine launcher path: {error}"))?;
    let directory = executable
        .parent()
        .ok_or_else(|| "launcher path has no parent directory".to_string())?;
    Ok(directory.join("poe2-auto-flask.exe"))
}

fn install_bundled_helper(source: &Path) -> Result<PathBuf, String> {
    if !source.is_file() {
        return Err(format!(
            "bundled helper executable was not found at {}",
            source.display()
        ));
    }

    let target_dir = user_data_dir()?.join("poe2-auto-flask");
    fs::create_dir_all(&target_dir).map_err(|error| {
        format!(
            "could not create helper directory {}: {error}",
            target_dir.display()
        )
    })?;

    let target = target_dir.join("poe2-auto-flask.exe");
    if files_match(source, &target).map_err(|error| {
        format!(
            "could not compare bundled helper with {}: {error}",
            target.display()
        )
    })? {
        return Ok(target);
    }

    let temporary = target_dir.join(format!(".poe2-auto-flask.exe.tmp-{}", std::process::id()));
    let _ = fs::remove_file(&temporary);

    if let Err(error) = fs::copy(source, &temporary) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "could not stage helper at {}: {error}",
            temporary.display()
        ));
    }

    if let Err(error) = fs::rename(&temporary, &target) {
        let _ = fs::remove_file(&temporary);
        return Err(format!(
            "could not install helper at {}: {error}",
            target.display()
        ));
    }

    println!("Helper updated: {}", target.display());
    Ok(target)
}

fn prepare_config_path() -> Result<PathBuf, String> {
    let target_dir = user_config_dir()?.join("poe2-auto-flask");
    fs::create_dir_all(&target_dir).map_err(|error| {
        format!(
            "could not create config directory {}: {error}",
            target_dir.display()
        )
    })?;

    let target = target_dir.join("config.toml");
    if target.is_file() {
        return Ok(target);
    }

    let legacy = user_data_dir()?.join("poe2-auto-flask").join("config.toml");
    if legacy.is_file() {
        fs::copy(&legacy, &target).map_err(|error| {
            format!(
                "could not migrate config from {} to {}: {error}",
                legacy.display(),
                target.display()
            )
        })?;
        println!("Config migrated: {}", target.display());
    }

    Ok(target)
}

fn user_config_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    let home = user_home_dir("XDG_CONFIG_HOME")?;
    Ok(home.join(".config"))
}

fn user_data_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    let home = user_home_dir("XDG_DATA_HOME")?;
    Ok(home.join(".local").join("share"))
}

fn user_home_dir(fallback_for: &str) -> Result<PathBuf, String> {
    env::var_os("HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| format!("HOME is not set and {fallback_for} is unavailable"))
}

fn host_path_to_wine_z(path: &Path) -> Result<OsString, String> {
    if !path.is_absolute() {
        return Err(format!("config path must be absolute: {}", path.display()));
    }

    let path = path
        .to_str()
        .ok_or_else(|| "config path is not valid UTF-8".to_string())?;
    Ok(OsString::from(format!("Z:{}", path.replace('/', "\\"))))
}

fn files_match(left: &Path, right: &Path) -> io::Result<bool> {
    let left_metadata = fs::metadata(left)?;
    let right_metadata = match fs::metadata(right) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };

    if left_metadata.len() != right_metadata.len() {
        return Ok(false);
    }

    Ok(fs::read(left)? == fs::read(right)?)
}

fn print_help() {
    println!("poe2-auto-flask Linux launcher");
    println!();
    println!("Usage:");
    println!(
        "  poe2-auto-flask-linux-launcher [--auto | --debug] [--game poe1|poe2] [--nearby] [--helper PATH]"
    );
    println!();
    println!("The selected Path of Exile game must already be running through Steam Proton.");
    println!("Without --game, detect the single running game by its Steam AppID.");
    println!("When both games are running, use --game poe1 or --game poe2.");
    println!("When neither is detected, PoE2 remains the default (and still must be running).");
    println!("Normal automation is the default for either game and starts DISARMED (F11 to arm).");
    println!("Use --debug for read-only diagnostics; --auto remains an optional alias.");
}

// Discovery happens once at launcher startup. All candidates must have a
// recognized, consistent Steam AppID and a complete, usable Proton context.
fn discover_launch_environment(
    requested: Option<Game>,
) -> Result<(Game, LaunchEnvironment), String> {
    let entries =
        fs::read_dir("/proc").map_err(|error| format!("could not read /proc: {error}"))?;
    let mut poe1 = None;
    let mut poe2 = None;

    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let Some(pid_text) = file_name.to_str() else {
            continue;
        };
        let Ok(pid) = pid_text.parse::<u32>() else {
            continue;
        };

        let process_dir = entry.path();
        let Ok(cmdline) = fs::read(process_dir.join("cmdline")) else {
            continue;
        };
        if ![Game::Poe1, Game::Poe2].iter().any(|game| {
            game.exe_names()
                .iter()
                .any(|name| contains_bytes(&cmdline, name))
        }) {
            continue;
        }

        let Ok(environ) = fs::read(process_dir.join("environ")) else {
            continue;
        };
        let Some(game) = detected_game(&environ) else {
            continue;
        };
        if requested.is_some_and(|selected| selected != game)
            || !game
                .exe_names()
                .iter()
                .any(|name| contains_bytes(&cmdline, name))
        {
            continue;
        }

        let Some(steam_client) = env_value(&environ, b"STEAM_COMPAT_CLIENT_INSTALL_PATH") else {
            continue;
        };
        let Some(compat_data) = env_value(&environ, b"STEAM_COMPAT_DATA_PATH") else {
            continue;
        };
        let Some(tool_paths) = env_value(&environ, b"STEAM_COMPAT_TOOL_PATHS") else {
            continue;
        };

        let steam_client = PathBuf::from(steam_client);
        let compat_data = PathBuf::from(compat_data);
        let Some(proton) = find_proton(&tool_paths) else {
            continue;
        };
        if !steam_client.is_dir() || !compat_data.join("pfx").is_dir() {
            continue;
        }

        let launch = LaunchEnvironment {
            pid,
            steam_client,
            compat_data,
            proton,
        };
        match game {
            Game::Poe1 if poe1.is_none() => poe1 = Some(launch),
            Game::Poe2 if poe2.is_none() => poe2 = Some(launch),
            _ => {}
        }
        // Explicit selection does not require inspecting the other game.
        if requested == Some(game) {
            break;
        }
        // Once both valid games are found, selection cannot be implicit.
        if poe1.is_some() && poe2.is_some() {
            break;
        }
    }

    let game = select_game(requested, poe1.is_some(), poe2.is_some())?;
    let launch = match game {
        Game::Poe1 => poe1,
        Game::Poe2 => poe2,
    }
    .ok_or_else(|| {
        format!(
            "could not find a running {} Proton environment (Steam AppID {}); start the selected game through Steam first",
            game.label(),
            game.app_id()
        )
    })?;
    Ok((game, launch))
}

fn select_game(
    requested: Option<Game>,
    poe1_running: bool,
    poe2_running: bool,
) -> Result<Game, String> {
    if let Some(game) = requested {
        return Ok(game);
    }
    match (poe1_running, poe2_running) {
        (true, false) => Ok(Game::Poe1),
        (false, true) | (false, false) => Ok(Game::Poe2),
        (true, true) => Err(
            "both PoE1 and PoE2 Steam Proton environments are running; specify --game poe1 or --game poe2"
                .into(),
        ),
    }
}

// Conflicting or unknown Steam IDs must not attach one game's policy to
// another game's Proton prefix, even if the executable basenames overlap.
fn detected_game(environ: &[u8]) -> Option<Game> {
    let mut detected = None;
    for key in [b"STEAM_COMPAT_APP_ID".as_slice(), b"SteamAppId".as_slice()] {
        let Some(app_id) = env_value(environ, key) else {
            continue;
        };
        let game = if app_id == OsStr::new(Game::Poe1.app_id()) {
            Game::Poe1
        } else if app_id == OsStr::new(Game::Poe2.app_id()) {
            Game::Poe2
        } else {
            return None;
        };
        if detected.is_some_and(|previous| previous != game) {
            return None;
        }
        detected = Some(game);
    }
    detected
}

fn env_value(environ: &[u8], key: &[u8]) -> Option<OsString> {
    environ.split(|byte| *byte == 0).find_map(|entry| {
        let separator = entry.iter().position(|byte| *byte == b'=')?;
        if &entry[..separator] != key {
            return None;
        }
        Some(OsString::from_vec(entry[separator + 1..].to_vec()))
    })
}

fn find_proton(tool_paths: &OsStr) -> Option<PathBuf> {
    env::split_paths(tool_paths)
        .map(|tool| tool.join("proton"))
        .find(|candidate| is_executable_file(candidate))
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::{
        contains_bytes, detected_game, env_value, files_match, find_proton, host_path_to_wine_z,
        parse_args_from, select_game, Game,
    };
    use std::env;
    use std::ffi::OsString;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_values_from_nul_separated_environment() {
        let environ = b"A=1\0STEAM_COMPAT_DATA_PATH=/tmp/compat data\0B=2\0";
        assert_eq!(
            env_value(environ, b"STEAM_COMPAT_DATA_PATH")
                .unwrap()
                .to_string_lossy(),
            "/tmp/compat data"
        );
    }

    #[test]
    fn default_launch_has_no_required_mode_or_game_flags() {
        let args = parse_args_from(std::iter::empty()).unwrap().unwrap();
        assert_eq!(args.game, None);
        assert!(args.helper_args.is_empty());
    }

    #[test]
    fn poe1_selection_does_not_require_auto_or_debug() {
        let args = parse_args_from([OsString::from("--game"), OsString::from("poe1")].into_iter())
            .unwrap()
            .unwrap();
        assert_eq!(args.game, Some(Game::Poe1));
        assert!(args.helper_args.is_empty());
    }

    #[test]
    fn debug_remains_an_explicit_override() {
        let args = parse_args_from([OsString::from("--debug")].into_iter())
            .unwrap()
            .unwrap();
        assert_eq!(args.game, None);
        assert_eq!(args.helper_args, vec![OsString::from("--debug")]);
    }

    #[test]
    fn auto_flag_remains_backwards_compatible() {
        let args = parse_args_from([OsString::from("--auto")].into_iter())
            .unwrap()
            .unwrap();
        assert_eq!(args.helper_args, vec![OsString::from("--auto")]);
    }

    #[test]
    fn recognizes_only_consistent_game_app_ids() {
        assert_eq!(
            detected_game(b"STEAM_COMPAT_APP_ID=2694490\0OTHER=value\0"),
            Some(Game::Poe2)
        );
        assert_eq!(detected_game(b"SteamAppId=2694490\0"), Some(Game::Poe2));
        assert_eq!(detected_game(b"SteamAppId=238960\0"), Some(Game::Poe1));
        assert_eq!(
            detected_game(b"STEAM_COMPAT_APP_ID=238960\0SteamAppId=238960\0"),
            Some(Game::Poe1)
        );
        assert_eq!(detected_game(b"SteamAppId=1234\0"), None);
        assert_eq!(detected_game(b"OTHER=value\0"), None);
        assert_eq!(
            detected_game(b"STEAM_COMPAT_APP_ID=238960\0SteamAppId=2694490\0"),
            None
        );
        assert_eq!(
            detected_game(b"STEAM_COMPAT_APP_ID=1234\0SteamAppId=238960\0"),
            None
        );
        assert!(Game::Poe1
            .exe_names()
            .contains(&b"PathOfExile_x64.exe".as_slice()));
    }

    #[test]
    fn automatic_game_selection_is_unambiguous_and_defaults_to_poe2() {
        assert_eq!(select_game(None, false, false).unwrap(), Game::Poe2);
        assert_eq!(select_game(None, true, false).unwrap(), Game::Poe1);
        assert_eq!(select_game(None, false, true).unwrap(), Game::Poe2);
        assert!(select_game(None, true, true)
            .unwrap_err()
            .contains("--game"));
    }

    #[test]
    fn explicit_override_wins_even_with_both_games_running() {
        assert_eq!(
            select_game(Some(Game::Poe1), true, true).unwrap(),
            Game::Poe1
        );
        assert_eq!(
            select_game(Some(Game::Poe2), true, true).unwrap(),
            Game::Poe2
        );
        assert_eq!(
            select_game(Some(Game::Poe1), false, true).unwrap(),
            Game::Poe1
        );
    }

    #[test]
    fn finds_game_name_inside_process_command_line() {
        assert!(contains_bytes(
            b"python\0proton\0waitforexitandrun\0PathOfExileSteam.exe\0",
            b"PathOfExileSteam.exe"
        ));
        assert!(!contains_bytes(
            b"some-other-game.exe\0",
            b"PathOfExileSteam.exe"
        ));
    }

    #[test]
    fn selects_executable_proton_tool() {
        let root = unique_temp_dir();
        let first = root.join("SteamLinuxRuntime_4");
        let second = root.join("Proton - Experimental");
        fs::create_dir_all(&first).unwrap();
        fs::create_dir_all(&second).unwrap();

        let proton = second.join("proton");
        fs::write(&proton, b"#!/bin/sh\n").unwrap();
        let mut permissions = fs::metadata(&proton).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&proton, permissions).unwrap();

        let tool_paths = env::join_paths([first.as_os_str(), second.as_os_str()]).unwrap();
        assert_eq!(find_proton(&tool_paths), Some(proton));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn converts_host_config_path_to_wine_z_drive() {
        assert_eq!(
            host_path_to_wine_z(Path::new("/home/test/.config/poe2-auto-flask/config.toml"))
                .unwrap()
                .to_string_lossy(),
            r"Z:\home\test\.config\poe2-auto-flask\config.toml"
        );
    }

    #[test]
    fn compares_helper_payload_contents() {
        let root = unique_temp_dir();
        fs::create_dir_all(&root).unwrap();

        let left = root.join("left.exe");
        let right = root.join("right.exe");
        fs::write(&left, b"same helper").unwrap();
        fs::write(&right, b"same helper").unwrap();
        assert!(files_match(&left, &right).unwrap());

        fs::write(&right, b"different").unwrap();
        assert!(!files_match(&left, &right).unwrap());

        fs::remove_dir_all(root).unwrap();
    }

    fn unique_temp_dir() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!(
            "poe2-auto-flask-launcher-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
