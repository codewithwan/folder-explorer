use super::*;

impl App {
    pub(crate) fn new(ui: &AppWindow, me: rc::Weak<RefCell<App>>, settings: Settings, icons: Rc<IconStore>) -> Self {
        let state = ListState::new(
            settings.columns.clone(),
            settings.unit,
            settings.sort,
            settings.descending,
            settings.show_hidden,
        );
        let (sender, receiver) = mpsc::channel();
        Self {
            ui: ui.as_weak(),
            me,
            list: Rc::new(ListModel::new(state, icons.clone())),
            sidebar: Sidebar::new(icons.clone(), settings.tree),
            icons,
            drive_icons_missing: Cell::new(false),
            settings,
            cache: Cache::default(),
            drives: Vec::new(),
            location: Location::Computer,
            back: Vec::new(),
            forward: Vec::new(),
            scan_id: 0,
            scanning: false,
            cancel: Arc::default(),
            lives: Arc::default(),
            scan_started: Instant::now(),
            last_progress: Instant::now(),
            activity: String::new(),
            errors: Vec::new(),
            search_id: 0,
            active_query: None,
            showing_results: false,
            search_timer: Timer::default(),
            summary_id: 0,
            summary_kind: 0,
            summary: Vec::new(),
            hash_run: 0,
            hash_cancel: Arc::default(),
            hashes_left: 0,
            cut_paths: HashSet::new(),
            cut_sequence: 0,
            pending_select: None,
            pending_rename: false,
            renaming: None,
            typed: String::new(),
            typed_at: Instant::now(),
            sender,
            receiver,
            workers: Arc::new(()),
            spawned: 0,
            pump: Timer::default(),
        }
    }

    pub(crate) fn ui(&self) -> AppWindow {
        self.ui.upgrade().expect("the window outlives the app state")
    }

    pub(crate) fn spawn(&mut self, work: impl FnOnce(Sender<Message>) + Send + 'static) {
        let (sender, worker) = (self.sender.clone(), self.workers.clone());
        thread::spawn(move || {
            let _worker = worker;
            work(sender);
        });
        self.spawned += 1;
        if !self.pump.running() {
            let me = self.me.clone();
            self.pump.start(TimerMode::Repeated, PUMP_INTERVAL, move || {
                if let Some(shared) = me.upgrade()
                    && let Ok(mut app) = shared.try_borrow_mut()
                {
                    app.drain();
                }
            });
        }
    }

    pub(crate) fn drain(&mut self) {
        let spawned = self.spawned;
        let idle = Arc::strong_count(&self.workers) == 1;
        while let Ok(message) = self.receiver.try_recv() {
            self.handle(message);
        }
        if self.scanning && self.last_progress.elapsed() >= PROGRESS_INTERVAL {
            self.last_progress = Instant::now();
            self.list.apply_live(&self.lives);
            self.show_scan_progress();
        }
        if idle && spawned == self.spawned {
            self.pump.stop();
        }
    }

    pub(crate) fn handle(&mut self, message: Message) {
        match message {
            Message::Drives(drives) => self.on_drives(drives),
            Message::Pins(pins) => self.on_pins(pins),
            Message::TreeChildren { parent, folders } => {
                let next = self.sidebar.set_children(&parent, folders);
                self.load_tree_children(next);
                self.scroll_sidebar();
            }
            Message::Listed { scan, items, lives } if scan == self.scan_id => self.on_listed(items, lives),
            Message::Unreadable { scan, reason } if scan == self.scan_id => self.on_unreadable(reason),
            Message::Disk { scan, title, usage } if scan == self.scan_id => self.on_disk(&title, usage),
            Message::Sized { scan, index, node } if scan == self.scan_id && !self.showing_results => {
                self.list.update_item(index, |item| item.apply_totals(&node));
            }
            Message::Finished { scan, generation, path, node, errors, seconds, from_cache } => {
                self.cache.store_scan(generation, &path, &node);
                if scan == self.scan_id {
                    self.on_finished(&node, errors, seconds, from_cache);
                }
            }
            Message::Found { search, items, seconds } if search == self.search_id => self.on_found(items, seconds),
            Message::Summary { id, entries } if id == self.summary_id => {
                self.summary = entries;
                self.show_summary();
            }
            Message::Hashed { run, index, hashes } if run == self.hash_run => self.on_hashed(index, hashes),
            Message::Deleted { paths, succeeded } => self.on_deleted(&paths, succeeded),
            Message::Pasted { paths, target, is_cut, succeeded, sequence } => {
                self.on_pasted(&paths, &target, is_cut, succeeded, sequence);
            }
            Message::RecycleBin(size) => self.show_recycle_bin(size),
            _ => {}
        }
    }

    pub(crate) fn save_settings(&mut self) {
        let ui = self.ui();
        self.settings.placement = platform::window_placement(ui.window()).or(self.settings.placement);
        self.settings.sidebar_width = ui.get_sidebar_width();
        self.settings.summary_open = ui.get_summary_open();
        self.settings.tree = self.sidebar.memory();
        self.settings.save();
    }
}

pub fn run(start: Option<PathBuf>) -> Result<(), slint::PlatformError> {
    platform::allow_dark_menus();
    let ui = AppWindow::new()?;
    let settings = Settings::load();
    let placement = settings.placement;
    match placement {
        Some(placement) => {
            let (width, height) = placement.size();
            ui.window().set_size(PhysicalSize::new(width, height));
        }
        None => ui.window().set_size(LogicalSize::new(DEFAULT_WINDOW_SIZE.0, DEFAULT_WINDOW_SIZE.1)),
    }
    ui.set_unit_index(settings.unit.index() as i32);
    ui.set_show_hidden(settings.show_hidden);
    ui.set_summary_open(settings.summary_open);
    ui.set_sidebar_width(settings.sidebar_width);

    ui.show()?;

    let icons = IconStore::new();
    icons.set_scale(ui.window().scale_factor());
    let shared: Shared = Rc::new_cyclic(|me| RefCell::new(App::new(&ui, me.clone(), settings, icons.clone())));
    wire(&ui, &shared);
    let (list, me) = (shared.borrow().list.clone(), Rc::downgrade(&shared));
    icons.set_listener(move || {
        list.refresh_icons();
        if let Some(shared) = me.upgrade()
            && let Ok(app) = shared.try_borrow()
        {
            app.refresh_icons();
        }
    });
    {
        let mut app = shared.borrow_mut();
        ui.set_rows(ModelRc::from(app.list.clone() as Rc<dyn slint::Model<Data = _>>));
        ui.set_tree_rows(app.sidebar.model());
        app.load_drives();
        app.load_pins();
        match start {
            Some(path) => app.location = Location::Folder(normalize(&path.to_string_lossy())),
            None => app.location = Location::Computer,
        }
        app.show_location();
    }
    let _ = ui.as_weak().upgrade_in_event_loop(move |ui| {
        let nav_ui = ui.as_weak();
        platform::restore_frame_appearance(ui.window(), move |forward| {
            let nav_ui = nav_ui.clone();
            let _ = slint::invoke_from_event_loop(move || {
                if let Some(ui) = nav_ui.upgrade() {
                    if forward {
                        ui.invoke_go_forward();
                    } else {
                        ui.invoke_go_back();
                    }
                }
            });
        });
        if let Some(placement) = placement {
            platform::apply_window_placement(ui.window(), placement);
        }
    });
    slint::run_event_loop()?;
    ui.hide()
}
