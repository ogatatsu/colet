//! An item as a column lists it, and the orders they come in.
use std::{
    cmp::Ordering,
    ffi::OsString,
    ops::Range,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

use windows::Win32::UI::Shell::StrCmpLogicalW;

use crate::paths::{path_key, shown};
use crate::wide::Wide;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The item as the OS names it: what it is found and acted on by.
    pub path: PathBuf,
    /// What is shown of its name, and searched and sorted by.
    pub name: String,
    /// What is shown of the folder it is in.
    pub parent: String,
    pub directory: bool,
    pub size: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<u64>,
    /// Hidden or system, as Explorer leaves out by default. A search result never says: Everything has
    /// no attributes for it to give cheaply.
    pub hidden: bool,
    /// Where a search matched it; empty outside the results.
    pub marks: Marks,
}

/// The byte ranges of `name` and of `parent` that a search matched.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    pub name: Vec<Range<usize>>,
    pub parent: Vec<Range<usize>>,
}

/// The ranges of `shown` that Everything's `highlighted` text stars: `*` turns the mark on and off,
/// and `**` stands for a `*`. None, when the text it marks is not where `shown` begins, as they would
/// fall on the wrong letters.
pub fn marked(highlighted: &str, shown: &str) -> Vec<Range<usize>> {
    let (mut plain, mut ranges, mut start) = (String::new(), Vec::new(), None);
    let mut chars = highlighted.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '*' {
            plain.push(c);
        } else if chars.next_if_eq(&'*').is_some() {
            plain.push('*');
        } else if let Some(from) = start.take() {
            if from < plain.len() {
                ranges.push(from..plain.len());
            }
        } else {
            start = Some(plain.len());
        }
    }
    if let Some(from) = start.filter(|from| *from < plain.len()) {
        ranges.push(from..plain.len());
    }
    if shown.starts_with(&plain) { ranges } else { Vec::new() }
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
            path: path.to_owned(),
            name: path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| shown(path)),
            parent: path.parent().map(shown).unwrap_or_default(),
            directory: metadata.is_dir(),
            size: (!metadata.is_dir()).then_some(metadata.len()),
            modified: metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64),
            hidden: is_hidden(path, metadata.file_attributes()),
            marks: Marks::default(),
        }
    }

    /// A folder known by its path alone, such as a drive.
    pub fn folder(path: impl Into<PathBuf>, name: &str) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            parent: String::new(),
            directory: true,
            size: None,
            modified: None,
            hidden: false,
            marks: Marks::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Sort {
    #[default]
    Name,
    Size,
    Modified,
}

impl Sort {
    pub const ALL: [Sort; 3] = [Sort::Name, Sort::Size, Sort::Modified];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Name => "名前",
            Sort::Size => "サイズ",
            Sort::Modified => "更新日時",
        }
    }
}

/// A sort and the way it runs. Descending turns the whole listing round, as Explorer does, so folders
/// come last; ties still go by name from A.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Order {
    pub by: Sort,
    pub descending: bool,
}

impl Order {
    pub fn label(self) -> String {
        format!("{}・{}", self.by.label(), direction_label(self.descending))
    }
}

pub fn direction_label(descending: bool) -> &'static str {
    if descending { "降順" } else { "昇順" }
}

/// A name ordered as Explorer orders it: without case, and numbers by their value.
struct ExplorerName(Wide);

impl Ord for ExplorerName {
    fn cmp(&self, other: &Self) -> Ordering {
        unsafe { StrCmpLogicalW(self.0.get(), other.0.get()) }.cmp(&0)
    }
}

impl PartialOrd for ExplorerName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ExplorerName {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for ExplorerName {}

struct Key {
    order: Order,
    file: bool,
    rank: Option<u64>,
    name: ExplorerName,
    path: OsString,
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut by = self.rank.cmp(&other.rank);
        if self.order.by == Sort::Name {
            by = by.then_with(|| self.name.cmp(&other.name));
        }
        if self.order.descending {
            by = by.reverse();
        }
        self.file
            .cmp(&other.file)
            .then(by)
            .then_with(|| self.name.cmp(&other.name))
            .then_with(|| self.path.cmp(&other.path))
    }
}

impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Key {}

/// Folders first either way, unlike Explorer's descending; within each, by the sort, turned round when
/// descending; ties broken by name as Explorer orders it and then by path.
pub fn sort_entries(entries: &mut [Entry], order: Order) {
    // Keyed once per entry: converting names inside the comparison would allocate on every one of its
    // n log n calls.
    entries.sort_by_cached_key(|e| Key {
        order,
        file: !e.directory,
        rank: match order.by {
            Sort::Name => None,
            Sort::Size => e.size,
            Sort::Modified => e.modified,
        },
        name: ExplorerName(Wide::new(&e.name)),
        path: path_key(&e.path),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_what_everything_stars_counting_a_double_star_as_one_and_bytes_not_chars() {
        assert_eq!(marked("*rep*ort.txt", "report.txt"), [0..3]);
        assert_eq!(marked("a**b*c*", "a*bc"), [3..4]);
        assert_eq!(marked("日本*語*", "日本語"), [6..9]);
        // A mark left open runs to the end; an empty one marks nothing.
        assert_eq!(marked("ab*c", "abc"), [2..3]);
        assert_eq!(marked("a**", "a*"), Vec::<Range<usize>>::new());
        assert_eq!(marked("**", "*"), Vec::<Range<usize>>::new());
        // Everything gives a drive's path without the separator the column shows.
        assert_eq!(marked("*C:*", r"C:\"), [0..2]);
        assert!(marked("*other*", "report").is_empty());
    }

    #[test]
    fn directories_precede_files_either_way() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), "data").unwrap();
        std::fs::create_dir(dir.path().join("z")).unwrap();
        let mut entries: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| Entry::from_path(&e.unwrap().path()).unwrap())
            .collect();
        for by in Sort::ALL {
            sort_entries(&mut entries, Order { by, descending: false });
            assert!(entries[0].directory);
            sort_entries(&mut entries, Order { by, descending: true });
            assert!(entries[0].directory);
        }
    }

    #[test]
    fn sorts_ignore_case_run_either_way_and_tie_by_name_from_a() {
        let entry = |path: &str, size: Option<u64>, modified: Option<u64>| Entry {
            path: path.into(),
            name: path.rsplit('\\').next().unwrap().into(),
            parent: String::new(),
            directory: false,
            size,
            modified,
            hidden: false,
            marks: Marks::default(),
        };
        let mut entries = vec![
            entry(r"C:\b", Some(1), None),
            entry(r"C:\A", Some(2), Some(5)),
            entry(r"C:\c", None, Some(9)),
            entry(r"C:\a2", Some(2), Some(5)),
        ];
        let order = |entries: &[Entry]| entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        let sort = |entries: &mut Vec<Entry>, by, descending| {
            sort_entries(entries, Order { by, descending });
            order(entries)
        };
        assert_eq!(sort(&mut entries, Sort::Name, false), ["A", "a2", "b", "c"]);
        assert_eq!(sort(&mut entries, Sort::Name, true), ["c", "b", "a2", "A"]);
        assert_eq!(sort(&mut entries, Sort::Size, false), ["c", "b", "A", "a2"]);
        assert_eq!(sort(&mut entries, Sort::Size, true), ["A", "a2", "b", "c"]);
        assert_eq!(sort(&mut entries, Sort::Modified, false), ["b", "A", "a2", "c"]);
        assert_eq!(sort(&mut entries, Sort::Modified, true), ["c", "A", "a2", "b"]);
    }

    #[test]
    fn names_sort_as_explorer_orders_them_numbers_by_value_and_widths_and_kana_alike() {
        let entry = |name: &str| Entry {
            path: format!(r"C:\{name}").into(),
            name: name.into(),
            parent: String::new(),
            directory: false,
            size: Some(1),
            modified: None,
            hidden: false,
            marks: Marks::default(),
        };
        let names = ["file10.txt", "File2.txt", "file1.txt", "あ10", "あ9", "ア1", "a01", "a1", "b", "ｆｉｌｅ３.txt"];
        let mut entries: Vec<_> = names.iter().map(|n| entry(n)).collect();
        let order = |entries: &[Entry]| entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        let expected = ["a01", "a1", "b", "file1.txt", "File2.txt", "ｆｉｌｅ３.txt", "file10.txt", "ア1", "あ9", "あ10"];
        sort_entries(&mut entries, Order::default());
        assert_eq!(order(&entries), expected);
        // The sizes all tie, so they fall back to the same order either way.
        for descending in [false, true] {
            entries.reverse();
            sort_entries(&mut entries, Order { by: Sort::Size, descending });
            assert_eq!(order(&entries), expected);
        }
    }

    #[test]
    fn hidden_and_system_items_are_hidden_but_drive_roots_are_not() {
        assert!(is_hidden(Path::new(r"C:\pagefile.sys"), 0x2 | 0x4 | 0x20));
        assert!(is_hidden(Path::new(r"C:\work\.git"), 0x2 | 0x10));
        assert!(!is_hidden(Path::new(r"C:\work\.gitignore"), 0x20));
        assert!(!is_hidden(Path::new(r"C:\"), 0x2 | 0x4 | 0x10));
    }
}
