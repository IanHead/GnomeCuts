//! User settings, stored under HKCU\Software\GnomeCuts.

use std::sync::atomic::{AtomicBool, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::*;

const KEY: PCWSTR = w!("Software\\GnomeCuts");
const RUN_KEY: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
const RUN_VALUE: PCWSTR = w!("GnomeCuts");
const HOT_CORNER_VALUE: PCWSTR = w!("HotCorner");
// Named from when this only covered the Start button; kept so existing settings carry over.
const BUTTON_SCROLL_VALUE: PCWSTR = w!("StartScroll");
const WIN_SCROLL_VALUE: PCWSTR = w!("WinScroll");

static HOT_CORNER: AtomicBool = AtomicBool::new(true);
static BUTTON_SCROLL: AtomicBool = AtomicBool::new(true);
static WIN_SCROLL: AtomicBool = AtomicBool::new(true);

pub fn load() {
    HOT_CORNER.store(read_bool(HOT_CORNER_VALUE, true), Ordering::Relaxed);
    BUTTON_SCROLL.store(read_bool(BUTTON_SCROLL_VALUE, true), Ordering::Relaxed);
    WIN_SCROLL.store(read_bool(WIN_SCROLL_VALUE, true), Ordering::Relaxed);
}

pub fn hot_corner() -> bool {
    HOT_CORNER.load(Ordering::Relaxed)
}

pub fn set_hot_corner(on: bool) {
    HOT_CORNER.store(on, Ordering::Relaxed);
    write_bool(HOT_CORNER_VALUE, on);
}

pub fn button_scroll() -> bool {
    BUTTON_SCROLL.load(Ordering::Relaxed)
}

pub fn set_button_scroll(on: bool) {
    BUTTON_SCROLL.store(on, Ordering::Relaxed);
    write_bool(BUTTON_SCROLL_VALUE, on);
}

pub fn win_scroll() -> bool {
    WIN_SCROLL.load(Ordering::Relaxed)
}

pub fn set_win_scroll(on: bool) {
    WIN_SCROLL.store(on, Ordering::Relaxed);
    write_bool(WIN_SCROLL_VALUE, on);
}

pub fn autostart() -> bool {
    unsafe { RegGetValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE, RRF_RT_REG_SZ, None, None, None).is_ok() }
}

pub fn set_autostart(on: bool) {
    unsafe {
        if on {
            let Ok(exe) = std::env::current_exe() else { return };
            let command: Vec<u16> = format!("\"{}\"\0", exe.display()).encode_utf16().collect();
            let _ = RegSetKeyValueW(
                HKEY_CURRENT_USER,
                RUN_KEY,
                RUN_VALUE,
                REG_SZ.0,
                Some(command.as_ptr().cast()),
                (command.len() * 2) as u32,
            );
        } else {
            let _ = RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, RUN_VALUE);
        }
    }
}

fn read_bool(name: PCWSTR, default: bool) -> bool {
    let mut value = 0u32;
    let mut size = std::mem::size_of::<u32>() as u32;
    let found = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            KEY,
            name,
            RRF_RT_REG_DWORD,
            None,
            Some(std::ptr::from_mut(&mut value).cast()),
            Some(&mut size),
        )
    };
    if found.is_ok() { value != 0 } else { default }
}

fn write_bool(name: PCWSTR, on: bool) {
    let value = on as u32;
    unsafe {
        let _ = RegSetKeyValueW(
            HKEY_CURRENT_USER,
            KEY,
            name,
            REG_DWORD.0,
            Some(std::ptr::from_ref(&value).cast()),
            std::mem::size_of::<u32>() as u32,
        );
    }
}
