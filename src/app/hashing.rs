use super::*;

impl App {
    pub(crate) fn cancel_hashing(&mut self) {
        self.hash_cancel.store(true, Relaxed);
        self.hash_run += 1;
        if std::mem::take(&mut self.hashes_left) > 0 {
            self.on_hashing_ended();
        }
    }

    pub(crate) fn on_hashing_ended(&self) {
        if self.list.state().sort.hash_slot().is_some() {
            self.list.update(ListState::rebuild);
        }
        if !self.scanning {
            self.ui().set_status_activity(self.activity.as_str().into());
        }
    }

    pub(crate) fn start_hashing(&mut self) {
        self.cancel_hashing();
        self.hash_cancel = Arc::default();
        let mut wanted = [false; 4];
        let targets: Vec<(usize, PathBuf)> = {
            let state = self.list.state();
            for slot in state.columns.iter().filter_map(|column| column.hash_slot()) {
                wanted[slot] = true;
            }
            let missing = |item: &Item| {
                wanted.contains(&true)
                    && !item.is_dir
                    && item.hashes.as_ref().is_none_or(|hashes| (0..4).any(|slot| wanted[slot] && hashes[slot].is_empty()))
            };
            state.items.iter().enumerate().filter(|(_, item)| missing(item)).map(|(index, item)| (index, item.path())).collect()
        };
        self.hashes_left = targets.len();
        if !targets.is_empty() {
            let (run, cancel) = (self.hash_run, self.hash_cancel.clone());
            self.spawn(move |sender| {
                targets.par_iter().for_each(|(index, path)| {
                    let hashes = hash_file(path, wanted, &cancel).ok();
                    let _ = sender.send(Message::Hashed { run, index: *index, hashes });
                });
            });
        }
        self.sync_busy();
    }

    pub(crate) fn on_hashed(&mut self, index: usize, hashes: Option<[String; 4]>) {
        self.hashes_left = self.hashes_left.saturating_sub(1);
        self.list.update_item(index, |item| {
            let merged = item.hashes.get_or_insert_with(Default::default);
            match hashes {
                Some(hashes) => {
                    for (slot, hash) in hashes.into_iter().enumerate().filter(|(_, hash)| !hash.is_empty()) {
                        merged[slot] = hash;
                    }
                }
                None => merged.iter_mut().filter(|hash| hash.is_empty()).for_each(|hash| *hash = "Unreadable".into()),
            }
        });
        if self.hashes_left == 0 {
            self.on_hashing_ended();
        } else if !self.scanning {
            let activity = format!("Calculating checksums · {} left", plural(self.hashes_left as u64, "file"));
            self.ui().set_status_activity(activity.into());
        }
        self.sync_busy();
    }

    // Columns and display

}
