//! PoE1-specific modules; validated memory code lives in reader.
pub mod activation;
pub mod adapter;
pub mod area;
pub mod automation;
pub mod bindings;
pub mod buffs;
pub mod charge_cost;
pub mod chat;
pub mod config;
pub mod debug;
pub mod decision;
pub mod flasks;
mod layout;
pub mod movement;
pub mod nearby;
mod offsets;
mod reader;
pub mod recovery;
pub mod rotation;
pub mod skill_rules;
pub mod skills;
pub mod state;
pub mod types;

pub(crate) use reader::{
    invalid, plausible_ptr, probe, read_metadata, read_player_id, read_ptr, refresh, sample_vitals,
    Probe,
};

/// Executable names accepted by the PoE1 process discovery code.
pub(crate) const PROCESS_NAMES: &[&str] = &[
    "PathOfExileSteam.exe",
    "PathOfExile.exe",
    "PathOfExile_x64.exe",
    "PathOfExile_KG.exe",
];
