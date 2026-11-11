//! One read-only game session owns pointer resolution, freshness and sampling.
//! Accessing stored state never implicitly issues a process-memory read.
//! A caller begins a decision tick, then explicitly requests the data it needs.
use std::io;

use crate::area::AreaInfo;
use crate::game::Game;
use crate::poe1::{self, buffs::EffectsSnapshot, flasks::FlaskSnapshot};
use crate::poe2::{self, LiveVitals, VitalsProbe};
use crate::process::Process;

enum Reader {
    Poe1(poe1::Probe),
    Poe2(VitalsProbe),
}

/// Cheap, consistent values sampled during the current tick only.
#[derive(Debug, Default)]
pub struct GameSnapshot {
    pub vitals: Option<LiveVitals>,
    pub area: Option<AreaInfo>,
    pub flasks: Option<FlaskSnapshot>,
    pub effects: Option<EffectsSnapshot>,
}

impl GameSnapshot {
    fn invalidate(&mut self) {
        *self = Self::default();
    }
}

pub struct GameSession {
    reader: Reader,
    snapshot: GameSnapshot,
    flask_cache: poe1::flasks::FlaskCache,
    effect_cache: poe1::buffs::EffectCache,
}

impl GameSession {
    pub fn probe(process: &Process, game: Game) -> io::Result<Self> {
        let reader = match game {
            Game::Poe1 => Reader::Poe1(poe1::probe(process)?),
            Game::Poe2 => Reader::Poe2(poe2::probe_vitals(process)?),
        };
        Ok(Self {
            reader,
            snapshot: GameSnapshot::default(),
            flask_cache: poe1::flasks::FlaskCache::default(),
            effect_cache: poe1::buffs::EffectCache::default(),
        })
    }

    /// Mark all previously sampled values stale, without discarding stable pointer caches.
    pub fn begin_tick(&mut self) {
        self.snapshot.invalidate();
    }

    /// Invalidate everything that can change with a zone, player, or game-state transition.
    pub fn refresh(&mut self, process: &Process) -> io::Result<()> {
        self.begin_tick();
        self.flask_cache.clear();
        self.effect_cache.clear();
        self.reader = match &self.reader {
            Reader::Poe1(previous) => Reader::Poe1(poe1::refresh(process, previous)?),
            Reader::Poe2(previous) => Reader::Poe2(poe2::refresh_vitals(process, previous)?),
        };
        Ok(())
    }

    pub fn vitals(&mut self, process: &Process) -> io::Result<LiveVitals> {
        if let Some(vitals) = self.snapshot.vitals {
            return Ok(vitals);
        }
        let sampled = match &self.reader {
            Reader::Poe1(probe) => poe1::sample_vitals(process, probe)?,
            Reader::Poe2(probe) => poe2::sample_vitals(process, probe)?,
        };
        self.snapshot.vitals = Some(sampled);
        Ok(sampled)
    }

    pub fn poe1_player_metadata(&self) -> Option<&str> {
        match &self.reader {
            Reader::Poe1(probe) => Some(&probe.metadata),
            Reader::Poe2(_) => None,
        }
    }

    /// Resolve PoE1 area metadata only when the caller requests it.
    pub fn poe1_area(&mut self, process: &Process) -> io::Result<&AreaInfo> {
        if self.snapshot.area.is_none() {
            let Reader::Poe1(probe) = &self.reader else {
                return Err(unsupported());
            };
            self.snapshot.area = Some(poe1::area::read(process, probe)?);
        }
        Ok(self.snapshot.area.as_ref().expect("area just sampled"))
    }

    pub fn poe1_flasks(&mut self, process: &Process) -> io::Result<&FlaskSnapshot> {
        if self.snapshot.flasks.is_none() {
            let Reader::Poe1(probe) = &self.reader else {
                return Err(unsupported());
            };
            self.snapshot.flasks =
                Some(poe1::flasks::sample(process, probe, &mut self.flask_cache)?);
        }
        Ok(self.snapshot.flasks.as_ref().expect("flasks just sampled"))
    }

    pub fn poe1_effects(&mut self, process: &Process) -> io::Result<&EffectsSnapshot> {
        if self.snapshot.effects.is_none() {
            let Reader::Poe1(probe) = &self.reader else {
                return Err(unsupported());
            };
            self.snapshot.effects =
                Some(poe1::buffs::sample(process, probe, &mut self.effect_cache)?);
        }
        Ok(self
            .snapshot
            .effects
            .as_ref()
            .expect("effects just sampled"))
    }

    /// Existing PoE2-specific subsystems retain their proven probes during migration.
    pub fn poe2_probe(&self) -> io::Result<&VitalsProbe> {
        match &self.reader {
            Reader::Poe2(probe) => Ok(probe),
            Reader::Poe1(_) => Err(unsupported()),
        }
    }

    pub fn into_poe2_probe(self) -> io::Result<VitalsProbe> {
        match self.reader {
            Reader::Poe2(probe) => Ok(probe),
            Reader::Poe1(_) => Err(unsupported()),
        }
    }
}

fn unsupported() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        "reader does not support this game",
    )
}

#[cfg(test)]
mod tests {
    use super::GameSnapshot;
    use crate::poe2::{LiveVitals, VitalPool};

    #[test]
    fn new_tick_cannot_reuse_stale_vitals() {
        let mut snapshot = GameSnapshot::default();
        snapshot.vitals = Some(LiveVitals {
            health: VitalPool {
                current: 20,
                total: 100,
                reserved_flat: 0,
                reserved_fraction: 0,
            },
            mana: None,
            energy_shield: None,
        });
        assert_eq!(snapshot.vitals.unwrap().health.current, 20);
        snapshot.invalidate();
        assert!(snapshot.vitals.is_none());
        assert!(snapshot.area.is_none());
        assert!(snapshot.flasks.is_none());
        assert!(snapshot.effects.is_none());
    }
}
