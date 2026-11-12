//! Shared, demand-driven session façade. It owns only the cross-game
//! decision-tick snapshot. Memory-layout knowledge and game-specific caches
//! belong exclusively to the individual adapters.
use std::io;

use crate::area::AreaInfo;
use crate::game::Game;
use crate::game_state::{GameSnapshot, GameStateReader, LiveVitals};
use crate::poe1::{self, buffs::EffectsSnapshot, flasks::FlaskSnapshot};
use crate::poe2::{self, Probe as Poe2Probe};
use crate::process::Process;

enum Reader {
    Poe1(Box<poe1::adapter::Adapter>),
    Poe2(Box<poe2::adapter::Adapter>),
}

pub struct GameSession {
    reader: Reader,
    snapshot: GameSnapshot,
}

impl GameSession {
    pub fn probe(process: &Process, game: Game) -> io::Result<Self> {
        let reader = match game {
            Game::Poe1 => Reader::Poe1(Box::new(poe1::adapter::Adapter::probe(process)?)),
            Game::Poe2 => Reader::Poe2(Box::new(poe2::adapter::Adapter::probe(process)?)),
        };
        Ok(Self {
            reader,
            snapshot: GameSnapshot::default(),
        })
    }

    /// Drop only this tick's values. Adapter pointer caches remain valid until
    /// a refresh, and no optional state is read unless explicitly requested.
    pub fn begin_tick(&mut self) {
        self.snapshot.begin_tick();
        if let Reader::Poe1(adapter) = &mut self.reader {
            adapter.begin_tick();
        }
    }

    /// Invalidate all cached state before retrying game-specific reconnection.
    pub fn refresh(&mut self, process: &Process) -> io::Result<()> {
        self.begin_tick();
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.refresh(process),
            Reader::Poe2(adapter) => adapter.refresh(process),
        }
    }

    /// Shared, validated vitals are read at most once per decision tick.
    pub fn vitals(&mut self, process: &Process) -> io::Result<LiveVitals> {
        if let Some(value) = self.snapshot.vitals {
            return Ok(value);
        }
        let value = match &self.reader {
            Reader::Poe1(adapter) => adapter.read_vitals(process)?,
            Reader::Poe2(adapter) => adapter.read_vitals(process)?,
        };
        self.snapshot.vitals = Some(value);
        Ok(value)
    }

    /// Optional game-specific capabilities remain available without forcing
    /// one game's raw state or memory layout onto the other adapter.
    pub fn poe1_player_metadata(&self) -> Option<&str> {
        match &self.reader {
            Reader::Poe1(adapter) => Some(adapter.player_metadata()),
            Reader::Poe2(_) => None,
        }
    }

    /// Directly verify current identity and vitals before a PoE1 key press.
    pub fn poe1_verify_vitals(&self, process: &Process) -> io::Result<LiveVitals> {
        match &self.reader {
            Reader::Poe1(adapter) => adapter.verify_vitals(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_chat_active(&self, process: &Process) -> io::Result<bool> {
        match &self.reader {
            Reader::Poe1(adapter) => adapter.chat_active(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_animation(&mut self, process: &Process) -> io::Result<i32> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.animation(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_verify_animation(&mut self, process: &Process) -> io::Result<i32> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.verify_animation(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_skill_readiness(
        &mut self,
        process: &Process,
        skill: poe1::skills::SkillId,
    ) -> io::Result<poe1::skills::SkillReadiness> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.skill_readiness(process, skill),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_nearby(&mut self, process: &Process) -> io::Result<poe1::nearby::NearbySnapshot> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.nearby(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_area(&mut self, process: &Process) -> io::Result<&AreaInfo> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.area(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_flasks(&mut self, process: &Process) -> io::Result<&FlaskSnapshot> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.flasks(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    pub fn poe1_effects(&mut self, process: &Process) -> io::Result<&EffectsSnapshot> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.effects(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    /// Fresh PoE1 status-effect read; used only immediately before skill input.
    pub fn poe1_verify_effects(&mut self, process: &Process) -> io::Result<EffectsSnapshot> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.verify_effects(process),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    /// Fresh PoE1 skill-usability check. Does not reuse the tick snapshot.
    pub fn poe1_verify_skill_readiness(
        &mut self,
        process: &Process,
        skill: poe1::skills::SkillId,
    ) -> io::Result<poe1::skills::SkillReadiness> {
        match &mut self.reader {
            Reader::Poe1(adapter) => adapter.verify_skill_readiness(process, skill),
            Reader::Poe2(_) => Err(unsupported()),
        }
    }

    /// Existing PoE2-specialized probes remain accessible for its proven
    /// automation policy; they migrate during the later engine stage.
    pub fn poe2_probe(&self) -> io::Result<&Poe2Probe> {
        match &self.reader {
            Reader::Poe2(adapter) => Ok(adapter.current_probe()),
            Reader::Poe1(_) => Err(unsupported()),
        }
    }

    pub fn into_poe2_probe(self) -> io::Result<Poe2Probe> {
        match self.reader {
            Reader::Poe2(adapter) => Ok(adapter.into_probe()),
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
    use super::unsupported;
    use std::io;

    #[test]
    fn unsupported_capability_is_an_explicit_error() {
        assert_eq!(unsupported().kind(), io::ErrorKind::InvalidInput);
    }
}
