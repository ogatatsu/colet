//! The keys home's rows go by: a favorite's folder, narrowed or not, or a saved search's query, filter,
//! narrowing and order.
use super::*;

/// What a row of home stands for, read from the path it is keyed by. Home's rows go through the columns
/// as any other's, by path, so a narrowed favorite and a saved search are keyed by paths no folder has:
/// a favorite narrowed to some names by its folder's path, a NUL and the narrowing, which no path can hold;
/// a saved search by [`SEARCH_KEY`] and its query, its filter's name after a NUL, what its results are
/// narrowed to after another, and their order, unless the default, after a third, none of which can hold
/// one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HomeKey<'a> {
    /// A place, or a favorite: its folder, and what a favorite opens narrowed to.
    Folder { path: Cow<'a, Path>, narrow: Option<Cow<'a, str>> },
    /// A saved search: its query, the filter it runs within, what its results are narrowed to, and the
    /// order they are listed in.
    Search { query: &'a str, filter: Option<&'a str>, narrow: Option<&'a str>, order: Order },
}

/// What a saved search's key starts with; no path starts this way, a drive letter being one character.
pub(super) const SEARCH_KEY: &str = "search:";

impl<'a> HomeKey<'a> {
    pub fn favorite(path: &'a Path, narrow: Option<&'a str>) -> Self {
        HomeKey::Folder { path: Cow::Borrowed(path), narrow: narrow.map(Cow::Borrowed) }
    }

    pub fn search(query: &'a str, filter: Option<&'a str>, narrow: Option<&'a str>, order: Order) -> Self {
        HomeKey::Search { query, filter, narrow, order }
    }

    /// What `key` stands for.
    pub fn of(key: &'a Path) -> Self {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        if let Some(search) = key.to_str().and_then(|key| key.strip_prefix(SEARCH_KEY)) {
            let mut parts = search.splitn(4, '\0');
            let query = parts.next().unwrap_or_default();
            // A filter's name and a narrowing are never empty: an empty one stands for none, before what
            // follows it.
            let filter = parts.next().filter(|filter| !filter.is_empty());
            let narrow = parts.next().filter(|narrow| !narrow.is_empty());
            let order = parts.next().and_then(|order| serde_json::from_str(order).ok()).unwrap_or_default();
            return HomeKey::Search { query, filter, narrow, order };
        }
        let wide: Vec<u16> = key.as_os_str().encode_wide().collect();
        match wide.iter().position(|&unit| unit == 0) {
            Some(at) => HomeKey::Folder {
                path: Cow::Owned(OsString::from_wide(&wide[..at]).into()),
                narrow: Some(Cow::Owned(String::from_utf16_lossy(&wide[at + 1..]))),
            },
            None => HomeKey::Folder { path: Cow::Borrowed(key), narrow: None },
        }
    }

    /// The path its row is keyed by.
    pub fn key(&self) -> PathBuf {
        match self {
            HomeKey::Folder { path, narrow: None } => path.to_path_buf(),
            HomeKey::Folder { path, narrow: Some(narrow) } => {
                let mut key = path.as_os_str().to_owned();
                key.push("\0");
                key.push(narrow.as_ref());
                key.into()
            }
            HomeKey::Search { query, filter, narrow, order } => {
                // The default order is no order of its own, so a search saved before orders were is the same.
                let order = (*order != Order::default()).then(|| serde_json::to_string(order).unwrap_or_default());
                let mut key = format!("{SEARCH_KEY}{query}");
                if filter.is_some() || narrow.is_some() || order.is_some() {
                    key.push('\0');
                    key.push_str(filter.unwrap_or_default());
                }
                if narrow.is_some() || order.is_some() {
                    key.push('\0');
                    key.push_str(narrow.unwrap_or_default());
                }
                if let Some(order) = order {
                    key.push('\0');
                    key.push_str(&order);
                }
                key.into()
            }
        }
    }

    pub fn is_search(&self) -> bool {
        matches!(self, HomeKey::Search { .. })
    }

    /// What tells home's rows apart: a place whatever its case, a saved search by its query, filter,
    /// narrowing and order exactly, and a narrowed favorite by its folder whatever its case and its narrowing exactly.
    pub fn pin(&self) -> OsString {
        match self {
            HomeKey::Folder { path, narrow } => {
                let mut pin = path_key(path);
                if let Some(narrow) = narrow {
                    pin.push("\0");
                    pin.push(narrow.as_ref());
                }
                pin
            }
            HomeKey::Search { .. } => self.key().into_os_string(),
        }
    }

    /// What the row of `key` stands for on disk: a place's or favorite's folder, a saved search's own key.
    pub fn on_disk(key: &Path) -> Cow<'_, Path> {
        match HomeKey::of(key) {
            HomeKey::Folder { path, .. } => path,
            HomeKey::Search { .. } => Cow::Borrowed(key),
        }
    }
}
