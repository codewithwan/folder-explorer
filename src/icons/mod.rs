//! Layer 1: icon loading. Dispatches to the Windows shell-icon implementation
//! under `windows/` or the macOS/Unix stub in `unix.rs`; both expose the same
//! `IconStore` API and the `THIS_PC` parsing name.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;
