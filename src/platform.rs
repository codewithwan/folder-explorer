//! OS integration. The Windows implementation lives in `platform_windows.rs`
//! and the macOS/Unix port in `platform_unix.rs`; both expose the same API.

#[cfg(windows)]
#[path = "platform_windows.rs"]
mod imp;

#[cfg(not(windows))]
#[path = "platform_unix.rs"]
mod imp;

pub use imp::*;
