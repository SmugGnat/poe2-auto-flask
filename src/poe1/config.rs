//! PoE1-specific configuration values, utility toggles, validation and status text.
//! The shared ConfigManager and Life/Mana threshold fields remain in crate::config.
use std::collections::BTreeMap;
use std::io;

use serde::Deserialize;

use super::types::{FlaskType, UniqueFlask};
use crate::config::{invalid, Config, FlaskConfig};
use crate::input::KeyBinding;

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct Poe1Config {
    pub utility: Poe1UtilityConfig,
    pub unique: Poe1UniqueConfig,
    /// Optional gates are stored separately from Boolean unique toggles,
    /// because TOML cannot use the same key as both a bool and a table.
    pub unique_conditions: BTreeMap<UniqueFlask, UniqueConditions>,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Poe1UniqueConfig {
    pub writhing_jar: bool,
}

impl Poe1UniqueConfig {
    pub fn enabled(&self, kind: UniqueFlask) -> bool {
        match kind {
            UniqueFlask::WrithingJar => self.writhing_jar,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct UniqueConditions {
    pub nearby: Option<NearbyRule>,
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

/// Startup and hot-reload status use one compact, grouped layout.
/// Each utility's gates are displayed with that utility, never as a
/// separate repeated section.
impl Config {
    pub fn poe1_summary(&self) -> String {
        let mut flasks = Vec::new();
        for kind in crate::poe1::types::UTILITY_TYPES {
            if !self.poe1.utility.enabled(kind) {
                continue;
            }

            let mut gates = Vec::new();
            if kind == FlaskType::Quicksilver && self.poe1.quicksilver_min_run_ms != 0 {
                gates.push(format!("run >={}ms", self.poe1.quicksilver_min_run_ms));
            }
            if let Some(rule) = self.poe1.nearby.get(&kind).filter(|rule| rule.enabled) {
                gates.push(nearby_summary(
                    rule.minimum_count,
                    rule.minimum_rarity,
                    rule.radius,
                ));
            }
            flasks.push(summary_entry(kind.name(), &gates));
        }

        for unique in [UniqueFlask::WrithingJar] {
            if !self.poe1.unique.enabled(unique) {
                continue;
            }
            let mut gates = Vec::new();
            if let Some(rule) = self
                .poe1
                .unique_conditions
                .get(&unique)
                .and_then(|conditions| conditions.nearby)
                .filter(|rule| rule.enabled)
            {
                gates.push(nearby_summary(
                    rule.minimum_count,
                    rule.minimum_rarity,
                    rule.radius,
                ));
            }
            flasks.push(summary_entry(unique.name(), &gates));
        }

        let mut skills = Vec::new();
        let shell = &self.poe1.skills.molten_shell;
        if shell.enabled {
            skills.push(summary_entry(
                "Molten Shell",
                &[format!(
                    "{}, Life <={}%",
                    shell.key, shell.threshold_percent
                )],
            ));
        }
        let blood = &self.poe1.skills.blood_rage;
        if blood.enabled {
            let mut gates = vec![
                blood.key.clone(),
                format!("Life >={}%", blood.min_life_percent),
            ];
            if blood.require_nearby_monsters {
                gates.push(nearby_summary(
                    blood.nearby_minimum_count,
                    MinimumRarity::Normal,
                    blood.nearby_radius,
                ));
            }
            skills.push(summary_entry("Blood Rage", &gates));
        }

        format!(
            "Options : Hideout {}\nLife    : {}\nMana    : {}\n{}\n{}",
            if self.general.enable_in_hideout {
                "ON"
            } else {
                "OFF"
            },
            threshold_summary(&self.health),
            threshold_summary(&self.mana),
            summary_group("Flasks", &flasks),
            summary_group("Skills", &skills),
        )
    }
}

fn threshold_summary(config: &FlaskConfig) -> String {
    if config.enabled {
        format!("<={}%", config.threshold_percent)
    } else {
        "OFF".to_string()
    }
}

fn nearby_summary(count: u16, rarity: MinimumRarity, radius: u16) -> String {
    format!("nearby >={count} {rarity:?}+ @{radius}")
}

fn summary_entry(name: &str, details: &[String]) -> String {
    if details.is_empty() {
        name.to_string()
    } else {
        format!("{name} [{}]", details.join(", "))
    }
}

fn summary_group(label: &str, entries: &[String]) -> String {
    if entries.is_empty() {
        format!("{label:<7} : OFF")
    } else {
        format!("{label:<7} : {}", entries.join("\n          "))
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
        for (kind, conditions) in &self.unique_conditions {
            if let Some(rule) = conditions.nearby {
                if !(1..=150).contains(&rule.radius) || !(1..=512).contains(&rule.minimum_count) {
                    return Err(invalid(format!(
                        "poe1.unique_conditions.{kind:?}.nearby requires radius 1..=150 and minimum_count 1..=512"
                    )));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod summary_tests {
    use super::*;

    #[test]
    fn unique_status_keeps_optional_nearby_gate_with_its_owner() {
        let mut config = Config::default();
        config.poe1.unique.writhing_jar = true;
        assert!(config
            .poe1_summary()
            .contains("Flasks  : Writhing Jar\nSkills"));
        config.poe1.unique_conditions.insert(
            UniqueFlask::WrithingJar,
            UniqueConditions {
                nearby: Some(NearbyRule::default()),
            },
        );
        assert!(config
            .poe1_summary()
            .contains("Flasks  : Writhing Jar [nearby >=1 Normal+ @60]"));
    }

    #[test]
    fn grouped_status_keeps_all_conditions_with_their_owners() {
        let mut config = Config::default();
        config.health.threshold_percent = 50.0;
        config.mana.threshold_percent = 25.0;
        config.poe1.utility.quicksilver = true;
        config.poe1.utility.granite = true;
        config.poe1.quicksilver_min_run_ms = 500;
        config
            .poe1
            .nearby
            .insert(FlaskType::Granite, NearbyRule::default());
        config.poe1.skills.molten_shell.enabled = true;
        config.poe1.skills.molten_shell.key = "R".into();
        config.poe1.skills.blood_rage.enabled = true;
        config.poe1.skills.blood_rage.key = "T".into();

        assert_eq!(
            config.poe1_summary(),
            concat!(
                "Options : Hideout OFF\n",
                "Life    : <=50%\n",
                "Mana    : <=25%\n",
                "Flasks  : Quicksilver [run >=500ms]\n",
                "          Granite [nearby >=1 Normal+ @60]\n",
                "Skills  : Molten Shell [R, Life <=95%]\n",
                "          Blood Rage [T, Life >=80%, nearby >=1 Normal+ @60]",
            )
        );
    }

    #[test]
    fn grouped_status_only_shows_enabled_conditions_and_handles_empty_groups() {
        let mut config = Config::default();
        config.health.enabled = false;
        config.mana.enabled = false;
        config.general.enable_in_hideout = true;
        config.poe1.quicksilver_min_run_ms = 500;
        config
            .poe1
            .nearby
            .insert(FlaskType::Granite, NearbyRule::default());

        assert_eq!(
            config.poe1_summary(),
            "Options : Hideout ON\nLife    : OFF\nMana    : OFF\nFlasks  : OFF\nSkills  : OFF"
        );

        config.poe1.utility.granite = true;
        let rule = config.poe1.nearby.get_mut(&FlaskType::Granite).unwrap();
        rule.minimum_count = 2;
        rule.minimum_rarity = MinimumRarity::Magic;
        rule.radius = 40;
        config.poe1.skills.blood_rage.enabled = true;
        config.poe1.skills.blood_rage.require_nearby_monsters = false;

        assert_eq!(
            config.poe1_summary(),
            "Options : Hideout ON\nLife    : OFF\nMana    : OFF\nFlasks  : Granite [nearby >=2 Magic+ @40]\nSkills  : Blood Rage [T, Life >=80%]"
        );
    }
}
