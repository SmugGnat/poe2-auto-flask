use std::env;
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const APP_ID: &str = "2694490";
const GAME_EXE: &[u8] = b"PathOfExileSteam.exe";
const BUNDLED_HELPER_ENV: &str = "POE2_AUTO_FLASK_BUNDLED_HELPER";

struct LaunchEnvironment {
    pid: u32,
    steam_client: PathBuf,
    compat_data: PathBuf,
    proton: PathBuf,
}

struct Args {
    helper: Option<PathBuf>,
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

    let helper = resolve_helper_path(args.helper)?;
    if !helper.is_file() {
        return Err(format!(
            "helper executable was not found at {}",
            helper.display()
        ));
    }

    let launch = discover_launch_environment()?;

    println!("PoE2 PID    : {}", launch.pid);
    println!("Steam       : {}", launch.steam_client.display());
    println!("Compat data : {}", launch.compat_data.display());
    println!("Proton      : {}", launch.proton.display());
    println!();

    let status = Command::new(&launch.proton)
        .arg("runinprefix")
        .arg(&helper)
        .args(&args.helper_args)
        .env("STEAM_COMPAT_CLIENT_INSTALL_PATH", &launch.steam_client)
        .env("STEAM_COMPAT_DATA_PATH", &launch.compat_data)
        .status()
        .map_err(|error| format!("failed to start Proton: {error}"))?;

    if !status.success() {
        return Err(format!("Proton exited with {status}"));
    }

    Ok(())
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut helper = None;
    let mut helper_args = Vec::new();
    let mut args = env::args_os().skip(1);

    while let Some(arg) = args.next() {
        if arg == OsStr::new("--help") || arg == OsStr::new("-h") {
            print_help();
            return Ok(None);
        }
        if arg == OsStr::new("--helper") {
            helper = Some(PathBuf::from(
                args.next()
                    .ok_or_else(|| "--helper requires a path".to_string())?,
            ));
            continue;
        }
        if arg == OsStr::new("--auto") || arg == OsStr::new("--debug") {
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

    let temporary = target_dir.join(format!(
        ".poe2-auto-flask.exe.tmp-{}",
        std::process::id()
    ));
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

fn user_data_dir() -> Result<PathBuf, String> {
    if let Some(path) = env::var_os("XDG_DATA_HOME").filter(|path| !path.is_empty()) {
        return Ok(PathBuf::from(path));
    }

    let home = env::var_os("HOME")
        .filter(|path| !path.is_empty())
        .ok_or_else(|| "HOME is not set and XDG_DATA_HOME is unavailable".to_string())?;

    Ok(PathBuf::from(home).join(".local").join("share"))
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
    println!("  poe2-auto-flask-linux-launcher [--auto | --debug] [--helper PATH]");
    println!();
    println!("Path of Exile 2 must already be running through Steam Proton.");
}

fn discover_launch_environment() -> Result<LaunchEnvironment, String> {
    let entries =
        fs::read_dir("/proc").map_err(|error| format!("could not read /proc: {error}"))?;

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
        if !contains_bytes(&cmdline, GAME_EXE) {
            continue;
        }

        let Ok(environ) = fs::read(process_dir.join("environ")) else {
            continue;
        };
        if !environment_matches_app(&environ) {
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

        return Ok(LaunchEnvironment {
            pid,
            steam_client,
            compat_data,
            proton,
        });
    }

    Err("could not find a running Path of Exile 2 Proton environment; start Path of Exile 2 through Steam first".to_string())
}

fn environment_matches_app(environ: &[u8]) -> bool {
    [b"STEAM_COMPAT_APP_ID".as_slice(), b"SteamAppId".as_slice()]
        .iter()
        .filter_map(|key| env_value(environ, key))
        .any(|value| value == OsStr::new(APP_ID))
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
        contains_bytes, env_value, environment_matches_app, files_match, find_proton,
    };
    use std::env;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
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
    fn recognizes_poe2_app_id() {
        assert!(environment_matches_app(
            b"STEAM_COMPAT_APP_ID=2694490\0OTHER=value\0"
        ));
        assert!(environment_matches_app(b"SteamAppId=2694490\0"));
        assert!(!environment_matches_app(b"STEAM_COMPAT_APP_ID=1234\0"));
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
