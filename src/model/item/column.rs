use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Column {
    Name,
    Size,
    Files,
    Folders,
    Modified,
    Created,
    Accessed,
    Type,
    Attributes,
    Status,
    Location,
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

impl Column {
    pub const ALL: [Column; 15] = [
        Column::Name,
        Column::Size,
        Column::Files,
        Column::Folders,
        Column::Modified,
        Column::Created,
        Column::Accessed,
        Column::Type,
        Column::Attributes,
        Column::Status,
        Column::Location,
        Column::Md5,
        Column::Sha1,
        Column::Sha256,
        Column::Sha512,
    ];

    pub const DEFAULT: [Column; 6] =
        [Column::Name, Column::Size, Column::Files, Column::Folders, Column::Modified, Column::Type];

    pub fn title(self) -> &'static str {
        match self {
            Column::Name => "Name",
            Column::Size => "Size",
            Column::Files => "Files",
            Column::Folders => "Folders",
            Column::Modified => "Date modified",
            Column::Created => "Date created",
            Column::Accessed => "Date accessed",
            Column::Type => "Type",
            Column::Attributes => "Attributes",
            Column::Status => "Status",
            Column::Location => "Location",
            Column::Md5 => HASH_NAMES[0],
            Column::Sha1 => HASH_NAMES[1],
            Column::Sha256 => HASH_NAMES[2],
            Column::Sha512 => HASH_NAMES[3],
        }
    }

    pub fn default_width(self) -> f32 {
        match self {
            Column::Name => 320.0,
            Column::Size => 120.0,
            Column::Files | Column::Folders => 80.0,
            Column::Modified | Column::Created | Column::Accessed => 140.0,
            Column::Type => 110.0,
            Column::Attributes | Column::Status => 80.0,
            Column::Location => 320.0,
            Column::Md5 => 250.0,
            Column::Sha1 => 300.0,
            Column::Sha256 => 450.0,
            Column::Sha512 => 800.0,
        }
    }

    pub fn right_aligned(self) -> bool {
        matches!(self, Column::Size | Column::Files | Column::Folders)
    }

    pub fn is_fixed(self) -> bool {
        matches!(self, Column::Name | Column::Size)
    }

    pub fn hash_slot(self) -> Option<usize> {
        match self {
            Column::Md5 => Some(0),
            Column::Sha1 => Some(1),
            Column::Sha256 => Some(2),
            Column::Sha512 => Some(3),
            _ => None,
        }
    }

    pub fn starts_descending(self) -> bool {
        matches!(
            self,
            Column::Size | Column::Files | Column::Folders | Column::Modified | Column::Created | Column::Accessed
        )
    }

    pub fn cell(self, item: &Item, unit: Unit) -> String {
        match self {
            Column::Name => item.name.to_string(),
            Column::Size => item.size_label(unit),
            Column::Files => item.count_label(item.file_count),
            Column::Folders => item.count_label(item.dir_count),
            Column::Modified => platform::format_time(item.times.modified),
            Column::Created => platform::format_time(item.times.created),
            Column::Accessed => platform::format_time(item.times.accessed),
            Column::Type => item.type_label(),
            Column::Attributes => item.attributes_label(),
            Column::Status => item.status_label().into(),
            Column::Location => item.parent.to_string_lossy().into_owned(),
            Column::Md5 | Column::Sha1 | Column::Sha256 | Column::Sha512 => {
                item.hash_label(self.hash_slot().unwrap_or(0))
            }
        }
    }

    pub fn export_cell(self, item: &Item) -> String {
        match self {
            Column::Size => item.size.to_string(),
            Column::Files if item.is_dir => item.file_count.to_string(),
            Column::Folders if item.is_dir => item.dir_count.to_string(),
            _ => self.cell(item, Unit::Auto),
        }
    }

    pub fn export_title(self) -> &'static str {
        match self {
            Column::Size => "Size (bytes)",
            _ => self.title(),
        }
    }

    pub fn compare(self, a: &Item, b: &Item) -> Ordering {
        match self {
            Column::Name => compare_text(&a.name, &b.name),
            Column::Size => a.size.cmp(&b.size),
            Column::Files => a.file_count.cmp(&b.file_count),
            Column::Folders => a.dir_count.cmp(&b.dir_count),
            Column::Modified => a.times.modified.cmp(&b.times.modified),
            Column::Created => a.times.created.cmp(&b.times.created),
            Column::Accessed => a.times.accessed.cmp(&b.times.accessed),
            Column::Type => compare_text(a.extension(), b.extension()),
            Column::Attributes => a.attrs.cmp(&b.attrs),
            Column::Status => a.status_label().cmp(b.status_label()),
            Column::Location => compare_text(&a.parent.to_string_lossy(), &b.parent.to_string_lossy()),
            Column::Md5 | Column::Sha1 | Column::Sha256 | Column::Sha512 => {
                let slot = self.hash_slot().unwrap_or(0);
                a.hash_label(slot).cmp(&b.hash_label(slot))
            }
        }
    }

    pub fn groups_folders_first(self) -> bool {
        matches!(self, Column::Name | Column::Type)
    }
}
