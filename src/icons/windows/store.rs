//! The icon cache and the `IconStore` the app talks to: look up or queue
//! icons for visible rows, collect finished ones, and evict by age.

use super::*;
use super::image::{Queue, Request, load_requests};
use crate::model::item::Item;
use crate::model::query::extension_of;
use slint::{Image, Timer, TimerMode};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;

struct Entry {
    image: Image,
    bytes: usize,
    last_used: u64,
}

pub struct IconStore {
    me: Weak<IconStore>,
    scale: Cell<f32>,
    entries: RefCell<HashMap<IconKey, Entry>>,
    cached_bytes: Cell<usize>,
    clock: Cell<u64>,
    pending: RefCell<HashSet<IconKey>>,
    queue: Arc<Queue>,
    results: Receiver<(IconKey, Option<Pixels>)>,
    timer: Timer,
    listener: RefCell<Option<Box<dyn Fn()>>>,
}

impl IconStore {
    pub fn new() -> Rc<Self> {
        let queue = Arc::new(Queue::default());
        let (sender, results) = mpsc::channel();
        let threads = thread::available_parallelism().map_or(2, |count| (count.get() / 2).clamp(2, MAX_LOADER_THREADS));
        for _ in 0..threads {
            let (queue, sender) = (queue.clone(), sender.clone());
            thread::spawn(move || load_requests(queue, sender));
        }
        Rc::new_cyclic(|me| Self {
            me: me.clone(),
            scale: Cell::new(1.0),
            entries: RefCell::default(),
            cached_bytes: Cell::new(0),
            clock: Cell::new(0),
            pending: RefCell::default(),
            queue,
            results,
            timer: Timer::default(),
            listener: RefCell::default(),
        })
    }

    pub fn set_scale(&self, scale: f32) {
        self.scale.set(scale);
    }

    pub fn set_listener(&self, listener: impl Fn() + 'static) {
        *self.listener.borrow_mut() = Some(Box::new(listener));
    }

    pub fn cancel_queued(&self) {
        let dropped: Vec<Request> = self.queue.requests.lock().unwrap().drain(..).collect();
        let mut pending = self.pending.borrow_mut();
        for request in dropped {
            pending.remove(&request.key);
        }
    }

    pub fn item_icon(&self, item: &Item, size: u32, thumbnails: bool) -> (Image, bool) {
        let thumbnail = !item.is_dir && wants_thumbnail(&extension_of(&item.name), item.attrs, thumbnails);
        let key = IconKey { source: item_source(item, thumbnail), size, thumbnail };
        if let Some(image) = self.lookup(&key) {
            return (image, true);
        }
        let path = item.path().to_string_lossy().to_string();
        let generic = IconKey { source: generic_source(item), size, thumbnail: false };
        let placeholder = self.lookup(&generic);
        if placeholder.is_none() && generic != key {
            self.request(generic, path.clone());
        }
        self.request(key, path);
        (placeholder.unwrap_or_default(), false)
    }

    pub fn folder_icon(&self, path: &Path, attrs: u32, size: u32) -> Option<Image> {
        let source = if attrs & CUSTOMIZABLE_FOLDER != 0 { Source::Path(path.to_path_buf(), 0) } else { Source::Folder };
        self.shell_icon(source, &path.to_string_lossy(), size)
    }

    pub fn location_icon(&self, parsing_name: &str, size: u32) -> Option<Image> {
        self.shell_icon(Source::Path(parsing_name.into(), 0), parsing_name, size)
    }

    fn shell_icon(&self, source: Source, path: &str, size: u32) -> Option<Image> {
        let key = IconKey { source, size, thumbnail: false };
        let image = self.lookup(&key);
        if image.is_none() {
            self.request(key, path.to_string());
        }
        image
    }

    fn lookup(&self, key: &IconKey) -> Option<Image> {
        let now = self.clock.get() + 1;
        self.clock.set(now);
        let mut entries = self.entries.borrow_mut();
        let entry = entries.get_mut(key)?;
        entry.last_used = now;
        Some(entry.image.clone())
    }

    fn request(&self, key: IconKey, path: String) {
        if !self.pending.borrow_mut().insert(key.clone()) {
            return;
        }
        let pixels = (key.size as f32 * self.scale.get()).ceil() as u32;
        let dropped = {
            let mut requests = self.queue.requests.lock().unwrap();
            requests.push_back(Request { key, path, pixels });
            (requests.len() > MAX_QUEUED).then(|| requests.pop_front()).flatten()
        };
        if let Some(request) = dropped {
            self.pending.borrow_mut().remove(&request.key);
        }
        self.queue.wake.notify_one();
        if !self.timer.running() {
            let me = self.me.clone();
            self.timer.start(TimerMode::Repeated, POLL_INTERVAL, move || {
                if let Some(store) = me.upgrade() {
                    store.collect();
                }
            });
        }
    }

    fn collect(&self) {
        let mut changed = false;
        while let Ok((key, pixels)) = self.results.try_recv() {
            self.pending.borrow_mut().remove(&key);
            self.insert(key, pixels);
            changed = true;
        }
        if self.pending.borrow().is_empty() {
            self.timer.stop();
        }
        if changed && let Some(listener) = &*self.listener.borrow() {
            listener();
        }
    }

    fn insert(&self, key: IconKey, pixels: Option<Pixels>) {
        let bytes = pixels.as_ref().map_or(0, |pixels| pixels.as_bytes().len());
        let image = pixels.map_or_else(Image::default, Image::from_rgba8);
        let entry = Entry { image, bytes, last_used: self.clock.get() };
        let replaced = self.entries.borrow_mut().insert(key, entry);
        let total = self.cached_bytes.get() + bytes - replaced.map_or(0, |entry| entry.bytes);
        self.cached_bytes.set(total);
        if total > CACHE_BYTES {
            self.evict();
        }
    }

    fn evict(&self) {
        let mut entries = self.entries.borrow_mut();
        let mut by_age: Vec<(u64, IconKey)> =
            entries.iter().map(|(key, entry)| (entry.last_used, key.clone())).collect();
        by_age.sort_unstable_by_key(|(last_used, _)| *last_used);
        let mut total = self.cached_bytes.get();
        for (_, key) in by_age {
            if total <= CACHE_BYTES / 2 {
                break;
            }
            if let Some(entry) = entries.remove(&key) {
                total -= entry.bytes;
            }
        }
        self.cached_bytes.set(total);
    }
}
