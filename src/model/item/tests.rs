use super::*;

fn file(name: &str) -> Item {
    Item {
        name: name.into(),
        native_name: None,
        parent: Arc::from(Path::new(r"C:\data")),
        is_dir: false,
        attrs: 0,
        times: Times::default(),
        size: 2048,
        file_count: 0,
        dir_count: 0,
        state: ItemState::Ready,
        hashes: None,
    }
}

#[test]
fn text_compares_ignoring_case() {
    assert_eq!(compare_text("apple", "Banana"), Ordering::Less);
    assert_eq!(compare_text("ÉCOLE", "école"), Ordering::Equal);
}

#[test]
fn type_comes_from_the_extension() {
    assert_eq!(file("report.pdf").type_label(), "PDF File");
    assert_eq!(file("README").type_label(), "File");
    assert_eq!(file(".gitignore").type_label(), "File");
}

#[test]
fn unfinished_sizes_read_as_lower_bounds() {
    let mut item = file("folder");
    item.is_dir = true;
    item.state = ItemState::Pending;
    item.size = 0;
    assert_eq!(Column::Size.cell(&item, Unit::Auto), "…");
    item.size = 2048;
    assert_eq!(Column::Size.cell(&item, Unit::Auto), "≥ 2.0 KB");
    item.state = ItemState::Partial;
    assert_eq!(Column::Size.cell(&item, Unit::Auto), "≥ 2.0 KB");
    item.state = ItemState::Ready;
    assert_eq!(Column::Size.cell(&item, Unit::Auto), "2.0 KB");
}

#[test]
fn exports_use_raw_numbers() {
    assert_eq!(Column::Size.export_cell(&file("a")), "2048");
    assert_eq!(Column::Size.export_title(), "Size (bytes)");
}

#[test]
fn attributes_read_as_letters() {
    let mut item = file("a");
    item.attrs = ATTR_READONLY | ATTR_HIDDEN | ATTR_ARCHIVE;
    assert_eq!(item.attributes_label(), "RHA");
}
