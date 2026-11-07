use crate::model::chart;
use crate::util::export;
use crate::util::names;
use crate::util::format::{Unit, format_elapsed, format_size, plural};
use crate::util::hash::hash_file;
use crate::icons::IconStore;
use crate::model::item::{ATTR_HIDDEN, Column, Item};
use crate::model::list::{ListModel, ListState, View};
use crate::platform::{self, ClipboardFiles, DiskUsage, Drive};
use crate::model::query::{self, DateField, DateRule, Query, SummaryEntry, SummaryKind};
use crate::model::scan::{Cache, DirNode, Level, Live, ScanContext, ScanError, SubDir, read_level, scan_tree};
use crate::settings::Settings;
use crate::sidebar::Sidebar;
use crate::ui::{AppWindow, ColumnChoice, ColumnInfo, Crumb, DriveCard};
use rayon::prelude::*;
use slint::{
    CloseRequestResponse, ComponentHandle, Image, LogicalSize, ModelRc, PhysicalSize, SharedString, Timer, TimerMode, VecModel, Weak,
};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Component, Path, PathBuf};
use std::rc::{self, Rc};
use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const PUMP_INTERVAL: Duration = Duration::from_millis(16);
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);
const SEARCH_DELAY: Duration = Duration::from_millis(180);
const TYPE_AHEAD_PAUSE: Duration = Duration::from_secs(1);
const NEARLY_FULL: f32 = 0.9;
const DEFAULT_WINDOW_SIZE: (f32, f32) = (1180.0, 720.0);
const DRIVE_ICON_SIZE: u32 = 48;
const BYTES_PER_MEGABYTE: f64 = 1_048_576.0;
const SUMMARY_KINDS: [SummaryKind; 5] = [
    SummaryKind::Folders,
    SummaryKind::Extension,
    SummaryKind::Age(DateField::Modified),
    SummaryKind::Age(DateField::Created),
    SummaryKind::Age(DateField::Accessed),
];
const DATE_FIELDS: [DateField; 3] = [DateField::Modified, DateField::Created, DateField::Accessed];
const DATE_RULES: [DateRule; 3] = [DateRule::After, DateRule::Before, DateRule::On];

type Shared = Rc<RefCell<App>>;

#[derive(Clone, PartialEq)]
enum Location {
    Computer,
    Folder(PathBuf),
}

enum Message {
    Drives(Vec<Drive>),
    Pins(Vec<(PathBuf, String)>),
    TreeChildren { parent: PathBuf, folders: Vec<(OsString, u32)> },
    Listed { scan: u64, items: Vec<Item>, lives: Arc<Vec<Live>> },
    Unreadable { scan: u64, reason: String },
    Disk { scan: u64, title: String, usage: Option<DiskUsage> },
    Sized { scan: u64, index: usize, node: Arc<DirNode> },
    Finished {
        scan: u64,
        generation: u64,
        path: PathBuf,
        node: Arc<DirNode>,
        errors: Vec<ScanError>,
        seconds: f64,
        from_cache: bool,
    },
    Found { search: u64, items: Vec<Item>, seconds: f64 },
    Summary { id: u64, entries: Vec<SummaryEntry> },
    Hashed { run: u64, index: usize, hashes: Option<[String; 4]> },
    Deleted { paths: Vec<PathBuf>, succeeded: bool },
    Pasted { paths: Vec<PathBuf>, target: PathBuf, is_cut: bool, succeeded: bool, sequence: u32 },
    RecycleBin(Option<u64>),
}

struct App {
    ui: Weak<AppWindow>,
    me: rc::Weak<RefCell<App>>,
    list: Rc<ListModel>,
    icons: Rc<IconStore>,
    drive_icons_missing: Cell<bool>,
    sidebar: Sidebar,
    settings: Settings,
    cache: Cache,
    drives: Vec<Drive>,
    location: Location,
    back: Vec<Location>,
    forward: Vec<Location>,
    scan_id: u64,
    scanning: bool,
    cancel: Arc<AtomicBool>,
    lives: Arc<Vec<Live>>,
    scan_started: Instant,
    last_progress: Instant,
    activity: String,
    errors: Vec<ScanError>,
    search_id: u64,
    active_query: Option<Query>,
    showing_results: bool,
    search_timer: Timer,
    summary_id: u64,
    summary_kind: usize,
    summary: Vec<SummaryEntry>,
    hash_run: u64,
    hash_cancel: Arc<AtomicBool>,
    hashes_left: usize,
    cut_paths: HashSet<PathBuf>,
    cut_sequence: u32,
    pending_select: Option<String>,
    pending_rename: bool,
    renaming: Option<PathBuf>,
    typed: String,
    typed_at: Instant,
    sender: Sender<Message>,
    receiver: Receiver<Message>,
    workers: Arc<()>,
    spawned: u64,
    pump: Timer,
}

mod fileops;
mod hashing;
mod lifecycle;
mod navigation;
mod places;
mod scanning;
mod search;
mod status;
mod summary;
mod support;
mod view;
mod wiring;

use support::*;
use wiring::*;

pub use lifecycle::run;

#[cfg(test)]
mod tests;
