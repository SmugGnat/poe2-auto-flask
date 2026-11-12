//! PoE2-specific flask binding discovery and INI reload policy.
use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};

use crate::bindings::{documents_path, invalid, parse_binding};
use crate::file_watch::{file_stamp, FileStamp};
use crate::input::KeyBinding;

const POE2_CONFIG_RELATIVE_PATH: [&str; 3] =
    ["My Games", "Path of Exile 2", "poe2_production_Config.ini"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameBindings {
    Keyboard {
        mode: KeyboardMode,
        life: KeyBinding,
        mana: KeyBinding,
    },
    Gamepad,
    UnsupportedMode(String),
    Unavailable(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardMode {
    MouseAndKeyboard,
    Wasd,
}

pub struct BindingManager {
    current: GameBindings,
    path: PathBuf,
    observed_stamp: Option<FileStamp>,
    file_present: bool,
    last_error: Option<String>,
}

pub enum BindingReload {
    Unchanged,
    Reloaded,
    Removed,
    Invalid(String),
    ReadError(String),
}

impl GameBindings {
    pub fn summary(&self) -> String {
        match self {
            Self::Keyboard { mode, life, mana } => format!(
                "PoE2 input: {} | Life {} | Mana {}",
                mode.label(),
                life.label(),
                mana.label()
            ),
            Self::Gamepad => "PoE2 input: gamepad (auto-flask input not supported yet)".to_string(),
            Self::UnsupportedMode(mode) => {
                format!("PoE2 input: {mode} (unsupported input mode)")
            }
            Self::Unavailable(error) => format!("PoE2 input bindings unavailable: {error}"),
        }
    }
}

impl KeyboardMode {
    fn label(self) -> &'static str {
        match self {
            Self::MouseAndKeyboard => "mouse_and_keyboard",
            Self::Wasd => "wasd",
        }
    }
}

impl BindingManager {
    pub fn load() -> io::Result<Self> {
        let path = poe2_config_path()?;
        let stamp = file_stamp(&path)?;
        let file_present = stamp.is_some();

        let current = match stamp {
            Some(_) => match parse_file(&path) {
                Ok(bindings) => bindings,
                Err(error) => GameBindings::Unavailable(error.to_string()),
            },
            None => GameBindings::Unavailable(format!("{} not found", path.display())),
        };

        Ok(Self {
            current,
            path,
            observed_stamp: stamp,
            file_present,
            last_error: None,
        })
    }

    pub fn current(&self) -> &GameBindings {
        &self.current
    }

    pub fn check_for_reload(&mut self) -> BindingReload {
        let stamp = match file_stamp(&self.path) {
            Ok(stamp) => stamp,
            Err(error) => {
                self.observed_stamp = None;
                return self
                    .read_error(format!("failed to check {}: {error}", self.path.display()));
            }
        };

        if stamp == self.observed_stamp {
            return BindingReload::Unchanged;
        }

        let Some(stamp) = stamp else {
            self.observed_stamp = None;
            self.last_error = None;
            self.current = GameBindings::Unavailable(format!("{} not found", self.path.display()));
            if self.file_present {
                self.file_present = false;
                return BindingReload::Removed;
            }
            return BindingReload::Unchanged;
        };

        self.file_present = true;
        match parse_file(&self.path) {
            Ok(bindings) => {
                self.observed_stamp = Some(stamp);
                self.last_error = None;
                if bindings == self.current {
                    BindingReload::Unchanged
                } else {
                    self.current = bindings;
                    BindingReload::Reloaded
                }
            }
            Err(error) => {
                self.observed_stamp = None;
                self.current = GameBindings::Unavailable(error.to_string());
                let message = error.to_string();
                if self.last_error.as_ref() == Some(&message) {
                    BindingReload::Unchanged
                } else {
                    self.last_error = Some(message.clone());
                    BindingReload::Invalid(message)
                }
            }
        }
    }

    fn read_error(&mut self, message: String) -> BindingReload {
        self.current = GameBindings::Unavailable(message.clone());
        if self.last_error.as_ref() == Some(&message) {
            BindingReload::Unchanged
        } else {
            self.last_error = Some(message.clone());
            BindingReload::ReadError(message)
        }
    }
}

fn poe2_config_path() -> io::Result<PathBuf> {
    let mut path = documents_path()?;
    for part in POE2_CONFIG_RELATIVE_PATH {
        path.push(part);
    }
    Ok(path)
}

fn parse_file(path: &Path) -> io::Result<GameBindings> {
    let file = File::open(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("failed to open {}: {error}", path.display()),
        )
    })?;
    parse_reader(BufReader::new(file), path)
}

fn parse_reader<R: BufRead>(reader: R, path: &Path) -> io::Result<GameBindings> {
    let mut mode = None;
    let mut section = Section::Other;
    let mut action_life = None;
    let mut action_mana = None;
    let mut wasd_life = None;
    let mut wasd_mana = None;

    for line in reader.lines() {
        let line = line.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("failed to read {}: {error}", path.display()),
            )
        })?;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            section = match line {
                "[ACTION_KEYS]" => Section::Action,
                "[WASD_ACTION_KEYS]" => Section::Wasd,
                _ => Section::Other,
            };
            continue;
        }

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();

        if key == "user_input_mode" {
            mode = Some(parse_input_mode(value));
            continue;
        }

        match (section, key) {
            (Section::Action, "use_flask_in_slot1") => action_life = Some(value.to_string()),
            (Section::Action, "use_flask_in_slot2") => action_mana = Some(value.to_string()),
            (Section::Wasd, "use_flask_in_slot1") => wasd_life = Some(value.to_string()),
            (Section::Wasd, "use_flask_in_slot2") => wasd_mana = Some(value.to_string()),
            _ => {}
        }
    }

    match mode.ok_or_else(|| invalid(path, "user_input_mode is missing"))? {
        ParsedInputMode::MouseAndKeyboard => keyboard_bindings(
            path,
            KeyboardMode::MouseAndKeyboard,
            action_life,
            action_mana,
        ),
        ParsedInputMode::Wasd => keyboard_bindings(path, KeyboardMode::Wasd, wasd_life, wasd_mana),
        ParsedInputMode::Gamepad => Ok(GameBindings::Gamepad),
        ParsedInputMode::Unsupported(mode) => Ok(GameBindings::UnsupportedMode(mode)),
    }
}

fn keyboard_bindings(
    path: &Path,
    mode: KeyboardMode,
    life: Option<String>,
    mana: Option<String>,
) -> io::Result<GameBindings> {
    let life = life.ok_or_else(|| invalid(path, "use_flask_in_slot1 is missing"))?;
    let mana = mana.ok_or_else(|| invalid(path, "use_flask_in_slot2 is missing"))?;
    Ok(GameBindings::Keyboard {
        mode,
        life: parse_binding(path, "use_flask_in_slot1", &life)?,
        mana: parse_binding(path, "use_flask_in_slot2", &mana)?,
    })
}

fn parse_input_mode(value: &str) -> ParsedInputMode {
    match value {
        "mouse_and_keyboard" => ParsedInputMode::MouseAndKeyboard,
        "wasd" => ParsedInputMode::Wasd,
        "gamepad" => ParsedInputMode::Gamepad,
        other => ParsedInputMode::Unsupported(other.to_string()),
    }
}

#[derive(Clone, Copy)]
enum Section {
    Other,
    Action,
    Wasd,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedInputMode {
    MouseAndKeyboard,
    Wasd,
    Gamepad,
    Unsupported(String),
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::path::Path;

    use super::{parse_reader, GameBindings, KeyboardMode};
    use crate::input::{KeyBinding, KeyModifier};

    #[test]
    fn wasd_uses_wasd_section_and_shift_modifier() {
        let text = r#"
user_input_mode=wasd
[ACTION_KEYS]
use_flask_in_slot1=49
use_flask_in_slot2=50
[WASD_ACTION_KEYS]
use_flask_in_slot1=81 1
use_flask_in_slot2=50
"#;
        let parsed = parse_reader(Cursor::new(text), Path::new("poe2.ini")).unwrap();
        assert_eq!(
            parsed,
            GameBindings::Keyboard {
                mode: KeyboardMode::Wasd,
                life: KeyBinding::new(81, KeyModifier::Shift),
                mana: KeyBinding::new(50, KeyModifier::None),
            }
        );
    }

    #[test]
    fn mouse_mode_uses_action_section() {
        let text = r#"
user_input_mode=mouse_and_keyboard
[ACTION_KEYS]
use_flask_in_slot1=65 2
use_flask_in_slot2=66 3
[WASD_ACTION_KEYS]
use_flask_in_slot1=81
use_flask_in_slot2=50
"#;
        let parsed = parse_reader(Cursor::new(text), Path::new("poe2.ini")).unwrap();
        assert_eq!(
            parsed,
            GameBindings::Keyboard {
                mode: KeyboardMode::MouseAndKeyboard,
                life: KeyBinding::new(65, KeyModifier::Control),
                mana: KeyBinding::new(66, KeyModifier::Alt),
            }
        );
    }

    #[test]
    fn gamepad_is_detected_without_keyboard_bindings() {
        let text = "user_input_mode=gamepad\n";
        assert_eq!(
            parse_reader(Cursor::new(text), Path::new("poe2.ini")).unwrap(),
            GameBindings::Gamepad
        );
    }

    #[test]
    fn unknown_input_mode_fails_closed() {
        let text = "user_input_mode=future_mode\n";
        assert_eq!(
            parse_reader(Cursor::new(text), Path::new("poe2.ini")).unwrap(),
            GameBindings::UnsupportedMode("future_mode".to_string())
        );
    }
}
