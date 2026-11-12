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

use crate::settings::{self, File, LoadError};

pub const SORTS: File =
    File { stem: "sort", what: "カラムの並び順", file: "カラムの並び順のファイル", fallback: "既定の並び順" };

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "SortsFile", into = "SortsFile")]
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
/// true}}]}`. A folder whose path is not text (an unpaired surrogate, which Windows allows in a name) is
/// left out, as the file holds a path only as text.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SortsFile {
    folders: Vec<FolderSort>,
}

#[derive(Serialize, Deserialize)]
struct FolderSort {
    path: PathBuf,
    order: Order,
}

impl From<SortsFile> for Sorts {
    fn from(file: SortsFile) -> Self {
        // An order the same as the default is no order of its own: one kept while the default could be
        // changed goes.
        let own = |order: &Order| *order != Order::default();
        let mut sorts = Sorts::default();
        for FolderSort { path, order } in file.folders.into_iter().filter(|f| !f.path.as_os_str().is_empty() && own(&f.order)) {
            // The first of a folder written twice by hand stands.
            sorts.folders.entry(path_key(&path)).or_insert((path, order));
        }
        sorts
    }
}

impl From<Sorts> for SortsFile {
    fn from(sorts: Sorts) -> Self {
        let folders = sorts.folders.into_values().filter(|(path, _)| path.to_str().is_some());
        SortsFile { folders: folders.map(|(path, order)| FolderSort { path, order }).collect() }
    }
}

/// `%APPDATA%\Colet\sort.json`.
pub fn path() -> Option<PathBuf> {
    settings::kept_path("sort.json")
}

/// The orders kept, or none when there are none yet.
pub fn load() -> Result<Sorts, LoadError> {
    let Some(path) = path() else { return Ok(Sorts::default()) };
    settings::read(&path, &SORTS)
}

pub fn save(sorts: &Sorts) -> Result<(), String> {
    let path = path().ok_or("カラムの並び順の保存先がわかりません")?;
    settings::write(sorts, &path, &SORTS)
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
    fn leaves_out_a_folder_whose_path_is_not_text() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(OsString::from_wide(&wide));
        let mut sorts = Sorts::default();
        sorts.set_folder(&odd, Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
        sorts.set_folder(Path::new(r"D:\b"), Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
        let read: Sorts = serde_json::from_str(&serde_json::to_string(&sorts).unwrap()).unwrap();
        assert_eq!(read.folder(&odd), None);
        assert_eq!(read.folder(Path::new(r"D:\b")), sorts.folder(Path::new(r"D:\b")));
    }

    #[test]
    fn puts_a_broken_file_aside_and_starts_on_the_default_orders() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sort.json");
        std::fs::write(&path, "{").unwrap();
        let error = settings::read::<Sorts>(&path, &SORTS).unwrap_err();
        assert!(!error.kept, "{error:?}");
        assert!(error.message.starts_with("カラムの並び順のファイルが壊れていたため、既定の並び順で始めました"), "{}", error.message);
        assert!(!path.exists());
        assert_eq!(settings::read::<Sorts>(&path, &SORTS), Ok(Sorts::default()));
        let mut sorts = Sorts::default();
        sorts.set_folder(Path::new(r"D:\a"), Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
        settings::write(&sorts, &path, &SORTS).unwrap();
        assert_eq!(settings::read::<Sorts>(&path, &SORTS), Ok(sorts));
    }
}
