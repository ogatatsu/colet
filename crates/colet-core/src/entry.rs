//! An item as a column lists it, and the orders they come in.
use std::{
    cmp::Ordering,
    ffi::OsString,
    ops::Range,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::UNIX_EPOCH,
};

use serde::{Deserialize, Serialize};
use windows::Win32::UI::Shell::StrCmpLogicalW;

use crate::paths::{path_key, shown};
use crate::wide::Wide;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The item as the OS names it: what it is found and acted on by.
    pub path: PathBuf,
    /// What is shown of its name, and searched and sorted by.
    pub name: String,
    /// What is shown of the folder it is in: shared by the items of one folder.
    pub parent: Arc<str>,
    pub directory: bool,
    pub size: Option<u64>,
    /// Milliseconds since the Unix epoch.
    pub modified: Option<u64>,
    /// Whether Explorer leaves it out by default, and under which of its settings. A search result never
    /// says: Everything has no attributes for it to give cheaply.
    pub hidden: Hiding,
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

/// Which of Explorer's settings shows an item.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hiding {
    /// Shown whatever they say, a system item that is not hidden as well among them.
    #[default]
    Shown,
    /// Shown along with the hidden items.
    Hidden,
    /// Hidden and system: a protected operating system file, shown only along with those too.
    Protected,
}

impl Hiding {
    /// Left out unless the hidden items show: faded when they do.
    pub fn is_hidden(self) -> bool {
        self != Hiding::Shown
    }
}

/// How an item with `attributes` hides. A drive root reports itself hidden and system, yet hides from no
/// one.
pub fn hiding(path: &Path, attributes: u32) -> Hiding {
    if attributes & FILE_ATTRIBUTE_HIDDEN == 0 || path.file_name().is_none() {
        Hiding::Shown
    } else if attributes & FILE_ATTRIBUTE_SYSTEM != 0 {
        Hiding::Protected
    } else {
        Hiding::Hidden
    }
}

/// What is shown of the folder `path` is in; empty for a drive.
pub fn parent_shown(path: &Path) -> Arc<str> {
    path.parent().map(|p| shown(p).into()).unwrap_or_default()
}

impl Entry {
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        Ok(Self::from_metadata(path, &path.metadata()?))
    }

    pub fn from_metadata(path: &Path, metadata: &std::fs::Metadata) -> Self {
        Self::from_metadata_in(path, metadata, parent_shown(path))
    }

    /// [`Entry::from_metadata`] for an item of the folder shown as `parent`, which a folder's items
    /// share rather than each holding its own.
    pub fn from_metadata_in(path: &Path, metadata: &std::fs::Metadata, parent: Arc<str>) -> Self {
        Self {
            path: path.to_owned(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| shown(path)),
            parent,
            directory: metadata.is_dir(),
            size: (!metadata.is_dir()).then_some(metadata.len()),
            modified: metadata.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64),
            hidden: hiding(path, metadata.file_attributes()),
            marks: Marks::default(),
        }
    }

    /// A folder known by its path alone, such as a drive.
    pub fn folder(path: impl Into<PathBuf>, name: &str) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            parent: Arc::default(),
            directory: true,
            size: None,
            modified: None,
            hidden: Hiding::Shown,
            marks: Marks::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    #[default]
    Name,
    /// By the folder an item is in, a level at a time, so a folder's subfolders follow it. Offered only
    /// for the search results: a folder's items are all in the one folder.
    Location,
    Size,
    Modified,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Name, Sort::Location, Sort::Size, Sort::Modified];

    pub fn label(self) -> &'static str {
        match self {
            Sort::Name => "名前",
            Sort::Location => "場所",
            Sort::Size => "サイズ",
            Sort::Modified => "更新日時",
        }
    }

    /// Whether a column of a folder's items, or the default, can be sorted by it.
    pub fn for_folders(self) -> bool {
        self != Sort::Location
    }
}

/// A sort, the way it runs, and whether folders come before files or mix in with them; ties still go by
/// name from A. Kept as `{"by": "size", "descending": true}`, with `"folders_first": false` only when they
/// mix, so a file written before they could reads as folders first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Order {
    pub by: Sort,
    pub descending: bool,
    #[serde(default = "folders_first", skip_serializing_if = "is_folders_first")]
    pub folders_first: bool,
}

fn folders_first() -> bool {
    true
}

fn is_folders_first(folders_first: &bool) -> bool {
    *folders_first
}

impl Default for Order {
    fn default() -> Self {
        Self { by: Sort::default(), descending: false, folders_first: true }
    }
}

impl Order {
    /// `名前・昇順`, with `・混在` when folders mix in with files.
    pub fn label(self) -> String {
        let mixed = if self.folders_first { "" } else { "・混在" };
        format!("{}・{}{mixed}", self.by.label(), direction_label(self.descending))
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
    /// The folders the item is in, from the drive down; only when sorting by them.
    location: Vec<ExplorerName>,
    name: ExplorerName,
    path: OsString,
}

impl Ord for Key {
    fn cmp(&self, other: &Self) -> Ordering {
        let mut by = self.rank.cmp(&other.rank).then_with(|| self.location.cmp(&other.location));
        if self.order.by == Sort::Name {
            by = by.then_with(|| self.name.cmp(&other.name));
        }
        if self.order.descending {
            by = by.reverse();
        }
        let file = if self.order.folders_first { self.file.cmp(&other.file) } else { Ordering::Equal };
        file.then(by).then_with(|| self.name.cmp(&other.name)).then_with(|| self.path.cmp(&other.path))
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

/// Folders first either way, unlike Explorer's descending, unless the order mixes them in with the files;
/// then by the sort, turned round when descending, a folder having no size and so coming below any file
/// by size, and a location compared a folder at a time as Explorer orders names; ties broken by name as
/// Explorer orders it, from A even descending, and then by path.
pub fn sort_entries(entries: &mut [Entry], order: Order) {
    // Keyed once per entry: converting names inside the comparison would allocate on every one of its
    // n log n calls.
    entries.sort_by_cached_key(|e| Key {
        order,
        file: !e.directory,
        rank: match order.by {
            Sort::Name | Sort::Location => None,
            Sort::Size => e.size,
            Sort::Modified => e.modified,
        },
        location: match (order.by, e.path.parent()) {
            (Sort::Location, Some(parent)) => parent.components().map(|c| ExplorerName(Wide::new(c.as_os_str()))).collect(),
            _ => Vec::new(),
        },
        name: ExplorerName(Wide::new(&e.name)),
        path: path_key(&e.path),
    });
}

#[cfg(test)]
// A mark is a list of ranges, most often holding only one.
#[allow(clippy::single_range_in_vec_init)]
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
        let mut entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| Entry::from_path(&e.unwrap().path()).unwrap()).collect();
        for by in Sort::ALL {
            sort_entries(&mut entries, Order { by, descending: false, ..Order::default() });
            assert!(entries[0].directory);
            sort_entries(&mut entries, Order { by, descending: true, ..Order::default() });
            assert!(entries[0].directory);
        }
    }

    #[test]
    fn mixes_folders_in_with_files_when_the_order_says_so_counting_a_folder_below_any_file_by_size() {
        let entry = |name: &str, directory: bool, size: Option<u64>| Entry {
            path: format!(r"C:\{name}").into(),
            name: name.into(),
            parent: Arc::default(),
            directory,
            size,
            modified: None,
            hidden: Hiding::Shown,
            marks: Marks::default(),
        };
        let mut entries =
            vec![entry("b", true, None), entry("a.txt", false, Some(0)), entry("d", true, None), entry("c.txt", false, Some(5))];
        let sort = |entries: &mut Vec<Entry>, by, descending| {
            sort_entries(entries, Order { by, descending, folders_first: false });
            entries.iter().map(|e| e.name.clone()).collect::<Vec<_>>()
        };
        assert_eq!(sort(&mut entries, Sort::Name, false), ["a.txt", "b", "c.txt", "d"]);
        assert_eq!(sort(&mut entries, Sort::Name, true), ["d", "c.txt", "b", "a.txt"]);
        assert_eq!(sort(&mut entries, Sort::Size, false), ["b", "d", "a.txt", "c.txt"]);
        assert_eq!(sort(&mut entries, Sort::Size, true), ["c.txt", "a.txt", "b", "d"]);
    }

    #[test]
    fn sorts_by_location_a_folder_at_a_time_turning_only_the_folders_round() {
        let entry = |path: &str| Entry {
            path: path.into(),
            name: path.rsplit('\\').next().unwrap().into(),
            parent: Arc::default(),
            directory: false,
            size: None,
            modified: None,
            hidden: Hiding::Shown,
            marks: Marks::default(),
        };
        let paths = [r"D:\a b\x", r"C:\z\y", r"D:\a\sub\x", r"D:\A\y", r"D:\a10\x", r"D:\a\x", r"D:\a2\x", r"D:\A\Sub\a"];
        let mut entries: Vec<_> = paths.iter().map(|p| entry(p)).collect();
        let sort = |entries: &mut Vec<Entry>, descending| {
            sort_entries(entries, Order { by: Sort::Location, descending, ..Order::default() });
            entries.iter().map(|e| e.path.display().to_string()).collect::<Vec<_>>()
        };
        assert_eq!(
            sort(&mut entries, false),
            [r"C:\z\y", r"D:\a\x", r"D:\A\y", r"D:\A\Sub\a", r"D:\a\sub\x", r"D:\a b\x", r"D:\a2\x", r"D:\a10\x"]
        );
        assert_eq!(
            sort(&mut entries, true),
            [r"D:\a10\x", r"D:\a2\x", r"D:\a b\x", r"D:\A\Sub\a", r"D:\a\sub\x", r"D:\a\x", r"D:\A\y", r"C:\z\y"]
        );
        assert_eq!(Order { by: Sort::Location, ..Order::default() }.label(), "場所・昇順");
    }

    #[test]
    fn says_when_folders_mix_in() {
        let first = Order { by: Sort::Size, descending: true, ..Order::default() };
        assert_eq!(first.label(), "サイズ・降順");
        assert_eq!(Order { folders_first: false, ..first }.label(), "サイズ・降順・混在");
    }

    #[test]
    fn sorts_ignore_case_run_either_way_and_tie_by_name_from_a() {
        let entry = |path: &str, size: Option<u64>, modified: Option<u64>| Entry {
            path: path.into(),
            name: path.rsplit('\\').next().unwrap().into(),
            parent: Arc::default(),
            directory: false,
            size,
            modified,
            hidden: Hiding::Shown,
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
            sort_entries(entries, Order { by, descending, ..Order::default() });
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
            parent: Arc::default(),
            directory: false,
            size: Some(1),
            modified: None,
            hidden: Hiding::Shown,
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
            sort_entries(&mut entries, Order { by: Sort::Size, descending, ..Order::default() });
            assert_eq!(order(&entries), expected);
        }
    }

    #[test]
    fn hidden_items_hide_protected_ones_apart_but_drive_roots_and_system_items_alone_do_not() {
        assert_eq!(hiding(Path::new(r"C:\pagefile.sys"), 0x2 | 0x4 | 0x20), Hiding::Protected);
        assert_eq!(hiding(Path::new(r"C:\work\.git"), 0x2 | 0x10), Hiding::Hidden);
        assert_eq!(hiding(Path::new(r"C:\work\.gitignore"), 0x20), Hiding::Shown);
        assert_eq!(hiding(Path::new(r"C:\Windows\system.ini"), 0x4 | 0x20), Hiding::Shown);
        assert_eq!(hiding(Path::new(r"C:\"), 0x2 | 0x4 | 0x10), Hiding::Shown);
    }
}
