//! Win+PgUp/PgDn shortcuts, via RegisterHotKey.

use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

use crate::desktops;

const PREV: i32 = 1;
const NEXT: i32 = 2;
const MOVE_PREV: i32 = 3;
const MOVE_NEXT: i32 = 4;

const HOTKEYS: [(i32, HOT_KEY_MODIFIERS, VIRTUAL_KEY, &str); 4] = [
    (PREV, MOD_WIN, VK_PRIOR, "Win+PgUp"),
    (NEXT, MOD_WIN, VK_NEXT, "Win+PgDn"),
    (MOVE_PREV, MOD_SHIFT, VK_PRIOR, "Win+Shift+PgUp"),
    (MOVE_NEXT, MOD_SHIFT, VK_NEXT, "Win+Shift+PgDn"),
];

/// Registers all shortcuts and returns the names of any that were already taken.
pub fn register(hwnd: HWND) -> Vec<&'static str> {
    HOTKEYS
        .iter()
        .filter(|(id, extra, vk, _)| unsafe {
            RegisterHotKey(hwnd, *id, MOD_WIN | *extra | MOD_NOREPEAT, vk.0 as u32).is_err()
        })
        .map(|(_, _, _, name)| *name)
        .collect()
}

pub fn unregister(hwnd: HWND) {
    for (id, ..) in HOTKEYS {
        unsafe {
            let _ = UnregisterHotKey(hwnd, id);
        }
    }
}

pub fn handle(id: i32) {
    match id {
        PREV => desktops::switch_relative(-1),
        NEXT => desktops::switch_relative(1),
        MOVE_PREV => desktops::move_foreground_relative(-1),
        MOVE_NEXT => desktops::move_foreground_relative(1),
        _ => {}
    }
}
