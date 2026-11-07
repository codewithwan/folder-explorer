//! Native message boxes and the save-file dialog, via `rfd`.

use std::path::PathBuf;

pub fn confirm(title: &str, message: &str) -> bool {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Warning)
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::YesNo)
        .show()
        == rfd::MessageDialogResult::Yes
}

pub fn alert(title: &str, message: &str) {
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title(title)
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

pub fn save_dialog(title: &str, file_name: &str, filters: &[(&str, &str)]) -> Option<(PathBuf, usize)> {
    let mut dialog = rfd::FileDialog::new().set_title(title).set_file_name(file_name);
    for (label, extension) in filters {
        dialog = dialog.add_filter(*label, &[*extension]);
    }
    let path = dialog.save_file()?;
    let chosen = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase());
    let index = chosen
        .as_deref()
        .and_then(|ext| filters.iter().position(|(_, candidate)| candidate.eq_ignore_ascii_case(ext)))
        .unwrap_or(0);
    Some((path, index))
}
