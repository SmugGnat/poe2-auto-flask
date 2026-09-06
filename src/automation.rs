use std::io;
use std::thread;
use std::time::{Duration, Instant};

use crate::area::{resolve_area_info, AreaInfo};
use crate::bindings::{BindingManager, BindingReload, GameBindings};
use crate::buffs::{resolve_flask_lock_probe, sample_flask_locks, FlaskLockProbe, FlaskLocks};
use crate::config::{Config, ConfigManager, ConfigReload, FlaskConfig};
use crate::flask_inventory::{
    resolve_flask_inventory_probe, sample_flask_charges_fast, FlaskChargeCache,
    FlaskChargeSnapshot, FlaskChargeState, FlaskInventoryProbe,
};
use crate::input::{is_process_foreground, master_toggle_down, send_flask_key, KeyBinding};
use crate::poe2::{refresh_vitals, sample_vitals, VitalPool, VitalsProbe};
use crate::process::Process;

const LIFE_FLASK_SLOT: usize = 0;
const MANA_FLASK_SLOT: usize = 1;
const AUTO_POLL_INTERVAL: Duration = Duration::from_millis(100);
const CONFIG_RELOAD_INTERVAL: Duration = Duration::from_secs(1);
const BINDING_RELOAD_INTERVAL: Duration = Duration::from_secs(2);
const POST_SEND_SAFETY_GUARD: Duration = Duration::from_millis(1000);
const REFRESH_RETRY: Duration = Duration::from_millis(250);
const IDLE_PROCESS_CHECK_INTERVAL: Duration = Duration::from_secs(1);
const BAD_SAMPLE_LIMIT: u32 = 3;

struct RuntimeState {
    vitals: VitalsProbe,
    area: AreaInfo,
    flask_probe: FlaskLockProbe,
    charge_probe: FlaskInventoryProbe,
    charge_cache: FlaskChargeCache,
}

impl RuntimeState {
    fn resolve(process: &Process, vitals: VitalsProbe) -> io::Result<Self> {
        let area = resolve_area_info(process, &vitals)?;
        let flask_probe = resolve_flask_lock_probe(process, &vitals)?;
        let charge_probe = resolve_flask_inventory_probe(process, &vitals)?;
        Ok(Self {
            vitals,
            area,
            flask_probe,
            charge_probe,
            charge_cache: FlaskChargeCache::default(),
        })
    }

    fn refresh(&mut self, process: &Process) -> io::Result<()> {
        self.charge_cache.clear();
        let vitals = refresh_vitals(process, &self.vitals)?;
        *self = Self::resolve(process, vitals)?;
        Ok(())
    }

    fn area_instance_is_current(&self, process: &Process) -> io::Result<bool> {
        let current = process
            .read_u64(self.vitals.in_game_state + self.vitals.area_instance_offset)?
            as usize;
        Ok(current == self.vitals.area_instance)
    }
}

struct FlaskState {
    last_fire: Option<Instant>,
    charge_block_reported: bool,
}

impl FlaskState {
    fn new() -> Self {
        Self {
            last_fire: None,
            charge_block_reported: false,
        }
    }

    fn ready(&self) -> bool {
        self.last_fire
            .is_none_or(|last| last.elapsed() >= POST_SEND_SAFETY_GUARD)
    }

    fn mark_fired(&mut self) {
        self.last_fire = Some(Instant::now());
        self.charge_block_reported = false;
    }

    fn clear_block_notice(&mut self) {
        self.charge_block_reported = false;
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EngineStatus {
    Disarmed,
    Armed,
    PausedTown,
    PausedHideout,
    PausedBackground,
    PausedDead,
    PausedGamepad,
    PausedBindings,
    PausedInputMode,
    Reconnecting,
}

impl EngineStatus {
    fn text(self) -> &'static str {
        match self {
            Self::Disarmed => "DISARMED",
            Self::Armed => "ARMED",
            Self::PausedTown => "PAUSED (town)",
            Self::PausedHideout => "PAUSED (hideout)",
            Self::PausedBackground => "PAUSED (PoE2 not focused)",
            Self::PausedDead => "PAUSED (dead)",
            Self::PausedGamepad => "PAUSED (gamepad input not supported yet)",
            Self::PausedBindings => "PAUSED (PoE2 input bindings unavailable)",
            Self::PausedInputMode => "PAUSED (unsupported PoE2 input mode)",
            Self::Reconnecting => "WAITING FOR GAME STATE",
        }
    }
}

pub fn run_auto(
    process: &Process,
    probe: VitalsProbe,
    config_manager: &mut ConfigManager,
) -> io::Result<()> {
    let poll_interval = AUTO_POLL_INTERVAL;
    let mut binding_manager = BindingManager::load()?;
    let mut runtime = RuntimeState::resolve(process, probe)?;
    let mut health_state = FlaskState::new();
    let mut mana_state = FlaskState::new();
    let mut armed = false;
    let mut toggle_was_down = false;
    let mut shown_status = None;
    let mut needs_refresh = false;
    let mut next_refresh = Instant::now();
    let mut next_idle_process_check = Instant::now();
    let mut next_config_check = Instant::now() + CONFIG_RELOAD_INTERVAL;
    let mut next_binding_check = Instant::now() + BINDING_RELOAD_INTERVAL;
    let mut bad_samples = 0u32;

    println!("{}", binding_manager.current().summary());
    show_status(&mut shown_status, EngineStatus::Disarmed);
    println!("Press F11 to arm/disarm while PoE2 is focused. Ctrl+C exits.\n");

    loop {
        let tick_started = Instant::now();
        check_config_reload(config_manager, &mut next_config_check);
        check_binding_reload(&mut binding_manager, &mut next_binding_check);
        let config = config_manager.current();
        let toggle_is_down = master_toggle_down();

        if toggle_is_down && !toggle_was_down && is_process_foreground(process.pid()) {
            armed = !armed;
            bad_samples = 0;
            if armed {
                match runtime.refresh(process) {
                    Ok(()) => {
                        needs_refresh = false;
                        show_area_status(&mut shown_status, &runtime.area, config);
                    }
                    Err(_) => {
                        if process_has_exited(process) {
                            return Ok(());
                        }
                        needs_refresh = true;
                        next_refresh = Instant::now();
                        show_status(&mut shown_status, EngineStatus::Reconnecting);
                    }
                }
            } else {
                needs_refresh = false;
                show_status(&mut shown_status, EngineStatus::Disarmed);
            }
        }
        toggle_was_down = toggle_is_down;

        if !armed {
            if idle_process_check_due(process, &mut next_idle_process_check) {
                return Ok(());
            }
            sleep_remaining(tick_started, poll_interval);
            continue;
        }

        if !is_process_foreground(process.pid()) {
            if idle_process_check_due(process, &mut next_idle_process_check) {
                return Ok(());
            }
            needs_refresh = true;
            show_status(&mut shown_status, EngineStatus::PausedBackground);
            sleep_remaining(tick_started, poll_interval);
            continue;
        }

        let (life_binding, mana_binding) = match binding_manager.current() {
            GameBindings::Keyboard { life, mana, .. } => (*life, *mana),
            GameBindings::Gamepad => {
                if idle_process_check_due(process, &mut next_idle_process_check) {
                    return Ok(());
                }
                show_status(&mut shown_status, EngineStatus::PausedGamepad);
                sleep_remaining(tick_started, poll_interval);
                continue;
            }
            GameBindings::UnsupportedMode(_) => {
                if idle_process_check_due(process, &mut next_idle_process_check) {
                    return Ok(());
                }
                show_status(&mut shown_status, EngineStatus::PausedInputMode);
                sleep_remaining(tick_started, poll_interval);
                continue;
            }
            GameBindings::Unavailable(_) => {
                if idle_process_check_due(process, &mut next_idle_process_check) {
                    return Ok(());
                }
                show_status(&mut shown_status, EngineStatus::PausedBindings);
                sleep_remaining(tick_started, poll_interval);
                continue;
            }
        };

        if needs_refresh {
            if Instant::now() < next_refresh {
                sleep_remaining(tick_started, poll_interval);
                continue;
            }
            match runtime.refresh(process) {
                Ok(()) => {
                    needs_refresh = false;
                    bad_samples = 0;
                    show_area_status(&mut shown_status, &runtime.area, config);
                }
                Err(_) => {
                    if process_has_exited(process) {
                        return Ok(());
                    }
                    next_refresh = Instant::now() + REFRESH_RETRY;
                    show_status(&mut shown_status, EngineStatus::Reconnecting);
                    sleep_remaining(tick_started, poll_interval);
                    continue;
                }
            }
        }

        if !runtime
            .area
            .allows_auto_flask(config.general.enable_in_hideout)
        {
            show_area_status(&mut shown_status, &runtime.area, config);
            match runtime.area_instance_is_current(process) {
                Ok(true) => {}
                Ok(false) => {
                    needs_refresh = true;
                    next_refresh = Instant::now();
                }
                Err(_) => {
                    if process_has_exited(process) {
                        return Ok(());
                    }
                    needs_refresh = true;
                    next_refresh = Instant::now();
                }
            }
            sleep_remaining(tick_started, poll_interval);
            continue;
        }

        let vitals = match sample_vitals(process, &runtime.vitals) {
            Ok(vitals) => {
                bad_samples = 0;
                vitals
            }
            Err(error) => {
                if process_has_exited(process) {
                    return Ok(());
                }
                handle_bad_sample(
                    error,
                    &mut bad_samples,
                    &mut needs_refresh,
                    &mut next_refresh,
                    &mut shown_status,
                );
                sleep_remaining(tick_started, poll_interval);
                continue;
            }
        };

        if player_is_dead(vitals.health) {
            health_state.clear_block_notice();
            mana_state.clear_block_notice();
            show_status(&mut shown_status, EngineStatus::PausedDead);
            sleep_remaining(tick_started, poll_interval);
            continue;
        }

        show_status(&mut shown_status, EngineStatus::Armed);

        let action_state =
            sample_flask_locks(process, &mut runtime.flask_probe).and_then(|locks| {
                let charges = sample_flask_charges_fast(
                    process,
                    &runtime.charge_probe,
                    &mut runtime.charge_cache,
                )?;
                Ok((locks, charges))
            });

        match action_state {
            Ok((locks, charges)) => {
                bad_samples = 0;

                if !ci_like_defense(vitals.health, vitals.energy_shield) {
                    maybe_fire(
                        "Life",
                        LIFE_FLASK_SLOT,
                        vitals.health,
                        &config.health,
                        life_binding,
                        &mut health_state,
                        &locks,
                        &charges,
                    )?;
                } else {
                    health_state.clear_block_notice();
                }

                if let Some(mana) = vitals.mana {
                    maybe_fire(
                        "Mana",
                        MANA_FLASK_SLOT,
                        mana,
                        &config.mana,
                        mana_binding,
                        &mut mana_state,
                        &locks,
                        &charges,
                    )?;
                }
            }
            Err(error) => {
                if process_has_exited(process) {
                    return Ok(());
                }
                handle_bad_sample(
                    error,
                    &mut bad_samples,
                    &mut needs_refresh,
                    &mut next_refresh,
                    &mut shown_status,
                );
            }
        }

        sleep_remaining(tick_started, poll_interval);
    }
}

fn check_config_reload(config_manager: &mut ConfigManager, next_check: &mut Instant) {
    let now = Instant::now();
    if now < *next_check {
        return;
    }
    *next_check = now + CONFIG_RELOAD_INTERVAL;

    match config_manager.check_for_reload() {
        ConfigReload::Unchanged => {}
        ConfigReload::Reloaded => {
            println!("Config reloaded: {}", config_manager.current().summary());
        }
        ConfigReload::Removed => {
            println!("Config: config.toml removed; keeping previous settings.");
        }
        ConfigReload::Invalid(error) => {
            println!("Config reload failed: {error}");
            println!("Keeping previous settings.");
        }
        ConfigReload::ReadError(error) => {
            println!("Config reload read failed: {error}");
            println!("Keeping previous settings.");
        }
    }
}

fn check_binding_reload(binding_manager: &mut BindingManager, next_check: &mut Instant) {
    let now = Instant::now();
    if now < *next_check {
        return;
    }
    *next_check = now + BINDING_RELOAD_INTERVAL;

    match binding_manager.check_for_reload() {
        BindingReload::Unchanged => {}
        BindingReload::Reloaded => {
            println!("Bindings reloaded: {}", binding_manager.current().summary());
        }
        BindingReload::Removed => {
            println!("PoE2 input config removed; auto input paused until it returns.");
        }
        BindingReload::Invalid(error) => {
            println!("PoE2 binding reload failed: {error}");
            println!("Auto input paused until valid bindings can be read.");
        }
        BindingReload::ReadError(error) => {
            println!("PoE2 binding check failed: {error}");
            println!("Auto input paused until bindings can be read again.");
        }
    }
}

fn player_is_dead(health: VitalPool) -> bool {
    health.current <= 0
}

fn ci_like_defense(health: VitalPool, energy_shield: Option<VitalPool>) -> bool {
    health.total == 1 && energy_shield.is_some_and(|es| es.total > 0)
}

fn idle_process_check_due(process: &Process, next_check: &mut Instant) -> bool {
    let now = Instant::now();
    if now < *next_check {
        return false;
    }
    *next_check = now + IDLE_PROCESS_CHECK_INTERVAL;
    process_has_exited(process)
}

fn process_has_exited(process: &Process) -> bool {
    !process.is_alive().unwrap_or(false)
}

fn handle_bad_sample(
    error: io::Error,
    bad_samples: &mut u32,
    needs_refresh: &mut bool,
    next_refresh: &mut Instant,
    shown_status: &mut Option<EngineStatus>,
) {
    *bad_samples = bad_samples.saturating_add(1);
    if error.kind() == io::ErrorKind::Interrupted || *bad_samples >= BAD_SAMPLE_LIMIT {
        *needs_refresh = true;
        *next_refresh = Instant::now();
        show_status(shown_status, EngineStatus::Reconnecting);
    }
}

#[allow(clippy::too_many_arguments)]
fn maybe_fire(
    label: &str,
    slot: usize,
    pool: VitalPool,
    flask: &FlaskConfig,
    binding: KeyBinding,
    state: &mut FlaskState,
    locks: &FlaskLocks,
    charges: &FlaskChargeSnapshot,
) -> io::Result<()> {
    if !flask.enabled {
        return Ok(());
    }

    let Some(percent) = pool.percentage() else {
        return Ok(());
    };
    if percent > flask.threshold_percent {
        state.charge_block_reported = false;
        return Ok(());
    }

    if locks.slots[slot] || !state.ready() {
        return Ok(());
    }

    let charge_state = charges.slots.get(slot).copied().flatten();
    if !flask_has_usable_charges(label, slot, charge_state, state) {
        return Ok(());
    }
    let charge_state = charge_state.expect("usable charge state must exist");

    send_flask_key(binding)?;
    println!(
        "{label} flask: {:.1}% -> {} (charges {}/{})",
        percent,
        binding.label(),
        charge_state.current,
        charge_state.per_use
    );
    state.mark_fired();
    Ok(())
}

fn flask_has_usable_charges(
    label: &str,
    slot: usize,
    charges: Option<FlaskChargeState>,
    state: &mut FlaskState,
) -> bool {
    match charges {
        Some(charges) if charges.usable() => {
            state.charge_block_reported = false;
            true
        }
        Some(charges) => {
            if !state.charge_block_reported {
                println!(
                    "{label} flask blocked: {} charges, {} needed",
                    charges.current, charges.per_use
                );
                state.charge_block_reported = true;
            }
            false
        }
        None => {
            if !state.charge_block_reported {
                println!(
                    "{label} flask blocked: slot {} is empty/unreadable",
                    slot + 1
                );
                state.charge_block_reported = true;
            }
            false
        }
    }
}

fn show_area_status(status: &mut Option<EngineStatus>, area: &AreaInfo, config: &Config) {
    let next = if area.is_town {
        EngineStatus::PausedTown
    } else if area.is_hideout && !config.general.enable_in_hideout {
        EngineStatus::PausedHideout
    } else {
        EngineStatus::Armed
    };
    show_status(status, next);
}

fn show_status(current: &mut Option<EngineStatus>, next: EngineStatus) {
    if *current != Some(next) {
        println!("Status: {}", next.text());
        *current = Some(next);
    }
}

fn sleep_remaining(started: Instant, interval: Duration) {
    let elapsed = started.elapsed();
    if elapsed < interval {
        thread::sleep(interval - elapsed);
    }
}

#[cfg(test)]
mod tests {
    use super::{ci_like_defense, player_is_dead};
    use crate::poe2::VitalPool;

    fn pool(current: i32, total: i32) -> VitalPool {
        VitalPool {
            current,
            total,
            reserved_flat: 0,
            reserved_fraction: 0,
        }
    }

    #[test]
    fn death_uses_current_life_not_total_life() {
        assert!(player_is_dead(pool(0, 675)));
        assert!(player_is_dead(pool(-1, 675)));
        assert!(!player_is_dead(pool(1, 1)));
    }

    #[test]
    fn ci_like_requires_one_total_life_and_energy_shield() {
        assert!(ci_like_defense(pool(1, 1), Some(pool(2500, 3000))));
        assert!(!ci_like_defense(pool(1, 675), Some(pool(2500, 3000))));
        assert!(!ci_like_defense(pool(1, 1), Some(pool(0, 0))));
        assert!(!ci_like_defense(pool(1, 1), None));
    }

    #[test]
    fn depleted_energy_shield_still_counts_as_ci_like() {
        assert!(ci_like_defense(pool(1, 1), Some(pool(0, 3000))));
    }
}
