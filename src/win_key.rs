//! Low-level keyboard hook that stops the Start menu opening after Win+scroll.
//!
//! Windows opens Start when Win is released with no other key pressed since it
//! went down, and a swallowed wheel event doesn't count. Tapping a dummy key
//! while Win is still held fixes that, but a desktop switch in between seems to
//! reset the shell's tracking, so the tap has to come right before the release.
//! When Win+scroll was used, the hook holds back the real Win key-up and
//! replays it after the dummy key. AutoHotkey works the same way.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::desktops::key_input;

/// An unassigned virtual key, so tapping it has no effect of its own.
const MASK_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

static HOOK: AtomicIsize = AtomicIsize::new(0);

/// Set when Win+scroll is used; cleared when Win is released.
static MASK_PENDING: AtomicBool = AtomicBool::new(false);

pub fn install() -> windows::core::Result<()> {
    unsafe {
        let hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook_proc), GetModuleHandleW(None)?, 0)?;
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

/// Called by the mouse hook when it swallows a wheel event while Win is held.
pub fn mask_next_release() {
    MASK_PENDING.store(true, Ordering::Relaxed);
}

unsafe extern "system" fn hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let info = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let vk = VIRTUAL_KEY(info.vkCode as u16);
        let key_up = matches!(wparam.0 as u32, WM_KEYUP | WM_SYSKEYUP);
        // Our own replayed key-up comes back through here flagged as injected.
        let injected = info.flags.contains(LLKHF_INJECTED);
        if key_up && !injected && matches!(vk, VK_LWIN | VK_RWIN) && MASK_PENDING.swap(false, Ordering::Relaxed) {
            let inputs = [key_input(MASK_KEY, false), key_input(MASK_KEY, true), key_input(vk, true)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            return LRESULT(1);
        }
    }
    CallNextHookEx(None, code, wparam, lparam)
}
