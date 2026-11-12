//! PoE2 validated GameStateReader adapter, independent of the shared engine.
use std::io;

use crate::game_state::{GameStateReader, LiveVitals};
use crate::process::Process;

use super::VitalsProbe;

pub struct Adapter {
    probe: VitalsProbe,
}

impl Adapter {
    pub fn probe(process: &Process) -> io::Result<Self> {
        Ok(Self {
            probe: super::probe_vitals(process)?,
        })
    }

    pub fn vitals_probe(&self) -> &VitalsProbe {
        &self.probe
    }

    pub fn into_vitals_probe(self) -> VitalsProbe {
        self.probe
    }
}

impl GameStateReader for Adapter {
    fn refresh(&mut self, process: &Process) -> io::Result<()> {
        self.probe = super::refresh_vitals(process, &self.probe)?;
        Ok(())
    }

    fn read_vitals(&self, process: &Process) -> io::Result<LiveVitals> {
        super::sample_vitals(process, &self.probe)
    }
}
