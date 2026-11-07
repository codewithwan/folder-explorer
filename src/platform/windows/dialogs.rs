//! Native message boxes and the save-file dialog.

use super::wide;
use std::ffi::OsString;
use std::mem::{size_of, zeroed};
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetSaveFileNameW, OFN_HIDEREADONLY, OFN_OVERWRITEPROMPT, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_TASKMODAL, MB_YESNO, MessageBoxW,
};

pub fn confirm(title: &str, message: &str) -> bool {
    let (title, message) = (wide(title), wide(message));
    let style = MB_YESNO | MB_ICONWARNING | MB_TASKMODAL;
    unsafe { MessageBoxW(GetActiveWindow(), message.as_ptr(), title.as_ptr(), style) == IDYES }
}

pub fn alert(title: &str, message: &str) {
    let (title, message) = (wide(title), wide(message));
    let style = MB_OK | MB_ICONERROR | MB_TASKMODAL;
    unsafe { MessageBoxW(GetActiveWindow(), message.as_ptr(), title.as_ptr(), style) };
}

pub fn save_dialog(title: &str, file_name: &str, filters: &[(&str, &str)]) -> Option<(PathBuf, usize)> {
    let filter_text: String = filters
        .iter()
        .map(|(label, extension)| format!("{label}\0*.{extension}\0"))
        .collect();
    let filter = wide(filter_text);
    let title = wide(title);
    let extension = wide(filters.first().map_or("", |(_, extension)| extension));
    let mut path = [0u16; 32_768];
    let name: Vec<u16> = file_name.encode_utf16().take(path.len() - 1).collect();
    path[..name.len()].copy_from_slice(&name);
    let mut dialog: OPENFILENAMEW = unsafe { zeroed() };
    dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    dialog.hwndOwner = unsafe { GetActiveWindow() };
    dialog.lpstrFilter = filter.as_ptr();
    dialog.nFilterIndex = 1;
    dialog.lpstrFile = path.as_mut_ptr();
    dialog.nMaxFile = path.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.lpstrDefExt = extension.as_ptr();
    dialog.Flags = OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY;
    if unsafe { GetSaveFileNameW(&mut dialog) } == 0 {
        return None;
    }
    let length = path.iter().position(|unit| *unit == 0)?;
    let chosen = PathBuf::from(OsString::from_wide(&path[..length]));
    Some((chosen, dialog.nFilterIndex.saturating_sub(1) as usize))
}
