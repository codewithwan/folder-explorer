//! Layer 1: OS integration. Dispatches to the per-OS implementation under
//! `windows/` or `unix/`; both expose the same API (time, window/placement,
//! drives, shell/file operations, clipboard, dialogs). Shared data types
//! (`Drive`, `DiskUsage`, `WindowPlacement`, `ClipboardFiles`) are defined
//! inside each OS module and re-exported here.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;
