//! Encounter exclusions used by the shared nearby census.
//! Static classifications may be cached; temporary status effects may not.
use super::buffs::MonsterEffects;

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct MonsterMods {
    pub pinnacle: bool,
    pub cannot_be_damaged: bool,
}

impl MonsterMods {
    pub fn from_name(name: &str) -> Self {
        Self {
            pinnacle: name == "PinnacleAtlasBoss",
            cannot_be_damaged: name == "MonsterCannotBeDamaged",
        }
    }

    pub fn include(&mut self, other: Self) {
        self.pinnacle |= other.pinnacle;
        self.cannot_be_damaged |= other.cannot_be_damaged;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Encounter {
    Ordinary,
    Pinnacle,
    Legion,
    Betrayal,
}

impl Encounter {
    pub fn classify(path: &str, has_npc: bool, mods: MonsterMods) -> Self {
        if path.starts_with("Metadata/Monsters/LegionLeague/") {
            Self::Legion
        } else if has_npc && path.starts_with("Metadata/Monsters/LeagueBetrayal/Betrayal") {
            Self::Betrayal
        } else if mods.pinnacle {
            Self::Pinnacle
        } else {
            Self::Ordinary
        }
    }

    pub fn allows(self, effects: MonsterEffects, unique: bool) -> bool {
        !effects.captured
            && !effects.augury_frozen
            // Campaign bosses and their phase entities may lack PinnacleAtlasBoss.
            && !(unique && effects.hidden)
            && match self {
                Self::Pinnacle | Self::Legion => !effects.hidden,
                Self::Betrayal => !effects.betrayal_safe,
                Self::Ordinary => true,
            }
    }
}

pub(super) fn is_harbinger(path: &str) -> bool {
    path.starts_with("Metadata/Monsters/Avatar/")
}

pub(super) fn is_excluded_monster_path(path: &str) -> bool {
    const NONCOMBAT: &[&str] = &[
        "Metadata/Monsters/Spirit/Tormented",
        "Metadata/Monsters/LeagueAffliction/DoodadDaemons/",
        "Metadata/Monsters/LeagueAffliction/Volatile/",
        "Metadata/Monsters/AtlasExiles/OrionArenaObjects/OrionBlockingWall",
        "Metadata/Monsters/AtlasExiles/OrionArenaObjects/OriathCivilian",
        "Metadata/Monsters/LeagueArchnemesis/LivingCrystal",
        "Metadata/Monsters/VolatileCore/VolatileCoreArchnemesis",
        "Metadata/Monsters/LeagueArchnemesis/ToxicVolatile",
        "Metadata/Monsters/SummonedPhantasm/SummonedPhantasmIncursionExplode",
        "Metadata/Monsters/InvisibleFire/MavenLaserBarrageTarget",
        "Metadata/Monsters/MavenBoss/MavenBrainOrbitDaemon",
        "Metadata/Monsters/MavenBoss/MavenBrainVoidsandDaemon",
        "Metadata/Monsters/MavenBoss/TheMavenMap",
        "Metadata/Monsters/MavenBoss/TheMavenProving",
        "Metadata/Monsters/LeagueSynthesis/SynthesisVenariusBoss",
        "Metadata/Monsters/VolatileCore/SynthesisWormholeVolatile",
        "Metadata/Monsters/LeagueAzmeri/TendrilSentinelVolatile",
    ];
    const APPARITIONS: &[&str] = &[
        "Metadata/Monsters/Daemon/DaemonElderTentacle",
        "Metadata/Monsters/AtlasBosses/TheShaperBossProjectiles",
        "Metadata/Monsters/AtlasExiles/AtlasExile5Wild",
        "Metadata/Monsters/AtlasExiles/AtlasExile5Apparition",
        "Metadata/Monsters/AtlasExiles/AtlasExile4Apparition",
        "Metadata/Monsters/AtlasExiles/AtlasExile3Apparition",
        "Metadata/Monsters/AtlasExiles/AtlasExile2Apparition",
        "Metadata/Monsters/AtlasExiles/AtlasExile1Apparition",
    ];
    NONCOMBAT.iter().any(|prefix| path.starts_with(prefix))
        || (!path.contains("Standalone")
            && APPARITIONS.iter().any(|prefix| path.starts_with(prefix)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encounter_props_and_spectators_do_not_hide_real_bosses_or_standalone_variants() {
        for path in [
            "Metadata/Monsters/MavenBoss/MavenBrainOrbitDaemon",
            "Metadata/Monsters/MavenBoss/MavenBrainVoidsandDaemon",
            "Metadata/Monsters/MavenBoss/TheMavenMap",
            "Metadata/Monsters/MavenBoss/TheMavenProving",
            "Metadata/Monsters/InvisibleFire/MavenLaserBarrageTarget",
            "Metadata/Monsters/AtlasExiles/OrionArenaObjects/OriathCivilian",
            "Metadata/Monsters/AtlasExiles/OrionArenaObjects/OrionBlockingWall",
            "Metadata/Monsters/AtlasExiles/AtlasExile5Apparition",
            "Metadata/Monsters/LeagueSynthesis/SynthesisVenariusBoss",
            "Metadata/Monsters/LeagueAzmeri/TendrilSentinelVolatile",
            "Metadata/Monsters/VolatileCore/VolatileCoreArchnemesis",
        ] {
            assert!(is_excluded_monster_path(path), "{path}");
        }
        for path in [
            "Metadata/Monsters/MavenBoss/TheMaven",
            "Metadata/Monsters/MavenBoss/MavenBrain",
            "Metadata/Monsters/AtlasExiles/AtlasExile5ApparitionStandalone",
            "Metadata/Monsters/AtlasExiles/AtlasExile5WildStandalone",
            "Metadata/Monsters/LeagueSynthesis/SynthesisBoss1",
            "Metadata/Monsters/Skeletons/Skeleton",
        ] {
            assert!(!is_excluded_monster_path(path), "{path}");
        }
    }

    #[test]
    fn hidden_pinnacle_and_legion_targets_can_return_to_combat() {
        let visible = MonsterEffects::default();
        let hidden = MonsterEffects::from_name("hidden_monster");
        let frozen_only = MonsterEffects::from_name("frozen_in_time");
        for encounter in [Encounter::Pinnacle, Encounter::Legion] {
            assert!(!encounter.allows(hidden, false));
            assert!(encounter.allows(visible, false));
            assert!(!encounter.allows(hidden, false));
        }
        // Legion monsters are attackable after revealing the monolith, while frozen.
        assert!(Encounter::Legion.allows(frozen_only, false));
        // The extension applies to Unique enemies, preserving ordinary non-Unique handling.
        assert!(Encounter::Ordinary.allows(hidden, false));
    }

    #[test]
    fn campaign_bosses_and_hearts_are_excluded_while_hidden_without_a_pinnacle_mod() {
        let hidden = MonsterEffects::from_name("hidden_monster");
        for path in [
            "Metadata/Monsters/KitavaBoss/KitavaFinal@67",
            "Metadata/Monsters/KitavaBoss/KitavaFinalHeart@67",
        ] {
            let encounter = Encounter::classify(path, false, MonsterMods::default());
            assert_eq!(encounter, Encounter::Ordinary);
            assert!(!encounter.allows(hidden, true));
            assert!(encounter.allows(MonsterEffects::default(), true));
            assert!(!encounter.allows(hidden, true));
        }
    }

    #[test]
    fn augury_frozen_bosses_are_excluded_until_thawed_in_every_encounter() {
        let frozen = MonsterEffects::from_name("augury_boss_frozen");
        for encounter in [
            Encounter::Ordinary,
            Encounter::Pinnacle,
            Encounter::Legion,
            Encounter::Betrayal,
        ] {
            assert!(!encounter.allows(frozen, false));
            assert!(encounter.allows(MonsterEffects::default(), false));
            assert!(!encounter.allows(frozen, false));
        }
    }

    #[test]
    fn captured_beasts_and_protected_betrayal_npcs_are_excluded() {
        let captured = MonsterEffects::from_name("capture_monster_trapped");
        for encounter in [
            Encounter::Ordinary,
            Encounter::Pinnacle,
            Encounter::Betrayal,
        ] {
            assert!(!encounter.allows(captured, false));
        }
        let protected = MonsterEffects::from_name("betrayal_target_safety_aura");
        assert!(!Encounter::Betrayal.allows(protected, false));
        assert!(Encounter::Betrayal.allows(MonsterEffects::default(), false));
        assert!(Encounter::Ordinary.allows(protected, false));
    }

    #[test]
    fn pinnacle_classification_uses_the_mod_and_betrayal_requires_an_npc() {
        assert_eq!(
            Encounter::classify(
                "Metadata/Monsters/MavenBoss/TheMaven",
                false,
                MonsterMods::from_name("PinnacleAtlasBoss")
            ),
            Encounter::Pinnacle
        );
        assert_eq!(
            Encounter::classify(
                "Metadata/Monsters/MavenBoss/TheMaven",
                false,
                MonsterMods::default()
            ),
            Encounter::Ordinary
        );
        let path = "Metadata/Monsters/LeagueBetrayal/BetrayalMasterNinja";
        assert_eq!(
            Encounter::classify(path, true, MonsterMods::default()),
            Encounter::Betrayal
        );
        assert_eq!(
            Encounter::classify(path, false, MonsterMods::default()),
            Encounter::Ordinary
        );
    }
}
