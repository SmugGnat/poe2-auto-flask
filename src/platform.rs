use windows_sys::{
    s,
    Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress},
};

/// Wine/Proton exports `wine_get_version` from ntdll. Native Windows does not.
/// This is checked once at startup so lifecycle policy has no steady-state cost.
pub fn running_under_wine() -> bool {
    unsafe {
        let ntdll = GetModuleHandleA(s!("ntdll.dll"));
        !ntdll.is_null() && GetProcAddress(ntdll, s!("wine_get_version")).is_some()
    }
}
