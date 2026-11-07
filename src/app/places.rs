use super::*;

impl App {
    pub(crate) fn load_drives(&mut self) {
        self.spawn(|sender| {
            let _ = sender.send(Message::Drives(platform::drives()));
        });
        self.spawn(|sender| {
            let _ = sender.send(Message::RecycleBin(platform::recycle_bin_size()));
        });
    }

    pub(crate) fn on_drives(&mut self, drives: Vec<Drive>) {
        self.drives = drives;
        let next = self.sidebar.set_drives(&self.drives);
        self.load_tree_children(next);
        self.scroll_sidebar();
        self.show_drives();
    }

    pub(crate) fn load_pins(&mut self) {
        self.spawn(|sender| {
            let _ = sender.send(Message::Pins(platform::quick_access_pins()));
        });
    }

    pub(crate) fn on_pins(&mut self, pins: Vec<(PathBuf, String)>) {
        self.sidebar.set_pins(pins);
        self.scroll_sidebar();
    }

    pub(crate) fn scroll_sidebar(&mut self) {
        if let Some(row) = self.sidebar.take_scroll_row() {
            self.ui().invoke_reveal_tree_row(row as i32);
        }
    }

    pub(crate) fn show_drives(&self) {
        let mut missing_icons = false;
        let mut devices = Vec::new();
        let mut network = Vec::new();
        for drive in &self.drives {
            let icon = self.icons.location_icon(&drive.root.to_string_lossy(), DRIVE_ICON_SIZE);
            missing_icons |= icon.is_none();
            let card = drive_card(drive, icon.unwrap_or_default(), self.settings.unit);
            if drive.network {
                network.push(card);
            } else {
                devices.push(card);
            }
        }
        self.drive_icons_missing.set(missing_icons);
        let ui = self.ui();
        if self.location == Location::Computer {
            ui.set_status_items(plural(self.drives.len() as u64, "drive").into());
        }
        ui.set_drives(ModelRc::new(VecModel::from(devices)));
        ui.set_network_drives(ModelRc::new(VecModel::from(network)));
    }

    pub(crate) fn load_tree_children(&mut self, parent: Option<PathBuf>) {
        let Some(parent) = parent else { return };
        let show_hidden = self.settings.show_hidden;
        self.spawn(move |sender| {
            let folders = list_subfolders(&parent, show_hidden);
            let _ = sender.send(Message::TreeChildren { parent, folders });
        });
    }

    pub(crate) fn refresh_tree(&mut self, folder: Option<&Path>) {
        if let Some(folder) = folder.filter(|folder| self.sidebar.is_expanded(folder)) {
            self.load_tree_children(Some(folder.to_path_buf()));
        }
    }

    pub(crate) fn tree_opened(&mut self, row: usize) {
        match self.sidebar.path_at(row) {
            Some(path) => self.go(path.map_or(Location::Computer, Location::Folder)),
            None => self.tree_toggled(row),
        }
    }

    pub(crate) fn tree_toggled(&mut self, row: usize) {
        let parent = self.sidebar.toggle(row);
        self.load_tree_children(parent);
    }

    // Selection

}
