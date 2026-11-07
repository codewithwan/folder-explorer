//! macOS / Unix implementation, split by concern: `time`, `window`, `drives`,
//! `fileops`, `clipboard`, `dialogs`. Timestamps stay in Windows FILETIME
//! ticks so the rest of the app is unchanged. Shared data types mirror the
//! Windows side and are re-exported for a flat `platform::*` API.

use std::path::PathBuf;

/// Parsing name for the Windows "Quick access" shell folder; unused here.
pub const QUICK_ACCESS: &str = "";

#[derive(Clone, Debug)]
pub struct Drive {
    pub root: PathBuf,
    pub title: String,
    pub usage: Option<DiskUsage>,
    pub network: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct DiskUsage {
    pub total: u64,
    pub free: u64,
}

impl DiskUsage {
    pub fn used_ratio(self) -> f32 {
        if self.total == 0 { 0.0 } else { (self.total - self.free) as f32 / self.total as f32 }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WindowPlacement {
    pub rect: [i32; 4],
    pub maximized: bool,
}

impl WindowPlacement {
    pub fn parse(value: &str) -> Option<Self> {
        let fields: Vec<&str> = value.split(',').collect();
        let [left, top, right, bottom, maximized] = fields[..] else { return None };
        let rect = [left.parse().ok()?, top.parse().ok()?, right.parse().ok()?, bottom.parse().ok()?];
        (rect[2] > rect[0] && rect[3] > rect[1]).then_some(Self { rect, maximized: maximized == "true" })
    }

    pub fn serialize(&self) -> String {
        let [left, top, right, bottom] = self.rect;
        format!("{left},{top},{right},{bottom},{}", self.maximized)
    }

    pub fn size(&self) -> (u32, u32) {
        ((self.rect[2] - self.rect[0]) as u32, (self.rect[3] - self.rect[1]) as u32)
    }
}

pub struct ClipboardFiles {
    pub paths: Vec<PathBuf>,
    pub is_cut: bool,
    pub sequence: u32,
}

mod clipboard;
mod dialogs;
mod drives;
mod fileops;
mod time;
mod window;

pub use clipboard::*;
pub use dialogs::*;
pub use drives::*;
pub use fileops::*;
pub use time::*;
pub use window::*;
