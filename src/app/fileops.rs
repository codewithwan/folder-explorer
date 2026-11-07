use super::*;

impl App {
    pub(crate) fn press_row(&mut self, row: usize, toggle: bool, extend: bool) {
        self.list.change_selection(|state| state.press(row, toggle, extend));
        self.show_selection();
    }

    pub(crate) fn context_row(&mut self, row: usize) {
        if !self.list.state().is_row_selected(row) {
            self.press_row(row, false, false);
        }
    }

    pub(crate) fn clear_selection(&mut self) {
        self.list.change_selection(ListState::clear_selection);
        self.show_selection();
    }

    pub(crate) fn move_cursor(&mut self, delta: i32, extend: bool) {
        if let Some(row) = self.list.change_selection(|state| state.move_cursor(delta, extend)) {
            self.ui().invoke_reveal_row(row as i32);
            self.show_selection();
        }
    }

    pub(crate) fn type_ahead(&mut self, text: &str) {
        if !is_name_text(text) {
            return;
        }
        if self.typed_at.elapsed() > TYPE_AHEAD_PAUSE {
            self.typed.clear();
        }
        self.typed_at = Instant::now();
        self.typed.push_str(text);
        if let Some(row) = self.list.change_selection(|state| state.select_typed(&self.typed)) {
            self.ui().invoke_reveal_row(row as i32);
            self.show_selection();
        }
    }

    pub(crate) fn selected_paths(&self) -> Vec<PathBuf> {
        self.list.state().selected_items().map(Item::path).collect()
    }

    pub(crate) fn first_selected(&self) -> Option<PathBuf> {
        self.list.state().selected_items().next().map(Item::path)
    }

    pub(crate) fn list_action(&mut self, action: &str) {
        match action {
            "cut" => self.cut_selection(),
            "copy" => self.copy_selection(),
            "rename" => self.begin_rename(),
            "copy-path" => {
                let paths: Vec<OsString> = self.selected_paths().into_iter().map(PathBuf::into_os_string).collect();
                platform::copy_text(paths.join(OsStr::new("\r\n")));
            }
            "select-all" => {
                self.list.change_selection(ListState::select_all);
                self.show_selection();
            }
            "select-inverse" => {
                self.list.change_selection(ListState::invert_selection);
                self.show_selection();
            }
            "recalculate" => {
                for path in self.selected_paths() {
                    self.forget(&path);
                }
                self.reload();
            }
            "open-location" => {
                if let Some(parent) = self.first_selected().as_deref().and_then(Path::parent) {
                    self.go(Location::Folder(parent.to_path_buf()));
                }
            }
            "refresh" => self.refresh(),
            _ => {}
        }
    }

    // File operations

    pub(crate) fn editable_folder(&self) -> Option<PathBuf> {
        match &self.location {
            Location::Folder(path) if !self.showing_results => Some(path.clone()),
            _ => None,
        }
    }

    pub(crate) fn refresh_can_paste(&self) {
        self.ui().set_can_paste(self.editable_folder().is_some() && platform::clipboard_has_files());
    }

    pub(crate) fn cut_selection(&mut self) {
        let paths = self.selected_paths();
        if !paths.is_empty() && platform::cut_files(&paths) {
            self.cut_sequence = platform::clipboard_sequence();
            self.cut_paths = paths.into_iter().collect();
            self.show_cut_marks();
        }
    }

    pub(crate) fn copy_selection(&mut self) {
        let paths = self.selected_paths();
        if !paths.is_empty() && platform::copy_files(&paths) {
            self.cut_paths.clear();
            self.show_cut_marks();
        }
    }

    pub(crate) fn show_cut_marks(&self) {
        self.list.update(|state| state.mark_cut(&self.cut_paths));
    }

    pub(crate) fn restore_cut_marks(&mut self) {
        if platform::clipboard_sequence() != self.cut_sequence {
            self.cut_paths.clear();
        }
        if !self.cut_paths.is_empty() {
            self.show_cut_marks();
        }
    }

    pub(crate) fn begin_rename(&mut self) {
        let target = self
            .list
            .state()
            .only_selected()
            .map(|(row, item)| (row, item.name.to_string(), item.is_dir, item.path()));
        if let Some((row, name, is_dir, path)) = target {
            self.renaming = Some(path);
            let stem = names::editable_stem_length(&name, is_dir);
            self.ui().invoke_begin_rename(row as i32, name.into(), stem as i32);
        }
    }

    pub(crate) fn cancel_rename(&mut self) {
        self.renaming = None;
        self.ui().invoke_focus_list();
    }

    pub(crate) fn on_renamed(&mut self, old: &Path, new: &Path) {
        let retired = self.cache.rename(old, new);
        thread::spawn(move || drop(retired));
        self.pending_select = new.file_name().map(|name| name.to_string_lossy().to_string());
        self.refresh_tree(new.parent());
        self.reload();
    }

    pub(crate) fn on_created(&mut self, parent: &Path, name: String) {
        self.forget(&parent.join(&name));
        self.pending_select = Some(name);
        self.pending_rename = true;
        self.refresh_tree(Some(parent));
        self.reload();
    }

    pub(crate) fn paste(&mut self, files: ClipboardFiles, owner: usize) {
        let Some(target) = self.editable_folder() else { return };
        let ClipboardFiles { mut paths, is_cut, sequence } = files;
        if is_cut {
            paths.retain(|path| path.parent() != Some(target.as_path()));
        }
        if paths.is_empty() {
            return;
        }
        self.spawn(move |sender| {
            let succeeded = match is_cut {
                true => platform::move_into(&paths, &target, owner),
                false => platform::copy_into(&paths, &target, owner),
            };
            let _ = sender.send(Message::Pasted { paths, target, is_cut, succeeded, sequence });
            let _ = sender.send(Message::RecycleBin(platform::recycle_bin_size()));
        });
    }

    pub(crate) fn on_pasted(&mut self, paths: &[PathBuf], target: &Path, is_cut: bool, succeeded: bool, sequence: u32) {
        for path in paths {
            if let Some(name) = path.file_name() {
                self.forget(&target.join(name));
            }
            if is_cut {
                self.forget(path);
                self.refresh_tree(path.parent());
            }
        }
        if is_cut && succeeded {
            platform::clear_clipboard_if_unchanged(sequence);
            self.cut_paths.clear();
        }
        self.refresh_tree(Some(target));
        self.reload();
        if !succeeded {
            self.ui().set_status_activity("Some items were not pasted.".into());
        }
    }

    pub(crate) fn delete(&mut self, paths: Vec<PathBuf>, permanently: bool, owner: usize) {
        self.cancel_scan();
        self.spawn(move |sender| {
            let succeeded = match permanently {
                true => platform::delete_permanently(&paths, owner),
                false => platform::recycle(&paths, owner),
            };
            let _ = sender.send(Message::Deleted { paths, succeeded });
            let _ = sender.send(Message::RecycleBin(platform::recycle_bin_size()));
        });
    }

    pub(crate) fn on_deleted(&mut self, paths: &[PathBuf], succeeded: bool) {
        for path in paths {
            self.forget(path);
        }
        self.refresh_tree(paths.first().and_then(|path| path.parent()));
        self.reload();
        if !succeeded {
            self.ui().set_status_activity("Some items could not be deleted.".into());
        }
    }

    pub(crate) fn show_recycle_bin(&self, size: Option<u64>) {
        let title = match size {
            Some(size) if size > 0 => format!("Empty Recycle Bin ({})", format_size(size, self.settings.unit)),
            _ => "Empty Recycle Bin".into(),
        };
        self.ui().set_recycle_bin_title(title.into());
    }
}
