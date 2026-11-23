//! The orders the folder columns are given apart from the default, one for each folder so given, kept in
//! `%APPDATA%\Colet\sort.json` apart from the settings. The search results' order is each tab's own, kept
//! with the tab; one kept here for them all before it was is left unread, and goes with the next save.
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use colet_core::{entry::Order, paths::path_key};
use serde::{Deserialize, Serialize};

use crate::kept_file::{self, Fields, File, Lenient, Loaded};
use crate::words;

pub const SORTS: File =
    File { stem: "sort", what: words::kept::sorts, file: words::kept::sorts_file, fallback: words::kept::sorts_fallback };

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(into = "SortsFile")]
pub struct Sorts {
    /// By the folder's path key, with the path as it was given.
    folders: BTreeMap<OsString, (PathBuf, Order)>,
}

impl Sorts {
    /// The order the folder at `path` is given, whatever its case.
    pub fn folder(&self, path: &Path) -> Option<Order> {
        self.folders.get(&path_key(path)).map(|(_, order)| *order)
    }

    /// Gives the folder at `path` its own order, or takes it away.
    pub fn set_folder(&mut self, path: &Path, order: Option<Order>) {
        match order {
            Some(order) => {
                self.folders.insert(path_key(path), (path.to_owned(), order));
            }
            None => {
                self.folders.remove(&path_key(path));
            }
        }
    }
}

/// The orders as the file holds them: `{"folders": [{"path": "D:\a", "order": {"by": "size", "descending":
/// true}}]}`, a path that is not text as [`crate::kept_path`] writes it. Read a folder at a time.
#[derive(Serialize)]
struct SortsFile {
    folders: Vec<FolderSort>,
}

#[derive(Serialize, Deserialize)]
struct FolderSort {
    #[serde(with = "crate::kept_path")]
    path: PathBuf,
    order: Order,
}

impl Lenient for Sorts {
    fn from_json(fields: &mut Fields) -> Self {
        // An order the same as the default is no order of its own: one kept while the default could be
        // changed goes.
        let own = |order: &Order| *order != Order::default();
        let mut sorts = Sorts::default();
        let folders = fields.many::<FolderSort>("folders");
        for FolderSort { path, order } in folders.into_iter().filter(|f| !f.path.as_os_str().is_empty() && own(&f.order)) {
            // The first of a folder written twice by hand stands.
            sorts.folders.entry(path_key(&path)).or_insert((path, order));
        }
        sorts
    }
}

impl<'de> Deserialize<'de> for Sorts {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        kept_file::lenient(deserializer)
    }
}

impl From<Sorts> for SortsFile {
    fn from(sorts: Sorts) -> Self {
        SortsFile { folders: sorts.folders.into_values().map(|(path, order)| FolderSort { path, order }).collect() }
    }
}

/// `%APPDATA%\Colet\sort.json`.
pub fn path() -> Option<PathBuf> {
    kept_file::kept_path("sort.json")
}

/// The orders kept, or none when there are none yet.
pub fn load() -> Loaded<Sorts> {
    let Some(path) = path() else { return Loaded::read(Sorts::default()) };
    kept_file::read(&path, &SORTS)
}

pub fn save(sorts: &Sorts) -> Result<(), String> {
    let path = path().ok_or(words::kept::no_sorts_place())?;
    kept_file::write(sorts, &path, &SORTS)
}

#[cfg(test)]
mod tests {
    use colet_core::entry::Sort;

    use super::*;

    #[test]
    fn keeps_each_folders_order_whatever_its_case() {
        let mut sorts = Sorts::default();
        let size = Order { by: Sort::Size, descending: true, ..Order::default() };
        sorts.set_folder(Path::new(r"D:\Work"), Some(size));
        assert_eq!(sorts.folder(Path::new(r"d:\work")), Some(size));
        let json = serde_json::to_string(&sorts).unwrap();
        assert_eq!(json, r#"{"folders":[{"path":"D:\\Work","order":{"by":"size","descending":true}}]}"#);
        assert_eq!(serde_json::from_str::<Sorts>(&json).unwrap(), sorts);
        sorts.set_folder(Path::new(r"D:\WORK"), None);
        assert_eq!(sorts.folder(Path::new(r"D:\Work")), None);
    }

    #[test]
    fn writes_folders_first_only_when_they_mix_and_reads_an_order_written_before_as_folders_first() {
        let first = Order { by: Sort::Size, descending: true, ..Order::default() };
        let mut sorts = Sorts::default();
        sorts.set_folder(Path::new(r"D:\a"), Some(Order { folders_first: false, ..first }));
        sorts.set_folder(Path::new(r"D:\b"), Some(first));
        let json = serde_json::to_string(&sorts).unwrap();
        assert_eq!(
            json,
            r#"{"folders":[{"path":"D:\\a","order":{"by":"size","descending":true,"folders_first":false}},{"path":"D:\\b","order":{"by":"size","descending":true}}]}"#
        );
        assert_eq!(serde_json::from_str::<Sorts>(&json).unwrap(), sorts);
        let before: Sorts = serde_json::from_str(r#"{"folders":[{"path":"D:\\a","order":{"by":"size","descending":false}}]}"#).unwrap();
        assert!(before.folder(Path::new(r"D:\a")).unwrap().folders_first);
    }

    #[test]
    fn keeps_the_first_of_a_folder_written_twice() {
        let sorts: Sorts = serde_json::from_str(
            r#"{"folders":[{"path":"","order":{"by":"name","descending":true}},{"path":"D:\\a","order":{"by":"size","descending":false}},{"path":"d:\\A","order":{"by":"modified","descending":false}}]}"#,
        )
        .unwrap();
        assert_eq!(sorts.folder(Path::new(r"D:\a")), Some(Order { by: Sort::Size, descending: false, ..Order::default() }));
        assert_eq!(sorts.folder(Path::new("")), None);
    }

    #[test]
    fn leaves_the_search_results_order_kept_for_every_tab_before_unread_and_writes_it_no_more() {
        let sorts: Sorts = serde_json::from_str(
            r#"{"search":{"by":"size","descending":true},"folders":[{"path":"D:\\a","order":{"by":"size","descending":true}}]}"#,
        )
        .unwrap();
        assert_eq!(serde_json::to_string(&sorts).unwrap(), r#"{"folders":[{"path":"D:\\a","order":{"by":"size","descending":true}}]}"#);
    }

    #[test]
    fn drops_an_order_the_same_as_the_default() {
        let sorts: Sorts = serde_json::from_str(
            r#"{"folders":[{"path":"D:\\a","order":{"by":"name","descending":false}},{"path":"D:\\b","order":{"by":"name","descending":false,"folders_first":false}}]}"#,
        )
        .unwrap();
        assert_eq!(sorts.folder(Path::new(r"D:\a")), None);
        assert_eq!(sorts.folder(Path::new(r"D:\b")), Some(Order { folders_first: false, ..Order::default() }));
    }

    #[test]
    fn keeps_a_folder_whose_path_is_not_text_by_its_units() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(OsString::from_wide(&wide));
        let mut sorts = Sorts::default();
        sorts.set_folder(&odd, Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
        sorts.set_folder(Path::new(r"D:\b"), Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
        let read: Sorts = serde_json::from_str(&serde_json::to_string(&sorts).unwrap()).unwrap();
        assert_eq!(read, sorts);
        assert!(read.folder(&odd).is_some());
    }

    #[test]
    fn reads_the_folders_it_can_and_copies_the_file_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sort.json");
        std::fs::write(&path, r#"{"folders":[{"path":"D:\\a","order":{"by":"size","descending":true}},{"path":"D:\\b","order":5}]}"#)
            .unwrap();
        let loaded = kept_file::read::<Sorts>(&path, &SORTS);
        assert!(loaded.value.folder(Path::new(r"D:\a")).is_some());
        let problem = loaded.problem.unwrap();
        assert!(problem.message.starts_with("カラムの並び順のファイルの読めない項目を1件飛ばしました"), "{}", problem.message);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn puts_a_broken_file_aside_and_starts_on_the_default_orders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sort.json");
        std::fs::write(&path, "{").unwrap();
        let error = kept_file::read::<Sorts>(&path, &SORTS).unwrap_err();
        assert!(!error.kept, "{error:?}");
        assert!(error.message.starts_with("カラムの並び順のファイルが壊れていたため、既定の並び順で始めました"), "{}", error.message);
        assert!(!path.exists());
        assert_eq!(kept_file::read::<Sorts>(&path, &SORTS).unwrap(), Sorts::default());
        let mut sorts = Sorts::default();
        sorts.set_folder(Path::new(r"D:\a"), Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
        kept_file::write(&sorts, &path, &SORTS).unwrap();
        assert_eq!(kept_file::read::<Sorts>(&path, &SORTS).unwrap(), sorts);
    }
}
