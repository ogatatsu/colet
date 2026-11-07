//! The orders the columns are given apart from the one they share: one for each folder so given, and one
//! for the search results, kept in `%APPDATA%\Colet\sort.json` apart from the settings.
use std::{
    collections::BTreeMap,
    ffi::OsString,
    path::{Path, PathBuf},
};

use colet_core::{entry::Order, paths::path_key};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::settings::{self, File, LoadError, order, paths};

pub const SORTS: File =
    File { stem: "sort", what: "カラムの並び順", file: "カラムの並び順のファイル", fallback: "既定の並び順" };

#[derive(Clone, Debug, Default, PartialEq)]
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

/// `{"search": {"by": "size", "descending": true}, "folders": [{"path": "D:\\a", "order": {...}}]}`; a
/// folder whose path reads neither way is left out alone, as a favorite is.
impl Serialize for Sorts {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct Folder {
            path: serde_json::Value,
            #[serde(with = "order")]
            order: Order,
        }
        #[derive(Serialize)]
        struct Kept {
            #[serde(skip_serializing_if = "Option::is_none")]
            search: Option<Search>,
            folders: Vec<Folder>,
        }
        #[derive(Serialize)]
        struct Search(#[serde(with = "order")] Order);
        let folders = self.folders.values().map(|(path, order)| Folder { path: paths::kept(path), order: *order }).collect();
        Kept { search: self.search.map(Search), folders }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Sorts {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Folder {
            path: serde_json::Value,
            #[serde(with = "order")]
            order: Order,
        }
        #[derive(Deserialize)]
        struct Search(#[serde(with = "order")] Order);
        #[derive(Deserialize, Default)]
        #[serde(default)]
        struct Kept {
            search: Option<Search>,
            folders: Vec<serde_json::Value>,
        }
        let kept = Kept::deserialize(deserializer)?;
        let mut sorts = Sorts { search: kept.search.map(|s| s.0), ..Sorts::default() };
        for folder in kept.folders {
            let Ok(folder) = serde_json::from_value::<Folder>(folder) else { continue };
            if let Some(path) = paths::read(folder.path).filter(|p| !p.as_os_str().is_empty()) {
                // The first of a folder written twice by hand stands.
                sorts.folders.entry(path_key(&path)).or_insert((path, folder.order));
            }
        }
        Ok(sorts)
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
    fn leaves_out_alone_a_folder_it_cannot_read() {
        let sorts: Sorts = serde_json::from_str(
            r#"{"folders":[1,{"path":2,"order":{"by":"size"}},{"path":"D:\\a","order":{"by":"size"}},{"path":"d:\\A","order":{"by":"modified"}}]}"#,
        )
        .unwrap();
        assert_eq!(sorts.folder(Path::new(r"D:\a")), Some(Order { by: Sort::Size, descending: false }));
        assert_eq!(sorts.search, None);
    }

    #[test]
    fn keeps_a_folder_whose_path_is_not_text_as_it_is() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(OsString::from_wide(&wide));
        let mut sorts = Sorts::default();
        sorts.set_folder(&odd, Some(Order { by: Sort::Size, descending: true }));
        let json = serde_json::to_string(&sorts).unwrap();
        assert!(json.contains(r#"{"utf16":[68,58,92,97,55296]}"#), "{json}");
        let read: Sorts = serde_json::from_str(&json).unwrap();
        assert_eq!(read.folder(&odd), sorts.folder(&odd));
        assert_eq!(read.folder(Path::new("D:\\a\u{FFFD}")), None);
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
