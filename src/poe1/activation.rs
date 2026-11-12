//! PoE1 input settling: 350ms per utility slot; recovery uses state feedback.
//! Life/Mana share a one-tick guard across all duplicate slots. Once the game
//! reports spent charges, a resource increase, or a fresh resource loss, the
//! ordinary vitals/effects decision is allowed to run again without a fixed
//! 900ms cooldown. No additional process memory reads are performed here.
use std::time::{Duration, Instant};

use crate::game_state::LiveVitals;

use super::buffs::EffectsSnapshot;
use super::flasks::{FlaskSnapshot, SLOT_COUNT};
use super::types::FlaskType;

pub const SETTLING_GUARD: Duration = Duration::from_millis(350);
pub const RECOVERY_SETTLING_GUARD: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Copy)]
struct RecoveryAttempt {
    slot: usize,
    sent_at: Instant,
    charges_before: i32,
    resource_before: i32,
    last_resource: i32,
}

#[derive(Debug, Default)]
pub struct ActivationGuard {
    last_sent: [Option<(FlaskType, Instant)>; SLOT_COUNT],
    life: Option<RecoveryAttempt>,
    mana: Option<RecoveryAttempt>,
}

impl ActivationGuard {
    pub fn reset(&mut self) {
        self.last_sent.fill(None);
        self.life = None;
        self.mana = None;
    }
    /// Queried after apply_to() has consumed any acknowledged recovery.
    /// An unacknowledged input cannot use the active-Life emergency bypass.
    pub fn life_input_pending(&self) -> bool {
        self.life.is_some()
    }

    /// Record exactly the sampled, pre-send charges and most recently verified
    /// vitals. Only called after SendInput succeeded.
    pub fn mark(
        &mut self,
        slot: usize,
        kind: FlaskType,
        now: Instant,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
    ) {
        let Some(entry) = self.last_sent.get_mut(slot) else {
            return;
        };
        *entry = Some((kind, now));
        if matches!(kind, FlaskType::Life | FlaskType::Mana) {
            let charges_before = flasks.slots[slot]
                .as_ref()
                .filter(|flask| flask.flask_type == kind)
                .map(|flask| flask.current_charges);
            if let (Some(charges_before), Some(resource_before)) =
                (charges_before, resource_current(vitals, kind))
            {
                let attempt = RecoveryAttempt {
                    slot,
                    sent_at: now,
                    charges_before,
                    resource_before,
                    last_resource: resource_before,
                };
                match kind {
                    FlaskType::Life => self.life = Some(attempt),
                    FlaskType::Mana => self.mana = Some(attempt),
                    _ => (),
                }
            }
        }
    }

    /// Adds temporary virtual effects only to the current decision snapshot.
    /// Active in-game effects are never removed or overwritten.
    pub fn apply_to(
        &mut self,
        now: Instant,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
        effects: &mut EffectsSnapshot,
    ) {
        // Utility key settling stays per-slot and must never delay Life/Mana.
        for (slot, last) in self.last_sent.iter().enumerate() {
            if let Some((kind, sent_at)) = last {
                if !matches!(kind, FlaskType::Life | FlaskType::Mana)
                    && now.saturating_duration_since(*sent_at) < SETTLING_GUARD
                    && flasks.slots[slot]
                        .as_ref()
                        .is_some_and(|flask| flask.flask_type == *kind)
                {
                    effects.slots_active[slot] = true;
                }
            }
        }

        Self::apply_recovery(
            &mut self.life,
            FlaskType::Life,
            now,
            vitals,
            flasks,
            effects,
        );
        Self::apply_recovery(
            &mut self.mana,
            FlaskType::Mana,
            now,
            vitals,
            flasks,
            effects,
        );
    }

    fn apply_recovery(
        pending: &mut Option<RecoveryAttempt>,
        kind: FlaskType,
        now: Instant,
        vitals: LiveVitals,
        flasks: &FlaskSnapshot,
        effects: &mut EffectsSnapshot,
    ) {
        let Some(attempt) = pending.as_mut() else {
            return;
        };
        let resource_now = resource_current(vitals, kind);
        let charges_now = flasks.slots[attempt.slot]
            .as_ref()
            .filter(|flask| flask.flask_type == kind)
            .map(|flask| flask.current_charges);

        // The one-tick guard bridges SendInput to the next usable sample.
        // A decrease in charges confirms consumption even if instant healing
        // was offset by damage. An increase in the resource confirms recovery.
        // Fresh damage / mana loss is a separate reason to try again.
        let response = charges_now.is_some_and(|charges| charges < attempt.charges_before)
            || resource_now.is_some_and(|value| value > attempt.resource_before);
        let fresh_loss = resource_now.is_some_and(|value| value < attempt.last_resource);
        if now.saturating_duration_since(attempt.sent_at) >= RECOVERY_SETTLING_GUARD
            && (response || fresh_loss)
        {
            *pending = None;
            return;
        }
        if let Some(value) = resource_now {
            attempt.last_resource = value;
        }

        // Any same-type slot blocks the entire type, including other equipped
        // instant flasks. It never blocks the opposite recovery resource.
        if let Some(slot) = flasks
            .slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|flask| flask.flask_type == kind))
        {
            effects.slots_active[slot] = true;
        }
    }
}

fn resource_current(vitals: LiveVitals, kind: FlaskType) -> Option<i32> {
    match kind {
        FlaskType::Life => Some(vitals.health.current),
        FlaskType::Mana => vitals.mana.map(|mana| mana.current),
        _ => None,
    }
}

#[cfg(test)]
#[path = "activation_tests.rs"]
mod tests;
