//! Dark menus, the custom window frame, and saved window placement.

use super::WindowPlacement;
use super::wide;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::RefCell;
use std::mem::{size_of, size_of_val, transmute, zeroed};
use std::sync::atomic::{AtomicIsize, Ordering::Relaxed};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{MONITOR_DEFAULTTONULL, MonitorFromRect};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows_sys::Win32::UI::Controls::MARGINS;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallWindowProcW, GWLP_WNDPROC, GetWindowPlacement, SW_SHOWMAXIMIZED, SW_SHOWMINIMIZED, SW_SHOWNORMAL,
    SetWindowLongPtrW, SetWindowPlacement, WINDOWPLACEMENT, WM_APPCOMMAND, WM_XBUTTONDOWN, WNDPROC,
    WPF_RESTORETOMAXIMIZED, XBUTTON1, XBUTTON2,
};

const APPCOMMAND_BROWSER_BACKWARD: u32 = 1;
const APPCOMMAND_BROWSER_FORWARD: u32 = 2;
const SET_PREFERRED_APP_MODE_ORDINAL: usize = 135;
const FLUSH_MENU_THEMES_ORDINAL: usize = 136;
const APP_MODE_ALLOW_DARK: i32 = 1;

pub fn allow_dark_menus() {
    unsafe {
        let uxtheme = LoadLibraryW(wide("uxtheme.dll").as_ptr());
        if uxtheme.is_null() {
            return;
        }
        if let Some(set_preferred_app_mode) = GetProcAddress(uxtheme, SET_PREFERRED_APP_MODE_ORDINAL as *const u8) {
            let set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32 = transmute(set_preferred_app_mode);
            set_preferred_app_mode(APP_MODE_ALLOW_DARK);
        }
        if let Some(flush_menu_themes) = GetProcAddress(uxtheme, FLUSH_MENU_THEMES_ORDINAL as *const u8) {
            let flush_menu_themes: unsafe extern "system" fn() = transmute(flush_menu_themes);
            flush_menu_themes();
        }
    }
}

static ORIGINAL_WNDPROC: AtomicIsize = AtomicIsize::new(0);

thread_local! {
    static NAVIGATE_HANDLER: RefCell<Option<Box<dyn Fn(bool)>>> = RefCell::new(None);
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_XBUTTONDOWN {
        let button = ((wparam >> 16) & 0xffff) as u16;
        if button == XBUTTON1 || button == XBUTTON2 {
            NAVIGATE_HANDLER.with(|slot| {
                if let Some(handler) = slot.borrow().as_ref() {
                    handler(button == XBUTTON2);
                }
            });
            return 1;
        }
    } else if msg == WM_APPCOMMAND {
        let cmd = ((lparam >> 16) & 0x0fff) as u32;
        if cmd == APPCOMMAND_BROWSER_BACKWARD || cmd == APPCOMMAND_BROWSER_FORWARD {
            NAVIGATE_HANDLER.with(|slot| {
                if let Some(handler) = slot.borrow().as_ref() {
                    handler(cmd == APPCOMMAND_BROWSER_FORWARD);
                }
            });
            return 1;
        }
    }
    let original = ORIGINAL_WNDPROC.load(Relaxed);
    if original != 0 {
        let proc: WNDPROC = unsafe { transmute(original) };
        unsafe { CallWindowProcW(proc, hwnd, msg, wparam, lparam) }
    } else {
        0
    }
}

fn native_handle(window: &slint::Window) -> Option<HWND> {
    let window = window.window_handle();
    let handle = window.window_handle().ok()?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return None };
    Some(handle.hwnd.get() as HWND)
}

fn current_placement(hwnd: HWND) -> Option<WINDOWPLACEMENT> {
    let mut placement: WINDOWPLACEMENT = unsafe { zeroed() };
    placement.length = size_of::<WINDOWPLACEMENT>() as u32;
    (unsafe { GetWindowPlacement(hwnd, &mut placement) } != 0).then_some(placement)
}

pub fn window_placement(window: &slint::Window) -> Option<WindowPlacement> {
    let placement = current_placement(native_handle(window)?)?;
    let rect = placement.rcNormalPosition;
    let minimized_from_maximized = placement.showCmd == SW_SHOWMINIMIZED as u32 && placement.flags & WPF_RESTORETOMAXIMIZED as u32 != 0;
    Some(WindowPlacement {
        rect: [rect.left, rect.top, rect.right, rect.bottom],
        maximized: placement.showCmd == SW_SHOWMAXIMIZED as u32 || minimized_from_maximized,
    })
}

pub fn apply_window_placement(window: &slint::Window, saved: WindowPlacement) {
    let Some(hwnd) = native_handle(window) else { return };
    let Some(mut placement) = current_placement(hwnd) else { return };
    let mut rect = placement.rcNormalPosition;
    [rect.left, rect.top, rect.right, rect.bottom] = saved.rect;
    if !unsafe { MonitorFromRect(&rect, MONITOR_DEFAULTTONULL) }.is_null() {
        placement.rcNormalPosition = rect;
    }
    placement.showCmd = if saved.maximized { SW_SHOWMAXIMIZED } else { SW_SHOWNORMAL } as u32;
    unsafe { SetWindowPlacement(hwnd, &placement) };
}

pub fn restore_frame_appearance(window: &slint::Window, on_navigate: impl Fn(bool) + 'static) {
    let Some(hwnd) = native_handle(window) else { return };
    let frame = MARGINS { cxLeftWidth: 1, cxRightWidth: 1, cyTopHeight: 1, cyBottomHeight: 1 };
    let corners = DWMWCP_ROUND;
    NAVIGATE_HANDLER.with(|slot| {
        *slot.borrow_mut() = Some(Box::new(on_navigate));
    });
    unsafe {
        DwmExtendFrameIntoClientArea(hwnd, &frame);
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            (&raw const corners).cast(),
            size_of_val(&corners) as u32,
        );
        if ORIGINAL_WNDPROC.load(Relaxed) == 0 {
            let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, window_proc as *const () as isize);
            if previous != 0 {
                ORIGINAL_WNDPROC.store(previous, Relaxed);
            }
        }
    }
}
