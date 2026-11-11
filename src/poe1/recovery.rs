//! PoE1 flask recovery classification, cached when the equipped item changes.
//! Only well-defined Mod.dat identifiers may authorize emergency bypass.
//! Unknown/unsupported mod IDs are treated as gradual, never instant.
use crate::poe2::VitalPool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RecoveryKind {
    #[default]
    Gradual,
    Instant,
    InstantOnLowLife,
    PartialInstant,
}

impl RecoveryKind {
    pub fn from_mod_name(name: &str) -> Option<Self> {
        // PoE1 Mods.dat IDs, not translated/visible affix labels. Match the
        // full family name and an optional numeric tier only; don't confuse
        // "more recovery on low life" with instant recovery.
        if mod_family(name, "FlaskInstantRecoveryOnLowLife") {
            Some(Self::InstantOnLowLife)
        } else if mod_family(name, "FlaskInstantRecovery") {
            Some(Self::Instant)
        } else if mod_family(name, "FlaskPartialInstantRecovery")
            || mod_family(name, "FlaskRecoveryPartialInstant")
        {
            Some(Self::PartialInstant)
        } else {
            None
        }
    }

    /// Fully instant *at this Life value*. Conditional prefixes do not turn
    /// instant above Low Life, even if the user's normal threshold is higher.
    pub fn instant_at_life(self, life: VitalPool) -> bool {
        match self {
            Self::Instant => true,
            Self::InstantOnLowLife => is_low_life(life),
            Self::Gradual | Self::PartialInstant => false,
        }
    }

    pub fn eligible_for_normal_life(self, life: VitalPool) -> bool {
        self != Self::InstantOnLowLife || is_low_life(life)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Gradual => "gradual/unknown",
            Self::Instant => "instant",
            Self::InstantOnLowLife => "instant on Low Life",
            Self::PartialInstant => "partially instant",
        }
    }
}

fn mod_family(name: &str, family: &str) -> bool {
    name.strip_prefix(family).is_some_and(|suffix| {
        suffix.is_empty() || (!suffix.starts_with('0')
            && suffix.bytes().all(|byte| byte.is_ascii_digit()))
    })
}

/// PoE1 Low Life = <= 50% of *maximum* Life, not unreserved Life.
/// Use i64 in case either field grows; the memory reader validates totals.
pub fn is_low_life(life: VitalPool) -> bool {
    life.total > 0 && life.current > 0
        && i64::from(life.current) * 2 <= i64::from(life.total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn life(current: i32, total: i32) -> VitalPool {
        VitalPool { current, total, reserved_flat: 0, reserved_fraction: 0 }
    }

    #[test]
    fn only_specific_mod_id_families_classify_as_instant() {
        assert_eq!(RecoveryKind::from_mod_name("FlaskInstantRecoveryOnLowLife2"), Some(RecoveryKind::InstantOnLowLife));
        assert_eq!(RecoveryKind::from_mod_name("FlaskInstantRecoveryOnLowLife5"), Some(RecoveryKind::InstantOnLowLife));
        assert_eq!(RecoveryKind::from_mod_name("FlaskInstantRecovery1"), Some(RecoveryKind::Instant));
        assert_eq!(RecoveryKind::from_mod_name("FlaskPartialInstantRecovery1"), Some(RecoveryKind::PartialInstant));
        assert_eq!(RecoveryKind::from_mod_name("FlaskIncreasedRecoveryOnLowLife1"), None);
        assert_eq!(RecoveryKind::from_mod_name("FlaskInstantRecoveryAtEnd"), None);
        assert_eq!(RecoveryKind::from_mod_name("FlaskInstantRecovery1More"), None);
        assert_eq!(RecoveryKind::from_mod_name("FlaskInstantRecoveryOnLowLifeX"), None);
    }

    #[test]
    fn conditional_instant_requires_at_most_half_actual_max() {
        let kind = RecoveryKind::InstantOnLowLife;
        assert!(!kind.instant_at_life(life(501, 1000)));
        assert!(kind.instant_at_life(life(500, 1000)));
        assert!(kind.instant_at_life(life(100, 1000)));
        assert!(!kind.instant_at_life(life(0, 1000)));
        let reserved = VitalPool {
            reserved_flat: 400,
            ..life(500, 1000)
        };
        assert!(kind.instant_at_life(reserved));
    }

    #[test]
    fn partial_instant_cannot_claim_full_instant_emergency_bypass() {
        assert!(!RecoveryKind::PartialInstant.instant_at_life(life(200, 1000)));
        assert!(RecoveryKind::Instant.instant_at_life(life(600, 1000)));
    }
}
