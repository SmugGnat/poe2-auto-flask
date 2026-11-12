//! Chat safety gate based on GameHelper's 2026-09 UI fields.
//! Unknown or unreadable UI state blocks automation.
use std::io;

use super::offsets::*;
use super::{read_ptr, Probe};
use crate::process::Process;

const CHAT_ALPHA_ACTIVE: u32 = 0x8C;

pub fn is_chat_active(process: &Process, probe: &Probe) -> io::Result<bool> {
    let ui = read_ptr(process, probe.in_game_state + IN_GAME_UI)?;
    let chat = read_ptr(process, ui + CHAT_PARENT)?;
    let bg_color = process.read_i32(chat + CHAT_BACKGROUND_COLOR)? as u32;
    Ok(is_active_alpha(bg_color))
}

fn is_active_alpha(background: u32) -> bool {
    (background >> 24) >= CHAT_ALPHA_ACTIVE
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_and_open_chat_colors() {
        assert!(!is_active_alpha(0x00FFFFFF));
        assert!(!is_active_alpha(0x8B000000));
        assert!(is_active_alpha(0x8C000000));
        assert!(is_active_alpha(0xFF000000));
    }
}
