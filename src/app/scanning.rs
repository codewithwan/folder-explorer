use super::*;

impl App {
    pub(crate) fn reload(&mut self) {
        let query = self.active_query.clone();
        self.showing_results &= query.is_some();
        self.enter_location();
        if let Some(query) = query {
            self.run_query(query);
        }
    }

    pub(crate) fn refresh(&mut self) {
        if let Location::Folder(path) = self.location.clone() {
            self.forget(&path);
        }
        self.load_pins();
        self.reload();
    }

    pub(crate) fn forget(&mut self, path: &Path) {
        let retired = self.cache.invalidate(path);
        thread::spawn(move || drop(retired));
    }

    // Scanning

    pub(crate) fn start_scan(&mut self, path: PathBuf) {
        self.scan_id += 1;
        self.scanning = true;
        self.scan_started = Instant::now();
        self.cancel = Arc::default();
        self.lives = Arc::default();
        let (known, generation) = (self.cache.known_below(&path), self.cache.generation());
        let (scan, cancel) = (self.scan_id, self.cancel.clone());
        self.spawn(move |sender| scan_folder(scan, generation, path, known, cancel, sender));
        self.sync_busy();
    }

    pub(crate) fn stop_work(&mut self) {
        self.cancel_scan();
        self.search_id += 1;
        self.summary_id += 1;
        self.search_timer.stop();
        self.scanning = false;
        self.scan_id += 1;
        self.sync_busy();
    }

    pub(crate) fn cancel_scan(&mut self) {
        self.cancel.store(true, Relaxed);
        self.cancel_hashing();
        self.sync_busy();
    }

    pub(crate) fn sync_busy(&self) {
        self.ui().set_busy(self.scanning || self.hashes_left > 0);
    }

    pub(crate) fn on_listed(&mut self, items: Vec<Item>, lives: Arc<Vec<Live>>) {
        if self.showing_results {
            return;
        }
        self.list.set_items(items);
        self.lives = lives;
        self.ui().set_list_message("This folder is empty.".into());
        self.restore_cut_marks();
        self.show_selection();
        self.start_hashing();
        self.apply_pending_selection();
    }

    pub(crate) fn apply_pending_selection(&mut self) {
        let Some(name) = self.pending_select.take() else { return };
        let rename = std::mem::take(&mut self.pending_rename);
        if let Some(row) = self.list.change_selection(|state| state.select_name(&name)) {
            self.ui().invoke_reveal_row(row as i32);
            self.show_selection();
            if rename {
                self.begin_rename();
            }
        }
    }

    pub(crate) fn on_unreadable(&mut self, reason: String) {
        self.scanning = false;
        self.active_query = None;
        let ui = self.ui();
        ui.set_list_message(reason.into());
        ui.set_status_activity(SharedString::new());
        self.sync_busy();
    }

    pub(crate) fn on_disk(&self, title: &str, usage: Option<DiskUsage>) {
        let ui = self.ui();
        match usage {
            Some(usage) => {
                ui.set_disk_text(disk_label(title, usage, self.settings.unit).into());
                ui.set_disk_ratio(usage.used_ratio());
                ui.set_disk_nearly_full(usage.used_ratio() > NEARLY_FULL);
            }
            None => ui.set_disk_text(SharedString::new()),
        }
    }

    pub(crate) fn on_finished(&mut self, node: &DirNode, errors: Vec<ScanError>, seconds: f64, from_cache: bool) {
        self.scanning = false;
        self.errors = errors;
        self.lives = Arc::default();
        self.list.update(ListState::rebuild);
        self.show_errors();
        self.show_selection();
        let files = plural(node.file_count, "file");
        let elapsed = format_elapsed(seconds);
        self.activity = match (node.complete, from_cache) {
            (false, _) => format!("Stopped after {files} in {elapsed}"),
            (true, true) => files,
            (true, false) => format!("Scanned {files} in {elapsed}"),
        };
        self.ui().set_status_activity(self.activity.as_str().into());
        self.sync_busy();
        if node.complete {
            self.refresh_summary();
            self.execute_query();
        } else {
            self.active_query = None;
        }
    }

    pub(crate) fn show_scan_progress(&self) {
        let (files, size) = self.lives.iter().fold((0, 0), |(files, size), live| {
            (files + live.files.load(Relaxed), size + live.size.load(Relaxed))
        });
        let elapsed = format_elapsed(self.scan_started.elapsed().as_secs_f64());
        let waiting = if self.active_query.is_some() { " · search runs when this finishes" } else { "" };
        let text = format!(
            "Scanning {} · {} · {elapsed}{waiting}",
            plural(files, "file"),
            format_size(size, self.settings.unit)
        );
        self.ui().set_status_activity(text.into());
        self.show_totals();
    }

    // Search

}
