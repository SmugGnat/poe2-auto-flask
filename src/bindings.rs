//! Cross-game parsing of PoE flask bindings and Windows Documents lookup.
//! Game-specific INI discovery and reload policy belong to poe1/ or poe2/.
use std::ffi::OsString;
use std::io;
use std::os::windows::ffi::OsStringExt;
use std::path::{Path, PathBuf};
use std::ptr::null_mut;

use windows_sys::Win32::UI::Shell::{SHGetFolderPathW, CSIDL_PERSONAL, SHGFP_TYPE_CURRENT};

use crate::input::KeyBinding;

pub(crate) fn documents_path() -> io::Result<PathBuf> {
    const MAX_PATH: usize = 260;
    let mut buffer = [0u16; MAX_PATH];
    let result = unsafe {
        SHGetFolderPathW(
            null_mut(),
            CSIDL_PERSONAL as i32,
            null_mut(),
            SHGFP_TYPE_CURRENT as u32,
            buffer.as_mut_ptr(),
        )
    };

    if result < 0 {
        if let Some(profile) = std::env::var_os("USERPROFILE") {
            return Ok(PathBuf::from(profile).join("Documents"));
        }
        return Err(io::Error::other(format!(
            "could not resolve Windows Documents folder (HRESULT 0x{:08X})",
            result as u32
        )));
    }

    let len = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(MAX_PATH);
    Ok(PathBuf::from(OsString::from_wide(&buffer[..len])))
}

pub(crate) fn parse_binding(path: &Path, name: &str, value: &str) -> io::Result<KeyBinding> {
    let mut parts = value.split_whitespace();
    let vk = parts
        .next()
        .ok_or_else(|| invalid(path, format!("{name} is empty")))?
        .parse::<u16>()
        .map_err(|_| invalid(path, format!("{name} has invalid key code {value:?}")))?;
    let modifier = match parts.next() {
        None => 0,
        Some(value) => value
            .parse::<u8>()
            .map_err(|_| invalid(path, format!("{name} has invalid modifier {value:?}")))?,
    };
    if parts.next().is_some() {
        return Err(invalid(
            path,
            format!("{name} has unexpected data {value:?}"),
        ));
    }
    KeyBinding::from_poe(vk, modifier).map_err(|error| {
        invalid(
            path,
            format!("{name} binding {value:?} is unsupported: {error}"),
        )
    })
}

pub(crate) fn invalid(path: &Path, message: impl Into<String>) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("{}: {}", path.display(), message.into()),
    )
}

