//! Icon loading. The Windows implementation (shell icons + thumbnails) lives in
//! `icons_windows.rs`; the macOS/Unix port in `icons_unix.rs` is a lightweight
//! store that leaves rows to the generic folder/file fallback in the UI.

#[cfg(windows)]
#[path = "icons_windows.rs"]
mod imp;

#[cfg(not(windows))]
#[path = "icons_unix.rs"]
mod imp;

pub use imp::*;
