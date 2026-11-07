//! Windows shell icons and thumbnails. `mod.rs` holds the shared constants,
//! the `Source`/`IconKey` keys and the pure classification logic; `image.rs`
//! does the Win32 image loading on worker threads; `store.rs` is the cache and
//! the `IconStore` the app talks to.

use crate::model::item::{ATTR_READONLY, ATTR_REPARSE_POINT, ATTR_SYSTEM, Item};
use crate::model::query::extension_of;
use slint::{Rgba8Pixel, SharedPixelBuffer};
use std::path::PathBuf;
use std::time::Duration;

pub const THIS_PC: &str = "::{20D04FE0-3AEA-1069-A2D8-08002B30309D}";

const PLAIN_FOLDER: &str = "shell:Windows";
const MAX_LOADER_THREADS: usize = 6;
const MAX_QUEUED: usize = 192;
const CACHE_BYTES: usize = 256 << 20;
const POLL_INTERVAL: Duration = Duration::from_millis(24);
const PENDING_RETRIES: u32 = 50;
const PENDING_RETRY_DELAY: Duration = Duration::from_millis(20);
const ATTR_OFFLINE: u32 = 0x1000;
const ATTR_RECALL_ON_OPEN: u32 = 0x4_0000;
const ATTR_RECALL_ON_DATA_ACCESS: u32 = 0x40_0000;
const NOT_DOWNLOADED: u32 = ATTR_OFFLINE | ATTR_RECALL_ON_OPEN | ATTR_RECALL_ON_DATA_ACCESS;
const CUSTOMIZABLE_FOLDER: u32 = ATTR_READONLY | ATTR_SYSTEM | ATTR_REPARSE_POINT;

const THUMBNAIL_EXTENSIONS: [&str; 30] = [
    "jpg", "jpeg", "jpe", "jfif", "png", "gif", "bmp", "dib", "tif", "tiff", "webp", "heic", "heif", "avif", "cr2",
    "nef", "arw", "dng", "orf", "rw2", "mp4", "m4v", "mkv", "mov", "avi", "wmv", "webm", "mpg", "mpeg", "pdf",
];
const OWN_ICON_EXTENSIONS: [&str; 9] = ["exe", "ico", "lnk", "url", "scr", "cur", "ani", "msc", "appref-ms"];

type Pixels = SharedPixelBuffer<Rgba8Pixel>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Source {
    Folder,
    Extension(String),
    Path(PathBuf, u64),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct IconKey {
    source: Source,
    size: u32,
    thumbnail: bool,
}

fn wants_thumbnail(extension: &str, attrs: u32, thumbnails: bool) -> bool {
    thumbnails && attrs & NOT_DOWNLOADED == 0 && THUMBNAIL_EXTENSIONS.contains(&extension)
}

fn item_source(item: &Item, thumbnail: bool) -> Source {
    let own_path = || Source::Path(item.path(), item.times.modified);
    if item.is_dir {
        return if item.attrs & CUSTOMIZABLE_FOLDER != 0 { own_path() } else { Source::Folder };
    }
    let extension = extension_of(&item.name);
    if thumbnail || OWN_ICON_EXTENSIONS.contains(&extension.as_str()) {
        own_path()
    } else {
        Source::Extension(extension)
    }
}

fn generic_source(item: &Item) -> Source {
    if item.is_dir { Source::Folder } else { Source::Extension(extension_of(&item.name)) }
}

fn shell_path<'a>(source: &Source, requester: &'a str) -> &'a str {
    if *source == Source::Folder { PLAIN_FOLDER } else { requester }
}

mod image;
mod store;

pub use store::IconStore;

#[cfg(test)]
mod tests;
