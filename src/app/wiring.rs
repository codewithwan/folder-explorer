use super::*;

pub(crate) fn open_selection(shared: &Shared) {
    let (folder, files) = {
        let app = shared.borrow();
        let state = app.list.state();
        let folder = state.selected_items().find(|item| item.is_dir).map(Item::path);
        let files: Vec<PathBuf> = state.selected_items().filter(|item| !item.is_dir).map(Item::path).collect();
        (folder, files)
    };
    for file in &files {
        platform::open(file);
    }
    if let Some(path) = folder {
        shared.borrow_mut().go(Location::Folder(path));
    }
}

pub(crate) fn activate_row(shared: &Shared, row: usize) {
    let item = shared.borrow().list.state().item(row).map(|item| (item.is_dir, item.path()));
    match item {
        Some((true, path)) => shared.borrow_mut().go(Location::Folder(path)),
        Some((false, path)) => {
            platform::open(&path);
        }
        None => {}
    }
}

pub(crate) fn delete_selection(shared: &Shared, permanently: bool) {
    let owner = platform::active_window();
    let paths = shared.borrow().selected_paths();
    let target = match paths.as_slice() {
        [] => return,
        [path] => format!("\"{}\"", path.display()),
        many => plural(many.len() as u64, "item"),
    };
    let (title, question) = match permanently {
        true => ("Delete permanently", format!("Permanently delete {target}? This can't be undone.")),
        false => ("Move to Recycle Bin", format!("Move {target} to the Recycle Bin?")),
    };
    if platform::confirm(title, &question) {
        shared.borrow_mut().delete(paths, permanently, owner);
    }
}

pub(crate) fn show_context_menu(shared: &Shared) {
    let (ui, row) = {
        let app = shared.borrow();
        let row = app.list.state().focused_row();
        (app.ui(), row)
    };
    if !ui.get_showing_drives() {
        ui.invoke_show_list_menu(row.map_or(-1, |row| row as i32));
    }
}

pub(crate) fn describe_rename_error(error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::AlreadyExists => "Something with that name already exists here.".into(),
        io::ErrorKind::PermissionDenied => "It may be open in another program, or you don't have permission.".into(),
        _ => error.to_string(),
    }
}

pub(crate) fn commit_rename(shared: &Shared, text: &str) {
    let old = shared.borrow_mut().renaming.take();
    let ui = shared.borrow().ui();
    ui.invoke_focus_list();
    let Some(old) = old else { return };
    if old.file_name().is_some_and(|current| current.to_string_lossy() == text) {
        return;
    }
    if let Err(reason) = names::validate_name(text) {
        platform::alert("Can't rename", &reason);
        return;
    }
    let new = old.with_file_name(text);
    match platform::rename(&old, &new) {
        Ok(()) => shared.borrow_mut().on_renamed(&old, &new),
        Err(error) => platform::alert("Can't rename", &describe_rename_error(&error)),
    }
}

pub(crate) fn new_folder(shared: &Shared) {
    let parent = shared.borrow().editable_folder();
    let Some(parent) = parent else { return };
    match names::create_new_folder(&parent) {
        Ok(name) => shared.borrow_mut().on_created(&parent, name),
        Err(error) => platform::alert("Can't create a folder", &error.to_string()),
    }
}

pub(crate) fn paste(shared: &Shared) {
    let owner = platform::active_window();
    if let Some(files) = platform::clipboard_files() {
        shared.borrow_mut().paste(files, owner);
    }
}

pub(crate) fn export_list(shared: &Shared) {
    let filters: Vec<(&str, &str)> = export::DELIMITERS.iter().map(|(label, extension, _)| (*label, *extension)).collect();
    let Some((path, filter)) = platform::save_dialog("Export list", "folder-list.csv", &filters) else { return };
    let delimiter = export::DELIMITERS.get(filter).map_or(',', |(_, _, delimiter)| *delimiter);
    let result = {
        let app = shared.borrow();
        let state = app.list.state();
        export::export_file(&path, &state.columns, state.visible_items(), delimiter)
    };
    if let Err(error) = result {
        platform::alert("Export failed", &error.to_string());
    }
}

pub(crate) fn run_action(shared: &Shared, action: &str) {
    match action {
        "open" => open_selection(shared),
        "delete" => delete_selection(shared, false),
        "delete-permanently" => delete_selection(shared, true),
        "context-menu" => show_context_menu(shared),
        "export" => export_list(shared),
        "new-folder" => new_folder(shared),
        "paste" => paste(shared),
        "properties" => {
            let path = shared.borrow().first_selected();
            if let Some(path) = path {
                platform::show_properties(&path);
            }
        }
        "reveal" => {
            let path = shared.borrow().first_selected();
            if let Some(path) = path {
                platform::reveal_in_explorer(&path);
            }
        }
        "empty-bin" => {
            platform::empty_recycle_bin();
            shared.borrow_mut().load_drives();
        }
        other => shared.borrow_mut().list_action(other),
    }
}

pub(crate) fn wire(ui: &AppWindow, shared: &Shared) {
    macro_rules! on {
        ($setter:ident, |$app:ident $(, $argument:ident)*| $body:expr) => {{
            let shared = shared.clone();
            ui.$setter(move |$($argument),*| {
                let $app = &mut *shared.borrow_mut();
                $body;
            });
        }};
    }

    on!(on_go_back, |app| app.go_back());
    on!(on_go_forward, |app| app.go_forward());
    on!(on_go_up, |app| app.go_up());
    on!(on_open_location, |app, target| app.open_target(&target));
    on!(on_path_entered, |app, text| app.enter_typed_path(&text));
    on!(on_tree_opened, |app, row| app.tree_opened(row as usize));
    on!(on_tree_toggled, |app, row| app.tree_toggled(row as usize));
    on!(on_sort_requested, |app, index| app.sort_by(index as usize));
    on!(on_column_resized, |app, index, width| app.resize_column(index as usize, width));
    on!(on_row_pressed, |app, row, toggle, extend| app.press_row(row as usize, toggle, extend));
    on!(on_row_context, |app, row| app.context_row(row as usize));
    on!(on_background_pressed, |app| app.clear_selection());
    on!(on_cursor_moved, |app, delta, extend| app.move_cursor(delta, extend));
    on!(on_search_changed, |app| app.schedule_search());
    on!(on_summary_kind_chosen, |app, kind| app.choose_summary(kind as usize));
    on!(on_summary_viz_mode_chosen, |app, _mode| {
        let hovered = app.ui().get_summary_hovered_index();
        app.refresh_summary_wheel((hovered >= 0).then_some(hovered as usize));
    });
    on!(on_summary_wheel_hovered, |app, nx, ny| app.hover_summary_wheel(nx, ny));
    on!(on_summary_row_hovered, |app, index| app.hover_summary_row(index));
    on!(on_summary_activated, |app, row| app.activate_summary(row as usize));
    on!(on_summary_toggled, |app| app.refresh_summary());
    on!(on_unit_chosen, |app, index| app.choose_unit(index as usize));
    on!(on_column_toggled, |app, choice| app.toggle_column(choice as usize));
    on!(on_hidden_toggled, |app| app.toggle_hidden());
    on!(on_view_chosen, |app, index| app.choose_view(index as usize));
    on!(on_sort_chosen, |app, index| app.sort_column(Column::DEFAULT[index as usize]));
    on!(on_menu_opening, |app| app.refresh_can_paste());
    on!(on_rename_cancelled, |app| app.cancel_rename());
    on!(on_text_typed, |app, text| app.type_ahead(&text));

    {
        let shared = shared.clone();
        ui.on_refresh_or_stop(move || {
            let mut app = shared.borrow_mut();
            if app.scanning || app.hashes_left > 0 {
                app.cancel_scan();
            } else {
                app.refresh();
            }
        });
    }
    {
        let shared = shared.clone();
        ui.on_cancel_pressed(move || {
            let mut app = shared.borrow_mut();
            if app.scanning || app.hashes_left > 0 {
                app.cancel_scan();
            } else if app.showing_results {
                app.show_location();
            }
        });
    }
    {
        let shared = shared.clone();
        ui.on_row_activated(move |row| activate_row(&shared, row as usize));
    }
    {
        let shared = shared.clone();
        ui.on_rename_committed(move |text| commit_rename(&shared, &text));
    }
    {
        let shared = shared.clone();
        ui.on_action(move |action| run_action(&shared, &action));
    }
    {
        let shared = shared.clone();
        ui.window().on_close_requested(move || {
            shared.borrow_mut().save_settings();
            CloseRequestResponse::HideWindow
        });
    }
}
