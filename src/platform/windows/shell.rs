//! Opening items, Explorer integration and shell file operations
//! (copy / move / recycle / delete / rename, Recycle Bin).

use super::wide;
use std::ffi::OsString;
use std::io;
use std::mem::{size_of, zeroed};
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr::{null, null_mut};
use windows_sys::Win32::Storage::FileSystem::MoveFileW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::GetActiveWindow;
use windows_sys::Win32::UI::Shell::{
    FO_COPY, FO_DELETE, FO_MOVE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_RENAMEONCOLLISION, FOF_WANTNUKEWARNING,
    SEE_MASK_INVOKEIDLIST, SHELLEXECUTEINFOW, SHEmptyRecycleBinW, SHFILEOPSTRUCTW, SHFileOperationW, SHQUERYRBINFO,
    SHQueryRecycleBinW, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn shell_execute(verb: &str, path: &Path, mask: u32) -> bool {
    let verb = wide(verb);
    let file = wide(path);
    let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = mask;
    info.hwnd = unsafe { GetActiveWindow() };
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    unsafe { ShellExecuteExW(&mut info) != 0 }
}

pub fn open(path: &Path) -> bool {
    shell_execute("open", path, 0)
}

pub fn show_properties(path: &Path) -> bool {
    shell_execute("properties", path, SEE_MASK_INVOKEIDLIST)
}

pub(super) fn reveal_argument(path: &Path) -> OsString {
    let mut argument = OsString::from("/select,\"");
    argument.push(path);
    argument.push("\"");
    argument
}

pub fn reveal_in_explorer(path: &Path) {
    let _ = Command::new("explorer.exe").raw_arg(reveal_argument(path)).spawn();
}

pub fn active_window() -> usize {
    unsafe { GetActiveWindow() as usize }
}

fn shell_operation(function: u32, paths: &[PathBuf], target: Option<&Path>, flags: u32, owner: usize) -> bool {
    let mut from: Vec<u16> = paths.iter().flat_map(wide).collect();
    from.push(0);
    let mut to: Vec<u16> = target.map(wide).unwrap_or_default();
    to.push(0);
    let mut operation: SHFILEOPSTRUCTW = unsafe { zeroed() };
    operation.hwnd = owner as _;
    operation.wFunc = function;
    operation.pFrom = from.as_ptr();
    operation.pTo = to.as_ptr();
    operation.fFlags = flags as u16;
    let failed = unsafe { SHFileOperationW(&mut operation) };
    failed == 0 && operation.fAnyOperationsAborted == 0
}

pub fn recycle(paths: &[PathBuf], owner: usize) -> bool {
    shell_operation(FO_DELETE, paths, None, FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_WANTNUKEWARNING, owner)
}

pub fn delete_permanently(paths: &[PathBuf], owner: usize) -> bool {
    shell_operation(FO_DELETE, paths, None, FOF_NOCONFIRMATION, owner)
}

pub fn copy_into(paths: &[PathBuf], target: &Path, owner: usize) -> bool {
    let same_folder = paths.iter().any(|path| path.parent() == Some(target));
    let collision = if same_folder { FOF_RENAMEONCOLLISION } else { 0 };
    shell_operation(FO_COPY, paths, Some(target), FOF_ALLOWUNDO | collision, owner)
}

pub fn move_into(paths: &[PathBuf], target: &Path, owner: usize) -> bool {
    shell_operation(FO_MOVE, paths, Some(target), FOF_ALLOWUNDO, owner)
}

pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (wide(from), wide(to));
    match unsafe { MoveFileW(from.as_ptr(), to.as_ptr()) } {
        0 => Err(io::Error::last_os_error()),
        _ => Ok(()),
    }
}

pub fn recycle_bin_size() -> Option<u64> {
    let mut info: SHQUERYRBINFO = unsafe { zeroed() };
    info.cbSize = size_of::<SHQUERYRBINFO>() as u32;
    let result = unsafe { SHQueryRecycleBinW(null(), &mut info) };
    (result == 0).then_some(info.i64Size as u64)
}

pub fn empty_recycle_bin() {
    unsafe { SHEmptyRecycleBinW(null_mut(), null(), 0) };
}
