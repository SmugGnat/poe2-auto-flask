//! PoE1 flask keys come from the game's production_Config.ini; never
//! require per-slot keys in our own application configuration.
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::bindings::{documents_path, parse_binding};
use crate::file_watch::{file_stamp, FileStamp};
use crate::input::KeyBinding;

pub const SLOTS: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingState {
    Ready([KeyBinding; SLOTS]),
    Unavailable(String),
}

pub struct BindingManager {
    path: PathBuf,
    stamp: Option<FileStamp>,
    state: BindingState,
}

impl BindingManager {
    pub fn load() -> io::Result<Self> {
        let path = documents_path()?
            .join("My Games")
            .join("Path of Exile")
            .join("production_Config.ini");
        let stamp = file_stamp(&path)?;
        let state = load_state(&path);
        Ok(Self { path, stamp, state })
    }

    pub fn current(&self) -> &BindingState {
        &self.state
    }

    pub fn reload(&mut self) -> bool {
        let stamp = match file_stamp(&self.path) {
            Ok(stamp) => stamp,
            Err(error) => {
                self.state = BindingState::Unavailable(error.to_string());
                self.stamp = None;
                return true;
            }
        };
        if stamp == self.stamp { return false; }
        self.stamp = stamp;
        let state = load_state(&self.path);
        let changed = state != self.state;
        self.state = state;
        changed
    }
}

fn load_state(path: &Path) -> BindingState {
    match File::open(path)
        .and_then(|file| parse_reader(BufReader::new(file), path))
    {
        Ok(bindings) => BindingState::Ready(bindings),
        Err(error) => BindingState::Unavailable(format!("{}: {error}", path.display())),
    }
}

fn parse_reader<R: BufRead>(input: R, path: &Path) -> io::Result<[KeyBinding; SLOTS]> {
    let mut action: [Option<String>; SLOTS] = std::array::from_fn(|_| None);
    let mut wasd: [Option<String>; SLOTS] = std::array::from_fn(|_| None);
    let mut section = "";
    let mut mode = "mouse_and_keyboard".to_string();
    for line in input.lines() {
        let line = line?;
        let line = line.trim();
        if line.starts_with('#') || line.starts_with(';') || line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = if line == "[ACTION_KEYS]" { "action" }
                else if line == "[WASD_ACTION_KEYS]" { "wasd" } else { "other" };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue; };
        let key = key.trim();
        let value = value.trim();
        if key == "user_input_mode" {
            mode = value.to_string();
            continue;
        }
        let Some(n) = key.strip_prefix("use_flask_in_slot")
            .and_then(|v| v.parse::<usize>().ok()) else { continue; };
        if !(1..=SLOTS).contains(&n) { continue; }
        match section {
            "action" => action[n - 1] = Some(value.into()),
            "wasd" => wasd[n - 1] = Some(value.into()),
            _ => (),
        }
    }
    let input = match mode.as_str() {
        "mouse_and_keyboard" => &action,
        "wasd" => &wasd,
        "gamepad" => return Err(io::Error::new(io::ErrorKind::Unsupported, "PoE1 gamepad input is unsupported")),
        _ => return Err(io::Error::new(io::ErrorKind::Unsupported, format!("unsupported PoE1 input mode {mode}"))),
    };
    let mut result = Vec::with_capacity(SLOTS);
    for (index, raw) in input.iter().enumerate() {
        let label = format!("use_flask_in_slot{}", index + 1);
        let value = raw.as_deref().ok_or_else(|| invalid(format!("{label} missing")))?;
        let binding = parse_binding(path, &label, value)?;
        if binding.vk == 0x7A || result.contains(&binding) {
            return Err(invalid(format!("flask slot {} is bound to the arm toggle or a duplicate key", index + 1)));
        }
        result.push(binding);
    }
    result.try_into().map_err(|_| invalid("wrong number of flask bindings"))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use crate::input::{KeyBinding, KeyModifier};

    const EXAMPLE: &str = "[ACTION_KEYS]\nuse_flask_in_slot1=49\nuse_flask_in_slot2=50\nuse_flask_in_slot3=51\nuse_flask_in_slot4=52\nuse_flask_in_slot5=53\n";

    #[test]
    fn discovers_five_poe1_keys_without_manual_mapping() {
        let bindings = parse_reader(Cursor::new(EXAMPLE), Path::new("production_Config.ini")).unwrap();
        assert_eq!(bindings[3], KeyBinding::new(52, KeyModifier::None));
        assert_eq!(bindings[4], KeyBinding::new(53, KeyModifier::None));
    }

    #[test]
    fn rejects_missing_duplicate_and_unsupported_keys() {
        let no_five = EXAMPLE.replace("use_flask_in_slot5=53\n", "");
        assert!(parse_reader(Cursor::new(no_five), Path::new("test")).is_err());
        let duplicate = EXAMPLE.replace("use_flask_in_slot5=53", "use_flask_in_slot5=52");
        assert!(parse_reader(Cursor::new(duplicate), Path::new("test")).is_err());
        let mouse = EXAMPLE.replace("use_flask_in_slot5=53", "use_flask_in_slot5=1");
        assert!(parse_reader(Cursor::new(mouse), Path::new("test")).is_err());
    }

    #[test]
    fn wasd_uses_separate_binding_section_when_enabled() {
        let ini = format!("user_input_mode=wasd\n{EXAMPLE}[WASD_ACTION_KEYS]\nuse_flask_in_slot1=81\nuse_flask_in_slot2=50\nuse_flask_in_slot3=51\nuse_flask_in_slot4=52\nuse_flask_in_slot5=53\n");
        let keys = parse_reader(Cursor::new(ini), Path::new("test")).unwrap();
        assert_eq!(keys[0], KeyBinding::new(81, KeyModifier::None));
    }

    #[test]
    fn rejects_controller_input() {
        let ini = format!("user_input_mode=gamepad\n{EXAMPLE}");
        assert!(parse_reader(Cursor::new(ini), Path::new("test")).is_err());
    }
}
