//! PoE2 validated GameStateReader adapter, independent of the shared engine.
use std::io;

use crate::game_state::{GameStateReader, LiveVitals};
use crate::process::Process;

use super::Probe;

pub struct Adapter {
    probe: Probe,
}

impl Adapter {
    pub fn probe(process: &Process) -> io::Result<Self> {
        Ok(Self {
            probe: super::probe(process)?,
        })
    }

    pub fn current_probe(&self) -> &Probe {
        &self.probe
    }

    pub fn into_probe(self) -> Probe {
        self.probe
    }
}

impl GameStateReader for Adapter {
    fn refresh(&mut self, process: &Process) -> io::Result<()> {
        self.probe = super::refresh(process, &self.probe)?;
        Ok(())
    }

    fn read_vitals(&self, process: &Process) -> io::Result<LiveVitals> {
        super::sample_vitals(process, &self.probe)
    }
}
