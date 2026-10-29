#![windows_subsystem = "windows"]

mod desktops;
mod hotkeys;
mod mouse_hook;
mod settings;
mod taskbar_buttons;
mod tray;
mod win_key;

use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::{CoInitializeEx, COINIT_APARTMENTTHREADED};
use windows::Win32::System::Console::{AttachConsole, ATTACH_PARENT_PROCESS};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::HiDpi::{
    SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::*;

/// Tray icon callback.
pub const WM_APP_TRAY: u32 = WM_APP + 1;
/// Posted by the mouse hook when the cursor hits a hot corner.
pub const WM_APP_HOT_CORNER: u32 = WM_APP + 2;
/// Posted by the mouse hook on a wheel notch over the Start or Task View button, or
/// anywhere while Win is held or Task View is open; wParam is the direction (-1/+1).
pub const WM_APP_SCROLL: u32 = WM_APP + 3;

static MAIN_HWND: AtomicIsize = AtomicIsize::new(0);
static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

pub fn main_hwnd() -> HWND {
    HWND(MAIN_HWND.load(Ordering::Relaxed) as *mut _)
}

fn main() {
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // winvd and the tray both need COM on the UI thread.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }

    if std::env::args().any(|a| a == "--diag") {
        diagnostics();
        return;
    }

    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\GnomeCuts.SingleInstance"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return;
        }

        settings::load();
        if let Err(e) = run() {
            let msg: Vec<u16> = format!("GnomeCuts failed to start:\n{e}\0").encode_utf16().collect();
            MessageBoxW(None, PCWSTR(msg.as_ptr()), w!("GnomeCuts"), MB_ICONERROR);
        }
    }
}

unsafe fn run() -> windows::core::Result<()> {
    let instance = GetModuleHandleW(None)?;
    let class = WNDCLASSW {
        lpfnWndProc: Some(wnd_proc),
        hInstance: instance.into(),
        lpszClassName: w!("GnomeCutsWindow"),
        ..Default::default()
    };
    if RegisterClassW(&class) == 0 {
        return Err(windows::core::Error::from_win32());
    }

    TASKBAR_CREATED.store(RegisterWindowMessageW(w!("TaskbarCreated")), Ordering::Relaxed);

    // A hidden top-level window rather than a message-only one, so we receive
    // broadcasts such as TaskbarCreated and WM_DISPLAYCHANGE.
    let hwnd = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        w!("GnomeCutsWindow"),
        w!("GnomeCuts"),
        WS_OVERLAPPED,
        0,
        0,
        0,
        0,
        None,
        None,
        instance,
        None,
    )?;
    MAIN_HWND.store(hwnd.0 as isize, Ordering::Relaxed);

    tray::add(hwnd);
    let failed = hotkeys::register(hwnd);
    if !failed.is_empty() {
        tray::balloon(
            "Some shortcuts are unavailable",
            &format!("Another program is already using: {}", failed.join(", ")),
        );
    }
    taskbar_buttons::spawn();
    mouse_hook::install()?;
    win_key::install()?;

    let mut msg = MSG::default();
    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }

    mouse_hook::uninstall();
    win_key::uninstall();
    Ok(())
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => hotkeys::handle(wparam.0 as i32),
        WM_APP_TRAY => match lparam.0 as u32 {
            WM_LBUTTONUP | WM_RBUTTONUP | WM_CONTEXTMENU => tray::show_menu(hwnd),
            _ => {}
        },
        WM_COMMAND => tray::handle_command(hwnd, (wparam.0 & 0xffff) as u16),
        WM_APP_HOT_CORNER => desktops::hot_corner(),
        WM_APP_SCROLL => desktops::switch_relative(wparam.0 as isize as i32),
        WM_TIMER if wparam.0 == desktops::TIMER_REFOCUS => desktops::refocus(hwnd),
        WM_DISPLAYCHANGE | WM_SETTINGCHANGE => taskbar_buttons::request_refresh(),
        WM_DESTROY => {
            hotkeys::unregister(hwnd);
            tray::remove(hwnd);
            PostQuitMessage(0);
        }
        _ if msg == TASKBAR_CREATED.load(Ordering::Relaxed) && msg != 0 => {
            // Explorer restarted: our icon is gone and the taskbar buttons have moved.
            tray::add(hwnd);
            taskbar_buttons::request_refresh();
        }
        _ => return DefWindowProcW(hwnd, msg, wparam, lparam),
    }
    LRESULT(0)
}

/// `gnomecuts --diag` prints what the app can see, for troubleshooting.
fn diagnostics() {
    unsafe {
        let _ = AttachConsole(ATTACH_PARENT_PROCESS);
    }
    let count = winvd::get_desktop_count();
    let current = winvd::get_current_desktop().and_then(|d| d.get_index());
    println!("Virtual desktops: count={count:?} current={current:?}");
    match taskbar_buttons::find_now() {
        Ok(layout) => {
            let taskbars: Vec<String> = layout.taskbars.iter().map(fmt_rect).collect();
            println!("Taskbars: {}", taskbars.join(" "));
            for (id, rect) in layout.buttons {
                println!("{id}: {}", fmt_rect(&rect));
            }
        }
        Err(e) => println!("UI Automation failed: {e}"),
    }
}

fn fmt_rect(r: &RECT) -> String {
    format!("({},{})-({},{})", r.left, r.top, r.right, r.bottom)
}
