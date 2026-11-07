use super::*;

impl App {
    pub(crate) fn go(&mut self, location: Location) {
        if location == self.location && !self.showing_results {
            return;
        }
        let previous = std::mem::replace(&mut self.location, location);
        if previous != self.location {
            self.back.push(previous);
            self.forward.clear();
        }
        self.show_location();
        self.ui().invoke_focus_list();
    }

    pub(crate) fn go_back(&mut self) {
        if self.showing_results || self.active_query.is_some() {
            self.show_location();
        } else if let Some(previous) = self.back.pop() {
            self.forward.push(std::mem::replace(&mut self.location, previous));
            self.show_location();
            self.ui().invoke_focus_list();
        }
    }

    pub(crate) fn go_forward(&mut self) {
        if let Some(next) = self.forward.pop() {
            self.back.push(std::mem::replace(&mut self.location, next));
            self.show_location();
            self.ui().invoke_focus_list();
        }
    }

    pub(crate) fn go_up(&mut self) {
        if let Location::Folder(path) = &self.location {
            let parent = path.parent().map_or(Location::Computer, |parent| Location::Folder(parent.to_path_buf()));
            self.go(parent);
        }
    }

    pub(crate) fn open_target(&mut self, target: &str) {
        let location =
            if target.is_empty() { Location::Computer } else { Location::Folder(resolve_target(&self.location, target)) };
        self.go(location);
    }

    pub(crate) fn enter_typed_path(&mut self, text: &str) {
        let text = text.trim();
        if text.eq_ignore_ascii_case("this pc") {
            self.go(Location::Computer);
        } else if !text.is_empty() {
            self.go(Location::Folder(resolve_target(&self.location, text)));
        }
    }

    pub(crate) fn show_location(&mut self) {
        self.showing_results = false;
        self.summary.clear();
        self.icons.cancel_queued();
        self.list.set_items(Vec::new());
        if let Location::Folder(path) = &self.location {
            self.show_view(self.settings.view_of(path));
        }
        let ui = self.ui();
        ui.set_search_text(SharedString::new());
        ui.invoke_scroll_to_top();
        self.enter_location();
    }

    pub(crate) fn enter_location(&mut self) {
        self.stop_work();
        self.active_query = None;
        self.errors.clear();
        let ui = self.ui();
        ui.set_in_search_results(self.showing_results);
        ui.set_can_go_back(!self.back.is_empty());
        ui.set_can_go_forward(!self.forward.is_empty());
        ui.set_can_go_up(self.location != Location::Computer);
        ui.set_crumbs(ModelRc::new(VecModel::from(crumbs_for(&self.location))));
        ui.set_status_activity(SharedString::new());
        self.sync_columns();
        self.show_errors();
        self.show_summary();

        let location = self.location.clone();
        let path = match &location {
            Location::Computer => None,
            Location::Folder(path) => Some(path.as_path()),
        };
        let next = self.sidebar.reveal(path);
        self.load_tree_children(next);
        self.scroll_sidebar();
        ui.set_current_path(path.map_or("This PC".into(), |path| path.to_string_lossy().as_ref().into()));
        ui.set_showing_drives(path.is_none());
        ui.set_can_search(path.is_some());
        ui.set_can_edit_folder(path.is_some() && !self.showing_results);
        self.show_selection();

        match location {
            Location::Computer => {
                ui.set_disk_text(SharedString::new());
                ui.set_status_items(plural(self.drives.len() as u64, "drive").into());
                self.load_drives();
            }
            Location::Folder(path) => {
                ui.set_list_message(SharedString::new());
                self.start_scan(path);
            }
        }
    }

}
