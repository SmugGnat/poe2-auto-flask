//! PoE2-specific modules. Game-specific readers and policies stay here.
pub mod adapter;
pub mod area;
pub mod automation;
pub mod bindings;
pub mod buffs;
pub mod debug;
pub mod flasks;
mod layout;
mod offsets;
mod reader;

pub(crate) use reader::{probe, refresh, sample_vitals, Probe};

const FLASK_SLOT_COUNT: usize = 2;

/// Executable names accepted by the PoE2 process discovery code.
pub(crate) const PROCESS_NAMES: &[&str] = &["PathOfExileSteam.exe", "PathOfExile.exe"];
