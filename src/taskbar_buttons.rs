//! Finds the taskbar's Start and Task View buttons.
//!
//! Windows 11's taskbar is XAML, so these buttons have no windows of their
//! own; UI Automation is the only way to find their bounds. UIA is far too
//! slow for the mouse hook, so a worker thread keeps a cache of rectangles
//! that the hook reads.

use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use windows::core::{w, BSTR, VARIANT};
use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::{FindWindowExW, FindWindowW, GetWindowRect};

/// UI Automation IDs of the buttons that switch desktops when scrolled over.
const BUTTON_IDS: [&str; 2] = ["StartButton", "TaskViewButton"];

/// Fallback refresh interval, in case we miss a layout change.
const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

static BUTTON_RECTS: Mutex<Vec<RECT>> = Mutex::new(Vec::new());
static TASKBAR_RECTS: Mutex<Vec<RECT>> = Mutex::new(Vec::new());
static REFRESH: OnceLock<Sender<()>> = OnceLock::new();

pub struct Layout {
    pub taskbars: Vec<RECT>,
    /// (UI Automation ID, bounds) of each button found.
    pub buttons: Vec<(String, RECT)>,
}

pub fn spawn() {
    let (tx, rx) = channel();
    let _ = REFRESH.set(tx);
    thread::spawn(move || {
        let Ok(finder) = Finder::new() else { return };
        loop {
            if let Ok(layout) = finder.find() {
                *TASKBAR_RECTS.lock().unwrap() = layout.taskbars;
                *BUTTON_RECTS.lock().unwrap() = layout.buttons.into_iter().map(|(_, rect)| rect).collect();
            }
            match rx.recv_timeout(REFRESH_INTERVAL) {
                Ok(()) => while rx.try_recv().is_ok() {},
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
        }
    });
}

pub fn request_refresh() {
    if let Some(tx) = REFRESH.get() {
        let _ = tx.send(());
    }
}

/// Called from the mouse hook: never blocks, and answers "no" if the worker holds the lock.
pub fn contains(pt: POINT) -> bool {
    BUTTON_RECTS.try_lock().is_ok_and(|rects| rects.iter().any(|r| inside(r, pt)))
}

/// Called from the mouse hook; see [`contains`].
pub fn over_taskbar(pt: POINT) -> bool {
    TASKBAR_RECTS.try_lock().is_ok_and(|rects| rects.iter().any(|r| inside(r, pt)))
}

/// One-off synchronous lookup, for `--diag`.
pub fn find_now() -> windows::core::Result<Layout> {
    thread::spawn(|| Finder::new()?.find()).join().unwrap()
}

fn inside(r: &RECT, pt: POINT) -> bool {
    pt.x >= r.left && pt.x < r.right && pt.y >= r.top && pt.y < r.bottom
}

struct Finder {
    uia: IUIAutomation,
    condition: IUIAutomationCondition,
}

impl Finder {
    fn new() -> windows::core::Result<Self> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let uia: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?;
            let conditions = BUTTON_IDS
                .iter()
                .map(|id| uia.CreatePropertyCondition(UIA_AutomationIdPropertyId, &VARIANT::from(BSTR::from(*id))))
                .collect::<windows::core::Result<Vec<_>>>()?;
            let condition = uia.CreateOrCondition(&conditions[0], &conditions[1])?;
            Ok(Self { uia, condition })
        }
    }

    /// Finds the taskbars and their buttons across all monitors.
    fn find(&self) -> windows::core::Result<Layout> {
        let mut taskbars = Vec::new();
        let mut buttons = Vec::new();
        for hwnd in taskbar_windows() {
            unsafe {
                let mut rect = RECT::default();
                if GetWindowRect(hwnd, &mut rect).is_ok() {
                    taskbars.push(rect);
                }
                let Ok(element) = self.uia.ElementFromHandle(hwnd) else { continue };
                let Ok(found) = element.FindAll(TreeScope_Descendants, &self.condition) else { continue };
                for i in 0..found.Length().unwrap_or(0) {
                    let Ok(button) = found.GetElement(i) else { continue };
                    if let (Ok(id), Ok(rect)) = (button.CurrentAutomationId(), button.CurrentBoundingRectangle()) {
                        buttons.push((id.to_string(), rect));
                    }
                }
            }
        }
        Ok(Layout { taskbars, buttons })
    }
}

/// The primary taskbar plus one per extra monitor.
fn taskbar_windows() -> Vec<HWND> {
    let mut windows = Vec::new();
    unsafe {
        if let Ok(hwnd) = FindWindowW(w!("Shell_TrayWnd"), None) {
            windows.push(hwnd);
        }
        let mut after = HWND::default();
        while let Ok(hwnd) = FindWindowExW(None, after, w!("Shell_SecondaryTrayWnd"), None) {
            windows.push(hwnd);
            after = hwnd;
        }
    }
    windows
}
