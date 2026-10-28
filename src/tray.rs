//! Notification-area icon and its menu.

use std::sync::atomic::{AtomicIsize, Ordering};

use windows::core::w;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::{main_hwnd, settings, WM_APP_TRAY};

const ID_HOT_CORNER: u16 = 100;
const ID_BUTTON_SCROLL: u16 = 101;
const ID_AUTOSTART: u16 = 102;
const ID_EXIT: u16 = 103;
const ID_WIN_SCROLL: u16 = 104;

const BLUE: u32 = 0xFF35_84E4;
const GREY: u32 = 0xFF9A_9996;

static ICON: AtomicIsize = AtomicIsize::new(0);

pub fn add(hwnd: HWND) {
    let mut data = base_data(hwnd);
    data.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
    data.uCallbackMessage = WM_APP_TRAY;
    data.hIcon = icon();
    copy_wide(&mut data.szTip, "GnomeCuts");
    unsafe {
        let _ = Shell_NotifyIconW(NIM_ADD, &data);
    }
}

pub fn remove(hwnd: HWND) {
    unsafe {
        let _ = Shell_NotifyIconW(NIM_DELETE, &base_data(hwnd));
    }
}

pub fn balloon(title: &str, text: &str) {
    let mut data = base_data(main_hwnd());
    data.uFlags = NIF_INFO;
    data.dwInfoFlags = NIIF_WARNING;
    copy_wide(&mut data.szInfoTitle, title);
    copy_wide(&mut data.szInfo, text);
    unsafe {
        let _ = Shell_NotifyIconW(NIM_MODIFY, &data);
    }
}

pub fn show_menu(hwnd: HWND) {
    unsafe {
        let Ok(menu) = CreatePopupMenu() else { return };
        let check = |on: bool| if on { MF_CHECKED } else { MF_UNCHECKED };
        let _ = AppendMenuW(menu, MF_STRING | check(settings::hot_corner()), ID_HOT_CORNER as usize, w!("Hot corner"));
        let _ = AppendMenuW(
            menu,
            MF_STRING | check(settings::button_scroll()),
            ID_BUTTON_SCROLL as usize,
            w!("Scroll on Start and Task View buttons"),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING | check(settings::win_scroll()),
            ID_WIN_SCROLL as usize,
            w!("Scroll anywhere while holding Win"),
        );
        let _ = AppendMenuW(menu, MF_STRING | check(settings::autostart()), ID_AUTOSTART as usize, w!("Start with Windows"));
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, None);
        let _ = AppendMenuW(menu, MF_STRING, ID_EXIT as usize, w!("Exit"));

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Required so the menu closes when the user clicks elsewhere.
        let _ = SetForegroundWindow(hwnd);
        let _ = TrackPopupMenu(menu, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None);
        let _ = DestroyMenu(menu);
    }
}

pub fn handle_command(hwnd: HWND, id: u16) {
    match id {
        ID_HOT_CORNER => settings::set_hot_corner(!settings::hot_corner()),
        ID_BUTTON_SCROLL => settings::set_button_scroll(!settings::button_scroll()),
        ID_WIN_SCROLL => settings::set_win_scroll(!settings::win_scroll()),
        ID_AUTOSTART => settings::set_autostart(!settings::autostart()),
        ID_EXIT => unsafe {
            let _ = DestroyWindow(hwnd);
        },
        _ => {}
    }
}

fn base_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    }
}

fn copy_wide(dst: &mut [u16], src: &str) {
    let len = dst.len() - 1;
    for (d, s) in dst.iter_mut().zip(src.encode_utf16().take(len).chain(std::iter::once(0))) {
        *d = s;
    }
}

fn icon() -> HICON {
    let cached = ICON.load(Ordering::Relaxed);
    if cached != 0 {
        return HICON(cached as *mut _);
    }
    let icon = draw_icon().unwrap_or_else(|| unsafe { LoadIconW(None, IDI_APPLICATION).unwrap_or_default() });
    ICON.store(icon.0 as isize, Ordering::Relaxed);
    icon
}

/// A 2×2 grid of workspaces with the current one highlighted in GNOME blue.
fn draw_icon() -> Option<HICON> {
    unsafe {
        let size = GetSystemMetrics(SM_CXSMICON).max(16);
        let pad = (size / 16).max(1);
        let gap = (size / 8).max(1);
        let tile = (size - 2 * pad - gap) / 2;

        let mut pixels = vec![0u32; (size * size) as usize];
        for (row, col) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
            let colour = if (row, col) == (0, 0) { BLUE } else { GREY };
            let (x0, y0) = (pad + col * (tile + gap), pad + row * (tile + gap));
            for y in 0..tile {
                for x in 0..tile {
                    // Clip the corners for a slightly rounded look.
                    let corner = (x == 0 || x == tile - 1) && (y == 0 || y == tile - 1);
                    if !corner || tile < 6 {
                        pixels[((y0 + y) * size + x0 + x) as usize] = colour;
                    }
                }
            }
        }

        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size,
                biHeight: -size, // top-down
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        let colour = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
        std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u32>(), pixels.len());
        // With a 32-bit colour bitmap the alpha channel decides transparency; the mask is unused.
        let mask_bits = vec![0u8; ((size + 15) / 16 * 2 * size) as usize];
        let mask = CreateBitmap(size, size, 1, 1, Some(mask_bits.as_ptr().cast()));

        let icon = CreateIconIndirect(&ICONINFO { fIcon: true.into(), hbmMask: mask, hbmColor: colour, ..Default::default() });
        let _ = DeleteObject(colour);
        let _ = DeleteObject(mask);
        icon.ok()
    }
}
