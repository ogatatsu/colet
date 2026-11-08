//! The orders the columns are given apart from the one they share: one for each folder so given, and one
//! for the search results, kept in `%APPDATA%\Colet\sort.json` apart from the settings.
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
    pub search: Option<Order>,
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

/// The orders as the file holds them: `{"search": {"by": "size", "descending": true}, "folders": [{"path":
/// "D:\a", "order": {...}}]}`. A folder whose path is not text (an unpaired surrogate, which Windows allows
/// in a name) is left out, as the file holds a path only as text.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct SortsFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    search: Option<Order>,
    folders: Vec<FolderSort>,
}

#[derive(Serialize, Deserialize)]
struct FolderSort {
    path: PathBuf,
    order: Order,
}

impl From<SortsFile> for Sorts {
    fn from(file: SortsFile) -> Self {
        let mut sorts = Sorts { search: file.search, ..Sorts::default() };
        for FolderSort { path, order } in file.folders.into_iter().filter(|f| !f.path.as_os_str().is_empty()) {
            // The first of a folder written twice by hand stands.
            sorts.folders.entry(path_key(&path)).or_insert((path, order));
        }
        sorts
    }
}

impl From<Sorts> for SortsFile {
    fn from(sorts: Sorts) -> Self {
        let folders = sorts.folders.into_values().filter(|(path, _)| path.to_str().is_some());
        SortsFile { search: sorts.search, folders: folders.map(|(path, order)| FolderSort { path, order }).collect() }
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
    fn keeps_each_folders_order_whatever_its_case_and_the_search_results() {
        let mut sorts = Sorts::default();
        let size = Order { by: Sort::Size, descending: true };
        sorts.set_folder(Path::new(r"D:\Work"), Some(size));
        sorts.search = Some(Order { by: Sort::Modified, descending: false });
        assert_eq!(sorts.folder(Path::new(r"d:\work")), Some(size));
        let json = serde_json::to_string(&sorts).unwrap();
        assert_eq!(
            json,
            r#"{"search":{"by":"modified","descending":false},"folders":[{"path":"D:\\Work","order":{"by":"size","descending":true}}]}"#
        );
        assert_eq!(serde_json::from_str::<Sorts>(&json).unwrap(), sorts);
        sorts.set_folder(Path::new(r"D:\WORK"), None);
        assert_eq!(sorts.folder(Path::new(r"D:\Work")), None);
    }

    #[test]
    fn keeps_the_first_of_a_folder_written_twice() {
        let sorts: Sorts = serde_json::from_str(
            r#"{"folders":[{"path":"","order":{"by":"name","descending":true}},{"path":"D:\\a","order":{"by":"size","descending":false}},{"path":"d:\\A","order":{"by":"modified","descending":false}}]}"#,
        )
        .unwrap();
        assert_eq!(sorts.folder(Path::new(r"D:\a")), Some(Order { by: Sort::Size, descending: false }));
        assert_eq!(sorts.folder(Path::new("")), None);
        assert_eq!(sorts.search, None);
    }

    #[test]
    fn leaves_out_a_folder_whose_path_is_not_text() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(OsString::from_wide(&wide));
        let mut sorts = Sorts::default();
        sorts.set_folder(&odd, Some(Order { by: Sort::Size, descending: true }));
        sorts.set_folder(Path::new(r"D:\b"), Some(Order { by: Sort::Size, descending: true }));
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
        let sorts = Sorts { search: Some(Order { by: Sort::Size, descending: true }), ..Default::default() };
        settings::write(&sorts, &path, &SORTS).unwrap();
        assert_eq!(settings::read::<Sorts>(&path, &SORTS), Ok(sorts));
    }
}
