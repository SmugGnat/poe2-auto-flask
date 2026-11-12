//! PoE2 flask *policy* for the shared automation engine. Keeps the
//! existing two-slot rules, one-second post-send guard, locks and charges.
use std::io;
use std::time::{Duration, Instant};

use super::area::resolve_area_info;
use super::bindings::{BindingManager, BindingReload, GameBindings};
use super::buffs::{resolve_flask_lock_probe, sample_flask_locks, FlaskLockProbe, FlaskLocks};
use super::flasks::{
    resolve_flask_inventory_probe, sample_flask_charges_fast, FlaskChargeCache,
    FlaskChargeSnapshot, FlaskChargeState, FlaskInventoryProbe,
};
use crate::action::{dispatch, ActionRequest, Dispatch, DispatchError};
use crate::area::AreaInfo;
use crate::config::{Config, ConfigManager, ConfigReload, FlaskConfig};
use crate::engine::{GameAutomationPolicy, TickOutcome};
use crate::game_state::VitalPool;
use crate::input::KeyBinding;
use crate::process::Process;
use crate::session::GameSession;

const LIFE_FLASK_SLOT: usize = 0;
const MANA_FLASK_SLOT: usize = 1;
const POST_SEND_SAFETY_GUARD: Duration = Duration::from_millis(1000);
const BAD_SAMPLE_LIMIT: u32 = 3;

struct RuntimeState {
    area: AreaInfo,
    flask_probe: FlaskLockProbe,
    charge_probe: FlaskInventoryProbe,
    charge_cache: FlaskChargeCache,
}

impl RuntimeState {
    fn resolve(process: &Process, session: &GameSession) -> io::Result<Self> {
        let vitals = session.poe2_probe()?;
        let area = resolve_area_info(process, vitals)?;
        let flask_probe = resolve_flask_lock_probe(process, vitals)?;
        let charge_probe = resolve_flask_inventory_probe(process, vitals)?;
        Ok(Self {
            area,
            flask_probe,
            charge_probe,
            charge_cache: FlaskChargeCache::default(),
        })
    }

    fn refresh(&mut self, process: &Process, session: &GameSession) -> io::Result<()> {
        self.charge_cache.clear();
        let vitals = session.poe2_probe()?;
        self.area = resolve_area_info(process, vitals)?;
        self.flask_probe = resolve_flask_lock_probe(process, vitals)?;
        self.charge_probe = resolve_flask_inventory_probe(process, vitals)?;
        Ok(())
    }

    fn area_instance_is_current(
        &self,
        process: &Process,
        session: &GameSession,
    ) -> io::Result<bool> {
        let vitals = session.poe2_probe()?;
        let current =
            process.read_u64(vitals.in_game_state + vitals.area_instance_offset)? as usize;
        Ok(current == vitals.area_instance)
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

struct Policy {
    bindings: BindingManager,
    runtime: RuntimeState,
    health_state: FlaskState,
    mana_state: FlaskState,
    bad_samples: u32,
}

impl Policy {
    fn new(process: &Process, session: &GameSession) -> io::Result<Self> {
        Ok(Self {
            bindings: BindingManager::load()?,
            runtime: RuntimeState::resolve(process, session)?,
            health_state: FlaskState::new(),
            mana_state: FlaskState::new(),
            bad_samples: 0,
        })
    }

    fn bad_sample(&mut self, error: io::Error) -> TickOutcome {
        self.bad_samples = self.bad_samples.saturating_add(1);
        if error.kind() == io::ErrorKind::Interrupted || self.bad_samples >= BAD_SAMPLE_LIMIT {
            TickOutcome::Refresh
        } else {
            // Preserve PoE2's historical tolerance of transient read errors.
            TickOutcome::Unchanged
        }
    }
}

pub fn run(
    process: &Process,
    session: GameSession,
    config_manager: &mut ConfigManager,
) -> io::Result<()> {
    let policy = Policy::new(process, &session)?;
    crate::engine::run(process, session, config_manager, policy)
}

impl GameAutomationPolicy for Policy {
    fn banner(&self, config: &Config) {
        println!("Connected to Path of Exile 2.");
        println!("{}", config.poe2_summary());
        println!("{}", self.bindings.current().summary());
        println!("Press F11 to arm/disarm while PoE2 is focused. Ctrl+C exits.\n");
    }

    fn on_config_reload(&mut self, event: ConfigReload, config: &Config) {
        match event {
            ConfigReload::Unchanged => {}
            ConfigReload::Reloaded => println!("Config reloaded: {}", config.poe2_summary()),
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

    fn check_bindings(&mut self) {
        match self.bindings.check_for_reload() {
            BindingReload::Unchanged => {}
            BindingReload::Reloaded => {
                println!("Bindings reloaded: {}", self.bindings.current().summary());
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

    fn bindings_pause(&self) -> Option<&'static str> {
        match self.bindings.current() {
            GameBindings::Keyboard { .. } => None,
            GameBindings::Gamepad => Some("PAUSED (gamepad input not supported yet)"),
            GameBindings::UnsupportedMode(_) => Some("PAUSED (unsupported PoE2 input mode)"),
            GameBindings::Unavailable(_) => Some("PAUSED (PoE2 input bindings unavailable)"),
        }
    }

    fn on_arm_change(&mut self, _armed: bool) {
        self.bad_samples = 0;
    }

    fn on_inactive(&mut self) {}
    fn on_focus_lost(&mut self) {}

    fn after_refresh(&mut self) {
        self.bad_samples = 0;
    }

    fn refresh(&mut self, process: &Process, session: &GameSession) -> io::Result<()> {
        self.runtime.refresh(process, session)
    }

    fn focus_status(&self) -> &'static str {
        "PAUSED (PoE2 not focused)"
    }

    fn waiting_status(&self) -> &'static str {
        "WAITING FOR GAME STATE"
    }

    fn tick(
        &mut self,
        process: &Process,
        session: &mut GameSession,
        config: &Config,
        _started: Instant,
    ) -> io::Result<TickOutcome> {
        // Keep PoE2's fast forbidden-area path: don't sample vitals in town.
        if !self
            .runtime
            .area
            .allows_auto_flask(config.general.enable_in_hideout)
        {
            let status = area_status(&self.runtime.area, config);
            return Ok(
                match self.runtime.area_instance_is_current(process, session) {
                    Ok(true) => TickOutcome::Status(status),
                    Ok(false) | Err(_) => TickOutcome::Refresh,
                },
            );
        }

        let vitals = match session.vitals(process) {
            Ok(vitals) => {
                self.bad_samples = 0;
                vitals
            }
            Err(error) => return Ok(self.bad_sample(error)),
        };

        if player_is_dead(vitals.health) {
            self.health_state.clear_block_notice();
            self.mana_state.clear_block_notice();
            return Ok(TickOutcome::Status("PAUSED (dead)"));
        }

        let action_state =
            sample_flask_locks(process, &mut self.runtime.flask_probe).and_then(|locks| {
                let charges = sample_flask_charges_fast(
                    process,
                    &self.runtime.charge_probe,
                    &mut self.runtime.charge_cache,
                )?;
                Ok((locks, charges))
            });

        match action_state {
            Ok((locks, charges)) => {
                self.bad_samples = 0;
                let (life_binding, mana_binding) = match self.bindings.current() {
                    GameBindings::Keyboard { life, mana, .. } => (*life, *mana),
                    _ => {
                        return Ok(TickOutcome::Status(
                            "PAUSED (PoE2 input bindings unavailable)",
                        ))
                    }
                };

                if !ci_like_defense(vitals.health, vitals.energy_shield) {
                    maybe_fire(
                        process,
                        "Life",
                        LIFE_FLASK_SLOT,
                        vitals.health,
                        &config.health,
                        life_binding,
                        &mut self.health_state,
                        &locks,
                        &charges,
                    )?;
                } else {
                    self.health_state.clear_block_notice();
                }

                if let Some(mana) = vitals.mana {
                    maybe_fire(
                        process,
                        "Mana",
                        MANA_FLASK_SLOT,
                        mana,
                        &config.mana,
                        mana_binding,
                        &mut self.mana_state,
                        &locks,
                        &charges,
                    )?;
                }
            }
            Err(error) => return Ok(self.bad_sample(error)),
        }
        Ok(TickOutcome::Status("ARMED"))
    }
}

fn area_status(area: &AreaInfo, config: &Config) -> &'static str {
    if area.is_town {
        "PAUSED (town)"
    } else if area.is_hideout && !config.general.enable_in_hideout {
        "PAUSED (hideout)"
    } else {
        "ARMED"
    }
}

fn player_is_dead(health: VitalPool) -> bool {
    health.current <= 0
}

fn ci_like_defense(health: VitalPool, energy_shield: Option<VitalPool>) -> bool {
    health.total == 1 && energy_shield.is_some_and(|es| es.total > 0)
}

#[allow(clippy::too_many_arguments)]
fn maybe_fire(
    process: &Process,
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

    // All game policies send through the shared action executor. PoE2's
    // preexisting checks above remain its decision policy; the executor
    // additionally refuses input if the selected game lost focus.
    let outcome = dispatch(process, ActionRequest::flask(slot, binding), || {
        Ok(Some(()))
    })
    .map_err(DispatchError::into_io)?;
    if let Dispatch::Sent { .. } = outcome {
        println!(
            "{label} flask: {:.1}% -> {} (charges {}/{})",
            percent,
            binding.label(),
            charge_state.current,
            charge_state.per_use
        );
        state.mark_fired();
    }
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

#[cfg(test)]
mod tests {
    use super::{ci_like_defense, player_is_dead, FlaskState};
    use crate::game_state::VitalPool;

    fn pool(current: i32, total: i32) -> VitalPool {
        VitalPool {
            current,
            total,
            reserved_flat: 0,
            reserved_fraction: 0,
        }
    }

    #[test]
    fn post_send_guard_blocks_immediate_repeat_and_clears_notice() {
        let mut state = FlaskState::new();
        assert!(state.ready());
        state.charge_block_reported = true;

        // Successful dispatch is the only caller of mark_fired.
        state.mark_fired();
        assert!(!state.ready());
        assert!(!state.charge_block_reported);
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
