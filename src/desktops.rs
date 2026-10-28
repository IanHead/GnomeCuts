//! Virtual desktop actions.
//!
//! Switching injects Windows' own Ctrl+Win+Left/Right so we get the native
//! slide animation and focus handling. Moving a window to another desktop has
//! no public API, so that goes through winvd.

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONULL};
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{main_hwnd, settings, tray};

pub const TIMER_REFOCUS: usize = 1;

/// Window to re-activate once the switch animation has finished.
static REFOCUS: AtomicIsize = AtomicIsize::new(0);

/// Switch one desktop left (-1) or right (+1). Like GNOME, this doesn't wrap.
pub fn switch_relative(dir: i32) {
    let arrow = if dir < 0 { VK_LEFT } else { VK_RIGHT };
    send_chord(&[VK_LCONTROL, VK_LWIN], arrow);
}

/// Move the foreground window one desktop left or right and follow it there.
/// Moving right from the last desktop creates a new one, like GNOME's dynamic workspaces.
pub fn move_foreground_relative(dir: i32) {
    let Some(hwnd) = foreground_app_window() else { return };

    // Pinned windows are already on every desktop, so just switch.
    if winvd::is_pinned_window(hwnd).unwrap_or(false) || winvd::is_pinned_app(hwnd).unwrap_or(false) {
        switch_relative(dir);
        return;
    }

    let result = (|| -> winvd::Result<bool> {
        let current = winvd::get_current_desktop()?.get_index()? as i32;
        let count = winvd::get_desktop_count()? as i32;
        let target = current + dir;
        if target < 0 {
            return Ok(false);
        }
        if target >= count {
            winvd::create_desktop()?;
        }
        winvd::move_window_to_desktop(target as u32, &hwnd)?;
        Ok(true)
    })();

    match result {
        Ok(true) => {
            switch_relative(dir);
            REFOCUS.store(hwnd.0 as isize, Ordering::Relaxed);
            unsafe { SetTimer(main_hwnd(), TIMER_REFOCUS, 400, None) };
        }
        Ok(false) => {}
        Err(e) => tray::balloon("Couldn't move window", &format!("{e:?}")),
    }
}

/// Make sure the window we just moved is focused once the switch has settled.
pub fn refocus(timer_hwnd: HWND) {
    unsafe {
        let _ = KillTimer(timer_hwnd, TIMER_REFOCUS);
        let hwnd = HWND(REFOCUS.swap(0, Ordering::Relaxed) as *mut _);
        if !hwnd.is_invalid() && IsWindow(hwnd).as_bool() && GetForegroundWindow() != hwnd {
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Toggle Task View (Win+Tab opens it, or closes it if it's already open),
/// unless the user is dragging something or a fullscreen app has focus.
pub fn hot_corner() {
    if !settings::hot_corner() {
        return;
    }
    unsafe {
        let held = [VK_LBUTTON, VK_RBUTTON, VK_MBUTTON]
            .iter()
            .any(|&vk| GetAsyncKeyState(vk.0 as i32) < 0);
        if held || foreground_is_fullscreen() {
            return;
        }
    }
    send_chord(&[VK_LWIN], VK_TAB);
}

/// Press `modifiers` + `key`, leaving the modifier state as we found it.
///
/// Modifiers the user is already holding (e.g. Win from Win+PgDn) aren't
/// re-sent: injecting their key-up would make Windows think they had been
/// released. Shift is lifted for the duration, since Win+Shift+PgDn would
/// otherwise turn our Ctrl+Win+Right into an unrelated shortcut.
fn send_chord(modifiers: &[VIRTUAL_KEY], key: VIRTUAL_KEY) {
    let held_shifts: Vec<VIRTUAL_KEY> = [VK_LSHIFT, VK_RSHIFT].into_iter().filter(|&vk| is_down(vk)).collect();
    let missing: Vec<VIRTUAL_KEY> = modifiers.iter().copied().filter(|&vk| !modifier_down(vk)).collect();

    let mut inputs = Vec::new();
    inputs.extend(held_shifts.iter().map(|&vk| key_input(vk, true)));
    inputs.extend(missing.iter().map(|&vk| key_input(vk, false)));
    inputs.push(key_input(key, false));
    inputs.push(key_input(key, true));
    inputs.extend(missing.iter().rev().map(|&vk| key_input(vk, true)));
    inputs.extend(held_shifts.iter().map(|&vk| key_input(vk, false)));

    unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
}

fn is_down(vk: VIRTUAL_KEY) -> bool {
    unsafe { GetAsyncKeyState(vk.0 as i32) < 0 }
}

/// Either side of a modifier counts as held.
fn modifier_down(vk: VIRTUAL_KEY) -> bool {
    match vk {
        VK_LWIN | VK_RWIN => is_down(VK_LWIN) || is_down(VK_RWIN),
        VK_LCONTROL | VK_RCONTROL => is_down(VK_LCONTROL) || is_down(VK_RCONTROL),
        _ => is_down(vk),
    }
}

pub fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS::default();
    if up {
        flags |= KEYEVENTF_KEYUP;
    }
    if matches!(vk, VK_LEFT | VK_RIGHT | VK_LWIN | VK_RWIN | VK_RCONTROL) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: vk, dwFlags: flags, ..Default::default() },
        },
    }
}

/// The foreground window, if it's an ordinary app window (not the desktop or taskbar).
fn foreground_app_window() -> Option<HWND> {
    unsafe {
        let hwnd = GetAncestor(GetForegroundWindow(), GA_ROOT);
        if hwnd.is_invalid() || hwnd == main_hwnd() || is_shell_window(hwnd) {
            return None;
        }
        Some(hwnd)
    }
}

/// The desktop, taskbars and Task View. Task View is fullscreen, so without
/// this the hot corner would treat it as a fullscreen app and refuse to close it.
fn is_shell_window(hwnd: HWND) -> bool {
    let mut buf = [0u16; 64];
    let len = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
    let class = String::from_utf16_lossy(&buf[..len]);
    matches!(
        class.as_str(),
        "Progman"
            | "WorkerW"
            | "Shell_TrayWnd"
            | "Shell_SecondaryTrayWnd"
            // Task View on Windows 11 24H2+, and on earlier builds.
            | "XamlExplorerHostIslandWindow"
            | "MultitaskingViewFrame"
    )
}

unsafe fn foreground_is_fullscreen() -> bool {
    let hwnd = GetForegroundWindow();
    if hwnd.is_invalid() || is_shell_window(hwnd) {
        return false;
    }
    let mut window = RECT::default();
    if GetWindowRect(hwnd, &mut window).is_err() {
        return false;
    }
    let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONULL);
    let mut info = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
    if monitor.is_invalid() || !GetMonitorInfoW(monitor, &mut info).as_bool() {
        return false;
    }
    let m = info.rcMonitor;
    window.left <= m.left && window.top <= m.top && window.right >= m.right && window.bottom >= m.bottom
}
