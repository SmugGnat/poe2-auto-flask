use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::file_watch::{file_stamp, FileStamp};

const DEFAULT_CONFIG_TEXT: &str = include_str!("../config.example.toml");
const CONFIG_PATH_ENV: &str = "POE2_AUTO_FLASK_CONFIG";

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub health: FlaskConfig,
    pub mana: FlaskConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub enable_in_hideout: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(default)]
pub struct FlaskConfig {
    pub enabled: bool,
    pub threshold_percent: f32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigStartup {
    Loaded,
    Created,
    BuiltInFallback(String),
}

pub struct ConfigManager {
    current: Config,
    path: PathBuf,
    observed_stamp: Option<FileStamp>,
    file_present: bool,
    current_from_file: bool,
    startup: ConfigStartup,
    last_read_error: Option<String>,
}

pub enum ConfigReload {
    Unchanged,
    Reloaded,
    Removed,
    Invalid(String),
    ReadError(String),
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            health: FlaskConfig {
                enabled: true,
                threshold_percent: 60.0,
            },
            mana: FlaskConfig {
                enabled: true,
                threshold_percent: 30.0,
            },
        }
    }
}

impl Default for FlaskConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_percent: 50.0,
        }
    }
}

impl Config {
    pub fn summary(&self) -> String {
        format!(
            "{} | {} | Hideout {}",
            flask_summary("Life", &self.health),
            flask_summary("Mana", &self.mana),
            if self.general.enable_in_hideout {
                "ON"
            } else {
                "OFF"
            }
        )
    }

    fn parse(path: &Path, text: &str) -> io::Result<Self> {
        let config: Self = toml::from_str(text).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse {}: {error}", path.display()),
            )
        })?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> io::Result<()> {
        validate_flask("health", &self.health)?;
        validate_flask("mana", &self.mana)?;
        Ok(())
    }
}

impl ConfigManager {
    pub fn load_default() -> io::Result<Self> {
        if let Some(path) = std::env::var_os(CONFIG_PATH_ENV).filter(|path| !path.is_empty()) {
            return Self::load(PathBuf::from(path));
        }

        let exe = std::env::current_exe()?;
        let legacy_path = exe
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("config.toml");

        let path = std::env::var_os("APPDATA")
            .filter(|path| !path.is_empty())
            .map(PathBuf::from)
            .map(|path| path.join("poe2-auto-flask").join("config.toml"))
            .unwrap_or_else(|| legacy_path.clone());

        if path != legacy_path && !path.is_file() && legacy_path.is_file() {
            create_parent_directory(&path)?;
            fs::copy(&legacy_path, &path)?;
        }

        Self::load(path)
    }

    fn load(path: PathBuf) -> io::Result<Self> {
        if let Some(stamp) = file_stamp(&path)? {
            return Self::load_existing(path, stamp, ConfigStartup::Loaded);
        }

        let current = Config::default();
        current.validate()?;
        create_parent_directory(&path)?;

        match create_default_config(&path) {
            Ok(()) => {
                let stamp = file_stamp(&path)?.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("{} disappeared after creation", path.display()),
                    )
                })?;
                Ok(Self {
                    current,
                    path,
                    observed_stamp: Some(stamp),
                    file_present: true,
                    current_from_file: true,
                    startup: ConfigStartup::Created,
                    last_read_error: None,
                })
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let stamp = file_stamp(&path)?.ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("{} disappeared while opening it", path.display()),
                    )
                })?;
                Self::load_existing(path, stamp, ConfigStartup::Loaded)
            }
            Err(error) => Ok(Self {
                current,
                path,
                observed_stamp: None,
                file_present: false,
                current_from_file: false,
                startup: ConfigStartup::BuiltInFallback(error.to_string()),
                last_read_error: None,
            }),
        }
    }

    fn load_existing(path: PathBuf, stamp: FileStamp, startup: ConfigStartup) -> io::Result<Self> {
        let text = fs::read_to_string(&path)?;
        let current = Config::parse(&path, &text)?;
        Ok(Self {
            current,
            path,
            observed_stamp: Some(stamp),
            file_present: true,
            current_from_file: true,
            startup,
            last_read_error: None,
        })
    }

    pub fn current(&self) -> &Config {
        &self.current
    }

    pub fn current_from_file(&self) -> bool {
        self.current_from_file
    }

    pub fn startup(&self) -> &ConfigStartup {
        &self.startup
    }

    pub fn check_for_reload(&mut self) -> ConfigReload {
        let stamp = match file_stamp(&self.path) {
            Ok(stamp) => stamp,
            Err(error) => return self.read_error(error.to_string()),
        };

        if stamp == self.observed_stamp {
            return ConfigReload::Unchanged;
        }

        let Some(stamp) = stamp else {
            self.observed_stamp = None;
            self.last_read_error = None;
            if self.file_present {
                self.file_present = false;
                return ConfigReload::Removed;
            }
            return ConfigReload::Unchanged;
        };

        match fs::read_to_string(&self.path) {
            Ok(text) => match Config::parse(&self.path, &text) {
                Ok(config) => {
                    self.current = config;
                    self.current_from_file = true;
                    self.file_present = true;
                    self.observed_stamp = Some(stamp);
                    self.last_read_error = None;
                    ConfigReload::Reloaded
                }
                Err(error) => {
                    self.file_present = true;
                    self.observed_stamp = Some(stamp);
                    self.last_read_error = None;
                    ConfigReload::Invalid(error.to_string())
                }
            },
            Err(error) => self.read_error(error.to_string()),
        }
    }

    fn read_error(&mut self, message: String) -> ConfigReload {
        if self.last_read_error.as_ref() == Some(&message) {
            ConfigReload::Unchanged
        } else {
            self.last_read_error = Some(message.clone());
            ConfigReload::ReadError(message)
        }
    }
}

fn create_parent_directory(path: &Path) -> io::Result<()> {
    let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    else {
        return Ok(());
    };
    fs::create_dir_all(parent)
}

fn create_default_config(path: &Path) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = file
        .write_all(DEFAULT_CONFIG_TEXT.as_bytes())
        .and_then(|_| file.flush())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

fn flask_summary(label: &str, flask: &FlaskConfig) -> String {
    if flask.enabled {
        format!("{label} <= {:.1}%", flask.threshold_percent)
    } else {
        format!("{label} OFF")
    }
}

fn validate_flask(name: &str, flask: &FlaskConfig) -> io::Result<()> {
    if !(1.0..=99.0).contains(&flask.threshold_percent) {
        return Err(invalid(format!(
            "{name}.threshold_percent must be between 1 and 99"
        )));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::{Config, ConfigManager, ConfigStartup, DEFAULT_CONFIG_TEXT};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn bundled_default_config_matches_built_in_defaults() {
        let config = Config::parse(Path::new("config.toml"), DEFAULT_CONFIG_TEXT)
            .expect("bundled default config must parse");
        assert_eq!(config, Config::default());
    }

    #[test]
    fn missing_config_is_created_with_defaults() {
        let (dir, path) = temp_config_path();
        let manager = ConfigManager::load(path.clone()).expect("config manager should load");

        assert_eq!(manager.startup(), &ConfigStartup::Created);
        assert!(manager.current_from_file());
        assert_eq!(manager.current(), &Config::default());
        assert_eq!(
            fs::read_to_string(&path).expect("created config should be readable"),
            DEFAULT_CONFIG_TEXT
        );

        fs::remove_dir_all(dir).expect("temporary directory should be removable");
    }

    #[test]
    fn existing_config_is_loaded_without_overwrite() {
        let (dir, path) = temp_config_path();
        let text = r#"
[general]
enable_in_hideout = true

[health]
enabled = true
threshold_percent = 55.0

[mana]
enabled = false
threshold_percent = 25.0
"#;
        fs::write(&path, text).expect("test config should be writable");

        let manager = ConfigManager::load(path.clone()).expect("config manager should load");
        assert_eq!(manager.startup(), &ConfigStartup::Loaded);
        assert!(manager.current_from_file());
        assert!(manager.current().general.enable_in_hideout);
        assert_eq!(manager.current().health.threshold_percent, 55.0);
        assert!(!manager.current().mana.enabled);
        assert_eq!(
            fs::read_to_string(&path).expect("existing config should remain readable"),
            text
        );

        fs::remove_dir_all(dir).expect("temporary directory should be removable");
    }

    #[test]
    fn legacy_poll_and_key_fields_are_ignored() {
        let text = r#"
[general]
poll_interval_ms = 50
enable_in_hideout = true

[health]
enabled = true
threshold_percent = 55.0
key = "Q"

[mana]
enabled = true
threshold_percent = 25.0
key = "2"
"#;

        let config = Config::parse(Path::new("config.toml"), text).expect("valid config");
        assert!(config.general.enable_in_hideout);
        assert_eq!(config.health.threshold_percent, 55.0);
        assert_eq!(config.mana.threshold_percent, 25.0);
    }

    #[test]
    fn invalid_threshold_is_rejected() {
        let text = r#"
[health]
threshold_percent = 0.0
"#;

        assert!(Config::parse(Path::new("config.toml"), text).is_err());
    }

    fn temp_config_path() -> (PathBuf, PathBuf) {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "poe2-auto-flask-config-test-{}-{id}",
            std::process::id()
        ));
        fs::create_dir(&dir).expect("temporary directory should be creatable");
        let path = dir.join("config.toml");
        (dir, path)
    }
}
