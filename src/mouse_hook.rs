//! Low-level mouse hook for the hot corner, scrolling over taskbar buttons and Win+scroll.
//!
//! The hook runs on the UI thread and must return quickly, so it only does
//! cheap geometry checks and posts messages; the real work happens in wnd_proc.

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU32, Ordering};

use windows::Win32::Foundation::{LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LWIN, VK_RWIN};
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{main_hwnd, settings, taskbar_buttons, WM_APP_HOT_CORNER, WM_APP_MASK_WIN, WM_APP_SCROLL};

/// Size of the hot corner, in pixels, from the main display's top-left.
const CORNER_SIZE: i32 = 4;
/// How far the cursor must leave the corner before it can trigger again.
const CORNER_REARM_DISTANCE: i32 = 10;
/// Wheel delta for one notch.
const WHEEL_NOTCH: i32 = 120;
/// Partial (touchpad) scroll deltas older than this are discarded.
const WHEEL_RESET_MS: u32 = 400;
/// Minimum gap between desktop switches, so a free-spinning wheel doesn't fly past every desktop.
const SWITCH_COOLDOWN_MS: u32 = 200;

static HOOK: AtomicIsize = AtomicIsize::new(0);

/// Cleared when the corner fires; set again once the cursor moves away from it.
static CORNER_ARMED: AtomicBool = AtomicBool::new(true);
static PREV_X: AtomicI32 = AtomicI32::new(0);
static PREV_Y: AtomicI32 = AtomicI32::new(0);

static OVER_TASKBAR: AtomicBool = AtomicBool::new(false);

static WHEEL_ACC: AtomicI32 = AtomicI32::new(0);
static LAST_WHEEL: AtomicU32 = AtomicU32::new(0);
static LAST_SWITCH: AtomicU32 = AtomicU32::new(0);

pub fn install() -> windows::core::Result<()> {
    unsafe {
        let hook = SetWindowsHookExW(WH_MOUSE_LL, Some(hook_proc), GetModuleHandleW(None)?, 0)?;
        HOOK.store(hook.0 as isize, Ordering::Relaxed);
    }
    Ok(())
}

pub fn uninstall() {
    let hook = HOOK.swap(0, Ordering::Relaxed);
    if hook != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(HHOOK(hook as *mut _));
        }
    }
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
        match wparam.0 as u32 {
            WM_MOUSEMOVE => on_move(info.pt),
            WM_MOUSEWHEEL if on_wheel(info) => return LRESULT(1),
            _ => {}
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}

unsafe fn on_move(pt: POINT) {
    // Refresh the taskbar button cache as the cursor arrives on a taskbar, since
    // a centred taskbar's buttons shift as apps open and close.
    let over_taskbar = taskbar_buttons::over_taskbar(pt);
    if over_taskbar && !OVER_TASKBAR.load(Ordering::Relaxed) {
        taskbar_buttons::request_refresh();
    }
    OVER_TASKBAR.store(over_taskbar, Ordering::Relaxed);

    let prev = POINT { x: PREV_X.swap(pt.x, Ordering::Relaxed), y: PREV_Y.swap(pt.y, Ordering::Relaxed) };

    if !CORNER_ARMED.load(Ordering::Relaxed) {
        if pt.x.abs() > CORNER_REARM_DISTANCE || pt.y.abs() > CORNER_REARM_DISTANCE {
            CORNER_ARMED.store(true, Ordering::Relaxed);
        }
        return;
    }

    if settings::hot_corner() && hits_hot_corner(prev, pt) {
        CORNER_ARMED.store(false, Ordering::Relaxed);
        let _ = PostMessageW(main_hwnd(), WM_APP_HOT_CORNER, WPARAM(0), LPARAM(0));
    }
}

/// The hot corner is the top-left of the main display, which Windows always
/// places at (0, 0). It fires when the cursor moves from the main display into
/// the area up and to the left of that point.
///
/// With no monitor beside the corner, Windows stops the cursor at (0, 0). If a
/// monitor sits to the left or above, the cursor slides past the corner onto
/// it instead, still level with the main display's top or left edge. Windows
/// pins a cursor pushed past an edge to that edge, so that is where it ends up.
fn hits_hot_corner(prev: POINT, pt: POINT) -> bool {
    let from_main_display = prev.x >= 0 && prev.y >= 0;
    from_main_display && pt.x < CORNER_SIZE && pt.y < CORNER_SIZE
}

/// Returns true if the event switches desktops and should be swallowed: it was
/// over the Start or Task View button, or anywhere while Win is held.
unsafe fn on_wheel(info: &MSLLHOOKSTRUCT) -> bool {
    let win_held = settings::win_scroll() && win_down();
    if !win_held && !(settings::button_scroll() && taskbar_buttons::contains(info.pt)) {
        return false;
    }
    if win_held {
        let _ = PostMessageW(main_hwnd(), WM_APP_MASK_WIN, WPARAM(0), LPARAM(0));
    }

    let delta = (info.mouseData >> 16) as i16 as i32;
    let now = info.time;
    if now.wrapping_sub(LAST_WHEEL.swap(now, Ordering::Relaxed)) > WHEEL_RESET_MS {
        WHEEL_ACC.store(0, Ordering::Relaxed);
    }
    let acc = WHEEL_ACC.fetch_add(delta, Ordering::Relaxed) + delta;
    if acc.abs() >= WHEEL_NOTCH {
        WHEEL_ACC.store(0, Ordering::Relaxed);
        if now.wrapping_sub(LAST_SWITCH.load(Ordering::Relaxed)) >= SWITCH_COOLDOWN_MS {
            LAST_SWITCH.store(now, Ordering::Relaxed);
            // Wheel up (positive delta) goes to the previous desktop, as in GNOME.
            let dir: isize = if acc > 0 { -1 } else { 1 };
            let _ = PostMessageW(main_hwnd(), WM_APP_SCROLL, WPARAM(dir as usize), LPARAM(0));
        }
    }
    true
}

unsafe fn win_down() -> bool {
    GetAsyncKeyState(VK_LWIN.0 as i32) < 0 || GetAsyncKeyState(VK_RWIN.0 as i32) < 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: i32, y: i32) -> POINT {
        POINT { x, y }
    }

    #[test]
    fn stops_in_corner() {
        assert!(hits_hot_corner(p(40, 30), p(0, 0)));
    }

    #[test]
    fn slides_past_corner_onto_left_monitor() {
        assert!(hits_hot_corner(p(25, 0), p(-30, 0)));
        assert!(hits_hot_corner(p(60, 20), p(-40, 0)));
    }

    #[test]
    fn slides_past_corner_onto_monitor_above() {
        assert!(hits_hot_corner(p(0, 25), p(0, -30)));
    }

    #[test]
    fn ignores_moves_away_from_corner() {
        assert!(!hits_hot_corner(p(-30, 0), p(25, 0)));
        assert!(!hits_hot_corner(p(0, 0), p(20, 20)));
    }

    #[test]
    fn ignores_moves_that_miss_corner() {
        assert!(!hits_hot_corner(p(500, 0), p(20, 0)));
        assert!(!hits_hot_corner(p(20, 300), p(-30, 200)));
    }
}
