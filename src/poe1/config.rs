//! PoE1-specific configuration values, utility toggles, validation and status text.
//! The shared ConfigManager and Life/Mana threshold fields remain in crate::config.
use std::collections::BTreeMap;
use std::io;

use serde::Deserialize;

use super::types::FlaskType;
use crate::config::{flask_summary, invalid, Config};

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Poe1Config {
    pub utility: Poe1UtilityConfig,
    /// Optional type-specific proximity requirements. Absent = no scan/gate.
    pub nearby: BTreeMap<FlaskType, NearbyRule>,
    /// 0 disables the optional movement gate; 100..=5000 requires continuous Run animation.
    pub quicksilver_min_run_ms: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Poe1UtilityConfig {
    pub quicksilver: bool,
    pub granite: bool,
    pub quartz: bool,
    pub diamond: bool,
    pub jade: bool,
    pub silver: bool,
    pub basalt: bool,
    pub stibnite: bool,
    pub sulphur: bool,
    pub ruby: bool,
    pub sapphire: bool,
    pub topaz: bool,
    pub amethyst: bool,
    pub bismuth: bool,
    pub aquamarine: bool,
    pub gold: bool,
    pub corundum: bool,
    pub iron: bool,
}

impl Poe1UtilityConfig {
    pub fn enabled(&self, kind: FlaskType) -> bool {
        match kind {
            FlaskType::Quicksilver => self.quicksilver,
            FlaskType::Granite => self.granite,
            FlaskType::Quartz => self.quartz,
            FlaskType::Jade => self.jade,
            FlaskType::Silver => self.silver,
            FlaskType::Basalt => self.basalt,
            FlaskType::Stibnite => self.stibnite,
            FlaskType::Sulphur => self.sulphur,
            FlaskType::Ruby => self.ruby,
            FlaskType::Sapphire => self.sapphire,
            FlaskType::Topaz => self.topaz,
            FlaskType::Amethyst => self.amethyst,
            FlaskType::Bismuth => self.bismuth,
            FlaskType::Aquamarine => self.aquamarine,
            FlaskType::Gold => self.gold,
            FlaskType::Corundum => self.corundum,
            FlaskType::Iron => self.iron,
            FlaskType::Diamond => self.diamond,
            _ => false,
        }
    }
}

/// Ordering is cumulative: Rare permits Rare or Unique, Normal permits all four.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinimumRarity {
    #[default]
    Normal,
    Magic,
    Rare,
    Unique,
}

impl MinimumRarity {
    pub fn first_index(self) -> usize {
        match self {
            Self::Normal => 0,
            Self::Magic => 1,
            Self::Rare => 2,
            Self::Unique => 3,
        }
    }
}

/// A table's presence adds a nearby gate; enabled=false temporarily disables it.
/// All numeric values use the same grid-unit scale as --debug --nearby.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct NearbyRule {
    pub enabled: bool,
    pub radius: u16,
    pub minimum_count: u16,
    pub minimum_rarity: MinimumRarity,
}

impl Default for NearbyRule {
    fn default() -> Self {
        Self {
            enabled: true,
            radius: 60,
            minimum_count: 1,
            minimum_rarity: MinimumRarity::Normal,
        }
    }
}

impl Config {
    pub fn poe1_summary(&self) -> String {
        let enabled = crate::poe1::types::UTILITY_TYPES
            .into_iter()
            .filter(|kind| self.poe1.utility.enabled(*kind))
            .map(FlaskType::name)
            .collect::<Vec<_>>();
        let base = format!(
            "{} | {} | Utility {} | Quicksilver movement {} | Hideout {}",
            flask_summary("Life", &self.health),
            flask_summary("Mana", &self.mana),
            if enabled.is_empty() {
                "OFF".to_string()
            } else {
                enabled.join(", ")
            },
            if self.poe1.quicksilver_min_run_ms == 0 {
                "OFF".to_string()
            } else {
                format!("{}ms", self.poe1.quicksilver_min_run_ms)
            },
            if self.general.enable_in_hideout {
                "ON"
            } else {
                "OFF"
            },
        );
        let rules = self
            .poe1
            .nearby
            .iter()
            .filter(|(kind, rule)| rule.enabled && self.poe1.utility.enabled(**kind))
            .map(|(kind, rule)| {
                format!(
                    "{} >= {} {:?}+ within {}",
                    kind.name(),
                    rule.minimum_count,
                    rule.minimum_rarity,
                    rule.radius
                )
            })
            .collect::<Vec<_>>();
        if rules.is_empty() {
            base
        } else {
            format!("{base} | Nearby {}", rules.join(", "))
        }
    }

}

impl Poe1Config {
    pub(crate) fn validate(&self) -> io::Result<()> {
        let ms = self.quicksilver_min_run_ms;
        if ms != 0 && !(100..=5000).contains(&ms) {
            return Err(invalid(
                "poe1.quicksilver_min_run_ms must be 0 (off) or 100..=5000",
            ));
        }
        for (kind, rule) in &self.nearby {
            if !crate::poe1::types::UTILITY_TYPES.contains(kind) {
                return Err(invalid(format!(
                    "PoE1 nearby rules only support utility flasks: {kind:?}"
                )));
            }
            if !(1..=150).contains(&rule.radius) || !(1..=512).contains(&rule.minimum_count) {
                return Err(invalid(format!(
                    "poe1.nearby.{} requires radius 1..=150 and minimum_count 1..=512",
                    kind.name().to_ascii_lowercase()
                )));
            }
        }
        Ok(())
    }
}
