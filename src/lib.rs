// Preserve the existing memory-reader implementation during host integration.
#![allow(clippy::chunks_exact_to_as_chunks, clippy::manual_is_multiple_of)]

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub mod config;
pub mod file_watch;
pub mod game;
pub mod input;
pub mod launch;
pub mod profiles;
pub mod runtime;

#[cfg(windows)]
pub mod backend;
#[cfg(windows)]
pub mod poe1;
#[cfg(not(windows))]
#[path = "poe1/shared.rs"]
pub mod poe1;
#[cfg(windows)]
pub mod telemetry;

#[cfg(windows)]
mod action;
#[cfg(windows)]
mod area;
#[cfg(windows)]
mod bindings;
#[cfg(windows)]
mod diagnostics;
#[cfg(windows)]
mod engine;
#[cfg(windows)]
mod entity;
#[cfg(windows)]
mod game_state;
#[cfg(windows)]
pub mod platform;
#[cfg(windows)]
mod poe2;
#[cfg(windows)]
mod process;
#[cfg(windows)]
mod session;
