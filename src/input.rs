use std::io;
use std::mem::size_of;

use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
    VK_CONTROL, VK_F11, VK_MENU, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyBinding {
    pub vk: u16,
    pub modifier: KeyModifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyModifier {
    None,
    Shift,
    Control,
    Alt,
}

impl KeyBinding {
    #[cfg(test)]
    pub const fn new(vk: u16, modifier: KeyModifier) -> Self {
        Self { vk, modifier }
    }

    pub fn from_poe(vk: u16, modifier: u8) -> io::Result<Self> {
        if vk == 0 || vk > 0xFF {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("virtual-key code {vk} is outside the Win32 range"),
            ));
        }

        // PoE2 may encode mouse-button bindings as virtual-key values. Their config encoding is
        // not currently supported, so fail closed instead of treating a mouse button as a key.
        if matches!(vk, 0x01 | 0x02 | 0x04 | 0x05 | 0x06) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "mouse-button flask bindings are not supported yet",
            ));
        }

        let modifier = match modifier {
            0 => KeyModifier::None,
            1 => KeyModifier::Shift,
            2 => KeyModifier::Control,
            3 => KeyModifier::Alt,
            value => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("unknown PoE2 modifier value {value}"),
                ));
            }
        };
        Ok(Self { vk, modifier })
    }

    pub fn label(self) -> String {
        let key = key_label(self.vk);
        match self.modifier {
            KeyModifier::None => key,
            KeyModifier::Shift => format!("Shift+{key}"),
            KeyModifier::Control => format!("Ctrl+{key}"),
            KeyModifier::Alt => format!("Alt+{key}"),
        }
    }
}

pub fn master_toggle_down() -> bool {
    unsafe { (GetAsyncKeyState(VK_F11 as i32) as u16 & 0x8000) != 0 }
}

pub fn is_process_foreground(pid: u32) -> bool {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        return false;
    }

    let mut foreground_pid = 0u32;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut foreground_pid);
    }
    foreground_pid == pid
}

pub fn send_flask_key(binding: KeyBinding) -> io::Result<()> {
    if let Some(modifier_vk) = modifier_vk(binding.modifier) {
        let inputs = [
            keyboard_input(modifier_vk, false),
            keyboard_input(binding.vk, false),
            keyboard_input(binding.vk, true),
            keyboard_input(modifier_vk, true),
        ];
        send_inputs(&inputs)
    } else {
        let inputs = [
            keyboard_input(binding.vk, false),
            keyboard_input(binding.vk, true),
        ];
        send_inputs(&inputs)
    }
}

fn modifier_vk(modifier: KeyModifier) -> Option<u16> {
    match modifier {
        KeyModifier::None => None,
        KeyModifier::Shift => Some(VK_SHIFT),
        KeyModifier::Control => Some(VK_CONTROL),
        KeyModifier::Alt => Some(VK_MENU),
    }
}

fn send_inputs(inputs: &[INPUT]) -> io::Result<()> {
    let sent = unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            size_of::<INPUT>() as i32,
        )
    };
    if sent != inputs.len() as u32 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn keyboard_input(vk: u16, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

fn key_label(vk: u16) -> String {
    if (b'0' as u16..=b'9' as u16).contains(&vk) || (b'A' as u16..=b'Z' as u16).contains(&vk) {
        return char::from_u32(vk as u32).unwrap_or('?').to_string();
    }
    if (0x70..=0x87).contains(&vk) {
        return format!("F{}", vk - 0x6F);
    }

    match vk {
        0x08 => "Backspace".to_string(),
        0x09 => "Tab".to_string(),
        0x0D => "Enter".to_string(),
        0x1B => "Esc".to_string(),
        0x20 => "Space".to_string(),
        0x21 => "PageUp".to_string(),
        0x22 => "PageDown".to_string(),
        0x23 => "End".to_string(),
        0x24 => "Home".to_string(),
        0x25 => "Left".to_string(),
        0x26 => "Up".to_string(),
        0x27 => "Right".to_string(),
        0x28 => "Down".to_string(),
        0x2D => "Insert".to_string(),
        0x2E => "Delete".to_string(),
        _ => format!("VK_0x{vk:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyBinding, KeyModifier};

    #[test]
    fn poe_modifier_values_map_to_supported_modifiers() {
        assert_eq!(
            KeyBinding::from_poe(81, 0).unwrap().modifier,
            KeyModifier::None
        );
        assert_eq!(
            KeyBinding::from_poe(81, 1).unwrap().modifier,
            KeyModifier::Shift
        );
        assert_eq!(
            KeyBinding::from_poe(81, 2).unwrap().modifier,
            KeyModifier::Control
        );
        assert_eq!(
            KeyBinding::from_poe(81, 3).unwrap().modifier,
            KeyModifier::Alt
        );
        assert!(KeyBinding::from_poe(81, 4).is_err());
    }

    #[test]
    fn binding_labels_are_readable() {
        assert_eq!(KeyBinding::new(81, KeyModifier::None).label(), "Q");
        assert_eq!(KeyBinding::new(81, KeyModifier::Shift).label(), "Shift+Q");
        assert_eq!(
            KeyBinding::new(0x70, KeyModifier::Control).label(),
            "Ctrl+F1"
        );
    }
}
