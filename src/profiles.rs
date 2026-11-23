//! Versioned, file-backed profile store shared by the UI and automation backend.
//! No game memory work and no filesystem traversal in the 100ms policy loop.
//! Active files are stat-ed by the engine's existing once-per-second poll.
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::config::{invalid, Config, ConfigReload, FlaskConfig, GeneralConfig};
use crate::file_watch::{file_stamp, FileStamp};
use crate::game::Game;
use crate::input::AutomationKey;
use crate::poe1::config::Poe1Config;

const SCHEMA_VERSION: u32 = 1;
const POE1_DEFAULT: &str = "poe1-default";
const POE2_DEFAULT: &str = "poe2-default";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppSettings {
    pub schema_version: u32,
    pub active_poe1: String,
    pub active_poe2: String,
    #[serde(default, skip_serializing_if = "AutomationKey::is_default")]
    pub automation_key: AutomationKey,
    #[serde(default)]
    pub ui: UiSettings,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UiSettings {
    pub hide_to_tray_on_close: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub window: Option<WindowGeometry>,
    #[serde(skip_serializing_if = "SkillSections::is_default")]
    pub skills: SkillSections,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkillSections {
    pub molten_shell_expanded: bool,
    pub blood_rage_expanded: bool,
    pub spirit_offering_expanded: bool,
    pub steelskin_expanded: bool,
    pub bladefall_of_trarthus_expanded: bool,
    pub convocation_expanded: bool,
}

impl Default for SkillSections {
    fn default() -> Self {
        Self {
            molten_shell_expanded: true,
            blood_rage_expanded: true,
            spirit_offering_expanded: true,
            steelskin_expanded: true,
            bladefall_of_trarthus_expanded: true,
            convocation_expanded: true,
        }
    }
}

impl SkillSections {
    fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowGeometry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<i32>,
    pub width: u32,
    pub height: u32,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            hide_to_tray_on_close: true,
            window: None,
            skills: SkillSections::default(),
        }
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            active_poe1: POE1_DEFAULT.into(),
            active_poe2: POE2_DEFAULT.into(),
            automation_key: AutomationKey::default(),
            ui: UiSettings::default(),
        }
    }
}

impl AppSettings {
    fn active_for(&self, game: Game) -> &str {
        match game {
            Game::Poe1 => &self.active_poe1,
            Game::Poe2 => &self.active_poe2,
        }
    }

    fn set_active(&mut self, game: Game, id: &str) {
        match game {
            Game::Poe1 => self.active_poe1 = id.to_string(),
            Game::Poe2 => self.active_poe2 = id.to_string(),
        }
    }

    fn validate(&self) -> io::Result<()> {
        version(self.schema_version)?;
        valid_id(&self.active_poe1)?;
        valid_id(&self.active_poe2)?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    pub game: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileDocument {
    schema_version: u32,
    profile: ProfileInfo,
    general: GeneralConfig,
    health: FlaskConfig,
    mana: FlaskConfig,
    #[serde(default, skip_serializing_if = "poe1_defaults")]
    poe1: Poe1Config,
}

fn poe1_defaults(config: &Poe1Config) -> bool {
    *config == Poe1Config::default()
}

impl ProfileDocument {
    fn new(id: &str, name: &str, game: Game, mut config: Config) -> io::Result<Self> {
        valid_id(id)?;
        valid_name(name)?;
        config.validate()?;
        if game == Game::Poe2 {
            // A PoE2 profile must not silently carry PoE1 skill/flask rules.
            if config.poe1 != Poe1Config::default() {
                return Err(invalid(
                    "PoE2 profiles cannot contain PoE1-specific settings",
                ));
            }
            config.poe1 = Poe1Config::default();
        }
        Ok(Self {
            schema_version: SCHEMA_VERSION,
            profile: ProfileInfo {
                id: id.into(),
                name: name.trim().into(),
                game: game_id(game).into(),
            },
            general: config.general,
            health: config.health,
            mana: config.mana,
            poe1: config.poe1,
        })
    }

    fn game(&self) -> io::Result<Game> {
        self.profile
            .game
            .parse()
            .map_err(|_| invalid("profile.game must be poe1 or poe2"))
    }

    fn config(&self) -> Config {
        Config {
            general: self.general.clone(),
            health: self.health.clone(),
            mana: self.mana.clone(),
            poe1: self.poe1.clone(),
        }
    }

    fn validate(&self) -> io::Result<()> {
        version(self.schema_version)?;
        valid_id(&self.profile.id)?;
        valid_name(&self.profile.name)?;
        let game = self.game()?;
        ProfileDocument::new(&self.profile.id, &self.profile.name, game, self.config())?;
        Ok(())
    }

    fn read(path: &Path, expected_id: &str, expected_game: Option<Game>) -> io::Result<Self> {
        valid_id(expected_id)?;
        let text = fs::read_to_string(path)?;
        let document: Self = parse(path, &text)?;
        document.validate()?;
        if document.profile.id != expected_id {
            return Err(invalid(format!(
                "{}: profile ID does not match its filename",
                path.display()
            )));
        }
        if expected_game.is_some_and(|game| document.game().ok() != Some(game)) {
            return Err(invalid(format!(
                "{}: profile belongs to the wrong game",
                path.display()
            )));
        }
        Ok(document)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileStartup {
    Loaded,
    Created,
    Migrated,
}

/// An editable copy with an opaque filesystem revision. The UI can keep this
/// while the user adjusts controls and refuse to overwrite an external edit.
#[derive(Clone, Debug)]
pub struct ProfileEdit {
    pub info: ProfileInfo,
    pub config: Config,
    revision: FileStamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingChange {
    Reloaded,
    Switched,
}

/// A small owned snapshot. No locks or per-tick profile IO.
pub struct ProfileStore {
    root: PathBuf,
    settings: AppSettings,
    active_game: Game,
    current: Config,
    settings_stamp: Option<FileStamp>,
    profile_stamp: Option<FileStamp>,
    failed_pair: Option<(Option<FileStamp>, Option<FileStamp>)>,
    pending_change: Option<PendingChange>,
}

impl ProfileStore {
    pub fn open(root: PathBuf, external_legacy: &Path) -> io::Result<(Self, ProfileStartup)> {
        let settings_path = root.join("settings.toml");
        let startup = if settings_path.is_file() {
            ProfileStartup::Loaded
        } else {
            // Parse and validate legacy data before creating or replacing anything.
            let local_legacy = root.join("config.toml");
            let source = if local_legacy.is_file() {
                Some(local_legacy)
            } else if external_legacy.is_file() {
                Some(external_legacy.to_path_buf())
            } else {
                None
            };
            let migrated = source.is_some();
            let original = match source.as_ref() {
                Some(path) => {
                    let text = fs::read_to_string(path)?;
                    Config::parse(path, &text)?
                }
                None => Config::default(),
            };
            let mut second = original.clone();
            second.poe1 = Poe1Config::default();
            let poe1 = ProfileDocument::new(POE1_DEFAULT, "Default PoE1", Game::Poe1, original)?;
            let poe2 = ProfileDocument::new(POE2_DEFAULT, "Default PoE2", Game::Poe2, second)?;
            fs::create_dir_all(root.join("profiles"))?;
            // Resume safely if an earlier migration wrote a profile before
            // crashing: never overwrite an existing file or alter legacy TOML.
            ensure_initial_profile(&root, &poe1)?;
            ensure_initial_profile(&root, &poe2)?;
            write_new(
                &settings_path,
                &encode(&settings_path, &AppSettings::default())?,
            )?;
            if migrated {
                ProfileStartup::Migrated
            } else {
                ProfileStartup::Created
            }
        };
        let settings = read_settings(&settings_path)?;
        // Both selections must exist and belong to the correct game, even
        // when the helper will currently attach to only one of the games.
        let poe1 = read_profile(&root, settings.active_for(Game::Poe1), Some(Game::Poe1))?;
        let poe2 = read_profile(&root, settings.active_for(Game::Poe2), Some(Game::Poe2))?;
        let _ = poe1;
        let current = poe2.config();
        let settings_stamp = file_stamp(&settings_path)?;
        let profile_stamp = file_stamp(&profile_path(&root, settings.active_for(Game::Poe2))?)?;
        Ok((
            Self {
                root,
                settings,
                active_game: Game::Poe2,
                current,
                settings_stamp,
                profile_stamp,
                failed_pair: None,
                pending_change: None,
            },
            startup,
        ))
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn settings(&self) -> &AppSettings {
        &self.settings
    }

    pub fn game(&self) -> Game {
        self.active_game
    }

    pub fn active_profile_id(&self) -> &str {
        self.settings.active_for(self.active_game)
    }

    pub fn current(&self) -> &Config {
        &self.current
    }

    pub fn select_game(&mut self, game: Game) -> io::Result<()> {
        if self.active_game == game {
            return Ok(());
        }
        let id = self.settings.active_for(game);
        let document = read_profile(&self.root, id, Some(game))?;
        let stamp = file_stamp(&profile_path(&self.root, id)?)?;
        self.current = document.config();
        self.active_game = game;
        self.profile_stamp = stamp;
        self.failed_pair = None;
        self.pending_change = None;
        Ok(())
    }

    /// Called only by ConfigManager's existing once-per-second config poll.
    /// Malformed, deleted, or mismatched documents never replace a live config.
    pub fn check_for_reload(&mut self) -> ConfigReload {
        // In-process profile edits use the same disarm/rebuild path as edits
        // made through settings.toml by another ProfileStore or manual editor.
        if let Some(change) = self.pending_change.take() {
            return match change {
                PendingChange::Reloaded => ConfigReload::Reloaded,
                PendingChange::Switched => ConfigReload::ProfileSwitched,
            };
        }
        let settings_path = self.root.join("settings.toml");
        let settings_stamp = match file_stamp(&settings_path) {
            Ok(value) => value,
            Err(error) => return ConfigReload::ReadError(error.to_string()),
        };
        // Always inspect the settings first: the active profile may have changed.
        let candidate_settings = if settings_stamp != self.settings_stamp {
            match read_settings(&settings_path) {
                Ok(value) => value,
                Err(error) => {
                    return self.reload_error(settings_stamp, self.profile_stamp, error);
                }
            }
        } else {
            self.settings.clone()
        };
        let new_id = candidate_settings.active_for(self.active_game);
        let path = match profile_path(&self.root, new_id) {
            Ok(path) => path,
            Err(error) => return self.reload_error(settings_stamp, None, error),
        };
        let profile_stamp = match file_stamp(&path) {
            Ok(value) => value,
            Err(error) => return ConfigReload::ReadError(error.to_string()),
        };
        if settings_stamp == self.settings_stamp && profile_stamp == self.profile_stamp {
            return ConfigReload::Unchanged;
        }
        if self.failed_pair == Some((settings_stamp, profile_stamp)) {
            return ConfigReload::Unchanged;
        }
        let document = match ProfileDocument::read(&path, new_id, Some(self.active_game)) {
            Ok(doc) => doc,
            Err(error) => return self.reload_error(settings_stamp, profile_stamp, error),
        };
        let changed_profile = new_id != self.settings.active_for(self.active_game);
        let changed_config = document.config() != self.current;
        self.settings = candidate_settings;
        self.current = document.config();
        self.settings_stamp = settings_stamp;
        self.profile_stamp = profile_stamp;
        self.failed_pair = None;
        if changed_profile {
            ConfigReload::ProfileSwitched
        } else if changed_config {
            ConfigReload::Reloaded
        } else {
            ConfigReload::Unchanged
        }
    }

    fn reload_error(
        &mut self,
        settings_stamp: Option<FileStamp>,
        profile_stamp: Option<FileStamp>,
        error: io::Error,
    ) -> ConfigReload {
        self.failed_pair = Some((settings_stamp, profile_stamp));
        ConfigReload::Invalid(error.to_string())
    }

    /// Directory enumeration is on-demand for the UI, never on the hot path.
    pub fn list_profiles(&self, game: Game) -> io::Result<Vec<ProfileInfo>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.root.join("profiles"))? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "toml") {
                continue;
            }
            let id = path
                .file_stem()
                .and_then(|id| id.to_str())
                .ok_or_else(|| invalid(format!("invalid profile filename: {}", path.display())))?;
            let document = ProfileDocument::read(&path, id, None)?;
            if document.game()? == game {
                entries.push(document.profile);
            }
        }
        entries.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
        Ok(entries)
    }

    /// Returns an owned, editable snapshot. No background filesystem watcher
    /// is created and no lock is held across UI interactions.
    pub fn profile_for_edit(&self, game: Game, id: &str) -> io::Result<ProfileEdit> {
        let path = profile_path(&self.root, id)?;
        let document = ProfileDocument::read(&path, id, Some(game))?;
        let revision = file_stamp(&path)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "profile disappeared during editing",
            )
        })?;
        let config = document.config();
        Ok(ProfileEdit {
            info: document.profile,
            config,
            revision,
        })
    }

    /// Validates before saving, rejecting concurrent external changes since
    /// the UI opened the profile. Caller can reload/reconcile and retry.
    pub fn save_edited(&mut self, edit: ProfileEdit) -> io::Result<()> {
        let game = edit
            .info
            .game
            .parse()
            .map_err(|_| invalid("invalid game"))?;
        let path = profile_path(&self.root, &edit.info.id)?;
        if file_stamp(&path)? != Some(edit.revision) {
            return Err(invalid("profile changed externally; reload before saving"));
        }
        edit.config
            .poe1
            .skills
            .validate_toggle_key(self.settings.automation_key)?;
        self.save_profile(game, &edit.info.id, edit.config)
    }

    /// Duplicates all per-game automation settings into a new stable-ID file.
    pub fn create_profile(
        &self,
        game: Game,
        name: &str,
        duplicate_from: Option<&str>,
    ) -> io::Result<ProfileInfo> {
        valid_name(name)?;
        let source = match duplicate_from {
            Some(id) => read_profile(&self.root, id, Some(game))?.config(),
            None => Config::default(),
        };
        let mut source = source;
        if game == Game::Poe2 {
            source.poe1 = Poe1Config::default();
        }
        let prefix = format!("{}-{}", game_id(game), slug(name));
        for index in 0..1000 {
            let id = if index == 0 {
                prefix.clone()
            } else {
                format!("{prefix}-{index}")
            };
            let path = profile_path(&self.root, &id)?;
            let doc = ProfileDocument::new(&id, name, game, source.clone())?;
            match write_new(&path, &encode(&path, &doc)?) {
                Ok(()) => return Ok(doc.profile),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(invalid("cannot allocate a unique profile ID"))
    }

    pub fn rename_profile(&mut self, game: Game, id: &str, name: &str) -> io::Result<()> {
        valid_name(name)?;
        let path = profile_path(&self.root, id)?;
        let mut document = ProfileDocument::read(&path, id, Some(game))?;
        document.profile.name = name.trim().into();
        atomic_replace(&path, &encode(&path, &document)?)?;
        self.update_observed_stamp(game, id)?;
        Ok(())
    }

    pub fn save_profile(&mut self, game: Game, id: &str, config: Config) -> io::Result<()> {
        let path = profile_path(&self.root, id)?;
        let old = ProfileDocument::read(&path, id, Some(game))?;
        let new = ProfileDocument::new(id, &old.profile.name, game, config)?;
        atomic_replace(&path, &encode(&path, &new)?)?;
        if game == self.active_game && self.settings.active_for(game) == id {
            self.current = new.config();
            if self.pending_change != Some(PendingChange::Switched) {
                self.pending_change = Some(PendingChange::Reloaded);
            }
        }
        self.update_observed_stamp(game, id)?;
        Ok(())
    }

    /// Update the durable active ID. The engine reads this at its next
    /// config poll, disarms, and then applies the validated profile.
    pub fn activate_profile(&mut self, game: Game, id: &str) -> io::Result<()> {
        let new = read_profile(&self.root, id, Some(game))?;
        let path = self.root.join("settings.toml");
        self.reject_external_settings_change()?;
        new.poe1
            .skills
            .validate_toggle_key(self.settings.automation_key)?;
        let changed = self.settings.active_for(game) != id;
        let mut settings = self.settings.clone();
        settings.set_active(game, id);
        atomic_replace(&path, &encode(&path, &settings)?)?;
        self.settings = settings;
        self.settings_stamp = file_stamp(&path)?;
        if self.active_game == game {
            self.current = new.config();
            self.profile_stamp = file_stamp(&profile_path(&self.root, id)?)?;
            if changed {
                self.pending_change = Some(PendingChange::Switched);
            }
        }
        self.failed_pair = None;
        Ok(())
    }

    pub fn save_ui_settings(&mut self, ui: UiSettings) -> io::Result<()> {
        let mut settings = self.settings.clone();
        settings.ui = ui;
        self.save_settings(settings)
    }

    pub fn save_automation_key(&mut self, key: AutomationKey) -> io::Result<()> {
        let profile = read_profile(&self.root, &self.settings.active_poe1, Some(Game::Poe1))?;
        profile.poe1.skills.validate_toggle_key(key)?;
        let mut settings = self.settings.clone();
        settings.automation_key = key;
        self.save_settings(settings)
    }

    fn save_settings(&mut self, settings: AppSettings) -> io::Result<()> {
        let path = self.root.join("settings.toml");
        self.reject_external_settings_change()?;
        atomic_replace(&path, &encode(&path, &settings)?)?;
        self.settings = settings;
        self.settings_stamp = file_stamp(&path)?;
        self.failed_pair = None;
        Ok(())
    }

    pub fn delete_profile(&self, game: Game, id: &str) -> io::Result<()> {
        valid_id(id)?;
        if self.settings.active_for(game) == id {
            return Err(invalid(
                "switch away from the active profile before deleting it",
            ));
        }
        let list = self.list_profiles(game)?;
        if list.len() <= 1 {
            return Err(invalid("cannot delete the last profile for a game"));
        }
        let path = profile_path(&self.root, id)?;
        ProfileDocument::read(&path, id, Some(game))?;
        fs::remove_file(path)
    }

    fn reject_external_settings_change(&self) -> io::Result<()> {
        if file_stamp(&self.root.join("settings.toml"))? != self.settings_stamp {
            Err(invalid(
                "settings.toml changed externally; reload before saving",
            ))
        } else {
            Ok(())
        }
    }

    fn update_observed_stamp(&mut self, game: Game, id: &str) -> io::Result<()> {
        if game == self.active_game && self.settings.active_for(game) == id {
            self.profile_stamp = file_stamp(&profile_path(&self.root, id)?)?;
        }
        self.failed_pair = None;
        Ok(())
    }
}

fn game_id(game: Game) -> &'static str {
    match game {
        Game::Poe1 => "poe1",
        Game::Poe2 => "poe2",
    }
}

fn version(value: u32) -> io::Result<()> {
    if value == SCHEMA_VERSION {
        Ok(())
    } else {
        Err(invalid(format!(
            "unsupported profile schema version {value} (expected {SCHEMA_VERSION})"
        )))
    }
}

fn valid_id(id: &str) -> io::Result<()> {
    if id.is_empty()
        || id.len() > 64
        || !id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-' || c == b'_')
        || !id.as_bytes()[0].is_ascii_alphanumeric()
    {
        return Err(invalid(
            "profile ID must be 1..=64 lowercase ASCII letters, digits, _ or -",
        ));
    }
    Ok(())
}

fn valid_name(name: &str) -> io::Result<()> {
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(invalid("profile name must be 1..=80 printable characters"));
    }
    Ok(())
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    let mut previous_dash = false;
    for c in name.bytes() {
        if out.len() >= 42 {
            break;
        }
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase() as char);
            previous_dash = false;
        } else if !previous_dash && !out.is_empty() {
            out.push('-');
            previous_dash = true;
        }
    }
    let trimmed = out.trim_end_matches('-');
    if trimmed.is_empty() {
        "profile".to_string()
    } else {
        trimmed.to_string()
    }
}

fn profile_path(root: &Path, id: &str) -> io::Result<PathBuf> {
    valid_id(id)?;
    Ok(root.join("profiles").join(format!("{id}.toml")))
}

fn read_settings(path: &Path) -> io::Result<AppSettings> {
    let settings: AppSettings = parse(path, &fs::read_to_string(path)?)?;
    settings.validate()?;
    Ok(settings)
}

fn read_profile(root: &Path, id: &str, game: Option<Game>) -> io::Result<ProfileDocument> {
    ProfileDocument::read(&profile_path(root, id)?, id, game)
}

fn parse<T: for<'de> Deserialize<'de>>(path: &Path, text: &str) -> io::Result<T> {
    toml::from_str(text)
        .map_err(|error| invalid(format!("invalid TOML in {}: {error}", path.display())))
}

fn encode<T: Serialize>(path: &Path, data: &T) -> io::Result<String> {
    toml::to_string_pretty(data)
        .map_err(|error| invalid(format!("cannot serialize {}: {error}", path.display())))
}

fn ensure_initial_profile(root: &Path, desired: &ProfileDocument) -> io::Result<()> {
    let path = profile_path(root, &desired.profile.id)?;
    let encoded = encode(&path, desired)?;
    match write_new(&path, &encoded) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            let existing =
                ProfileDocument::read(&path, &desired.profile.id, Some(desired.game()?))?;
            if existing == *desired {
                Ok(())
            } else {
                Err(invalid(format!(
                    "existing {} does not match interrupted migration; refusing overwrite",
                    path.display()
                )))
            }
        }
        Err(error) => Err(error),
    }
}

fn write_new(path: &Path, content: &str) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    if let Err(error) = file
        .write_all(content.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(path);
        return Err(error);
    }
    Ok(())
}

fn atomic_replace(path: &Path, content: &str) -> io::Result<()> {
    let id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let extension = format!("tmp-{}-{id}", std::process::id());
    let temporary = path.with_extension(extension);
    write_new(&temporary, content)?;
    let replaced = replace_file(&temporary, path);
    if replaced.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    replaced
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let to: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: Both NUL-terminated path buffers remain alive for the call.
    if unsafe {
        MoveFileExW(
            from.as_ptr(),
            to.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_CASE: AtomicU64 = AtomicU64::new(0);
    struct Temporary(PathBuf);
    impl Temporary {
        fn new() -> Self {
            let id = NEXT_CASE.fetch_add(1, Ordering::Relaxed);
            let root =
                std::env::temp_dir().join(format!("poe-profile-test-{}-{id}", std::process::id()));
            fs::create_dir(&root).expect("test directory");
            Self(root)
        }
        fn root(&self) -> PathBuf {
            self.0.clone()
        }
    }
    impl Drop for Temporary {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn window_settings_preserve_app_preferences_without_reloading_automation() {
        let temp = Temporary::new();
        let legacy = temp.0.join("missing.toml");
        let (mut editor, _) = ProfileStore::open(temp.root(), &legacy).unwrap();
        editor
            .save_automation_key(AutomationKey::parse("F8").unwrap())
            .unwrap();
        let (mut helper, _) = ProfileStore::open(temp.root(), &legacy).unwrap();
        let profile = helper.active_profile_id().to_owned();
        let key = editor.settings().automation_key;
        let geometry = WindowGeometry {
            x: Some(-387),
            y: Some(214),
            width: 1170,
            height: 828,
        };
        let mut settings = editor.settings().ui.clone();
        settings.window = Some(geometry);
        settings.skills.molten_shell_expanded = false;
        settings.skills.spirit_offering_expanded = false;
        editor.save_ui_settings(settings).unwrap();
        assert!(matches!(helper.check_for_reload(), ConfigReload::Unchanged));
        assert_eq!(helper.settings().ui.window, Some(geometry));
        assert!(!helper.settings().ui.skills.molten_shell_expanded);
        assert!(helper.settings().ui.skills.blood_rage_expanded);
        assert_eq!(helper.settings().automation_key, key);
        assert_eq!(helper.active_profile_id(), profile);

        let mut settings = editor.settings().ui.clone();
        settings.hide_to_tray_on_close = false;
        editor.save_ui_settings(settings).unwrap();
        let (again, _) = ProfileStore::open(temp.root(), &legacy).unwrap();
        assert_eq!(again.settings().ui.window, Some(geometry));
        assert_eq!(again.settings().ui.skills, editor.settings().ui.skills);
        assert!(!again.settings().ui.hide_to_tray_on_close);
        assert!(matches!(helper.check_for_reload(), ConfigReload::Unchanged));
    }

    #[test]
    fn defaults_create_two_distinct_game_profiles() {
        let temp = Temporary::new();
        let (mut store, status) =
            ProfileStore::open(temp.root(), &temp.0.join("missing.toml")).unwrap();
        assert_eq!(status, ProfileStartup::Created);
        assert_eq!(store.active_profile_id(), POE2_DEFAULT);
        assert_eq!(store.current(), &Config::default());
        assert_eq!(store.list_profiles(Game::Poe1).unwrap().len(), 1);
        assert_eq!(store.list_profiles(Game::Poe2).unwrap().len(), 1);
        store.select_game(Game::Poe1).unwrap();
        assert_eq!(store.active_profile_id(), POE1_DEFAULT);
        let (again, status) =
            ProfileStore::open(temp.root(), &temp.0.join("missing.toml")).unwrap();
        assert_eq!(status, ProfileStartup::Loaded);
        assert_eq!(again.current(), &Config::default());
    }

    #[test]
    fn automation_key_reloads_across_profiles_and_reserves_its_base_key() {
        let temp = Temporary::new();
        let legacy = temp.0.join("missing.toml");
        let (mut editor, _) = ProfileStore::open(temp.root(), &legacy).unwrap();
        let (mut helper, _) = ProfileStore::open(temp.root(), &legacy).unwrap();
        assert_eq!(helper.settings().automation_key.label(), "F11");
        assert!(!fs::read_to_string(temp.0.join("settings.toml"))
            .unwrap()
            .contains("automation_key"));

        let enabled_key = AutomationKey::parse("f8").unwrap();
        editor.save_automation_key(enabled_key).unwrap();
        helper.check_for_reload();
        assert_eq!(helper.settings().automation_key, enabled_key);
        helper.select_game(Game::Poe1).unwrap();
        assert_eq!(helper.settings().automation_key, enabled_key);
        assert!(AutomationKey::parse("Ctrl+F8").is_err());

        let mut edit = editor.profile_for_edit(Game::Poe1, POE1_DEFAULT).unwrap();
        edit.config.poe1.skills.molten_shell.enabled = true;
        edit.config.poe1.skills.molten_shell.key = "Ctrl+F8".into();
        assert!(editor.save_edited(edit.clone()).is_err());
        edit.config.poe1.skills.molten_shell.key = "F11".into();
        editor.save_edited(edit).unwrap();
        assert!(editor
            .save_automation_key(AutomationKey::default())
            .is_err());
        assert_eq!(editor.settings().automation_key, enabled_key);
    }

    #[test]
    fn migrates_legacy_without_modifying_or_deleting_original() {
        let temp = Temporary::new();
        let legacy = temp.0.join("config.toml");
        let source = concat!(
            "[general]\nenable_in_hideout = true\n",
            "[health]\nenabled = false\nthreshold_percent = 57.5\n",
            "[mana]\nenabled = true\nthreshold_percent = 42\n",
            "[poe1.utility]\ngranite = true\n",
            "[poe1.skills.spirit_offering]\nenabled = true\nkey = \"E\"\n",
        );
        fs::write(&legacy, source).unwrap();
        let (mut store, status) = ProfileStore::open(temp.root(), &legacy).unwrap();
        assert_eq!(status, ProfileStartup::Migrated);
        store.select_game(Game::Poe1).unwrap();
        assert!(store.current().general.enable_in_hideout);
        assert!(store.current().poe1.utility.granite);
        assert!(store.current().poe1.skills.spirit_offering.enabled);
        assert_eq!(store.current().health.threshold_percent, 57.5);
        store.select_game(Game::Poe2).unwrap();
        assert_eq!(store.current().poe1, Poe1Config::default());
        assert_eq!(store.current().mana.threshold_percent, 42.0);
        assert_eq!(fs::read_to_string(&legacy).unwrap(), source);
    }

    #[test]
    fn independent_profiles_duplicate_save_switch_and_hot_reload() {
        let temp = Temporary::new();
        let (mut store, _) = ProfileStore::open(temp.root(), &temp.0.join("missing")).unwrap();
        store.select_game(Game::Poe1).unwrap();
        let copied = store
            .create_profile(Game::Poe1, "Summoner / Maps", Some(POE1_DEFAULT))
            .unwrap();
        assert!(copied.id.starts_with("poe1-summoner-maps"));
        let mut edit = store.current().clone();
        edit.poe1.skills.spirit_offering.enabled = true;
        edit.poe1.utility.granite = true;
        store
            .save_profile(Game::Poe1, &copied.id, edit.clone())
            .unwrap();
        store.activate_profile(Game::Poe1, &copied.id).unwrap();
        assert_eq!(store.current(), &edit);
        assert_eq!(store.active_profile_id(), copied.id);
        store
            .rename_profile(Game::Poe1, &copied.id, "Mapping")
            .unwrap();
        assert_eq!(store.list_profiles(Game::Poe1).unwrap().len(), 2);
        let (mut second, _) = ProfileStore::open(temp.root(), &temp.0.join("missing")).unwrap();
        second.select_game(Game::Poe1).unwrap();
        assert_eq!(second.current(), &edit);
        store.activate_profile(Game::Poe1, POE1_DEFAULT).unwrap();
        assert!(matches!(
            second.check_for_reload(),
            ConfigReload::ProfileSwitched
        ));
        assert!(!second.current().poe1.skills.spirit_offering.enabled);
        store.delete_profile(Game::Poe1, &copied.id).unwrap();
        assert_eq!(store.list_profiles(Game::Poe1).unwrap().len(), 1);
    }

    #[test]
    fn in_process_editor_notifies_engine_and_detects_stale_snapshot() {
        let temp = Temporary::new();
        let (mut store, _) = ProfileStore::open(temp.root(), &temp.0.join("missing")).unwrap();
        store.select_game(Game::Poe1).unwrap();
        let stale = store.profile_for_edit(Game::Poe1, POE1_DEFAULT).unwrap();
        let mut changed = stale.clone();
        changed.config.health.threshold_percent = 47.0;
        store.save_edited(changed).unwrap();
        assert!(matches!(store.check_for_reload(), ConfigReload::Reloaded));
        assert_eq!(store.current().health.threshold_percent, 47.0);
        assert!(store.save_edited(stale).is_err());
        let added = store
            .create_profile(Game::Poe1, "Mapping", Some(POE1_DEFAULT))
            .unwrap();
        store.activate_profile(Game::Poe1, &added.id).unwrap();
        assert!(matches!(
            store.check_for_reload(),
            ConfigReload::ProfileSwitched
        ));
        assert!(matches!(store.check_for_reload(), ConfigReload::Unchanged));
    }

    #[test]
    fn invalid_external_edits_fail_closed_and_recover() {
        let temp = Temporary::new();
        let (mut store, _) = ProfileStore::open(temp.root(), &temp.0.join("missing")).unwrap();
        store.select_game(Game::Poe1).unwrap();
        let original = store.current().clone();
        let path = profile_path(&temp.0, POE1_DEFAULT).unwrap();
        fs::write(&path, "[profile]\nname = \"broken\"\n").unwrap();
        assert!(matches!(store.check_for_reload(), ConfigReload::Invalid(_)));
        assert!(matches!(store.check_for_reload(), ConfigReload::Unchanged));
        assert_eq!(store.current(), &original);
        let valid =
            ProfileDocument::new(POE1_DEFAULT, "Default PoE1", Game::Poe1, original.clone())
                .unwrap();
        atomic_replace(&path, &encode(&path, &valid).unwrap()).unwrap();
        let mut changed = valid;
        changed.health.threshold_percent = 72.0;
        atomic_replace(&path, &encode(&path, &changed).unwrap()).unwrap();
        assert!(matches!(store.check_for_reload(), ConfigReload::Reloaded));
        assert_eq!(store.current().health.threshold_percent, 72.0);
        let settings = temp.0.join("settings.toml");
        fs::write(&settings, "schema_version = 999\nactive_poe1 = \"poe1-default\"\nactive_poe2 = \"poe2-default\"\n").unwrap();
        assert!(matches!(store.check_for_reload(), ConfigReload::Invalid(_)));
        assert_eq!(store.current().health.threshold_percent, 72.0);
    }

    #[test]
    fn invalid_profiles_cannot_be_activated_or_saved() {
        let temp = Temporary::new();
        let (mut store, _) = ProfileStore::open(temp.root(), &temp.0.join("missing")).unwrap();
        assert!(store.activate_profile(Game::Poe1, "../outside").is_err());
        assert!(store.activate_profile(Game::Poe1, POE2_DEFAULT).is_err());
        assert!(store.delete_profile(Game::Poe2, POE2_DEFAULT).is_err());
        let id = store.create_profile(Game::Poe1, "One", None).unwrap().id;
        let mut invalid_config = Config::default();
        invalid_config.health.threshold_percent = 0.0;
        assert!(store.save_profile(Game::Poe1, &id, invalid_config).is_err());
        assert!(store
            .save_profile(Game::Poe2, &id, Config::default())
            .is_err());
        let mut wrong = Config::default();
        wrong.poe1.utility.granite = true;
        assert!(ProfileDocument::new("poe2-x", "x", Game::Poe2, wrong).is_err());
        assert!(valid_id("x\\y").is_err());
        assert!(valid_id("..").is_err());
        assert!(valid_name("   ").is_err());
    }

    #[test]
    fn serializes_nearby_and_unique_flask_maps_without_losing_keys() {
        let mut config = Config::default();
        config.poe1.utility.granite = true;
        config.poe1.unique.writhing_jar = true;
        config.poe1.nearby.insert(
            crate::poe1::types::FlaskType::Granite,
            crate::poe1::config::NearbyRule::default(),
        );
        config.poe1.unique_conditions.insert(
            crate::poe1::types::UniqueFlask::WrithingJar,
            crate::poe1::config::UniqueConditions {
                nearby: Some(crate::poe1::config::NearbyRule::default()),
            },
        );
        let document =
            ProfileDocument::new("poe1-test", "Test", Game::Poe1, config.clone()).unwrap();
        let temp = Temporary::new();
        let path = temp.0.join("poe1-test.toml");
        fs::write(&path, encode(&path, &document).unwrap()).unwrap();
        let decoded = ProfileDocument::read(&path, "poe1-test", Some(Game::Poe1)).unwrap();
        assert_eq!(decoded.config(), config);
    }
}
