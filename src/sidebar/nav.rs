use super::*;

impl Sidebar {
    pub fn take_scroll_row(&mut self) -> Option<usize> {
        if !std::mem::take(&mut self.scroll_pending) {
            return None;
        }
        self.nodes.iter().position(|node| node.path.is_some() && node.path == self.selected)
    }

    pub fn path_at(&self, row: usize) -> Option<Option<PathBuf>> {
        self.nodes.get(row).filter(|node| node.kind != KIND_QUICK_ACCESS).map(|node| node.path.clone())
    }

    pub fn toggle(&mut self, row: usize) -> Option<PathBuf> {
        let node = self.nodes.get(row)?;
        if node.kind == KIND_QUICK_ACCESS {
            self.memory.quick_access_closed = !self.memory.quick_access_closed;
            self.show_pins();
            return None;
        }
        self.target = None;
        self.memory.auto_expand_off = node.expanded;
        if node.kind == KIND_COMPUTER {
            if node.expanded {
                self.memory.computer_closed = true;
                self.collapse(row);
            } else {
                self.memory.computer_closed = false;
                self.expand_computer();
            }
            return None;
        }
        let path = node.path.clone()?;
        if node.expanded {
            self.collapse(row);
            return None;
        }
        Some(path)
    }

    fn expand_computer(&mut self) {
        let root = self.root;
        self.nodes[root].expanded = true;
        self.nodes[root].loaded = true;
        let children: Vec<Node> = self
            .drives
            .iter()
            .map(|(root, title)| Node {
                path: Some(root.clone()),
                name: title.clone(),
                depth: 1,
                kind: KIND_DRIVE,
                attrs: 0,
                expanded: false,
                loaded: false,
            })
            .collect();
        let count = children.len();
        let synced = self.model.row_count() == self.nodes.len();
        self.nodes.splice(root + 1..root + 1, children);
        if synced {
            self.model.set_row_data(root, self.row(root));
            for index in root + 1..=root + count {
                self.model.insert(index, self.row(index));
            }
        } else {
            self.sync();
        }
    }

    fn collapse(&mut self, row: usize) {
        let before_len = self.nodes.len();
        let depth = self.nodes[row].depth;
        let end = self.nodes[row + 1..]
            .iter()
            .position(|node| node.depth <= depth)
            .map_or(self.nodes.len(), |offset| row + 1 + offset);
        self.nodes.drain(row + 1..end);
        self.nodes[row].expanded = false;
        if self.nodes[row].kind != KIND_COMPUTER {
            self.nodes[row].loaded = false;
        }
        if self.model.row_count() == before_len {
            for _ in row + 1..end {
                self.model.remove(row + 1);
            }
            self.model.set_row_data(row, self.row(row));
        } else {
            self.sync();
        }
    }

    pub fn is_expanded(&self, path: &Path) -> bool {
        self.nodes.iter().any(|node| node.expanded && node.path.as_deref() == Some(path))
    }

    pub fn set_children(&mut self, parent: &Path, mut folders: Vec<(OsString, u32)>) -> Option<PathBuf> {
        let Some(row) = (self.root..self.nodes.len()).find(|&row| self.nodes[row].path.as_deref() == Some(parent)) else {
            return self.advance();
        };
        if self.nodes[row].expanded {
            self.collapse(row);
        }
        let before_len = self.nodes.len();
        folders.sort_by(|(a, _), (b, _)| compare_text(&a.to_string_lossy(), &b.to_string_lossy()));
        let depth = self.nodes[row].depth + 1;
        let children: Vec<Node> = folders
            .into_iter()
            .map(|(name, attrs)| Node {
                path: Some(parent.join(&name)),
                name: name.to_string_lossy().into_owned(),
                depth,
                kind: KIND_FOLDER,
                attrs,
                expanded: false,
                loaded: false,
            })
            .collect();
        let count = children.len();
        self.nodes.splice(row + 1..row + 1, children);
        self.nodes[row].expanded = true;
        self.nodes[row].loaded = true;
        let request = self.advance();
        if self.model.row_count() == before_len {
            self.model.set_row_data(row, self.row(row));
            for index in row + 1..row + 1 + count {
                self.model.insert(index, self.row(index));
            }
        } else {
            self.sync();
        }
        request
    }

    pub fn reveal(&mut self, path: Option<&Path>) -> Option<PathBuf> {
        self.computer_selected = path.is_none();
        self.selected = path.map(Path::to_path_buf);
        self.target = path.map(Path::to_path_buf);
        let request = self.advance();
        self.sync();
        request
    }

    pub(super) fn advance(&mut self) -> Option<PathBuf> {
        let target = self.target.clone()?;
        let row = (self.root..self.nodes.len())
            .filter(|row| self.nodes[*row].path.as_ref().is_some_and(|path| target.starts_with(path)))
            .max_by_key(|row| self.nodes[*row].depth)?;
        let node = &self.nodes[row];
        if self.memory.auto_expand_off || node.path.as_deref() == Some(target.as_path()) || (node.loaded && node.expanded) {
            self.target = None;
            self.scroll_pending = true;
            return None;
        }
        node.path.clone()
    }
}
