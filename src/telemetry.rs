//! Presentation-only reads, using independent handles and the validated adapters.
//! This module never participates in automation decisions or sends input.
use std::io;

use crate::game::Game;
use crate::game_state::VitalPool;
use crate::process::Process;
use crate::session::GameSession;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pool {
    pub current: i32,
    pub maximum: i32,
}

impl From<VitalPool> for Pool {
    fn from(value: VitalPool) -> Self {
        Self {
            current: value.current.max(0),
            maximum: value.usable_max(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub life: Pool,
    pub mana: Option<Pool>,
    pub shield: Option<Pool>,
    pub area: String,
    pub character_name: Option<String>,
}

pub struct Reader {
    process: Process,
    session: GameSession,
    game: Game,
}

impl Reader {
    pub fn open(game: Game, pid: u32) -> io::Result<Self> {
        let process = Process::reader_for_pid(pid, game)?;
        process.validate_pe_headers()?;
        let session = GameSession::probe(&process, game)?;
        Ok(Self {
            process,
            session,
            game,
        })
    }

    pub fn sample(&mut self) -> io::Result<Snapshot> {
        // Refresh the presentation session independently after area/player
        // changes. A failed read clears the UI; it cannot affect automation.
        self.session.refresh(&self.process)?;
        let vitals = self.session.vitals(&self.process)?;
        let area = match self.game {
            Game::Poe1 => self.session.poe1_area(&self.process)?.name.clone(),
            Game::Poe2 => {
                crate::poe2::area::resolve_area_info(&self.process, self.session.poe2_probe()?)?
                    .name
            }
        };
        Ok(Snapshot {
            life: vitals.health.into(),
            mana: vitals.mana.map(Into::into),
            // PoE1 validates all three pools, then represents a verified zero
            // pool as None. Keep unknown PoE2 offsets distinct from that zero.
            shield: display_shield(self.game, vitals.energy_shield),
            area,
            character_name: self.session.poe1_character_name(&self.process).ok(),
        })
    }
}

fn display_shield(game: Game, pool: Option<VitalPool>) -> Option<Pool> {
    pool.map(Into::into).or_else(|| {
        (game == Game::Poe1).then_some(Pool {
            current: 0,
            maximum: 0,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_pool_uses_unreserved_maximum_and_handles_death() {
        let pool = Pool::from(VitalPool {
            current: -1,
            total: 1000,
            reserved_flat: 100,
            reserved_fraction: 2000,
        });
        assert_eq!(
            pool,
            Pool {
                current: 0,
                maximum: 700
            }
        );
    }

    #[test]
    fn verified_absent_poe1_shield_is_zero_and_unknown_poe2_shield_stays_unknown() {
        assert_eq!(
            display_shield(Game::Poe1, None),
            Some(Pool {
                current: 0,
                maximum: 0
            })
        );
        assert_eq!(display_shield(Game::Poe2, None), None);
    }
}
