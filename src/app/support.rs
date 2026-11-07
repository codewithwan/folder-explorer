use super::*;

pub(crate) fn scan_folder(
    scan: u64,
    generation: u64,
    path: PathBuf,
    known: HashMap<PathBuf, Arc<DirNode>>,
    cancel: Arc<AtomicBool>,
    sender: Sender<Message>,
) {
    let started = Instant::now();
    let Level { mut node, subdirs } = match SubDir::at(&path).and_then(|root| read_level(&root)) {
        Ok(level) => level,
        Err(error) => {
            let _ = sender.send(Message::Unreadable { scan, reason: describe_error(&path, &error) });
            return;
        }
    };

    let parent: Arc<Path> = path.as_path().into();
    let lives: Arc<Vec<Live>> = Arc::new(subdirs.iter().map(|_| Live::default()).collect());
    let mut items: Vec<Item> = subdirs.iter().map(|dir| Item::pending_folder(dir, &parent)).collect();
    items.extend(node.files().map(|(name, entry)| Item::file(name, entry, &parent)));
    let _ = sender.send(Message::Listed { scan, items, lives: lives.clone() });

    let root = root_of(&path);
    let title = root.to_string_lossy().trim_end_matches('\\').to_string();
    let _ = sender.send(Message::Disk { scan, title, usage: platform::disk_usage(&root) });

    let from_cache = subdirs.iter().all(|dir| known.contains_key(&dir.path));
    let context = ScanContext { cancel, known };
    let children = subdirs
        .par_iter()
        .enumerate()
        .map(|(index, dir)| {
            let child = scan_tree(dir, &context, &lives[index]);
            let _ = sender.send(Message::Sized { scan, index, node: child.clone() });
            child
        })
        .collect();
    node.absorb(children);
    let mut errors = Vec::new();
    node.collect_errors(&path, &mut errors);
    let seconds = started.elapsed().as_secs_f64();
    let _ = sender.send(Message::Finished { scan, generation, path, node: Arc::new(node), errors, seconds, from_cache });
}

pub(crate) fn is_name_text(text: &str) -> bool {
    let special_key = |character: char| character.is_control() || ('\u{f700}'..='\u{f7ff}').contains(&character);
    !text.is_empty() && !text.chars().any(special_key)
}

pub(crate) fn describe_error(path: &Path, error: &io::Error) -> String {
    match error.kind() {
        io::ErrorKind::NotFound => format!("{} does not exist.", path.display()),
        io::ErrorKind::PermissionDenied => format!("You don't have permission to open {}.", path.display()),
        _ => format!("Can't open {}: {error}", path.display()),
    }
}

pub(crate) fn root_of(path: &Path) -> PathBuf {
    path.ancestors().last().unwrap_or(path).to_path_buf()
}

pub(crate) fn normalize(text: &str) -> PathBuf {
    let trimmed = text.trim().trim_matches('"');
    let rooted = if trimmed.len() == 2 && trimmed.ends_with(':') { format!("{trimmed}\\") } else { trimmed.to_string() };
    let absolute = std::path::absolute(&rooted).unwrap_or_else(|_| PathBuf::from(rooted));
    absolute.components().collect()
}

pub(crate) fn resolve_target(location: &Location, text: &str) -> PathBuf {
    let exact = match location {
        Location::Folder(path) => path.ancestors().find(|ancestor| ancestor.to_string_lossy() == text),
        Location::Computer => None,
    };
    exact.map_or_else(|| normalize(text), Path::to_path_buf)
}

pub(crate) fn size_range(text: &str, larger: bool) -> Option<Range<u64>> {
    let megabytes = text.trim().parse::<f64>().ok().filter(|megabytes| megabytes.is_finite() && *megabytes >= 0.0)?;
    let bytes = (megabytes * BYTES_PER_MEGABYTE) as u64;
    Some(if larger { bytes.saturating_add(1)..u64::MAX } else { 0..bytes })
}

pub(crate) fn crumbs_for(location: &Location) -> Vec<Crumb> {
    let mut crumbs = vec![Crumb { name: "This PC".into(), target: SharedString::new() }];
    if let Location::Folder(path) = location {
        let mut ancestors: Vec<&Path> = path.ancestors().collect();
        ancestors.reverse();
        crumbs.extend(ancestors.iter().map(|ancestor| {
            let name = match ancestor.components().next_back() {
                Some(Component::Normal(name)) => name.to_string_lossy().to_string(),
                _ => ancestor.to_string_lossy().trim_end_matches('\\').to_string(),
            };
            Crumb { name: name.into(), target: ancestor.to_string_lossy().as_ref().into() }
        }));
    }
    crumbs
}

pub(crate) fn list_subfolders(parent: &Path, show_hidden: bool) -> Vec<(OsString, u32)> {
    let Ok(entries) = fs::read_dir(parent) else { return Vec::new() };
    entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name();
            let attrs = crate::util::fsmeta::attrs(&entry.metadata().ok()?, &name);
            Some((name, attrs))
        })
        .filter(|(_, attrs)| show_hidden || attrs & ATTR_HIDDEN == 0)
        .collect()
}

pub(crate) fn disk_label(title: &str, usage: DiskUsage, unit: Unit) -> String {
    let percent = (usage.used_ratio() * 100.0).round();
    format!("{title} {percent}% used · {} free", format_size(usage.free, unit))
}

pub(crate) fn drive_card(drive: &Drive, icon: Image, unit: Unit) -> DriveCard {
    let usage = drive.usage;
    let ratio = usage.map_or(0.0, DiskUsage::used_ratio);
    let detail = match usage {
        Some(usage) => format!("{} free of {}", format_size(usage.free, unit), format_size(usage.total, unit)),
        None => "Not available".into(),
    };
    DriveCard {
        title: drive.title.as_str().into(),
        detail: detail.into(),
        path: drive.root.to_string_lossy().as_ref().into(),
        icon,
        ratio,
        nearly_full: ratio > NEARLY_FULL,
    }
}
