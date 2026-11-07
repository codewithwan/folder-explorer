use crate::icons::{IconStore, THIS_PC};
use crate::model::item::compare_text;
use crate::platform::{Drive, QUICK_ACCESS};
use crate::settings::TreeMemory;
use crate::ui::TreeRow;
use slint::{Image, Model, ModelRc, SharedString, VecModel};
use std::cell::Cell;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const KIND_COMPUTER: i32 = 0;
const KIND_DRIVE: i32 = 1;
const KIND_FOLDER: i32 = 2;
const KIND_PIN: i32 = 3;
const KIND_QUICK_ACCESS: i32 = 4;
const ICON_SIZE: u32 = 16;

struct Node {
    path: Option<PathBuf>,
    name: String,
    depth: i32,
    kind: i32,
    attrs: u32,
    expanded: bool,
    loaded: bool,
}

pub struct Sidebar {
    nodes: Vec<Node>,
    root: usize,
    pins: Vec<(PathBuf, String)>,
    drives: Vec<(PathBuf, String)>,
    memory: TreeMemory,
    model: Rc<VecModel<TreeRow>>,
    selected: Option<PathBuf>,
    computer_selected: bool,
    target: Option<PathBuf>,
    scroll_pending: bool,
    icons: Rc<IconStore>,
    missing_icons: Cell<bool>,
}

impl Sidebar {
    pub fn new(icons: Rc<IconStore>, memory: TreeMemory) -> Self {
        let computer = Node {
            path: None,
            name: "This PC".into(),
            depth: 0,
            kind: KIND_COMPUTER,
            attrs: 0,
            expanded: !memory.computer_closed,
            loaded: true,
        };
        let sidebar = Self {
            nodes: vec![computer],
            root: 0,
            pins: Vec::new(),
            drives: Vec::new(),
            memory,
            model: Rc::new(VecModel::default()),
            selected: None,
            computer_selected: true,
            target: None,
            scroll_pending: false,
            icons,
            missing_icons: Cell::new(false),
        };
        sidebar.sync();
        sidebar
    }

    pub fn model(&self) -> ModelRc<TreeRow> {
        ModelRc::from(self.model.clone())
    }

    pub fn memory(&self) -> TreeMemory {
        self.memory
    }

    fn sync(&self) {
        self.missing_icons.set(false);
        let rows: Vec<TreeRow> = (0..self.nodes.len()).map(|index| self.row(index)).collect();
        if self.model.row_count() == rows.len() {
            for (index, row) in rows.into_iter().enumerate() {
                self.model.set_row_data(index, row);
            }
        } else {
            self.model.set_vec(rows);
        }
    }

    pub fn refresh_icons(&self) {
        if self.missing_icons.get() {
            self.sync();
        }
    }

    fn icon(&self, node: &Node) -> Image {
        let icon = match &node.path {
            None if node.kind == KIND_QUICK_ACCESS => self.icons.location_icon(QUICK_ACCESS, ICON_SIZE),
            None => self.icons.location_icon(THIS_PC, ICON_SIZE),
            Some(path) if matches!(node.kind, KIND_DRIVE | KIND_PIN) => {
                self.icons.location_icon(&path.to_string_lossy(), ICON_SIZE)
            }
            Some(path) => self.icons.folder_icon(path, node.attrs, ICON_SIZE),
        };
        if icon.is_none() {
            self.missing_icons.set(true);
        }
        icon.unwrap_or_default()
    }

    fn selected_row(&self) -> Option<usize> {
        if self.computer_selected {
            return Some(self.root);
        }
        let selected = self.selected.as_ref()?;
        if let Some(pin) = (0..self.root).find(|&row| self.nodes[row].path.as_ref() == Some(selected)) {
            return Some(pin);
        }
        if self.memory.computer_closed {
            return Some(self.root);
        }
        (self.root..self.nodes.len())
            .filter(|&row| self.nodes[row].path.as_ref().is_some_and(|path| selected.starts_with(path)))
            .max_by_key(|&row| self.nodes[row].depth)
    }

    fn row(&self, index: usize) -> TreeRow {
        let node = &self.nodes[index];
        let selected = self.selected_row() == Some(index);
        let has_children = match node.kind {
            KIND_COMPUTER => !self.drives.is_empty(),
            KIND_QUICK_ACCESS => true,
            _ => self.nodes.get(index + 1).is_some_and(|next| next.depth > node.depth),
        };
        TreeRow {
            name: SharedString::from(&node.name),
            depth: node.depth,
            expanded: node.expanded,
            expandable: !node.loaded || has_children,
            selected,
            kind: node.kind,
            icon: self.icon(node),
        }
    }

    pub fn set_pins(&mut self, pins: Vec<(PathBuf, String)>) {
        if self.pins != pins {
            self.pins = pins;
            self.show_pins();
            self.scroll_pending = true;
        }
    }

    fn show_pins(&mut self) {
        let header = (!self.pins.is_empty()).then(|| Node {
            path: None,
            name: "Quick access".into(),
            depth: 0,
            kind: KIND_QUICK_ACCESS,
            attrs: 0,
            expanded: !self.memory.quick_access_closed,
            loaded: true,
        });
        let shown: &[(PathBuf, String)] = if self.memory.quick_access_closed { &[] } else { &self.pins };
        let pins = shown.iter().map(|(path, name)| Node {
            path: Some(path.clone()),
            name: name.clone(),
            depth: 1,
            kind: KIND_PIN,
            attrs: 0,
            expanded: false,
            loaded: true,
        });
        let rows: Vec<Node> = header.into_iter().chain(pins).collect();
        let root = rows.len();
        self.nodes.splice(..self.root, rows);
        self.root = root;
        self.sync();
    }

    pub fn set_drives(&mut self, drives: &[Drive]) -> Option<PathBuf> {
        let drives: Vec<(PathBuf, String)> =
            drives.iter().map(|drive| (drive.root.clone(), drive.title.clone())).collect();
        if drives == self.drives {
            return None;
        }
        self.drives = drives;
        self.nodes.truncate(self.root + 1);
        if !self.memory.computer_closed {
            self.nodes[self.root].expanded = true;
            self.nodes.extend(self.drives.iter().map(|(root, title)| Node {
                path: Some(root.clone()),
                name: title.clone(),
                depth: 1,
                kind: KIND_DRIVE,
                attrs: 0,
                expanded: false,
                loaded: false,
            }));
        } else {
            self.nodes[self.root].expanded = false;
        }
        let request = self.advance();
        self.sync();
        request
    }

}

mod nav;

#[cfg(test)]
mod tests;
