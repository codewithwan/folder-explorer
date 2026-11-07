//! macOS/Unix icon store. For the first usable port we do not fetch native
//! shell icons or thumbnails; `item_icon` returns an empty image, which the
//! `FileIcon` control in the UI renders as a generic (colorized) folder or
//! file glyph. The public API matches `icons_windows.rs` exactly.

use crate::item::Item;
use slint::Image;
use std::path::Path;
use std::rc::Rc;

/// Parsing name for "This PC" on Windows; unused on this platform.
pub const THIS_PC: &str = "";

pub struct IconStore;

impl IconStore {
    pub fn new() -> Rc<Self> {
        Rc::new(IconStore)
    }

    pub fn set_scale(&self, _scale: f32) {}

    pub fn set_listener(&self, _listener: impl Fn() + 'static) {}

    pub fn cancel_queued(&self) {}

    /// Returns an empty image marked resolved, so the UI draws its generic
    /// folder/file fallback and never re-requests the row.
    pub fn item_icon(&self, _item: &Item, _size: u32, _thumbnails: bool) -> (Image, bool) {
        (Image::default(), true)
    }

    pub fn folder_icon(&self, _path: &Path, _attrs: u32, _size: u32) -> Option<Image> {
        None
    }

    pub fn location_icon(&self, _parsing_name: &str, _size: u32) -> Option<Image> {
        None
    }
}
