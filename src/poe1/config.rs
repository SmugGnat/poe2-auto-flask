//! PoE1-specific configuration values, utility toggles, validation and status text.
//! The shared ConfigManager and Life/Mana threshold fields remain in crate::config.
use std::collections::BTreeMap;
use std::io;

use serde::Deserialize;

use super::types::FlaskType;
use crate::config::{flask_summary, invalid, Config};
use crate::input::KeyBinding;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Poe1Config {
    pub utility: Poe1UtilityConfig,
    pub skills: Poe1SkillsConfig,
    /// Optional type-specific proximity requirements. Absent = no scan/gate.
    pub nearby: BTreeMap<FlaskType, NearbyRule>,
    /// 0 disables the optional movement gate; 100..=5000 requires continuous Run animation.
    pub quicksilver_min_run_ms: u64,
}

/// Skill-specific settings remain explicit, rather than forcing every skill to
/// use a Life threshold or the same conditions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Poe1SkillsConfig {
    pub molten_shell: MoltenShellConfig,
    pub blood_rage: BloodRageConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct MoltenShellConfig {
    pub enabled: bool,
    pub threshold_percent: u8,
    /// Windows keyboard key; the user must bind the same key to Molten Shell in PoE1.
    pub key: String,
}

impl Default for MoltenShellConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold_percent: 95,
            key: "Q".to_string(),
        }
    }
}

impl MoltenShellConfig {
    pub fn binding(&self) -> io::Result<KeyBinding> {
        skill_binding(&self.key, "poe1.skills.molten_shell.key")
    }

    pub fn validate(&self) -> io::Result<()> {
        if !(1..=99).contains(&self.threshold_percent) {
            return Err(invalid(
                "poe1.skills.molten_shell.threshold_percent must be 1..=99",
            ));
        }
        self.binding()?;
        Ok(())
    }
}

/// Optional self-damaging buff preset. Nearby monsters are required by
/// default so Blood Rage doesn't activate automatically while idle.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct BloodRageConfig {
    pub enabled: bool,
    pub key: String,
    pub min_life_percent: u8,
    pub require_nearby_monsters: bool,
    pub nearby_radius: u16,
    pub nearby_minimum_count: u16,
}

impl Default for BloodRageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            key: "T".to_string(),
            min_life_percent: 80,
            require_nearby_monsters: true,
            nearby_radius: 60,
            nearby_minimum_count: 1,
        }
    }
}

impl BloodRageConfig {
    pub fn binding(&self) -> io::Result<KeyBinding> {
        skill_binding(&self.key, "poe1.skills.blood_rage.key")
    }

    pub fn validate(&self) -> io::Result<()> {
        if !(1..=100).contains(&self.min_life_percent) {
            return Err(invalid(
                "poe1.skills.blood_rage.min_life_percent must be 1..=100",
            ));
        }
        if !(1..=150).contains(&self.nearby_radius) {
            return Err(invalid(
                "poe1.skills.blood_rage.nearby_radius must be 1..=150",
            ));
        }
        if !(1..=512).contains(&self.nearby_minimum_count) {
            return Err(invalid(
                "poe1.skills.blood_rage.nearby_minimum_count must be 1..=512",
            ));
        }
        self.binding()?;
        Ok(())
    }
}

fn skill_binding(key: &str, config_path: &str) -> io::Result<KeyBinding> {
    let key = key.trim().to_ascii_uppercase();
    let invalid_format = || invalid(format!("{config_path} must be A-Z, 0-9 or F1-F12"));
    let vk = match key.as_bytes() {
        [value] if value.is_ascii_uppercase() || value.is_ascii_digit() => u16::from(*value),
        _ => {
            let number = key
                .strip_prefix('F')
                .and_then(|rest| rest.parse::<u16>().ok())
                .ok_or_else(invalid_format)?;
            if !(1..=12).contains(&number) {
                return Err(invalid_format());
            }
            0x6F + number
        }
    };
    if vk == 0x7A {
        return Err(invalid(format!("{config_path} cannot be F11 (arm toggle)")));
    }
    KeyBinding::from_poe(vk, 0)
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
        let base = if rules.is_empty() {
            base
        } else {
            format!("{base} | Nearby {}", rules.join(", "))
        };
        let mut base = base;
        let shell = &self.poe1.skills.molten_shell;
        if shell.enabled {
            base.push_str(&format!(
                " | Molten Shell <= {}% ({})",
                shell.threshold_percent, shell.key
            ));
        }
        let blood = &self.poe1.skills.blood_rage;
        if blood.enabled {
            let nearby = if blood.require_nearby_monsters {
                format!(
                    "{} within {}",
                    blood.nearby_minimum_count, blood.nearby_radius
                )
            } else {
                "OFF".to_string()
            };
            base.push_str(&format!(
                " | Blood Rage >= {}% ({}, nearby {})",
                blood.min_life_percent, blood.key, nearby
            ));
        }
        base
    }
}

impl Poe1Config {
    pub(crate) fn validate(&self) -> io::Result<()> {
        self.skills.molten_shell.validate()?;
        self.skills.blood_rage.validate()?;
        if self.skills.molten_shell.enabled
            && self.skills.blood_rage.enabled
            && self.skills.molten_shell.binding()? == self.skills.blood_rage.binding()?
        {
            return Err(invalid(
                "enabled PoE1 skill presets cannot use the same key",
            ));
        }
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
