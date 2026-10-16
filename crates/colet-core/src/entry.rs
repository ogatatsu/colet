//! An item as a column lists it, and the orders they come in.
use std::{cmp::Reverse, os::windows::fs::MetadataExt, path::Path, time::UNIX_EPOCH};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub directory: bool,
    pub size: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<u64>,
    /// Hidden or system, as Explorer leaves out by default. A search result never says: Everything has
    /// no attributes for it to give cheaply.
    pub hidden: bool,
}

const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;

/// A drive root reports itself hidden and system, yet hides from no one.
pub fn is_hidden(path: &Path, attributes: u32) -> bool {
    attributes & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM) != 0 && path.file_name().is_some()
}

impl Entry {
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        Ok(Self::from_metadata(path, &path.metadata()?))
    }

    pub fn from_metadata(path: &Path, metadata: &std::fs::Metadata) -> Self {
        Self {
            path: path.to_string_lossy().into_owned(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.display().to_string()),
            parent: path.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(),
            directory: metadata.is_dir(),
            size: (!metadata.is_dir()).then_some(metadata.len()),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64),
            hidden: is_hidden(path, metadata.file_attributes()),
        }
    }

    /// A folder known by its path alone, such as a drive.
    pub fn folder(path: &str, name: &str) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            parent: String::new(),
            directory: true,
            size: None,
            modified: None,
            hidden: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    #[default]
    Name,
    /// Largest first.
    Size,
    /// Newest first.
    Modified,
}

impl Sort {
    pub const ALL: [Sort; 3] = [Sort::Name, Sort::Size, Sort::Modified];

    /// Named for their direction, which is fixed per key.
    pub fn label(self) -> &'static str {
        match self {
            Sort::Name => "名前順",
            Sort::Size => "大きい順",
            Sort::Modified => "新しい順",
        }
    }
}

/// Folders first, then by the sort, ties broken by path.
pub fn sort_entries(entries: &mut [Entry], sort: Sort) {
    // Keyed once per entry: lowering names inside the comparison would allocate on every one of its
    // n log n calls.
    entries.sort_by_cached_key(|e| {
        let (rank, name) = match sort {
            Sort::Name => (None, e.name.to_lowercase()),
            Sort::Size => (e.size, String::new()),
            Sort::Modified => (e.modified, String::new()),
        };
        (!e.directory, Reverse(rank), name, e.path.to_lowercase())
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn directories_precede_files_for_all_sorts() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "data").unwrap();
        std::fs::create_dir(dir.path().join("z")).unwrap();
        let mut entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| Entry::from_path(&e.unwrap().path()).unwrap())
            .collect();
        for sort in Sort::ALL {
            sort_entries(&mut entries, sort);
            assert!(entries[0].directory);
        }
    }

    #[test]
    fn sorts_ignore_case_put_the_largest_and_newest_first_and_tie_by_path() {
        let entry = |path: &str, size: Option<u64>, modified: Option<u64>| Entry {
            path: path.into(),
            name: path.rsplit('\\').next().unwrap().into(),
            parent: String::new(),
            directory: false,
            size,
            modified,
            hidden: false,
        };
        let mut entries = vec![
            entry(r"C:\b", Some(1), None),
            entry(r"C:\A", Some(2), Some(5)),
            entry(r"C:\c", None, Some(9)),
            entry(r"C:\a2", Some(2), Some(5)),
        ];
        let order = |entries: &[Entry]| entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        sort_entries(&mut entries, Sort::Name);
        assert_eq!(order(&entries), ["A", "a2", "b", "c"]);
        sort_entries(&mut entries, Sort::Size);
        assert_eq!(order(&entries), ["A", "a2", "b", "c"]);
        sort_entries(&mut entries, Sort::Modified);
        assert_eq!(order(&entries), ["c", "A", "a2", "b"]);
    }

    #[test]
    fn hidden_and_system_items_are_hidden_but_drive_roots_are_not() {
        assert!(is_hidden(Path::new(r"C:\pagefile.sys"), 0x2 | 0x4 | 0x20));
        assert!(is_hidden(Path::new(r"C:\work\.git"), 0x2 | 0x10));
        assert!(!is_hidden(Path::new(r"C:\work\.gitignore"), 0x20));
        assert!(!is_hidden(Path::new(r"C:\"), 0x2 | 0x4 | 0x10));
    }
}
