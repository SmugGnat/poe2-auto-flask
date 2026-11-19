#![cfg_attr(windows, windows_subsystem = "windows")]
// Slint runs on the main thread. The shared Rust backend owns automation.

use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime};

use slint::winit_030::{winit, EventResult, WinitWindowAccessor};
use slint::{
    CloseRequestResponse, ComponentHandle, ModelRc, PhysicalPosition, PhysicalSize, SharedString,
};

use poe2_auto_flask::launch::LaunchOptions;
use poe2_auto_flask::runtime::{BackendStatus, Connection};
use poe2_auto_flask::{config, game, input, poe1, profiles};
#[cfg(windows)]
mod backend_host;
#[cfg(windows)]
mod desktop;

use config::Config;
use game::Game;
use input::AutomationKey;
use poe1::config::{skill_binding, MinimumRarity, NearbyRule, UniqueConditions};
use poe1::types::{FlaskType, UniqueFlask, UTILITY_TYPES};
use profiles::{ProfileEdit, ProfileInfo, ProfileStore, SkillSections, WindowGeometry};

slint::include_modules!();

fn window_position(geometry: WindowGeometry) -> Option<PhysicalPosition> {
    Some(PhysicalPosition::new(geometry.x?, geometry.y?))
}

fn fit_window_geometry(
    geometry: WindowGeometry,
    origin: PhysicalPosition,
    size: PhysicalSize,
) -> WindowGeometry {
    let width = geometry.width.min(size.width);
    let height = geometry.height.min(size.height);
    let fit = |position: Option<i32>, origin: i32, available: u32| {
        let end = i64::from(origin) + i64::from(available);
        let position = position.map_or(i64::from(origin) + i64::from(available / 2), i64::from);
        position.clamp(i64::from(origin), end) as i32
    };
    let position = window_position(geometry);
    WindowGeometry {
        x: Some(fit(position.map(|p| p.x), origin.x, size.width - width)),
        y: Some(fit(position.map(|p| p.y), origin.y, size.height - height)),
        width,
        height,
    }
}

fn valid_window_geometry(geometry: &WindowGeometry) -> bool {
    (1..=10000).contains(&geometry.width) && (1..=10000).contains(&geometry.height)
}

fn load_window_geometry(store: &mut ProfileStore) -> io::Result<Option<WindowGeometry>> {
    let legacy = store.root().join("window-state.toml");
    let mut geometry = store.settings().ui.window.filter(valid_window_geometry);
    if geometry.is_none() {
        geometry = fs::read_to_string(&legacy)
            .ok()
            .and_then(|text| toml::from_str::<WindowGeometry>(&text).ok())
            .filter(valid_window_geometry);
        if let Some(geometry) = geometry {
            let mut settings = store.settings().ui.clone();
            settings.window = Some(geometry);
            store.save_ui_settings(settings)?;
        }
    }
    // A successful settings save precedes removing the old file. If saving
    // fails, the only copy of the user's placement remains intact.
    if geometry.is_some() {
        let _ = fs::remove_file(legacy);
    }
    Ok(geometry)
}

fn restore_window_geometry(ui: &AppWindow, saved: Option<WindowGeometry>) {
    let window = ui.window();
    let Some(Some((geometry, can_position))) = window.with_winit_window(|native| {
        let initial_size = native.inner_size();
        let geometry = saved.unwrap_or(WindowGeometry {
            x: None,
            y: None,
            width: initial_size.width,
            height: initial_size.height,
        });
        let can_position = native.outer_position().is_ok();
        let monitor = if can_position {
            window_position(geometry).and_then(|position| {
                native.available_monitors().find(|monitor| {
                    let origin = monitor.position();
                    let size = monitor.size();
                    let x = i64::from(position.x) - i64::from(origin.x);
                    let y = i64::from(position.y) - i64::from(origin.y);
                    (0..i64::from(size.width)).contains(&x)
                        && (0..i64::from(size.height)).contains(&y)
                })
            })
        } else {
            None
        };
        let monitor = monitor
            .or_else(|| native.current_monitor())
            .or_else(|| native.primary_monitor())
            .or_else(|| native.available_monitors().next())?;
        let origin = monitor.position();
        let size = monitor.size();
        Some((
            fit_window_geometry(
                geometry,
                PhysicalPosition::new(origin.x, origin.y),
                PhysicalSize::new(size.width, size.height),
            ),
            can_position,
        ))
    }) else {
        return;
    };

    let size = PhysicalSize::new(geometry.width, geometry.height);
    if window.size() != size {
        window.set_size(size);
    }
    if can_position {
        if let Some(position) = window_position(geometry) {
            window.set_position(position);
        }
    }
}

#[cfg(windows)]
fn round_window_corners(window: &slint::Window, last_frame: &Cell<Option<(u32, u32, i32)>>) {
    use windows_sys::Win32::Graphics::Gdi::{CreateRoundRectRgn, DeleteObject, SetWindowRgn};
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    window.with_winit_window(|native| {
        if native.is_minimized() == Some(true) {
            return;
        }
        let Ok(handle) = native.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        let hwnd = handle.hwnd.get() as windows_sys::Win32::Foundation::HWND;
        let size = native.outer_size();
        let diameter = if native.is_maximized() {
            0
        } else {
            (20.0 * native.scale_factor()).round() as i32
        };
        let frame = (size.width, size.height, diameter);
        if last_frame.get() == Some(frame) || size.width == 0 || size.height == 0 {
            return;
        }
        last_frame.set(Some(frame));
        // The region shapes the opaque Win32 window. A successful call takes ownership.
        unsafe {
            let region = if diameter == 0 {
                std::ptr::null_mut()
            } else {
                CreateRoundRectRgn(
                    0,
                    0,
                    size.width as i32,
                    size.height as i32,
                    diameter,
                    diameter,
                )
            };
            // Slint paints the resize; avoid a separate synchronous native
            // erase before that frame, particularly during first mapping.
            if (diameter > 0 && region.is_null()) || SetWindowRgn(hwnd, region, 0) == 0 {
                last_frame.set(None);
                if !region.is_null() {
                    DeleteObject(region);
                }
            }
        }
    });
}

fn bind_window_events(ui: &AppWindow, presentation: Rc<Presentation>) {
    use winit::event::WindowEvent;
    use winit::keyboard::Key;
    use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

    let weak = ui.as_weak();
    let visibility_ui = ui.as_weak();
    let mut modifiers = winit::keyboard::ModifiersState::empty();
    #[cfg(windows)]
    let last_frame = Cell::new(None);
    ui.window().on_winit_window_event(move |_window, event| {
        if matches!(
            event,
            WindowEvent::Occluded(_) | WindowEvent::Resized(_) | WindowEvent::Focused(true)
        ) {
            if let Some(ui) = visibility_ui.upgrade() {
                let occluded = matches!(event, WindowEvent::Occluded(true));
                presentation.set_active(
                    &ui,
                    ui.window().is_visible() && !occluded && !_window.is_minimized(),
                );
            }
        }
        if let Some(ui) = visibility_ui.upgrade() {
            match event {
                WindowEvent::Focused(true) | WindowEvent::Occluded(false) => {
                    #[cfg(windows)]
                    desktop::repaint(ui.window());
                    ui.set_dirty_settings(presentation.editor.borrow().is_dirty(&ui));
                }
                WindowEvent::Focused(false) => {
                    let weak = ui.as_weak();
                    let activity = presentation.clone();
                    // WM_ACTIVATE can arrive before Wine's foreground HWND
                    // changes. Check once after the handoff has settled.
                    slint::Timer::single_shot(Duration::from_millis(60), move || {
                        if let Some(ui) = weak.upgrade() {
                            activity.game_focused(&ui);
                        }
                    });
                }
                WindowEvent::MouseInput {
                    state: winit::event::ElementState::Pressed,
                    ..
                }
                | WindowEvent::KeyboardInput { .. }
                    if !ui.get_presentation_active() && !_window.is_minimized() =>
                {
                    presentation.set_active(&ui, true);
                }
                _ => {}
            }
        }
        match event {
            WindowEvent::ModifiersChanged(changed) => modifiers = changed.state(),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic: false,
                ..
            } if event.state.is_pressed() => {
                let Some(ui) = weak.upgrade() else {
                    return EventResult::Propagate;
                };
                let fields = ui.global::<FieldInputState>();
                let replacement = if fields.get_skill_focused() {
                    // Use the base key for chords such as Shift+1, which normally types '!'.
                    match event.key_without_modifiers() {
                        Key::Character(base) => Some(SharedString::from(base.as_str())),
                        _ => None,
                    }
                } else if fields.get_numeric_focused()
                    && modifiers.is_empty()
                    && !matches!(&event.logical_key, Key::Character(_))
                {
                    numpad_digit(event.physical_key).map(SharedString::from)
                } else {
                    None
                };
                if let Some(text) = replacement {
                    ui.window()
                        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text });
                    return EventResult::PreventDefault;
                }
            }
            #[cfg(windows)]
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                round_window_corners(_window, &last_frame);
            }
            _ => {}
        }
        EventResult::Propagate
    });
}

fn numpad_digit(key: winit::keyboard::PhysicalKey) -> Option<char> {
    use winit::keyboard::{KeyCode, PhysicalKey};
    match key {
        PhysicalKey::Code(KeyCode::Numpad0) => Some('0'),
        PhysicalKey::Code(KeyCode::Numpad1) => Some('1'),
        PhysicalKey::Code(KeyCode::Numpad2) => Some('2'),
        PhysicalKey::Code(KeyCode::Numpad3) => Some('3'),
        PhysicalKey::Code(KeyCode::Numpad4) => Some('4'),
        PhysicalKey::Code(KeyCode::Numpad5) => Some('5'),
        PhysicalKey::Code(KeyCode::Numpad6) => Some('6'),
        PhysicalKey::Code(KeyCode::Numpad7) => Some('7'),
        PhysicalKey::Code(KeyCode::Numpad8) => Some('8'),
        PhysicalKey::Code(KeyCode::Numpad9) => Some('9'),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct DiskState {
    settings: Option<(u64, SystemTime)>,
    profiles: Vec<(String, u64, SystemTime)>,
    profile_dir: Option<(u64, SystemTime)>,
}

fn disk_file_state(path: &Path) -> Option<(u64, SystemTime)> {
    let metadata = fs::metadata(path).ok()?;
    Some((metadata.len(), metadata.modified().ok()?))
}

struct Editor {
    backend_status: Option<BackendStatus>,
    store: ProfileStore,
    game: Game,
    profiles: Vec<ProfileInfo>,
    edit: ProfileEdit,
    saved_config: Config,
    utility: FlaskType,
    observed_disk: DiskState,
    last_window_geometry: Option<WindowGeometry>,
}

impl Editor {
    fn open(root: PathBuf, legacy: &Path) -> io::Result<Self> {
        let (mut store, _) = ProfileStore::open(root, legacy)?;
        let last_window_geometry = load_window_geometry(&mut store)?;
        // run() applies explicit/Steam selection next; backend detection can
        // update this fallback once the editor has no unsaved draft.
        let game = Game::Poe1;
        store.select_game(game)?;
        let profiles = store.list_profiles(game)?;
        let id = store.active_profile_id().to_string();
        let edit = store.profile_for_edit(game, &id)?;
        let saved_config = edit.config.clone();
        let mut editor = Self {
            backend_status: None,
            store,
            game,
            profiles,
            edit,
            saved_config,
            utility: UTILITY_TYPES[0],
            observed_disk: DiskState {
                settings: None,
                profiles: Vec::new(),
                profile_dir: None,
            },
            last_window_geometry,
        };
        editor.observed_disk = editor.current_disk_state();
        Ok(editor)
    }

    fn update_backend_state(&mut self, ui: &AppWindow) {
        if !ui.window().is_visible()
            || ui
                .window()
                .with_winit_window(|w| w.is_minimized() == Some(true))
                .unwrap_or(false)
        {
            return;
        }
        let Some(state) = self.backend_status.clone() else {
            return;
        };
        if let Some(game) = state.game {
            if game != self.game && self.ensure_clean(ui).is_ok() {
                let result = self.store.select_game(game).and_then(|()| {
                    self.game = game;
                    self.reload_selected()
                });
                match result {
                    Ok(()) => self.update_ui(ui),
                    Err(error) => status_error(ui, format!("Game profile: {error}")),
                }
            }
        }
        let waiting = state.reason.starts_with("WAITING");
        let connected = state.connection == Connection::Connected;
        let game_name = match state.game.unwrap_or(self.game) {
            Game::Poe1 => "Path of Exile",
            Game::Poe2 => "Path of Exile 2",
        };
        ui.set_backend_game_name(game_name.into());
        ui.set_backend_connected(connected);
        ui.set_backend_failed(
            state.connection == Connection::Failed || state.config_error.is_some(),
        );
        ui.set_backend_enabled(state.connection == Connection::Connected && state.enabled);
        ui.set_choose_game_open(state.connection == Connection::ChoosingGame);
        let reason = state
            .reason
            .strip_prefix("PAUSED (")
            .and_then(|reason| reason.strip_suffix(')'))
            .map(|reason| format!("Paused · {reason}"))
            .unwrap_or_else(|| state.reason.clone());
        let mut connection_detail = match state.connection {
            Connection::Starting => "Starting helper".to_owned(),
            Connection::Waiting => format!("Waiting for {game_name}"),
            Connection::ChoosingGame => "Choose the running game".to_owned(),
            Connection::Validating => "Waiting for game state".to_owned(),
            Connection::Failed => format!("Helper error\n{}", state.reason),
            Connection::Exited => "Game disconnected".to_owned(),
            Connection::Connected => format!("Connected to {game_name}"),
        };
        let error_label = if let Some(error) = &state.config_error {
            connection_detail = format!("Settings could not be loaded\n{error}");
            "Settings could not be loaded"
        } else if state.connection == Connection::Failed {
            "Helper error"
        } else if state.game.is_some_and(|game| game != self.game) {
            "Finish edits to switch game"
        } else {
            ""
        };
        ui.set_backend_error(error_label.into());
        ui.set_backend_detail(status_tooltip(&connection_detail));
        let automation_detail = if state.connection == Connection::Failed {
            "Automation disabled\nThe helper is stopped.".to_owned()
        } else if state.connection != Connection::Connected {
            format!("Automation disabled\n{connection_detail}")
        } else if state.enabled && (state.reason.starts_with("PAUSED") || waiting) {
            format!("Automation enabled\n{reason}")
        } else {
            format!(
                "Automation {}\nPress {} while the game is focused to {}.",
                if state.enabled { "enabled" } else { "disabled" },
                ui.get_automation_key(),
                if state.enabled { "disable" } else { "enable" }
            )
        };
        ui.set_backend_automation_detail(status_tooltip(&automation_detail));
    }

    fn update_ui(&mut self, ui: &AppWindow) {
        ui.set_app_version(poe2_auto_flask::VERSION.into());
        ui.set_profile_rename_open(false);
        ui.set_profile_create_open(false);
        ui.set_profile_create_duplicate(false);
        ui.set_profile_delete_open(false);
        ui.set_profile_reset_open(false);
        self.observed_disk = self.current_disk_state();
        ui.set_game_index(if self.game == Game::Poe1 { 0 } else { 1 });
        if self.game == Game::Poe2 && ui.get_page() == 1 {
            ui.set_page(0);
        }
        ui.set_profile_names(profile_labels(&self.profiles));
        ui.set_profile_index(
            self.profiles
                .iter()
                .position(|p| p.id == self.edit.info.id)
                .unwrap_or(0) as i32,
        );
        ui.set_profile_action_index(ui.get_profile_index());
        ui.set_profile_create_source_index(ui.get_profile_index());
        ui.set_profile_action_name(self.edit.info.name.as_str().into());
        ui.set_profile_id(self.edit.info.id.as_str().into());
        ui.set_profile_display(self.edit.info.name.as_str().into());
        ui.set_config_location(self.store.root().display().to_string().into());
        ui.set_rename_to(self.edit.info.name.as_str().into());
        ui.set_utility_names(ModelRc::from(
            UTILITY_TYPES.map(|kind| SharedString::from(kind.name())),
        ));
        ui.set_utility_index(
            UTILITY_TYPES
                .iter()
                .position(|x| *x == self.utility)
                .unwrap_or(0) as i32,
        );
        self.update_values(ui);
        ui.set_dirty_settings(false);
    }

    fn update_values(&self, ui: &AppWindow) {
        let c = &self.edit.config;
        ui.set_automation_key(self.store.settings().automation_key.label().into());
        ui.set_hideout(c.general.enable_in_hideout);
        ui.set_life_enabled(c.health.enabled);
        ui.set_life_threshold(c.health.threshold_percent.to_string().into());
        ui.set_mana_enabled(c.mana.enabled);
        ui.set_mana_threshold(c.mana.threshold_percent.to_string().into());
        ui.set_hide_on_close(self.store.settings().ui.hide_to_tray_on_close);
        let sections = self.store.settings().ui.skills;
        ui.set_molten_expanded(sections.molten_shell_expanded);
        ui.set_blood_expanded(sections.blood_rage_expanded);
        ui.set_offering_expanded(sections.spirit_offering_expanded);
        ui.set_quicksilver_ms((c.poe1.quicksilver_min_run_ms as i32).to_string().into());
        ui.set_writhing_enabled(c.poe1.unique.writhing_jar);
        let writhing = c
            .poe1
            .unique_conditions
            .get(&UniqueFlask::WrithingJar)
            .and_then(|entry| entry.nearby);
        let writhing_rule = writhing.unwrap_or_default();
        ui.set_writhing_nearby(writhing.is_some_and(|rule| rule.enabled));
        ui.set_writhing_radius(f32::from(writhing_rule.radius));
        ui.set_writhing_count((writhing_rule.minimum_count as i32).to_string().into());
        ui.set_writhing_rarity(rarity_index(writhing_rule.minimum_rarity));
        let ms = &c.poe1.skills.molten_shell;
        ui.set_molten_enabled(ms.enabled);
        ui.set_molten_key(ms.key.as_str().into());
        ui.set_molten_life((ms.threshold_percent as i32).to_string().into());
        let blood = &c.poe1.skills.blood_rage;
        ui.set_blood_enabled(blood.enabled);
        ui.set_blood_key(blood.key.as_str().into());
        ui.set_blood_life((blood.min_life_percent as i32).to_string().into());
        ui.set_blood_nearby(blood.require_nearby_monsters);
        ui.set_blood_radius(f32::from(blood.nearby_radius));
        ui.set_blood_count((blood.nearby_minimum_count as i32).to_string().into());
        let offering = &c.poe1.skills.spirit_offering;
        ui.set_offering_enabled(offering.enabled);
        ui.set_offering_key(offering.key.as_str().into());
        ui.set_offering_radius(f32::from(offering.nearby_radius));
        ui.set_offering_count((offering.nearby_minimum_count as i32).to_string().into());
        self.update_utility(ui);
    }

    fn update_utility(&self, ui: &AppWindow) {
        let config = &self.edit.config.poe1;
        ui.set_utility_enabled_entries(ModelRc::from(
            UTILITY_TYPES.map(|kind| config.utility.enabled(kind)),
        ));
        ui.set_utility_enabled(config.utility.enabled(self.utility));
        let nearby = config.nearby.get(&self.utility).copied();
        let rule = nearby.unwrap_or_default();
        ui.set_utility_nearby(nearby.is_some_and(|rule| rule.enabled));
        ui.set_utility_radius(f32::from(rule.radius));
        ui.set_utility_count((rule.minimum_count as i32).to_string().into());
        ui.set_utility_rarity(rarity_index(rule.minimum_rarity));
    }

    fn collect_utility(&mut self, ui: &AppWindow) -> io::Result<()> {
        // Commit the old flask's draft before the selector changes. Work on
        // a clone so invalid inputs cannot partially mutate the profile.
        let mut proposed = self.edit.config.clone();
        apply_selected_utility(&mut proposed, self.utility, ui)?;
        self.edit.config = proposed;
        Ok(())
    }

    fn change_utility(&mut self, ui: &AppWindow, index: usize) -> io::Result<bool> {
        let next = *UTILITY_TYPES
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Unknown utility flask"))?;
        if next == self.utility {
            return Ok(false);
        }

        self.collect_utility(ui)?;
        self.utility = next;
        // The Slint selection is read-only until this method accepts it.
        // Only now reveal the chosen flask's own enabled/nearby/radius/rule.
        ui.set_utility_index(index as i32);
        self.update_utility(ui);
        Ok(true)
    }

    fn snapshot(&self, ui: &AppWindow) -> io::Result<Config> {
        let mut c = self.edit.config.clone();
        c.health.enabled = ui.get_life_enabled();
        if c.health.enabled {
            c.health.threshold_percent = parse_percent(&ui.get_life_threshold(), "Life")?;
        }
        c.mana.enabled = ui.get_mana_enabled();
        if c.mana.enabled {
            c.mana.threshold_percent = parse_percent(&ui.get_mana_threshold(), "Mana")?;
        }
        c.general.enable_in_hideout = ui.get_hideout();
        if self.game == Game::Poe1 {
            apply_selected_utility(&mut c, self.utility, ui)?;
            if c.poe1.utility.quicksilver {
                let running =
                    parse_integer(&ui.get_quicksilver_ms(), "Quicksilver running", 0, 5000)?;
                if running != 0 && running < 100 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Quicksilver running must be 0 or 100–5000 ms",
                    ));
                }
                c.poe1.quicksilver_min_run_ms = running as u64;
            }
            c.poe1.unique.writhing_jar = ui.get_writhing_enabled();
            let gate = ui.get_writhing_nearby();
            if gate && c.poe1.unique.writhing_jar {
                c.poe1.unique_conditions.insert(
                    UniqueFlask::WrithingJar,
                    UniqueConditions {
                        nearby: Some(NearbyRule {
                            enabled: gate,
                            radius: ui.get_writhing_radius() as u16,
                            minimum_count: parse_integer(
                                &ui.get_writhing_count(),
                                "Writhing Jar enemies",
                                1,
                                512,
                            )? as u16,
                            minimum_rarity: rarity(ui.get_writhing_rarity()),
                        }),
                    },
                );
            } else if let Some(rule) = c
                .poe1
                .unique_conditions
                .get_mut(&UniqueFlask::WrithingJar)
                .and_then(|entry| entry.nearby.as_mut())
            {
                rule.enabled = gate;
            }
            let ms = &mut c.poe1.skills.molten_shell;
            ms.enabled = ui.get_molten_enabled();
            if ms.enabled {
                update_skill_key(&mut ms.key, &ui.get_molten_key());
                ms.threshold_percent =
                    parse_integer(&ui.get_molten_life(), "Molten Shell Life", 1, 99)? as u8;
            }

            let blood = &mut c.poe1.skills.blood_rage;
            blood.enabled = ui.get_blood_enabled();
            if blood.enabled {
                update_skill_key(&mut blood.key, &ui.get_blood_key());
                blood.min_life_percent =
                    parse_integer(&ui.get_blood_life(), "Blood Rage Life", 1, 100)? as u8;
                blood.require_nearby_monsters = ui.get_blood_nearby();
                if blood.require_nearby_monsters {
                    blood.nearby_radius = ui.get_blood_radius() as u16;
                    blood.nearby_minimum_count =
                        parse_integer(&ui.get_blood_count(), "Blood Rage enemies", 1, 512)? as u16;
                }
            }

            let offering = &mut c.poe1.skills.spirit_offering;
            offering.enabled = ui.get_offering_enabled();
            if offering.enabled {
                update_skill_key(&mut offering.key, &ui.get_offering_key());
                offering.nearby_radius = ui.get_offering_radius() as u16;
                offering.nearby_minimum_count =
                    parse_integer(&ui.get_offering_count(), "Spirit Offering corpses", 1, 512)?
                        as u16;
            }
        }
        c.validate()?;
        Ok(c)
    }

    fn is_dirty(&self, ui: &AppWindow) -> bool {
        self.snapshot(ui).map_or(true, |c| c != self.saved_config)
    }

    fn ensure_clean(&self, ui: &AppWindow) -> io::Result<()> {
        if self.snapshot(ui)? != self.saved_config {
            Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Unsaved settings. Use Save or Discard before switching profiles.",
            ))
        } else {
            Ok(())
        }
    }

    /// The single profile-switch path used by the UI and regression tests.
    fn switch_profile(&mut self, index: usize) -> io::Result<bool> {
        let selected = self
            .profiles
            .get(index)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Profile no longer exists"))?
            .id
            .clone();
        if selected == self.edit.info.id {
            return Ok(false);
        }
        self.store.activate_profile(self.game, &selected)?;
        self.reload_selected()?;
        Ok(true)
    }

    // A profile change must always validate the currently edited UI state.
    // Read the selected profile and rebind *every* displayed setting together.
    fn change_profile(&mut self, ui: &AppWindow, index: usize) -> io::Result<bool> {
        self.ensure_clean(ui)?;
        let switched = self.switch_profile(index)?;
        if switched {
            self.update_ui(ui);
        }
        Ok(switched)
    }

    fn reset_profile(&mut self, ui: &AppWindow, index: usize) -> io::Result<()> {
        let target = self.profiles.get(index).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Selected profile no longer exists",
            )
        })?;
        let is_active = target.id == self.edit.info.id;
        let mut reset = if is_active {
            self.edit.clone()
        } else {
            self.store.profile_for_edit(self.game, &target.id)?
        };
        reset.config = Config::default();
        self.store.save_edited(reset)?;
        if is_active {
            self.edit = self.store.profile_for_edit(self.game, &target.id)?;
            self.saved_config = self.edit.config.clone();
            self.update_values(ui);
            ui.set_dirty_settings(false);
        }
        self.observed_disk = self.current_disk_state();
        ui.set_profile_reset_open(false);
        Ok(())
    }

    fn reload_selected(&mut self) -> io::Result<()> {
        self.profiles = self.store.list_profiles(self.game)?;
        self.edit = self
            .store
            .profile_for_edit(self.game, self.store.active_profile_id())?;
        self.saved_config = self.edit.config.clone();
        self.utility = UTILITY_TYPES[0];
        self.observed_disk = self.current_disk_state();
        Ok(())
    }

    fn save_window_geometry(&mut self, ui: &AppWindow) -> io::Result<()> {
        let window = ui.window();
        if !window.is_visible() {
            return Ok(());
        }
        let Some(Some(geometry)) = window.with_winit_window(|native| {
            if native.is_minimized() == Some(true) || native.is_maximized() {
                return None;
            }
            let size = native.inner_size();
            if size.width == 0 || size.height == 0 {
                return None;
            }
            // Wayland leaves placement to the compositor. Keep any coordinates
            // saved by a previous Windows/X11 session and update the size.
            let position = native
                .outer_position()
                .ok()
                .map(|position| PhysicalPosition::new(position.x, position.y))
                .or_else(|| self.last_window_geometry.and_then(window_position));
            Some(WindowGeometry {
                x: position.map(|position| position.x),
                y: position.map(|position| position.y),
                width: size.width,
                height: size.height,
            })
        }) else {
            return Ok(());
        };
        // Leave external settings untouched until the editor can reconcile
        // them. Profile drafts must not be discarded just to save placement.
        if disk_file_state(&self.store.root().join("settings.toml")) != self.observed_disk.settings
        {
            return Ok(());
        }
        self.persist_window_geometry(geometry)
    }

    fn persist_window_geometry(&mut self, geometry: WindowGeometry) -> io::Result<()> {
        if self.last_window_geometry == Some(geometry) {
            return Ok(());
        }
        let mut settings = self.store.settings().ui.clone();
        settings.window = Some(geometry);
        self.store.save_ui_settings(settings)?;
        self.last_window_geometry = Some(geometry);
        // Do not acknowledge unrelated profile edits that happened on disk.
        self.observed_disk.settings = disk_file_state(&self.store.root().join("settings.toml"));
        Ok(())
    }

    fn current_disk_state(&self) -> DiskState {
        let root = self.store.root();
        let profiles_dir = root.join("profiles");
        let mut profiles = Vec::new();
        if let Ok(entries) = fs::read_dir(&profiles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().is_none_or(|ext| ext != "toml") {
                    continue;
                }
                let Some((len, modified)) = disk_file_state(&path) else {
                    continue;
                };
                profiles.push((
                    entry.file_name().to_string_lossy().into_owned(),
                    len,
                    modified,
                ));
            }
        }
        profiles.sort_unstable_by(|a, b| a.0.cmp(&b.0));
        DiskState {
            settings: disk_file_state(&root.join("settings.toml")),
            profiles,
            profile_dir: disk_file_state(&profiles_dir),
        }
    }

    // Poll metadata only. Parse TOML only when an external file actually
    // changes, and never overwrite unsaved in-memory settings or a profile
    // name currently being edited. There is intentionally no manual Reload.
    fn refresh_if_external(&mut self, ui: &AppWindow) -> io::Result<bool> {
        if self.current_disk_state() == self.observed_disk {
            return Ok(false);
        }
        if ui.get_profile_rename_open()
            || ui.get_profile_create_open()
            || ui.get_profile_delete_open()
            || ui.get_profile_reset_open()
            || self.ensure_clean(ui).is_err()
        {
            return Ok(false);
        }
        let root = self.store.root().to_path_buf();
        let (mut fresh, _) = ProfileStore::open(root.clone(), &root.join("config.toml"))?;
        fresh.select_game(self.game)?;
        self.store = fresh;
        self.reload_selected()?;
        self.update_ui(ui);
        Ok(true)
    }
}

// Keep the spelling in an existing profile when the field has not changed.
fn update_skill_key(saved: &mut String, text: &str) {
    let key = text.trim();
    if saved.trim() != key {
        *saved = key.to_owned();
    }
}

fn format_skill_key(
    text: SharedString,
    shift: bool,
    control: bool,
    alt: bool,
    meta: bool,
) -> SharedString {
    use slint::platform::Key;

    let modifier = match (shift, control, alt, meta) {
        (false, false, false, false) => "",
        (true, false, false, false) => "Shift+",
        (false, true, false, false) => "Ctrl+",
        (false, false, true, false) => "Alt+",
        _ => return SharedString::default(),
    };
    let mut chars = text.chars();
    let (Some(key), None) = (chars.next(), chars.next()) else {
        return SharedString::default();
    };
    let key = if key.is_ascii_alphanumeric() {
        key.to_ascii_uppercase().to_string()
    } else if (char::from(Key::F1)..=char::from(Key::F12)).contains(&key) {
        format!("F{}", u32::from(key) - u32::from(char::from(Key::F1)) + 1)
    } else {
        return SharedString::default();
    };
    skill_binding(&format!("{modifier}{key}"), "Keybind").map_or_else(
        |_| SharedString::default(),
        |binding| binding.label().into(),
    )
}

fn bind_field_input(ui: &AppWindow) {
    let fields = ui.global::<FieldInputState>();
    fields.on_format_binding(format_skill_key);
    fields.on_valid_number_text(|text, decimal| {
        let mut separator_seen = false;
        text.bytes().all(|byte| {
            if byte.is_ascii_digit() {
                true
            } else if decimal && !separator_seen && matches!(byte, b'.' | b',') {
                separator_seen = true;
                true
            } else {
                false
            }
        })
    });
}

fn apply_selected_utility(c: &mut Config, utility: FlaskType, ui: &AppWindow) -> io::Result<()> {
    let flask_enabled = ui.get_utility_enabled();
    set_utility(&mut c.poe1.utility, utility, flask_enabled);
    let nearby = ui.get_utility_nearby();
    if flask_enabled && nearby {
        c.poe1.nearby.insert(
            utility,
            NearbyRule {
                enabled: true,
                radius: ui.get_utility_radius() as u16,
                minimum_count: parse_integer(&ui.get_utility_count(), "Utility enemies", 1, 512)?
                    as u16,
                minimum_rarity: rarity(ui.get_utility_rarity()),
            },
        );
    } else if let Some(rule) = c.poe1.nearby.get_mut(&utility) {
        rule.enabled = nearby;
    }
    Ok(())
}

fn rarity(index: i32) -> MinimumRarity {
    match index {
        1 => MinimumRarity::Magic,
        2 => MinimumRarity::Rare,
        3 => MinimumRarity::Unique,
        _ => MinimumRarity::Normal,
    }
}
fn rarity_index(value: MinimumRarity) -> i32 {
    match value {
        MinimumRarity::Normal => 0,
        MinimumRarity::Magic => 1,
        MinimumRarity::Rare => 2,
        MinimumRarity::Unique => 3,
    }
}
fn parse_percent(text: &str, label: &str) -> io::Result<f32> {
    let value: f32 = text.trim().replace(',', ".").parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} threshold must be a number between 1 and 99"),
        )
    })?;
    if !value.is_finite() || !(1.0..=99.0).contains(&value) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{label} threshold must be between 1 and 99"),
        ));
    }
    Ok(value)
}

fn parse_integer(text: &str, name: &str, min: u32, max: u32) -> io::Result<u32> {
    let value: u32 = text.trim().parse().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be a whole number ({min}–{max})"),
        )
    })?;
    if !(min..=max).contains(&value) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{name} must be {min}–{max}"),
        ));
    }
    Ok(value)
}

fn set_utility(c: &mut poe1::config::Poe1UtilityConfig, kind: FlaskType, enabled: bool) {
    match kind {
        FlaskType::Quicksilver => c.quicksilver = enabled,
        FlaskType::Granite => c.granite = enabled,
        FlaskType::Quartz => c.quartz = enabled,
        FlaskType::Diamond => c.diamond = enabled,
        FlaskType::Jade => c.jade = enabled,
        FlaskType::Silver => c.silver = enabled,
        FlaskType::Basalt => c.basalt = enabled,
        FlaskType::Stibnite => c.stibnite = enabled,
        FlaskType::Sulphur => c.sulphur = enabled,
        FlaskType::Ruby => c.ruby = enabled,
        FlaskType::Sapphire => c.sapphire = enabled,
        FlaskType::Topaz => c.topaz = enabled,
        FlaskType::Amethyst => c.amethyst = enabled,
        FlaskType::Bismuth => c.bismuth = enabled,
        FlaskType::Aquamarine => c.aquamarine = enabled,
        FlaskType::Gold => c.gold = enabled,
        FlaskType::Corundum => c.corundum = enabled,
        FlaskType::Iron => c.iron = enabled,
        _ => {}
    }
}

fn profile_labels(profiles: &[ProfileInfo]) -> ModelRc<SharedString> {
    ModelRc::from(
        profiles
            .iter()
            .map(|p| p.name.as_str().into())
            .collect::<Vec<SharedString>>()
            .as_slice(),
    )
}

fn status(ui: &AppWindow, value: impl AsRef<str>) {
    ui.set_error_line("".into());
    ui.set_status_line(value.as_ref().into());
}

// Keep standard tooltips compact enough to fit beside the sidebar status.
fn status_tooltip(text: &str) -> slint::StyledText {
    let mut lines = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            if !line.is_empty() && line.chars().count() + word.chars().count() + 1 > 56 {
                lines.push(std::mem::take(&mut line));
            }
            if !line.is_empty() {
                line.push(' ');
            }
            for character in word.chars() {
                if line.chars().count() == 56 {
                    lines.push(std::mem::take(&mut line));
                }
                line.push(character);
            }
        }
        lines.push(line);
    }
    if lines.len() > 3 {
        lines.truncate(3);
        lines.last_mut().unwrap().push('…');
    }
    slint::StyledText::from_plain_text(&lines.join("\n"))
}

fn status_error(ui: &AppWindow, value: impl AsRef<str>) {
    ui.set_status_line("".into());
    ui.set_error_line(value.as_ref().into());
}

fn linux_config_dir() -> Option<PathBuf> {
    let base = env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("poe2-auto-flask"))
}

#[cfg(windows)]
fn wine_host_path(path: &Path) -> Option<PathBuf> {
    let path = path.to_string_lossy();
    path.starts_with('/')
        .then(|| PathBuf::from(format!("Z:{}", path.replace('/', "\\"))))
}

fn profile_root(explicit: Option<PathBuf>, #[cfg(windows)] legacy: &Path) -> io::Result<PathBuf> {
    // A single root is shared by both games. The AppImage launcher maps
    // its XDG directory through Wine's Z: drive for the Windows helper.
    let requested = explicit.or_else(|| {
        env::var_os("POE2_AUTO_FLASK_PROFILE_DIR")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    });
    if let Some(path) = requested {
        #[cfg(windows)]
        if let Some(wine_path) = wine_host_path(&path) {
            return Ok(wine_path);
        }
        return Ok(path);
    }
    #[cfg(windows)]
    {
        // Prefer a visible Unix XDG path whenever Wine inherits the Linux
        // environment, even if the launcher was started outside Steam.
        if let Some(path) = linux_config_dir().and_then(|path| wine_host_path(&path)) {
            return Ok(path);
        }
        if env::var_os("STEAM_COMPAT_DATA_PATH").is_some()
            || env::var_os("WINEPREFIX").is_some()
            || env::var_os("WINELOADERNOEXEC").is_some()
        {
            // Never silently create a second store in the prefix's APPDATA.
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot locate the Linux XDG config directory. Set HOME/XDG_CONFIG_HOME or pass --config-dir.",
            ));
        }
        Ok(env::var_os("APPDATA")
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .map(|p| p.join("poe2-auto-flask"))
            .unwrap_or_else(|| legacy.parent().unwrap_or(Path::new(".")).to_path_buf()))
    }
    #[cfg(not(windows))]
    {
        linux_config_dir().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Cannot locate the XDG config directory. Set HOME/XDG_CONFIG_HOME or pass --config-dir.",
            )
        })
    }
}

fn bind_profile_editor(window: &AppWindow, editor: &Rc<RefCell<Editor>>) {
    bind_field_input(window);
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_save_skill_sections(move || {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let result = (|| -> io::Result<()> {
                let mut editor = state.borrow_mut();
                let sections = SkillSections {
                    molten_shell_expanded: ui.get_molten_expanded(),
                    blood_rage_expanded: ui.get_blood_expanded(),
                    spirit_offering_expanded: ui.get_offering_expanded(),
                };
                if editor.store.settings().ui.skills == sections {
                    return Ok(());
                }
                let mut settings = editor.store.settings().ui.clone();
                settings.skills = sections;
                editor.store.save_ui_settings(settings)?;
                editor.observed_disk.settings =
                    disk_file_state(&editor.store.root().join("settings.toml"));
                Ok(())
            })();
            if let Err(error) = result {
                status_error(&ui, format!("Skill layout: {error}"));
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_profile_draft_changed(move || {
            if let Some(ui) = weak.upgrade() {
                ui.set_dirty_settings(state.borrow().is_dirty(&ui));
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_reset_profile(move || {
            if let Some(ui) = weak.upgrade() {
                let index = ui.get_profile_action_index() as usize;
                match state.borrow_mut().reset_profile(&ui, index) {
                    Ok(()) => status(&ui, "Profile reset to defaults"),
                    Err(e) => status_error(&ui, format!("Reset failed: {e}")),
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_save_automation_key(move |name| {
            let Some(ui) = weak.upgrade() else {
                return;
            };
            let result = (|| -> io::Result<AutomationKey> {
                let key = AutomationKey::parse(&name)?;
                let mut editor = state.borrow_mut();
                if key != editor.store.settings().automation_key {
                    editor.store.save_automation_key(key)?;
                    editor.observed_disk = editor.current_disk_state();
                }
                Ok(key)
            })();
            match result {
                Ok(key) => {
                    ui.set_automation_key(key.label().into());
                    status(
                        &ui,
                        format!("Saved · Press {} in game to enable automation", key.label()),
                    );
                }
                Err(error) => status_error(&ui, error.to_string()),
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_save_settings(move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> io::Result<()> {
                    let mut editor = state.borrow_mut();
                    let config = editor.snapshot(&ui)?;
                    let mut edit = editor.edit.clone();
                    edit.config = config;
                    editor.store.save_edited(edit)?;
                    editor.edit = editor
                        .store
                        .profile_for_edit(editor.game, &editor.edit.info.id)?;
                    editor.saved_config = editor.edit.config.clone();
                    editor.observed_disk = editor.current_disk_state();
                    editor.update_values(&ui);
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        ui.set_dirty_settings(false);
                        status(&ui, "Saved");
                    }
                    Err(e) => status_error(&ui, format!("Save failed: {e}")),
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_discard_settings(move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> io::Result<()> {
                    let mut editor = state.borrow_mut();
                    let current = editor.edit.info.id.clone();
                    editor.edit = editor.store.profile_for_edit(editor.game, &current)?;
                    editor.saved_config = editor.edit.config.clone();
                    editor.update_values(&ui);
                    Ok(())
                })();
                match result {
                    Ok(()) => {
                        ui.set_dirty_settings(false);
                        status(&ui, "Changes discarded");
                    }
                    Err(e) => status_error(&ui, format!("Discard failed: {e}")),
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_change_profile(move |index| {
            if let Some(ui) = weak.upgrade() {
                let result = state.borrow_mut().change_profile(&ui, index as usize);
                match result {
                    Ok(true) => status(
                        &ui,
                        format!(
                            "Profile loaded · Press {} in game to enable automation",
                            ui.get_automation_key()
                        ),
                    ),
                    Ok(false) => status(&ui, ""),
                    Err(e) => {
                        let editor = state.borrow();
                        ui.set_profile_index(
                            editor
                                .profiles
                                .iter()
                                .position(|p| p.id == editor.edit.info.id)
                                .unwrap_or(0) as i32,
                        );
                        status_error(&ui, e.to_string());
                    }
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_change_utility(move |index| {
            if let Some(ui) = weak.upgrade() {
                let mut editor = state.borrow_mut();
                match editor.change_utility(&ui, index as usize) {
                    Ok(_) => {
                        ui.set_dirty_settings(editor.is_dirty(&ui));
                        status(&ui, "");
                    }
                    Err(error) => {
                        ui.set_utility_index(
                            UTILITY_TYPES
                                .iter()
                                .position(|kind| *kind == editor.utility)
                                .unwrap_or(0) as i32,
                        );
                        status_error(&ui, format!("Invalid utility setting: {error}"));
                    }
                }
            }
        });
    }
}

struct Presentation {
    refresh: slint::Timer,
    editor: Rc<RefCell<Editor>>,
}

impl Presentation {
    fn game_focused(&self, ui: &AppWindow) {
        #[cfg(windows)]
        let pid = self
            .editor
            .borrow()
            .backend_status
            .as_ref()
            .and_then(|s| s.process_id);
        #[cfg(windows)]
        if let Some(pid) = pid {
            if desktop::lower_below_fullscreen_game(ui.window(), pid) {
                self.set_active(ui, false);
            }
        }
    }

    fn set_active(&self, ui: &AppWindow, active: bool) {
        if ui.get_presentation_active() == active {
            return;
        }
        ui.set_presentation_active(active);
        ui.invoke_presentation_changed(active);
        if active {
            ui.invoke_refresh_backend();
            self.refresh_editor(ui);
            self.refresh.restart();
        } else {
            self.refresh.stop();
            ui.global::<StatusTooltipState>().set_visible(false);
        }
    }

    fn refresh_editor(&self, ui: &AppWindow) {
        let mut editor = self.editor.borrow_mut();
        match editor.refresh_if_external(ui) {
            Ok(true) => status(ui, "Profiles updated"),
            Ok(false) => {}
            Err(error) => status_error(ui, format!("External change: {error}")),
        }
        ui.set_dirty_settings(editor.is_dirty(ui));
    }

    fn show(&self, ui: &AppWindow) {
        ui.window().set_minimized(false);
        let _ = ui.show();
        self.set_active(ui, true);
        #[cfg(windows)]
        desktop::repaint(ui.window());
    }
}

fn start_editor_refresh(window: &AppWindow, editor: &Rc<RefCell<Editor>>) -> Rc<Presentation> {
    // No periodic presentation work runs while the window is hidden.
    let disk_refresh = slint::Timer::default();
    {
        let state = editor.clone();
        let weak = window.as_weak();
        disk_refresh.start(
            slint::TimerMode::Repeated,
            Duration::from_secs(2),
            move || {
                let Some(ui) = weak.upgrade() else {
                    return;
                };
                if !ui.get_presentation_active() {
                    return;
                }
                {
                    // Reuse the existing 2 s event-loop tick. Polling geometry
                    // requires no extra worker, and writes only on movement
                    // or resize (also persisted immediately on close).
                    if ui.window().is_visible() {
                        if let Err(error) = state.borrow_mut().save_window_geometry(&ui) {
                            status_error(&ui, format!("Window placement: {error}"));
                        }
                    }
                    match state.borrow_mut().refresh_if_external(&ui) {
                        Ok(true) => status(&ui, "Profiles updated"),
                        Ok(false) => {}
                        Err(e) => status_error(&ui, format!("External change: {e}")),
                    }
                }
            },
        );
    }
    Rc::new(Presentation {
        refresh: disk_refresh,
        editor: editor.clone(),
    })
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut options = LaunchOptions::parse(env::args().skip(1))?;
    if options.game.is_none() {
        options.game = Game::from_steam_context()?;
    }
    if options.help || options.debug {
        #[cfg(windows)]
        desktop::attach_console();
        if options.help {
            println!("{}", LaunchOptions::HELP);
            return Ok(());
        }
        #[cfg(windows)]
        return poe2_auto_flask::backend::run(
            options.backend(),
            poe2_auto_flask::runtime::BackendControl::standalone(),
        )
        .map_err(Into::into);
        #[cfg(not(windows))]
        return Err("Diagnostics require the Windows application under Proton.".into());
    }
    let renderer = if options.femtovg {
        "femtovg"
    } else {
        "software"
    };
    // Tested safe under Proton/KWin. Never request transparent native alpha.
    slint::BackendSelector::new()
        .backend_name("winit".into())
        .renderer_name(renderer.into())
        .with_winit_window_attributes_hook(|a| a.with_transparent(false).with_decorations(false))
        .select()?;
    let legacy = env::current_exe()?
        .parent()
        .unwrap_or(Path::new("."))
        .join("config.toml");
    let root = profile_root(
        options.config_dir.take(),
        #[cfg(windows)]
        &legacy,
    )?;
    fs::create_dir_all(&root)?;
    #[cfg(windows)]
    let Some(mut instance) = desktop::Instance::acquire(&root)?
    else {
        return Ok(());
    };
    options.config_dir = Some(root.clone());
    // XDG's existing config.toml is the migration source when the editor
    // shares the Linux AppImage directory; do not create profiles in APPDATA.
    let migration = if root.join("config.toml").is_file() {
        root.join("config.toml")
    } else {
        legacy
    };
    let editor = Rc::new(RefCell::new(Editor::open(root, &migration)?));
    if let Some(game) = options.game {
        let mut editor = editor.borrow_mut();
        editor.store.select_game(game)?;
        editor.game = game;
        editor.reload_selected()?;
    }
    let window = AppWindow::new()?;
    let tray = AppTray::new()?;
    editor.borrow_mut().update_ui(&window);
    let saved_geometry = editor.borrow().last_window_geometry;
    if let Some(geometry) = saved_geometry {
        window
            .window()
            .set_size(PhysicalSize::new(geometry.width, geometry.height));
    }
    status(&window, "");

    bind_profile_editor(&window, &editor);
    let presentation = start_editor_refresh(&window, &editor);
    {
        let activity = presentation.clone();
        let weak = window.as_weak();
        window.on_game_window_focused(move || {
            if let Some(ui) = weak.upgrade() {
                activity.game_focused(&ui);
            }
        });
    }
    for duplicate in [false, true] {
        let state = editor.clone();
        let weak = window.as_weak();
        let action = move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> io::Result<()> {
                    let mut editor = state.borrow_mut();
                    editor.ensure_clean(&ui)?;
                    let name = ui.get_new_profile_name().trim().to_string();
                    let from = if duplicate {
                        Some(
                            editor
                                .profiles
                                .get(ui.get_profile_create_source_index() as usize)
                                .ok_or_else(|| {
                                    io::Error::new(
                                        io::ErrorKind::InvalidInput,
                                        "Source profile no longer exists",
                                    )
                                })?
                                .id
                                .clone(),
                        )
                    } else {
                        None
                    };
                    let new = editor
                        .store
                        .create_profile(editor.game, &name, from.as_deref())?;
                    // Duplicating a different profile must not switch away
                    // from the currently loaded automation profile.
                    if !duplicate {
                        let game = editor.game;
                        editor.store.activate_profile(game, &new.id)?;
                    }
                    editor.reload_selected()?;
                    editor.update_ui(&ui);
                    ui.set_new_profile_name("".into());
                    Ok(())
                })();
                match result {
                    Ok(()) => status(&ui, "Profile created"),
                    Err(e) => status_error(&ui, format!("Profile operation failed: {e}")),
                }
            }
        };
        if duplicate {
            window.on_duplicate_profile(action);
        } else {
            window.on_create_profile(action);
        }
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_rename_profile(move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> io::Result<()> {
                    let mut editor = state.borrow_mut();
                    editor.ensure_clean(&ui)?;
                    let id = editor
                        .profiles
                        .get(ui.get_profile_action_index() as usize)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "Selected profile no longer exists",
                            )
                        })?
                        .id
                        .clone();
                    let game = editor.game;
                    editor
                        .store
                        .rename_profile(game, &id, &ui.get_rename_to())?;
                    editor.reload_selected()?;
                    editor.update_ui(&ui);
                    Ok(())
                })();
                match result {
                    Ok(()) => status(&ui, "Profile renamed"),
                    Err(e) => status_error(&ui, format!("Rename failed: {e}")),
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_delete_profile(move || {
            if let Some(ui) = weak.upgrade() {
                let result = (|| -> io::Result<()> {
                    let mut editor = state.borrow_mut();
                    editor.ensure_clean(&ui)?;
                    let target = editor
                        .profiles
                        .get(ui.get_profile_action_index() as usize)
                        .ok_or_else(|| {
                            io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "Selected profile no longer exists",
                            )
                        })?
                        .id
                        .clone();
                    if editor.profiles.len() < 2 {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            "Cannot delete the last profile",
                        ));
                    }
                    let game = editor.game;
                    if target == editor.edit.info.id {
                        // Switch only if deleting the currently loaded profile.
                        let replacement = editor
                            .profiles
                            .iter()
                            .find(|p| p.id != target)
                            .ok_or_else(|| {
                                io::Error::new(
                                    io::ErrorKind::InvalidInput,
                                    "No replacement profile available",
                                )
                            })?
                            .id
                            .clone();
                        editor.store.activate_profile(game, &replacement)?;
                    }
                    editor.store.delete_profile(game, &target)?;
                    editor.reload_selected()?;
                    editor.update_ui(&ui);
                    Ok(())
                })();
                match result {
                    Ok(()) => status(&ui, "Profile deleted"),
                    Err(e) => status_error(&ui, format!("Delete failed: {e}")),
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_save_global(move || {
            if let Some(ui) = weak.upgrade() {
                let previous = state.borrow().store.settings().ui.hide_to_tray_on_close;
                let result = (|| -> io::Result<()> {
                    let mut editor = state.borrow_mut();
                    let mut settings = editor.store.settings().ui.clone();
                    settings.hide_to_tray_on_close = ui.get_hide_on_close();
                    editor.store.save_ui_settings(settings)?;
                    editor.observed_disk = editor.current_disk_state();
                    Ok(())
                })();
                match result {
                    Ok(()) => status(&ui, "Saved"),
                    Err(e) => {
                        ui.set_hide_on_close(previous);
                        status(&ui, format!("Save failed: {e}"));
                    }
                }
            }
        });
    }

    {
        let state = editor.clone();
        let weak = window.as_weak();
        let activity = presentation.clone();
        window.on_minimize_window(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = state.borrow_mut().save_window_geometry(&ui);
                activity.set_active(&ui, false);
                ui.window().set_minimized(true);
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        let activity = presentation.clone();
        window.on_close_window(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = state.borrow_mut().save_window_geometry(&ui);
                if state.borrow().store.settings().ui.hide_to_tray_on_close {
                    activity.set_active(&ui, false);
                    let _ = ui.hide();
                } else {
                    let _ = slint::quit_event_loop();
                }
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        let activity = presentation.clone();
        window.window().on_close_requested(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = state.borrow_mut().save_window_geometry(&ui);
                activity.set_active(&ui, false);
            }
            if !state.borrow().store.settings().ui.hide_to_tray_on_close {
                let _ = slint::quit_event_loop();
            }
            CloseRequestResponse::HideWindow
        });
    }
    {
        let weak = window.as_weak();
        let activity = presentation.clone();
        tray.on_restore_window(move || {
            if let Some(ui) = weak.upgrade() {
                activity.show(&ui);
            }
        });
    }
    {
        let weak = window.as_weak();
        let activity = presentation.clone();
        tray.on_open_settings(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.get_page() != 3 {
                    ui.set_previous_page(ui.get_page());
                }
                ui.set_page(3);
                activity.show(&ui);
            }
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        window.on_quit_app(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = state.borrow_mut().save_window_geometry(&ui);
            }
            let _ = slint::quit_event_loop();
        });
    }
    {
        let state = editor.clone();
        let weak = window.as_weak();
        tray.on_quit_app(move || {
            if let Some(ui) = weak.upgrade() {
                if ui.window().is_visible() {
                    let _ = state.borrow_mut().save_window_geometry(&ui);
                }
            }
            let _ = slint::quit_event_loop();
        });
    }
    #[cfg(windows)]
    instance.listen(&window, presentation.clone())?;
    #[cfg(windows)]
    let mut backend = backend_host::BackendHost::start(options.backend(), &window, &editor)?;
    bind_window_events(&window, presentation);
    window.set_startup_ready(false);
    // Windows creates hidden native windows as the event loop starts. Prepare
    // placement and the opaque background before mapping the first frame.
    // Wayland requires show() to create its native window, so keep that path.
    #[cfg(not(windows))]
    window.show()?;
    let weak = window.as_weak();
    let startup_error = Rc::new(RefCell::new(None));
    let initialization_error = startup_error.clone();
    slint::spawn_local(async move {
        if let Some(ui) = weak.upgrade() {
            let initialized = (async {
                // Drop the temporary native Arc before hiding is possible.
                drop(ui.window().winit_window().await?);
                restore_window_geometry(&ui, saved_geometry);
                #[cfg(windows)]
                {
                    desktop::prepare_background(ui.window())?;
                    round_window_corners(ui.window(), &Cell::new(None));
                    ui.show()?;
                }
                Ok::<(), Box<dyn std::error::Error>>(())
            })
            .await;
            if let Err(error) = initialized {
                *initialization_error.borrow_mut() = Some(error);
                let _ = slint::quit_event_loop();
                return;
            }
            ui.set_startup_ready(true);
        }
    })?;
    tray.show()?;
    let result = slint::run_event_loop();
    #[cfg(windows)]
    backend.shutdown();
    result?;
    if let Some(error) = startup_error.borrow_mut().take() {
        return Err(error);
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        show_startup_error(&error.to_string());
        std::process::exit(1);
    }
}

#[cfg(windows)]
fn show_startup_error(message: &str) {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
    let title: Vec<u16> = OsStr::new("Auto Flask")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let body: Vec<u16> = OsStr::new(message).encode_wide().chain(Some(0)).collect();
    // SAFETY: NUL-terminated buffers stay alive during the Win32 call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            body.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR | MB_OK,
        );
    }
}

#[cfg(not(windows))]
fn show_startup_error(message: &str) {
    eprintln!("PoE Auto Flask UI: {message}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    fn init_slint_test() -> MutexGuard<'static, ()> {
        // The Windows test executable can exit early under Proton when
        // several headless Slint contexts initialize at once.
        static SLINT_TESTS: Mutex<()> = Mutex::new(());
        let guard = SLINT_TESTS
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        i_slint_backend_testing::init_no_event_loop();
        guard
    }

    #[test]
    fn threshold_rejects_non_finite_invalid_and_empty() {
        assert_eq!(parse_percent("57.5", "Life").unwrap(), 57.5);
        assert_eq!(parse_percent("57,5", "Life").unwrap(), 57.5);
        assert!(parse_percent("nan", "Life").is_err());
        assert!(parse_percent("100", "Life").is_err());
        assert!(parse_percent("", "Mana").is_err());
        assert_eq!(parse_integer("42", "count", 1, 512).unwrap(), 42);
        assert!(parse_integer("2.5", "count", 1, 512).is_err());
        assert!(parse_integer("513", "count", 1, 512).is_err());
    }

    #[test]
    fn edits_reveal_save_discard_without_a_refresh_timer_on_every_page() {
        use i_slint_backend_testing::ElementHandle;
        let _slint = init_slint_test();
        let path = env::temp_dir().join(format!("auto-flask-footer-{}", std::process::id()));
        let mut state = Editor::open(path.clone(), &path.join("missing.toml")).unwrap();
        let ui = AppWindow::new().unwrap();
        ui.window().set_size(PhysicalSize::new(710, 470));
        state.update_ui(&ui);
        let editor = Rc::new(RefCell::new(state));
        bind_profile_editor(&ui, &editor);
        ui.show().unwrap();
        // A missed compositor restore notification must not suppress edits.
        ui.set_presentation_active(false);
        ui.set_life_threshold("43".into());
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        assert!(ui.get_dirty_settings());
        for page in 0..4 {
            ui.set_page(page);
            i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
            for label in ["Save", "Discard"] {
                let button = ElementHandle::find_by_accessible_label(&ui, label)
                    .next()
                    .unwrap();
                let position = button.absolute_position();
                assert!(button.size().height > 0.0);
                assert!(position.y >= 0.0 && position.y + button.size().height <= 470.0);
            }
        }
        ui.invoke_discard_settings();
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        assert!(!ui.get_dirty_settings());
        ui.set_hideout(!ui.get_hideout());
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        assert!(ui.get_dirty_settings());
        ui.invoke_save_settings();
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        assert!(!ui.get_dirty_settings());
        ui.set_mana_threshold("".into());
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        assert!(ui.get_dirty_settings());
        fs::remove_dir_all(path).unwrap();
    }

    #[test]
    fn hidden_editor_stops_refreshing_and_restores_external_changes_without_losing_drafts() {
        let _slint = init_slint_test();
        let path = env::temp_dir().join(format!("auto-flask-visibility-{}", std::process::id()));
        let mut editor = Editor::open(path.clone(), &path.join("missing.toml")).unwrap();
        let id = editor.edit.info.id.clone();
        let game = editor.game;
        let ui = AppWindow::new().unwrap();
        editor.update_ui(&ui);
        let editor = Rc::new(RefCell::new(editor));
        ui.show().unwrap();
        let presentation = start_editor_refresh(&ui, &editor);
        presentation.set_active(&ui, false);
        ui.hide().unwrap();
        assert!(!presentation.refresh.running());

        let mut config = editor.borrow().edit.config.clone();
        config.health.threshold_percent = 47.0;
        editor
            .borrow_mut()
            .store
            .save_profile(game, &id, config.clone())
            .unwrap();
        i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(10));
        assert_eq!(ui.get_life_threshold().as_str(), "60");

        presentation.show(&ui);
        assert!(presentation.refresh.running());
        assert_eq!(ui.get_life_threshold().as_str(), "47");
        ui.set_life_threshold("51".into());
        presentation.set_active(&ui, false);
        ui.hide().unwrap();
        config.health.threshold_percent = 72.0;
        editor
            .borrow_mut()
            .store
            .save_profile(game, &id, config)
            .unwrap();
        presentation.show(&ui);
        assert_eq!(ui.get_life_threshold().as_str(), "51");
        assert!(ui.get_dirty_settings());
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn steam_context_id_maps_to_configured_game_without_claiming_connection() {
        assert_eq!(Game::from_steam_app_id("238960"), Some(Game::Poe1));
        assert_eq!(Game::from_steam_app_id("2694490"), Some(Game::Poe2));
        assert_eq!(Game::from_steam_app_id("unknown"), None);
    }

    #[test]
    fn folding_skills_preserves_enabled_rules_and_unsaved_keybinds() {
        use i_slint_backend_testing::ElementHandle;
        use slint::platform::PointerEventButton;

        let _slint = init_slint_test();
        let path = env::temp_dir().join(format!("auto-flask-skill-folding-{}", std::process::id()));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let mut state = Editor::open(path.clone(), &path.join("missing.toml")).unwrap();
        let mut config = state.saved_config.clone();
        config.poe1.skills.molten_shell.enabled = true;
        config.poe1.skills.blood_rage.enabled = true;
        config.poe1.skills.spirit_offering.enabled = true;
        state
            .store
            .save_profile(state.game, &state.edit.info.id, config)
            .unwrap();
        state.reload_selected().unwrap();
        let ui = AppWindow::new().unwrap();
        ui.window().set_size(PhysicalSize::new(780, 900));
        state.update_ui(&ui);
        let editor = Rc::new(RefCell::new(state));
        bind_profile_editor(&ui, &editor);
        ui.set_page(1);
        ui.show().unwrap();
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);

        let click = |label: &str| {
            ElementHandle::find_by_accessible_label(&ui, label)
                .next()
                .unwrap_or_else(|| panic!("Missing skill control: {label}"))
                .mock_single_click(PointerEventButton::Left);
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
        };
        let has_keybind = |name: &str| {
            ElementHandle::find_by_accessible_label(&ui, &format!("{name} keybind"))
                .next()
                .is_some()
        };
        for (index, name) in ["Molten Shell", "Blood Rage", "Spirit Offering"]
            .iter()
            .enumerate()
        {
            assert!(has_keybind(name), "Loaded enabled skills start expanded");
            let saved = editor.borrow().snapshot(&ui).unwrap();
            // The header's center is blank space, away from both label and toggle.
            click(&format!("{name} options"));
            assert!(!has_keybind(name));
            assert_eq!(editor.borrow().snapshot(&ui).unwrap(), saved);
            assert!(!ui.get_dirty_settings());

            click(&format!("{name} options"));
            assert!(has_keybind(name));
            match index {
                0 => ui.set_molten_key("Ctrl+Q".into()),
                1 => ui.set_blood_key("Shift+T".into()),
                _ => ui.set_offering_key("Alt+E".into()),
            }
            i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
            let draft = editor.borrow().snapshot(&ui).unwrap();
            assert!(ui.get_dirty_settings());
            click(&format!("{name} options"));
            assert!(!has_keybind(name));
            assert_eq!(editor.borrow().snapshot(&ui).unwrap(), draft);
            // Saving while folded must persist the edited key, not a stale value.
            ui.invoke_save_settings();
            assert_eq!(editor.borrow().saved_config, draft);
            assert!(!ui.get_dirty_settings());

            click(&format!("{name} enabled"));
            let disabled = editor.borrow().snapshot(&ui).unwrap();
            let skills = &disabled.poe1.skills;
            assert!(!match index {
                0 => skills.molten_shell.enabled,
                1 => skills.blood_rage.enabled,
                _ => skills.spirit_offering.enabled,
            });
            assert!(!has_keybind(name));
            click(&format!("{name} options"));
            assert_eq!(editor.borrow().snapshot(&ui).unwrap(), disabled);
            click(&format!("{name} enabled"));
            assert!(has_keybind(name), "Enabling must open the skill options");
            assert_eq!(editor.borrow().snapshot(&ui).unwrap(), draft);
            assert!(!ui.get_dirty_settings());
            click(&format!("{name} options"));
        }
        ui.set_page(0);
        ui.set_page(1);
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        for name in ["Molten Shell", "Blood Rage", "Spirit Offering"] {
            assert!(!has_keybind(name), "Navigation must preserve folding");
        }
        ui.hide().unwrap();
        // Reopen from disk with a fresh editor and component, like an app restart.
        let mut reopened = Editor::open(path.clone(), &path.join("missing.toml")).unwrap();
        let restored = AppWindow::new().unwrap();
        reopened.update_ui(&restored);
        let reopened = Rc::new(RefCell::new(reopened));
        bind_profile_editor(&restored, &reopened);
        restored.set_page(1);
        restored.show().unwrap();
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        for name in ["Molten Shell", "Blood Rage", "Spirit Offering"] {
            assert!(
                ElementHandle::find_by_accessible_label(&restored, &format!("{name} keybind"))
                    .next()
                    .is_none(),
                "Restart must preserve folding"
            );
        }
        assert_eq!(reopened.borrow().saved_config, editor.borrow().saved_config);
        assert!(!restored.get_dirty_settings());
        ElementHandle::find_by_accessible_label(&restored, "Molten Shell enabled")
            .next()
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
        ElementHandle::find_by_accessible_label(&restored, "Molten Shell enabled")
            .next()
            .unwrap()
            .mock_single_click(PointerEventButton::Left);
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
        assert!(
            ElementHandle::find_by_accessible_label(&restored, "Molten Shell keybind")
                .next()
                .is_some()
        );
    }

    #[test]
    fn skill_recording_and_numeric_controls_filter_keyboard_input() {
        use i_slint_backend_testing::ElementHandle;
        use slint::platform::{Key, PointerEventButton, WindowEvent};

        let _slint = init_slint_test();
        let ui = AppWindow::new().unwrap();
        ui.window().set_size(PhysicalSize::new(780, 552));
        bind_field_input(&ui);
        ui.set_page(1);
        ui.set_molten_enabled(true);
        ui.set_blood_enabled(true);
        ui.show().unwrap();

        let click = |label| {
            ElementHandle::find_by_accessible_label(&ui, label)
                .next()
                .unwrap_or_else(|| panic!("Missing UI control: {label}"))
                .mock_single_click(PointerEventButton::Left);
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(16));
        };
        let press = |text: SharedString| {
            ui.window().dispatch_event(WindowEvent::KeyPressed { text });
        };
        let release = |text: SharedString| {
            ui.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        };
        let type_key = |text: SharedString| {
            press(text.clone());
            release(text);
        };

        click("Molten Shell keybind");
        press(Key::Control.into());
        type_key("q".into());
        release(Key::Control.into());
        assert_eq!(ui.get_molten_key().as_str(), "Ctrl+Q");

        click("Blood Rage keybind");
        press(Key::Shift.into());
        type_key("b".into());
        release(Key::Shift.into());
        assert_eq!(ui.get_blood_key().as_str(), "Shift+B");
        click("Blood Rage keybind");
        type_key(Key::F13.into());
        type_key(Key::Escape.into());
        assert_eq!(ui.get_blood_key().as_str(), "Shift+B");

        ui.set_page(0);
        ui.set_utility_enabled(true);
        ui.set_quicksilver_ms("".into());
        click("Minimum Run Time value");
        assert!(ui.global::<FieldInputState>().get_numeric_focused());
        type_key("1".into());
        type_key("y".into());
        type_key("t".into());
        assert_eq!(ui.get_quicksilver_ms().as_str(), "1");
        type_key("0".into());
        type_key("0".into());
        assert_eq!(ui.get_quicksilver_ms().as_str(), "100");
    }

    #[test]
    fn dropdown_clicks_switch_saved_profiles_and_keep_per_flask_drafts() {
        use i_slint_backend_testing::ElementHandle;
        use slint::platform::{Key, PointerEventButton, WindowEvent};
        use slint::Model;

        let _slint = init_slint_test();
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "auto-flask-ui-click-test-{}-{unique}",
            std::process::id()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(path.clone());
        let mut editor = Editor::open(path.clone(), &path.join("missing.toml")).unwrap();
        editor.game = Game::Poe1;
        editor.store.select_game(Game::Poe1).unwrap();
        let first = editor.store.active_profile_id().to_string();
        let alternate = editor
            .store
            .create_profile(Game::Poe1, "Second profile", None)
            .unwrap();
        let mut original = Config::default();
        original.health.threshold_percent = 41.0;
        original.poe1.skills.molten_shell.enabled = true;
        original.poe1.skills.molten_shell.key = " Q ".into();
        original.poe1.nearby.insert(
            FlaskType::Granite,
            NearbyRule {
                enabled: true,
                radius: 75,
                minimum_count: 3,
                minimum_rarity: MinimumRarity::Rare,
            },
        );
        editor
            .store
            .save_profile(Game::Poe1, &first, original)
            .unwrap();
        let mut second = Config::default();
        second.health.threshold_percent = 76.0;
        editor
            .store
            .save_profile(Game::Poe1, &alternate.id, second)
            .unwrap();
        editor.reload_selected().unwrap();

        let editor = Rc::new(RefCell::new(editor));
        let ui = AppWindow::new().unwrap();
        editor.borrow_mut().update_ui(&ui);
        bind_profile_editor(&ui, &editor);
        let _editor_refresh = start_editor_refresh(&ui, &editor);
        ui.show().unwrap();

        let click = |label: &str| {
            let handle = ElementHandle::find_by_accessible_label(&ui, label)
                .next()
                .unwrap_or_else(|| panic!("Missing UI control: {label}"));
            handle.mock_single_click(PointerEventButton::Left);
        };
        let select = |selector, option| {
            click(selector);
            click(option);
        };

        assert!(
            !editor.borrow().is_dirty(&ui),
            "Loaded TOML must start clean"
        );
        ui.set_page(3);
        click("Automation keybind");
        ui.window().dispatch_event(WindowEvent::KeyPressed {
            text: Key::F8.into(),
        });
        assert_eq!(ui.get_automation_key().as_str(), "F8");
        assert!(!editor.borrow().is_dirty(&ui));
        ui.set_page(0);

        select("Utility flask selector", "Granite");
        let granite_index = ui.get_utility_index() as usize;
        assert!(!ui.get_utility_enabled());
        click("Enable selected utility flask");
        select("Utility flask selector", "Diamond");
        assert!(!ui.get_utility_enabled(), "Diamond must keep its own state");
        assert!(!ui.get_utility_nearby());
        assert_eq!(ui.get_utility_radius(), 60.0);
        assert_eq!(
            ui.get_utility_enabled_entries().row_data(granite_index),
            Some(true)
        );
        select("Utility flask selector", "Granite");
        assert!(ui.get_utility_enabled(), "Granite must retain its draft");
        assert!(ui.get_utility_nearby());
        assert_eq!(ui.get_utility_radius(), 75.0);
        assert_eq!(ui.get_utility_count().as_str(), "3");
        assert_eq!(ui.get_utility_rarity(), 2);
        let radius = ElementHandle::find_by_accessible_label(&ui, "Radius slider")
            .next()
            .unwrap();
        radius.invoke_accessible_increment_action();
        assert_eq!(ui.get_utility_radius(), 76.0);
        let origin = radius.absolute_position();
        let size = radius.size();
        let at_radius = |value: f32| {
            slint::LogicalPosition::new(
                origin.x + 9.0 + (size.width - 18.0) * (value - 1.0) / 149.0,
                origin.y + size.height / 2.0,
            )
        };
        let window = ui.window();
        let button = PointerEventButton::Left;
        let rail = ElementHandle::find_by_element_id(&ui, "RadiusSlider::rail")
            .next()
            .unwrap();
        let rail_height = || {
            i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(150));
            rail.size().height
        };
        let tooltip_visible = || {
            ElementHandle::find_by_element_id(&ui, "RadiusSlider::value-tooltip")
                .next()
                .is_some()
        };
        assert_eq!(rail_height(), 4.0);
        assert!(!tooltip_visible());
        let position = at_radius(58.0);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        assert_eq!(rail_height(), 6.0);
        assert!(tooltip_visible());
        window.dispatch_event(WindowEvent::PointerPressed { position, button });
        assert_eq!(ui.get_utility_radius(), 60.0);
        let position = at_radius(102.0);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        assert_eq!(ui.get_utility_radius(), 100.0);
        assert_eq!(rail_height(), 6.0);
        window.dispatch_event(WindowEvent::PointerReleased { position, button });
        assert_eq!(rail_height(), 6.0);
        let position = slint::LogicalPosition::new(origin.x - 20.0, position.y);
        window.dispatch_event(WindowEvent::PointerMoved { position });
        assert_eq!(rail_height(), 4.0);
        assert!(!tooltip_visible());
        window.dispatch_event(WindowEvent::KeyPressed {
            text: Key::RightArrow.into(),
        });
        assert_eq!(ui.get_utility_radius(), 101.0);
        assert!(tooltip_visible());

        select("Profile selector", "Second profile");
        assert_eq!(
            editor.borrow().edit.info.id,
            first,
            "Dirty draft blocks switching"
        );
        assert!(ui.get_error_line().contains("Unsaved settings"));
        click("Discard");
        assert!(!ui.get_utility_enabled());

        select("Profile selector", "Second profile");
        assert_eq!(editor.borrow().edit.info.id, alternate.id);
        assert_eq!(ui.get_profile_display().as_str(), "Second profile");
        assert_eq!(ui.get_life_threshold().as_str(), "76");
        assert!(ui.get_status_line().contains("Press F8"));
        let (store, _) = ProfileStore::open(path.clone(), &path.join("missing.toml")).unwrap();
        assert_eq!(store.settings().active_poe1, alternate.id);
        assert_eq!(store.settings().automation_key.label(), "F8");

        ui.set_page(2);
        let original_name = editor
            .borrow()
            .profiles
            .iter()
            .find(|profile| profile.id == first)
            .unwrap()
            .name
            .clone();
        click(&format!("Load {original_name}"));
        assert_eq!(editor.borrow().edit.info.id, first);
        assert_eq!(ui.get_life_threshold().as_str(), "41");

        ui.set_page(0);
        select("Utility flask selector", "Granite");
        click("Enable selected utility flask");
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(250));
        click("Save");
        assert!(
            editor
                .borrow()
                .store
                .profile_for_edit(Game::Poe1, &first)
                .unwrap()
                .config
                .poe1
                .utility
                .granite
        );
        assert!(!editor.borrow().is_dirty(&ui));

        ui.set_life_threshold("53".into());
        ui.set_page(2);
        select("Actions for Second profile", "Reset");
        assert!(ui.get_profile_reset_open());
        click("Cancel");
        assert!(!ui.get_profile_reset_open());
        assert_eq!(
            editor
                .borrow()
                .store
                .profile_for_edit(Game::Poe1, &alternate.id)
                .unwrap()
                .config
                .health
                .threshold_percent,
            76.0
        );

        select("Actions for Second profile", "Reset");
        click("Reset");
        assert!(!ui.get_profile_reset_open());
        assert_eq!(ui.get_profile_id().as_str(), first);
        assert_eq!(ui.get_life_threshold().as_str(), "53");
        assert!(editor.borrow().is_dirty(&ui));

        select(&format!("Actions for {original_name}"), "Reset");
        click("Reset");
        assert!(!ui.get_profile_reset_open());
        assert_eq!(ui.get_profile_id().as_str(), first);
        assert_eq!(ui.get_profile_display().as_str(), original_name);
        assert_eq!(ui.get_life_threshold().as_str(), "60");
        assert!(!editor.borrow().is_dirty(&ui));
        let (store, _) = ProfileStore::open(path.clone(), &path.join("missing.toml")).unwrap();
        assert_eq!(store.settings().active_poe1, first);
        assert_eq!(store.settings().automation_key.label(), "F8");
        for id in [&first, &alternate.id] {
            assert_eq!(
                store.profile_for_edit(Game::Poe1, id).unwrap().config,
                Config::default()
            );
        }
        let mut backend = BackendStatus {
            connection: Connection::Connected,
            game: Some(Game::Poe1),
            enabled: true,
            reason: "PAUSED (PoE1 not focused)".into(),
            applied: Some(poe2_auto_flask::runtime::AppliedSettings {
                profile_id: Some(first.clone()),
                config: Config::default(),
                automation_key: "F8".into(),
            }),
            ..BackendStatus::default()
        };
        editor.borrow_mut().backend_status = Some(backend.clone());
        editor.borrow_mut().update_backend_state(&ui);
        assert!(ui.get_backend_connected());
        assert!(ui.get_backend_enabled());
        assert!(ui.get_backend_error().is_empty());
        assert_eq!(
            ui.get_backend_automation_detail(),
            slint::StyledText::from_plain_text("Automation enabled\nPaused · PoE1 not focused")
        );
        assert_eq!(
            ui.get_backend_detail(),
            slint::StyledText::from_plain_text("Connected to Path of Exile")
        );
        assert_eq!(ui.get_app_version().as_str(), poe2_auto_flask::VERSION);
        let dot_hint = ElementHandle::find_by_element_id(&ui, "AppWindow::automation-hint")
            .next()
            .unwrap();
        let dot_origin = dot_hint.absolute_position();
        let dot_size = dot_hint.size();
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(
                dot_origin.x + dot_size.width / 2.0,
                dot_origin.y + dot_size.height / 2.0,
            ),
        });
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(100));
        assert!(!ui.global::<StatusTooltipState>().get_visible());
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(100));
        assert!(ui.global::<StatusTooltipState>().get_visible());
        let tooltip = ElementHandle::find_by_element_id(&ui, "AppWindow::status-tooltip")
            .next()
            .unwrap();
        assert!(tooltip.absolute_position().y + tooltip.size().height <= dot_origin.y - 9.0);
        let game_hint = ElementHandle::find_by_element_id(&ui, "AppWindow::connection-hint")
            .next()
            .unwrap();
        let game_origin = game_hint.absolute_position();
        let game_size = game_hint.size();
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(
                game_origin.x + game_size.width / 2.0,
                game_origin.y + game_size.height / 2.0,
            ),
        });
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(200));
        assert_eq!(
            ui.global::<StatusTooltipState>().get_text(),
            slint::StyledText::from_plain_text("Connected to Path of Exile")
        );
        ui.window().dispatch_event(WindowEvent::PointerMoved {
            position: slint::LogicalPosition::new(0.0, 0.0),
        });
        i_slint_backend_testing::mock_elapsed_time(Duration::ZERO);
        assert!(!ui.global::<StatusTooltipState>().get_visible());
        backend.enabled = false;
        backend.reason = "DISABLED".into();
        editor.borrow_mut().backend_status = Some(backend.clone());
        editor.borrow_mut().update_backend_state(&ui);
        assert!(ui.get_backend_connected());
        assert!(!ui.get_backend_enabled());
        assert!(ui.get_backend_error().is_empty());
        assert_eq!(
            ui.get_backend_automation_detail(),
            slint::StyledText::from_plain_text(
                "Automation disabled\nPress F8 while the game is focused to enable."
            )
        );
        backend.config_error = Some("Invalid configuration".into());
        editor.borrow_mut().backend_status = Some(backend.clone());
        editor.borrow_mut().update_backend_state(&ui);
        assert_eq!(
            ui.get_backend_error().as_str(),
            "Settings could not be loaded"
        );
        assert!(ui.get_backend_failed());
        backend.config_error = None;
        backend.game = Some(Game::Poe2);
        ui.set_life_threshold("53".into());
        editor.borrow_mut().backend_status = Some(backend);
        editor.borrow_mut().update_backend_state(&ui);
        assert_eq!(editor.borrow().game, Game::Poe1);
        assert_eq!(ui.get_life_threshold().as_str(), "53");
        assert_eq!(
            ui.get_backend_error().as_str(),
            "Finish edits to switch game"
        );
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(250));
        click("Discard");
        editor.borrow_mut().update_backend_state(&ui);
        assert_eq!(editor.borrow().game, Game::Poe2);
    }

    #[test]
    fn dropdowns_scroll_and_keep_their_anchors_at_minimum_window_size() {
        use i_slint_backend_testing::ElementHandle;
        use slint::platform::{Key, PointerEventButton, WindowEvent};

        let _slint = init_slint_test();
        let ui = AppWindow::new().unwrap();
        let window = ui.window();
        window.set_size(PhysicalSize::new(710, 470));
        ui.set_utility_names(ModelRc::from(
            UTILITY_TYPES.map(|kind| SharedString::from(kind.name())),
        ));
        ui.set_profile_names(ModelRc::new(slint::VecModel::from(
            (1..=18)
                .map(|index| format!("Profile {index}").into())
                .collect::<Vec<SharedString>>(),
        )));
        let weak = ui.as_weak();
        ui.on_change_utility(move |index| weak.unwrap().set_utility_index(index));
        ui.show().unwrap();

        let find = |label: &str| {
            ElementHandle::find_by_accessible_label(&ui, label)
                .next()
                .unwrap_or_else(|| panic!("Missing UI control: {label}"))
        };
        let click = |label| find(label).mock_single_click(PointerEventButton::Left);
        let key = |key: Key| {
            window.dispatch_event(WindowEvent::KeyPressed { text: key.into() });
            window.dispatch_event(WindowEvent::KeyReleased { text: key.into() });
        };
        let scroll_to_end = |label| {
            find(label).scroll(0.0, -2000.0);
            i_slint_backend_testing::mock_elapsed_time(Duration::from_secs(1));
        };

        click("Utility flask selector");
        key(Key::End);
        assert_eq!(ui.get_utility_index(), 0);
        key(Key::Escape);
        assert!(ElementHandle::find_by_accessible_label(&ui, "Iron")
            .next()
            .is_none());
        key(Key::Return);
        key(Key::End);
        key(Key::Return);
        assert_eq!(ui.get_utility_index(), UTILITY_TYPES.len() as i32 - 1);
        click("Utility flask selector");
        key(Key::Home);
        key(Key::Return);
        assert_eq!(ui.get_utility_index(), 0);
        click("Utility flask selector");
        scroll_to_end("Quicksilver");
        click("Iron");
        assert_eq!(ui.get_utility_index(), UTILITY_TYPES.len() as i32 - 1);

        ui.set_page(2);
        let new_profile_y = find("New Profile").absolute_position().y;
        scroll_to_end("Load Profile 1");
        assert_eq!(find("New Profile").absolute_position().y, new_profile_y);
        assert!((58.0..132.0).contains(&new_profile_y));
        let action = find("Actions for Profile 18");
        let anchor = action.absolute_position();
        assert!((0.0..470.0).contains(&anchor.y));
        action.mock_single_click(PointerEventButton::Left);
        let rename = find("Rename");
        let opening_opacity = rename.computed_opacity();
        i_slint_backend_testing::mock_elapsed_time(Duration::from_millis(160));
        assert!(opening_opacity < rename.computed_opacity());
        assert_eq!(rename.computed_opacity(), 1.0);
        let menu_height = 160.0;
        let below = anchor.y + action.size().height + 4.0;
        let menu_top = if below + menu_height <= 462.0 {
            below
        } else {
            (anchor.y - menu_height - 4.0).max(8.0)
        };
        let position =
            slint::LogicalPosition::new(anchor.x + action.size().width - 80.0, menu_top + 22.0);
        let button = PointerEventButton::Left;
        window.dispatch_event(WindowEvent::PointerPressed { position, button });
        window.dispatch_event(WindowEvent::PointerReleased { position, button });
        assert!(ui.get_profile_rename_open());
        assert_eq!(ui.get_rename_to().as_str(), "Profile 18");
    }

    #[test]
    fn headless_ui_rebinds_flask_switch_on_selection() {
        let _slint = init_slint_test();
        use std::time::UNIX_EPOCH;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "auto-flask-ui-rebind-test-{}-{unique}",
            std::process::id()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fs::create_dir_all(&path).unwrap();
        let _cleanup = Cleanup(path.clone());
        let mut editor = Editor::open(path.clone(), &path.join("missing.toml")).unwrap();
        editor.game = Game::Poe1;
        editor.store.select_game(Game::Poe1).unwrap();
        editor.reload_selected().unwrap();
        editor.edit.config.poe1.utility.diamond = true;
        editor.edit.config.poe1.utility.bismuth = false;
        let ui = AppWindow::new().unwrap();
        editor.update_ui(&ui);

        let diamond = UTILITY_TYPES
            .iter()
            .position(|x| *x == FlaskType::Diamond)
            .unwrap();
        let bismuth = UTILITY_TYPES
            .iter()
            .position(|x| *x == FlaskType::Bismuth)
            .unwrap();

        editor.change_utility(&ui, diamond).unwrap();
        assert_eq!(ui.get_utility_index(), diamond as i32);
        assert!(ui.get_utility_enabled(), "Diamond should render enabled");
        editor.change_utility(&ui, bismuth).unwrap();
        assert_eq!(ui.get_utility_index(), bismuth as i32);
        assert!(
            !ui.get_utility_enabled(),
            "Bismuth must not inherit Diamond"
        );

        // Toggle Bismuth without saving; moving away must retain the draft,
        // while Diamond still displays its independently stored value.
        ui.set_utility_enabled(true);
        editor.change_utility(&ui, diamond).unwrap();
        assert!(ui.get_utility_enabled());
        editor.change_utility(&ui, bismuth).unwrap();
        assert!(ui.get_utility_enabled(), "Bismuth keeps its own new value");
        assert!(editor.edit.config.poe1.utility.diamond);
        assert!(editor.edit.config.poe1.utility.bismuth);

        ui.set_utility_enabled(false);
        editor.change_utility(&ui, diamond).unwrap();
        assert!(ui.get_utility_enabled(), "Diamond remains enabled");
        editor.change_utility(&ui, bismuth).unwrap();
        assert!(!ui.get_utility_enabled(), "Bismuth alone was turned off");

        // Save two distinct TOML profiles, then exercise the same change_profile
        // path as a row click or sidebar selection. All displayed fields, not
        // only ProfileStore.current(), must be rebound to the selected file.
        let original_id = editor.edit.info.id.clone();
        let mut original_config = Config::default();
        original_config.health.threshold_percent = 41.0;
        original_config.poe1.utility.diamond = true;
        editor
            .store
            .save_profile(Game::Poe1, &original_id, original_config)
            .unwrap();
        let alternate = editor
            .store
            .create_profile(Game::Poe1, "Alternate", None)
            .unwrap();
        let mut alternate_config = Config::default();
        alternate_config.health.threshold_percent = 76.0;
        alternate_config.poe1.utility.bismuth = true;
        editor
            .store
            .save_profile(Game::Poe1, &alternate.id, alternate_config)
            .unwrap();
        editor.reload_selected().unwrap();
        editor.update_ui(&ui);
        assert!(
            editor.ensure_clean(&ui).is_ok(),
            "Clean profile must be switchable"
        );

        let alternate_index = editor
            .profiles
            .iter()
            .position(|p| p.id == alternate.id)
            .unwrap();
        assert!(editor.change_profile(&ui, alternate_index).unwrap());
        assert_eq!(ui.get_profile_index(), alternate_index as i32);
        assert_eq!(ui.get_life_threshold().as_str(), "76");
        editor.change_utility(&ui, bismuth).unwrap();
        assert!(ui.get_utility_enabled(), "Alternate Bismuth enabled");
        editor.change_utility(&ui, diamond).unwrap();
        assert!(!ui.get_utility_enabled(), "Alternate Diamond disabled");
        assert!(
            editor.ensure_clean(&ui).is_ok(),
            "Viewing other flasks is not a change"
        );

        let original_index = editor
            .profiles
            .iter()
            .position(|p| p.id == original_id)
            .unwrap();
        assert!(editor.change_profile(&ui, original_index).unwrap());
        assert_eq!(ui.get_profile_index(), original_index as i32);
        assert_eq!(ui.get_life_threshold().as_str(), "41");
        editor.change_utility(&ui, diamond).unwrap();
        assert!(ui.get_utility_enabled(), "Original Diamond restored");
    }

    #[test]
    fn hydrated_toml_profile_and_discard_remain_clean() {
        use std::time::UNIX_EPOCH;

        let _slint = init_slint_test();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = env::temp_dir().join(format!(
            "auto-flask-ui-clean-profile-{}-{unique}",
            std::process::id()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fs::create_dir_all(&root).unwrap();
        let _cleanup = Cleanup(root.clone());

        let mut editor = Editor::open(root.clone(), &root.join("missing.toml")).unwrap();
        editor.game = Game::Poe1;
        editor.store.select_game(Game::Poe1).unwrap();
        let id = editor.store.active_profile_id().to_string();

        let mut config = Config::default();
        config.health.threshold_percent = 65.0;
        config.poe1.utility.quicksilver = true;
        config.poe1.utility.granite = true;
        config.poe1.quicksilver_min_run_ms = 0;
        config.poe1.unique.writhing_jar = true;
        config.poe1.unique_conditions.insert(
            UniqueFlask::WrithingJar,
            UniqueConditions {
                nearby: Some(NearbyRule {
                    enabled: true,
                    radius: 60,
                    minimum_count: 1,
                    minimum_rarity: MinimumRarity::Rare,
                }),
            },
        );
        config.poe1.nearby.insert(
            FlaskType::Granite,
            NearbyRule {
                enabled: false,
                radius: 75,
                minimum_count: 3,
                minimum_rarity: MinimumRarity::Magic,
            },
        );
        config.poe1.skills.blood_rage.enabled = true;
        config.poe1.skills.blood_rage.require_nearby_monsters = true;
        editor
            .store
            .save_profile(Game::Poe1, &id, config.clone())
            .unwrap();
        editor.reload_selected().unwrap();

        let ui = AppWindow::new().unwrap();
        editor.update_ui(&ui);
        assert_eq!(editor.snapshot(&ui).unwrap(), config);
        assert!(
            !editor.is_dirty(&ui),
            "Loading TOML must not mark it unsaved"
        );

        let granite = UTILITY_TYPES
            .iter()
            .position(|kind| *kind == FlaskType::Granite)
            .unwrap();
        editor.change_utility(&ui, granite).unwrap();
        assert!(
            !editor.is_dirty(&ui),
            "Viewing Granite must not change its rule"
        );

        ui.set_utility_enabled(false);
        assert!(editor.is_dirty(&ui));
        // Simulate Discard's exact code path while a different utility is
        // selected, including a previously disabled nearby gate.
        editor.edit = editor.store.profile_for_edit(Game::Poe1, &id).unwrap();
        editor.saved_config = editor.edit.config.clone();
        editor.update_values(&ui);
        assert!(
            !editor.is_dirty(&ui),
            "Discard must clear all draft changes"
        );
    }

    #[test]
    fn window_geometry_migrates_to_settings_and_preserves_profile_edits() {
        let root = env::temp_dir().join(format!(
            "auto-flask-ui-window-geometry-{}",
            std::process::id()
        ));
        let mut editor = Editor::open(root.clone(), &root.join("missing.toml")).unwrap();
        let geometry = WindowGeometry {
            x: Some(-387),
            y: Some(214),
            width: 1170,
            height: 828,
        };
        let legacy = root.join("window-state.toml");
        fs::write(&legacy, toml::to_string(&geometry).unwrap()).unwrap();
        editor.last_window_geometry = load_window_geometry(&mut editor.store).unwrap();
        assert_eq!(editor.last_window_geometry, Some(geometry));
        assert!(!legacy.exists());

        // A movement save must not hide an unrelated manual profile edit from
        // the metadata watcher, even while an in-memory draft is being edited.
        let id = editor.edit.info.id.clone();
        let mut changed = editor.saved_config.clone();
        changed.health.threshold_percent = 48.0;
        let (mut external, _) = ProfileStore::open(root.clone(), &legacy).unwrap();
        external.save_profile(editor.game, &id, changed).unwrap();
        let size_only = WindowGeometry {
            x: None,
            y: None,
            ..geometry
        };
        editor.persist_window_geometry(size_only).unwrap();
        assert_ne!(editor.current_disk_state(), editor.observed_disk);
        let again = Editor::open(root.clone(), &legacy).unwrap();
        assert_eq!(again.last_window_geometry, Some(size_only));
        assert_eq!(again.saved_config.health.threshold_percent, 48.0);
        assert!(!legacy.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn window_migration_keeps_legacy_file_when_settings_changed_externally() {
        let root = env::temp_dir().join(format!(
            "auto-flask-window-migration-{}",
            std::process::id()
        ));
        let legacy = root.join("window-state.toml");
        let (mut stale, _) = ProfileStore::open(root.clone(), &root.join("missing.toml")).unwrap();
        fs::write(&legacy, "x = 100\ny = 200\nwidth = 780\nheight = 552\n").unwrap();
        let (mut external, _) = ProfileStore::open(root.clone(), &legacy).unwrap();
        let mut settings = external.settings().ui.clone();
        settings.hide_to_tray_on_close = false;
        external.save_ui_settings(settings).unwrap();
        assert!(load_window_geometry(&mut stale).is_err());
        assert!(legacy.is_file());
        assert!(load_window_geometry(&mut external).unwrap().is_some());
        assert!(!legacy.exists());
        assert!(!external.settings().ui.hide_to_tray_on_close);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn window_geometry_fits_the_current_monitor() {
        let geometry = WindowGeometry {
            x: Some(-1400),
            y: Some(800),
            width: 1170,
            height: 900,
        };
        let fitted = fit_window_geometry(
            geometry,
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(1920, 1080),
        );
        assert_eq!(
            window_position(fitted),
            Some(PhysicalPosition::new(-1400, 180))
        );
        let fitted = fit_window_geometry(
            geometry,
            PhysicalPosition::new(0, 0),
            PhysicalSize::new(1024, 768),
        );
        assert_eq!(window_position(fitted), Some(PhysicalPosition::new(0, 0)));
        assert_eq!((fitted.width, fitted.height), (1024, 768));
        let first_open = WindowGeometry {
            x: None,
            y: None,
            ..geometry
        };
        let centered = fit_window_geometry(
            first_open,
            PhysicalPosition::new(-1920, 0),
            PhysicalSize::new(1920, 1080),
        );
        assert_eq!(
            window_position(centered),
            Some(PhysicalPosition::new(-1545, 90))
        );
    }

    #[test]
    fn switching_utility_flasks_preserves_independent_enabled_flags() {
        let mut config = poe1::config::Poe1UtilityConfig::default();
        set_utility(&mut config, FlaskType::Diamond, true);
        assert!(config.enabled(FlaskType::Diamond));
        assert!(!config.enabled(FlaskType::Bismuth));
        set_utility(&mut config, FlaskType::Bismuth, true);
        assert!(config.enabled(FlaskType::Diamond));
        assert!(config.enabled(FlaskType::Bismuth));
        set_utility(&mut config, FlaskType::Diamond, false);
        assert!(!config.enabled(FlaskType::Diamond));
        assert!(config.enabled(FlaskType::Bismuth));
    }

    #[test]
    fn editor_reloads_distinct_saved_profiles_and_utility_rules() {
        // Exercise the same ProfileStore / Editor reload sequence used by the
        // actual UI, rather than only toggling isolated boolean flags.
        use std::time::UNIX_EPOCH;
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = env::temp_dir().join(format!(
            "auto-flask-ui-profile-test-{}-{unique}",
            std::process::id()
        ));
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        fs::create_dir_all(&path).unwrap();
        let _cleanup = Cleanup(path.clone());
        let missing = path.join("missing-legacy.toml");
        let (mut store, _) = ProfileStore::open(path.clone(), &missing).unwrap();
        store.select_game(Game::Poe1).unwrap();
        let first = store.active_profile_id().to_string();
        let second = store.create_profile(Game::Poe1, "Second", None).unwrap();
        let mut first_config = Config::default();
        first_config.health.threshold_percent = 41.0;
        first_config.poe1.utility.diamond = true;
        store
            .save_profile(Game::Poe1, &first, first_config.clone())
            .unwrap();
        let mut second_config = Config::default();
        second_config.health.threshold_percent = 76.0;
        second_config.poe1.utility.bismuth = true;
        store
            .save_profile(Game::Poe1, &second.id, second_config.clone())
            .unwrap();
        store.activate_profile(Game::Poe1, &first).unwrap();
        drop(store);

        let mut editor = Editor::open(path, &missing).unwrap();
        assert_eq!(editor.game, Game::Poe1);
        assert_eq!(editor.edit.info.id, first);
        assert_eq!(editor.edit.config, first_config);
        assert!(editor.edit.config.poe1.utility.diamond);
        assert!(!editor.edit.config.poe1.utility.bismuth);

        let second_index = editor
            .profiles
            .iter()
            .position(|p| p.id == second.id)
            .unwrap();
        assert!(editor.switch_profile(second_index).unwrap());
        assert_eq!(editor.edit.info.id, second.id);
        assert_eq!(editor.saved_config, second_config);
        assert!(!editor.edit.config.poe1.utility.diamond);
        assert!(editor.edit.config.poe1.utility.bismuth);

        let first_index = editor.profiles.iter().position(|p| p.id == first).unwrap();
        assert!(editor.switch_profile(first_index).unwrap());
        assert_eq!(editor.edit.config, first_config);
    }

    #[test]
    fn all_utility_types_have_setter_and_getter() {
        let mut c = poe1::config::Poe1UtilityConfig::default();
        for kind in UTILITY_TYPES {
            set_utility(&mut c, kind, true);
            assert!(c.enabled(kind), "{kind:?}");
            set_utility(&mut c, kind, false);
            assert!(!c.enabled(kind), "{kind:?}");
        }
    }
}
