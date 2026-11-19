//! Windows console attachment and per-configuration-directory instance ownership.
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io;
use std::path::Path;
use std::ptr::null;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

use slint::winit_030::WinitWindowAccessor;
use slint::ComponentHandle;
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0,
};
use windows_sys::Win32::System::Console::{
    AllocConsole, AttachConsole, GetStdHandle, ATTACH_PARENT_PROCESS, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject,
};

use crate::{AppWindow, Presentation};

fn native_handle(window: &slint::Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use slint::winit_030::winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    window
        .with_winit_window(|native| {
            let handle = native.window_handle().ok()?;
            match handle.as_raw() {
                RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as _),
                _ => None,
            }
        })
        .flatten()
}

pub fn prepare_background(window: &slint::Window) -> io::Result<()> {
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows_sys::Win32::Graphics::Gdi::{
        FillRect, GetStockObject, SetDCBrushColor, DC_BRUSH, HDC,
    };
    use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetClientRect, WM_ERASEBKGND, WM_NCDESTROY};
    const BACKGROUND_SUBCLASS: usize = 0xAF110;

    unsafe extern "system" fn background(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        id: usize,
        _data: usize,
    ) -> LRESULT {
        // Winit has no class background brush. An opaque Wine window can expose
        // a white backing surface between mapping and Slint's first present.
        // Handle only native background erasure; Slint still owns every frame.
        unsafe {
            if message == WM_ERASEBKGND {
                let dc = wparam as HDC;
                let mut rect = RECT::default();
                if !dc.is_null() && GetClientRect(hwnd, &mut rect) != 0 {
                    // COLORREF is 0x00BBGGRR; match Window.background (#111113).
                    let previous = SetDCBrushColor(dc, 0x00131111);
                    let painted = FillRect(dc, &rect, GetStockObject(DC_BRUSH));
                    SetDCBrushColor(dc, previous);
                    if painted != 0 {
                        return 1;
                    }
                }
            } else if message == WM_NCDESTROY {
                RemoveWindowSubclass(hwnd, Some(background), id);
            }
            DefSubclassProc(hwnd, message, wparam, lparam)
        }
    }

    let hwnd =
        native_handle(window).ok_or_else(|| io::Error::other("Native window unavailable"))?;
    // Called on the UI thread before the HWND is mapped. The subclass is
    // removed at destruction and uses a stock brush, with no owned resources.
    if unsafe { SetWindowSubclass(hwnd, Some(background), BACKGROUND_SUBCLASS, 0) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub fn repaint(window: &slint::Window) {
    use windows_sys::Win32::Graphics::Gdi::{RedrawWindow, RDW_INVALIDATE, RDW_NOCHILDREN};
    if let Some(hwnd) = native_handle(window) {
        // Wine can discard the presented surface across Alt+Tab without a
        // useful damage region. Slint 1.19 reads GetUpdateRect before painting.
        // Invalidate the whole client once on activation, without a repaint loop.
        unsafe {
            RedrawWindow(
                hwnd,
                null(),
                std::ptr::null_mut(),
                RDW_INVALIDATE | RDW_NOCHILDREN,
            );
        }
        window.request_redraw();
    }
}

pub fn lower_below_fullscreen_game(window: &slint::Window, game_pid: u32) -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, IsIconic, SetWindowPos,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    };
    if !window.is_visible() || !poe2_auto_flask::platform::running_under_wine() {
        return false;
    }
    let Some(hwnd) = native_handle(window) else {
        return false;
    };
    unsafe {
        let foreground = GetForegroundWindow();
        let mut pid = 0;
        GetWindowThreadProcessId(foreground, &mut pid);
        if foreground.is_null()
            || foreground == hwnd
            || pid != game_pid
            || IsIconic(foreground) != 0
        {
            return false;
        }
        let mut game = RECT::default();
        let mut ui = RECT::default();
        let mut monitor = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetWindowRect(foreground, &mut game) == 0
            || GetWindowRect(hwnd, &mut ui) == 0
            || GetMonitorInfoW(
                MonitorFromWindow(foreground, MONITOR_DEFAULTTONEAREST),
                &mut monitor,
            ) == 0
        {
            return false;
        }
        let screen = monitor.rcMonitor;
        let fullscreen = covers(game, screen);
        let overlaps = game.left < ui.right
            && game.right > ui.left
            && game.top < ui.bottom
            && game.bottom > ui.top;
        if !fullscreen || !overlaps {
            return false;
        }
        // Adjust only our own window. Preserve activation and Alt+Tab access,
        // and keep a separate-monitor UI available for live stats.
        let lowered = SetWindowPos(
            hwnd,
            foreground,
            0,
            0,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOOWNERZORDER,
        ) != 0;
        lowered && covers(game, ui)
    }
}

fn covers(
    outer: windows_sys::Win32::Foundation::RECT,
    inner: windows_sys::Win32::Foundation::RECT,
) -> bool {
    outer.left <= inner.left
        && outer.top <= inner.top
        && outer.right >= inner.right
        && outer.bottom >= inner.bottom
}

pub fn attach_console() {
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
            return;
        }
        let output = GetStdHandle(STD_OUTPUT_HANDLE);
        // Wine and redirected launches can already have inherited output.
        if output.is_null() || output == INVALID_HANDLE_VALUE {
            AllocConsole();
        }
    }
}

struct Handle(HANDLE);
// Windows kernel handles may be used and closed by any process thread.
unsafe impl Send for Handle {}
impl Handle {
    fn wait(&self) -> u32 {
        unsafe { WaitForSingleObject(self.0, u32::MAX) }
    }
}
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub struct Instance {
    _mutex: Handle,
    event: usize,
    stopping: Arc<AtomicBool>,
    listener: Option<JoinHandle<()>>,
    signal: Option<Handle>,
}

impl Instance {
    pub fn acquire(root: &Path) -> io::Result<Option<Self>> {
        let mut hasher = DefaultHasher::new();
        root.canonicalize()?
            .to_string_lossy()
            .to_lowercase()
            .hash(&mut hasher);
        let prefix = format!("Local\\PoEAutoFlask-{:016x}", hasher.finish());
        let name = |suffix: &str| -> Vec<u16> {
            format!("{prefix}-{suffix}")
                .encode_utf16()
                .chain(Some(0))
                .collect()
        };
        unsafe {
            let mutex = CreateMutexW(null(), 0, name("owner").as_ptr());
            if mutex.is_null() {
                return Err(io::Error::last_os_error());
            }
            let already_running = GetLastError() == ERROR_ALREADY_EXISTS;
            let mutex = Handle(mutex);
            let event = CreateEventW(null(), 0, 0, name("show").as_ptr());
            if event.is_null() {
                return Err(io::Error::last_os_error());
            }
            let signal = Handle(event);
            if already_running {
                SetEvent(signal.0);
                return Ok(None);
            }
            Ok(Some(Self {
                _mutex: mutex,
                event: event as usize,
                stopping: Arc::new(AtomicBool::new(false)),
                listener: None,
                signal: Some(signal),
            }))
        }
    }

    pub fn listen(&mut self, ui: &AppWindow, presentation: Rc<Presentation>) -> io::Result<()> {
        let weak = ui.as_weak();
        let restore = ui.as_weak();
        // Keep the event-loop-only controller out of the listener thread.
        ui.on_restore_existing(move || {
            if let Some(ui) = restore.upgrade() {
                presentation.show(&ui);
                let _ = ui
                    .window()
                    .with_winit_window(|window| window.focus_window());
            }
        });
        let stopping = self.stopping.clone();
        let signal = self.signal.take().expect("instance listener starts once");
        self.listener = Some(
            thread::Builder::new()
                .name("instance".into())
                .spawn(move || loop {
                    if signal.wait() != WAIT_OBJECT_0 || stopping.load(Ordering::Relaxed) {
                        break;
                    }
                    let weak = weak.clone();
                    let _ = slint::invoke_from_event_loop(move || {
                        if let Some(ui) = weak.upgrade() {
                            ui.invoke_restore_existing();
                        }
                    });
                })?,
        );
        Ok(())
    }
}

impl Drop for Instance {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Relaxed);
        if let Some(listener) = self.listener.take() {
            unsafe {
                SetEvent(self.event as HANDLE);
            }
            let _ = listener.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::RECT;

    #[test]
    fn fullscreen_coverage_does_not_suspend_a_second_monitor_ui() {
        let game = RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(covers(
            game,
            RECT {
                left: 200,
                top: 100,
                right: 980,
                bottom: 652
            }
        ));
        assert!(!covers(
            game,
            RECT {
                left: 1800,
                top: 100,
                right: 2580,
                bottom: 652
            }
        ));
        assert!(!covers(
            game,
            RECT {
                left: -1280,
                top: 100,
                right: -500,
                bottom: 652
            }
        ));
    }
}
