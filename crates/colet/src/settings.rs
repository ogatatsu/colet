//! What Colet keeps between runs: the widths, the favorites, the saved searches, what is pinned in home,
//! the search filters, the tabs with what each searched and the folders open, the sort order, the theme, whether
//! the hidden items show, and where the window was. The way back and forth and what is picked besides are not kept.
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

use colet_core::{
    entry::{Order, Sort},
    paths::{folder_name, path_key, shown},
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    app::{COLUMN_WIDTHS, SIDEBAR_WIDTHS},
    state::{Kept, favorite_key, favorite_parts, filter_words, narrowed_folder, narrowing, saved_query, search_key},
};

/// A search kept in home. An empty name shows the query itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSearch {
    pub name: String,
    pub query: String,
    /// The search filter it was saved with, by name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    /// What its results were narrowed to as it was saved.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narrow: Option<String>,
}

impl SavedSearch {
    pub fn label(&self) -> &str {
        if self.name.is_empty() { &self.query } else { &self.name }
    }

    /// Its row's key in home.
    pub fn key(&self) -> PathBuf {
        search_key(&self.query, self.filter.as_deref(), self.narrow.as_deref())
    }
}

/// A folder kept in home, narrowed to some names or not. An empty name shows the folder's own.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Favorite {
    pub path: PathBuf,
    pub name: String,
    pub narrow: Option<String>,
}

impl From<PathBuf> for Favorite {
    fn from(path: PathBuf) -> Self {
        Self::new(path)
    }
}

impl From<&str> for Favorite {
    fn from(path: &str) -> Self {
        Self::new(path)
    }
}

impl Favorite {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into(), ..Self::default() }
    }

    pub fn label(&self) -> String {
        if self.name.is_empty() { shown(folder_name(&self.path)) } else { self.name.clone() }
    }

    /// Its row's key in home.
    pub fn key(&self) -> PathBuf {
        favorite_key(&self.path, self.narrow.as_deref())
    }

    /// Whether it is the favorite of `path` narrowed to `narrow`, whatever the path's case.
    pub fn is(&self, path: &Path, narrow: Option<&str>) -> bool {
        self.narrow.as_deref() == narrow && path_key(&self.path) == path_key(path)
    }
}

/// An Everything query a search can be narrowed by, one at a time, found by its name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchFilter {
    pub name: String,
    pub query: String,
}

/// What Everything is asked: the query narrowed by the filter's, each in `< >` so an `|` in one does not
/// reach into the other. A filter alone asks for all it lets through.
pub fn everything_query(query: &str, filter: Option<&str>) -> String {
    match filter {
        None => query.to_owned(),
        Some(filter) if query.trim().is_empty() => format!("<{}>", closed(filter)),
        Some(filter) => format!("<{}> <{}>", closed(filter), closed(query)),
    }
}

/// The search `query` narrowed to the names holding each word of `text` as it is, in any order, as the
/// columns Colet narrows itself are: not the folders they are in, and with no wildcards.
pub fn narrowed_query(query: &str, text: &str) -> String {
    let words: String = filter_words(text).iter().map(|word| format!(" nopath:nowildcards:\"{word}\"")).collect();
    format!("<{}>{words}", closed(query))
}

/// `text` with a quote left open closed, as Everything closes it at the end of the text: inside `< >` it
/// would run on over the `>` and take it for a letter, finding nothing while `"my doc` is being typed.
fn closed(text: &str) -> std::borrow::Cow<'_, str> {
    if text.matches('"').count() % 2 == 1 { format!("{text}\"").into() } else { text.into() }
}

/// The colors Colet takes: Windows's setting, or one of its own whatever Windows says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

impl Theme {
    /// In the order the settings offer them.
    pub const ALL: [Theme; 3] = [Theme::System, Theme::Light, Theme::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "Windowsに合わせる",
            Theme::Light => "ライト",
            Theme::Dark => "ダーク",
        }
    }
}

/// A tab kept between starts: where it was, and the filter its next search is narrowed by, by name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeptTab {
    pub place: Kept,
    pub filter: Option<String>,
}

/// The window's place in logical pixels, and whether it was maximized over it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Placement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The folders' shared width.
    pub column_width: f32,
    /// The pinned search results, apart from the folders.
    pub search_width: f32,
    /// Home's pane on the left.
    pub sidebar_width: f32,
    #[serde(with = "favorites")]
    pub favorites: Vec<Favorite>,
    pub saved_searches: Vec<SavedSearch>,
    /// Home's rows pinned above the rest, in their order, by the path each row has: a folder's, or a
    /// narrowed favorite's or a saved search's key.
    #[serde(with = "paths")]
    pub pinned: Vec<PathBuf>,
    pub filters: Vec<SearchFilter>,
    /// The tabs, in their order, and the one in front.
    #[serde(with = "tabs")]
    pub tabs: Vec<KeptTab>,
    pub tab: usize,
    /// The one order the folders and the search results share.
    #[serde(with = "order")]
    pub sort: Order,
    pub theme: Theme,
    /// Whether the folders list their hidden and system items, as Explorer leaves them out by default.
    pub show_hidden: bool,
    /// Whether the protected operating system files show along with them, as Explorer leaves them out
    /// even then by default.
    pub show_protected: bool,
    pub window: Option<Placement>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            column_width: 320.,
            search_width: 320.,
            sidebar_width: 240.,
            favorites: Vec::new(),
            saved_searches: Vec::new(),
            pinned: Vec::new(),
            filters: Vec::new(),
            tabs: Vec::new(),
            tab: 0,
            sort: Order::default(),
            theme: Theme::System,
            show_hidden: false,
            show_protected: false,
            window: None,
        }
    }
}

impl Settings {
    /// Keeps a file edited by hand, or written by an older Colet, within what the window can show.
    pub fn normalize(&mut self) {
        let clamp = |width: f32, (min, max): (f32, f32)| width.round().clamp(min, max);
        self.column_width = clamp(self.column_width, COLUMN_WIDTHS);
        self.search_width = clamp(self.search_width, COLUMN_WIDTHS);
        self.sidebar_width = clamp(self.sidebar_width, SIDEBAR_WIDTHS);
        // A filter is found by its name, a search by its query, filter and narrowing, and a favorite by its
        // path and narrowing: one of each, none empty. What names a filter gone goes without it. A
        // narrowing is kept as one is typed again, with no word none.
        let mut names = std::collections::HashSet::new();
        self.filters.retain(|f| !f.name.trim().is_empty() && !f.query.trim().is_empty() && names.insert(f.name.clone()));
        let known = |name: &Option<String>| name.as_ref().is_none_or(|name| names.contains(name));
        for tab in &mut self.tabs {
            if !known(&tab.filter) {
                tab.filter = None;
            }
            if let Some(name) = tab.place.search_filter.clone().filter(|name| !names.contains(name)) {
                tab.place.rename_search_filter(&name, None);
            }
        }
        self.tab = self.tab.min(self.tabs.len().saturating_sub(1));
        for search in &mut self.saved_searches {
            let was = search.key();
            if !known(&search.filter) {
                search.filter = None;
            }
            search.narrow = search.narrow.as_deref().and_then(narrowing);
            // Its pin follows it to the search it is now.
            let is = search.key();
            for pin in self.pinned.iter_mut().filter(|pin| **pin == was) {
                *pin = is.clone();
            }
        }
        let mut seen = std::collections::HashSet::new();
        self.saved_searches.retain(|s| !s.query.trim().is_empty() && seen.insert((s.query.clone(), s.filter.clone(), s.narrow.clone())));
        for favorite in &mut self.favorites {
            let was = pin_key(&favorite.key());
            favorite.narrow = favorite.narrow.as_deref().and_then(narrowing);
            let is = favorite.key();
            if pin_key(&is) == was {
                continue;
            }
            for pin in self.pinned.iter_mut().filter(|pin| pin_key(pin) == was) {
                *pin = is.clone();
            }
        }
        let mut seen = std::collections::HashSet::new();
        self.favorites.retain(|f| !f.path.as_os_str().is_empty() && seen.insert(pin_key(&f.key())));
        // A pin is one of each row, and goes with the saved search or narrowed favorite it pins.
        let saved: std::collections::HashSet<PathBuf> = self.saved_searches.iter().map(SavedSearch::key).collect();
        let narrowed: std::collections::HashSet<std::ffi::OsString> =
            self.favorites.iter().filter(|f| f.narrow.is_some()).map(|f| pin_key(&f.key())).collect();
        let mut seen = std::collections::HashSet::new();
        self.pinned.retain(|pin| {
            !pin.as_os_str().is_empty()
                && (saved_query(pin).is_none() || saved.contains(pin))
                && (narrowed_folder(pin).is_none() || narrowed.contains(&pin_key(pin)))
                && seen.insert(pin_key(pin))
        });
        if self.window.is_some_and(|w| !(w.width >= 200. && w.height >= 150.)) {
            self.window = None;
        }
    }

    pub fn saved(&self, query: &str, filter: Option<&str>, narrow: Option<&str>) -> Option<&SavedSearch> {
        self.saved_searches.iter().find(|s| s.query == query && s.filter.as_deref() == filter && s.narrow.as_deref() == narrow)
    }

    /// The favorite of `path` narrowed to `narrow`, if there is one.
    pub fn favorite(&self, path: &Path, narrow: Option<&str>) -> Option<&Favorite> {
        self.favorites.iter().find(|f| f.is(path, narrow))
    }

    /// The query of the filter named `name`, if there is one.
    pub fn filter_query(&self, name: &str) -> Option<&str> {
        self.filters.iter().find(|f| f.name == name).map(|f| f.query.as_str())
    }

    pub fn is_pinned(&self, path: &Path) -> bool {
        let key = pin_key(path);
        self.pinned.iter().any(|pin| pin_key(pin) == key)
    }

    /// Whether `key`, a folder's path or a narrowed favorite's key, is a favorite.
    pub fn is_favorite(&self, key: &Path) -> bool {
        let (path, narrow) = favorite_parts(key);
        self.favorite(&path, narrow.as_deref()).is_some()
    }
}

/// What tells home's rows apart: a place whatever its case, a saved search by its query, filter and
/// narrowing exactly, and a narrowed favorite by its folder whatever its case and its narrowing exactly.
pub fn pin_key(path: &Path) -> std::ffi::OsString {
    if saved_query(path).is_some() {
        return path.as_os_str().to_owned();
    }
    match narrowed_folder(path) {
        Some((folder, narrow)) => {
            let mut key = path_key(&folder);
            key.push("\0");
            key.push(narrow);
            key
        }
        None => path_key(path),
    }
}

/// The favorites and the pins, each as its path's text; one whose path is not text (an unpaired surrogate,
/// which Windows allows in a name) as `{"utf16": [...]}`, the units Windows holds, so it still names that
/// folder alone. One read neither way (a file edited by hand) is left out alone.
pub(crate) mod paths {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    use super::*;

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    enum Kept {
        Text(String),
        Wide { utf16: Vec<u16> },
    }

    /// One path as it is written.
    pub fn kept(path: &Path) -> serde_json::Value {
        let kept = match path.to_str() {
            Some(text) => Kept::Text(text.to_owned()),
            None => Kept::Wide { utf16: path.as_os_str().encode_wide().collect() },
        };
        serde_json::to_value(kept).unwrap_or_default()
    }

    /// One path as it is written; none, when it reads neither way.
    pub fn read(value: serde_json::Value) -> Option<PathBuf> {
        match serde_json::from_value(value).ok()? {
            Kept::Text(text) => Some(PathBuf::from(text)),
            Kept::Wide { utf16 } => Some(OsString::from_wide(&utf16).into()),
        }
    }

    pub fn serialize<S: Serializer>(paths: &[PathBuf], serializer: S) -> Result<S::Ok, S::Error> {
        paths.iter().map(|path| kept(path)).collect::<Vec<_>>().serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<PathBuf>, D::Error> {
        let values = Vec::<serde_json::Value>::deserialize(deserializer)?;
        Ok(values.into_iter().filter_map(read).collect())
    }
}

/// The favorites, each as `{"path": ..., "name": ..., "narrow": ...}`, its path as [`paths`] writes one, and
/// with no narrowing none. One written by an older Colet, its path alone, is read as a favorite of it; one
/// read neither way (a file edited by hand) is left out alone.
pub(crate) mod favorites {
    use serde_json::{Map, Value};

    use super::*;

    pub fn serialize<S: Serializer>(favorites: &[Favorite], serializer: S) -> Result<S::Ok, S::Error> {
        favorites
            .iter()
            .map(|f| {
                let mut kept = Map::new();
                kept.insert("path".into(), paths::kept(&f.path));
                kept.insert("name".into(), f.name.clone().into());
                if let Some(narrow) = &f.narrow {
                    kept.insert("narrow".into(), narrow.clone().into());
                }
                Value::Object(kept)
            })
            .collect::<Vec<_>>()
            .serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<Favorite>, D::Error> {
        let values = Vec::<Value>::deserialize(deserializer)?;
        Ok(values.into_iter().filter_map(read).collect())
    }

    fn read(mut value: Value) -> Option<Favorite> {
        let Some(path) = value.get_mut("path").map(Value::take) else { return paths::read(value).map(Favorite::new) };
        let text = |value: &Value, field: &str| value.get(field).and_then(Value::as_str).map(str::to_owned);
        Some(Favorite { path: paths::read(path)?, name: text(&value, "name").unwrap_or_default(), narrow: text(&value, "narrow") })
    }
}

/// The tabs, each as `{"query": ..., "search_filter": ..., "filter": ..., "folders": [{"path": ...,
/// "highlight": ...}], "picks": [...], "active": ..., "fold": {"anchor": ..., "open": ...}}`, its paths as
/// [`paths`] writes one and what it lacks left out. Read leniently: what is missing or not read goes without
/// it, and a tab read no way at all (a file edited by hand) is left out alone.
pub(crate) mod tabs {
    use serde_json::{Map, Value};

    use super::*;

    pub fn serialize<S: Serializer>(tabs: &[KeptTab], serializer: S) -> Result<S::Ok, S::Error> {
        tabs.iter().map(kept).collect::<Vec<_>>().serialize(serializer)
    }

    fn kept(tab: &KeptTab) -> Value {
        let place = &tab.place;
        let mut kept = Map::new();
        if !place.query.is_empty() {
            kept.insert("query".into(), place.query.clone().into());
        }
        if let Some(name) = &place.search_filter {
            kept.insert("search_filter".into(), name.clone().into());
        }
        if let Some(name) = &tab.filter {
            kept.insert("filter".into(), name.clone().into());
        }
        let folders = place
            .folders
            .iter()
            .map(|(path, highlight)| {
                let mut folder = Map::new();
                folder.insert("path".into(), paths::kept(path));
                if let Some(highlight) = highlight {
                    folder.insert("highlight".into(), paths::kept(highlight));
                }
                Value::Object(folder)
            })
            .collect();
        kept.insert("folders".into(), Value::Array(folders));
        kept.insert("picks".into(), place.picks.iter().map(|pick| pick.as_deref().map_or(Value::Null, paths::kept)).collect());
        kept.insert("active".into(), place.active.into());
        if let Some((anchor, open)) = place.fold {
            kept.insert("fold".into(), serde_json::json!({ "anchor": anchor, "open": open }));
        }
        Value::Object(kept)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<KeptTab>, D::Error> {
        let values = Vec::<Value>::deserialize(deserializer)?;
        Ok(values.into_iter().filter_map(read).collect())
    }

    fn read(mut value: Value) -> Option<KeptTab> {
        value.as_object()?;
        let text = |value: &Value, field: &str| value.get(field).and_then(Value::as_str).map(str::to_owned);
        let folders: Vec<(PathBuf, Option<PathBuf>)> = match value.get_mut("folders").map(Value::take) {
            Some(Value::Array(folders)) => folders
                .into_iter()
                .filter_map(|mut folder| {
                    let path = paths::read(folder.get_mut("path")?.take())?;
                    Some((path, folder.get_mut("highlight").map(Value::take).and_then(paths::read)))
                })
                .collect(),
            _ => Vec::new(),
        };
        let picks = match value.get_mut("picks").map(Value::take) {
            Some(Value::Array(picks)) => picks.into_iter().map(paths::read).collect(),
            _ => Vec::new(),
        };
        let fold = value.get("fold").and_then(|fold| Some((fold.get("anchor")?.as_u64()? as usize, fold.get("open")?.as_bool()?)));
        let place = Kept {
            query: text(&value, "query").unwrap_or_default(),
            search_filter: text(&value, "search_filter"),
            fold: fold.filter(|(anchor, _)| *anchor < folders.len()),
            folders,
            picks,
            active: value.get("active").and_then(Value::as_u64).unwrap_or_default() as usize,
        };
        Some(KeptTab { place, filter: text(&value, "filter") })
    }
}

/// The sort order as `{"by": "size", "descending": true}`, read leniently: what is missing or not known
/// (a file edited by hand, or written by a later Colet) goes back to its default alone, rather than have
/// the whole file taken for broken.
pub(crate) mod order {
    use super::*;

    fn name(by: Sort) -> &'static str {
        match by {
            Sort::Name => "name",
            Sort::Size => "size",
            Sort::Modified => "modified",
        }
    }

    #[derive(Serialize)]
    struct Kept {
        by: &'static str,
        descending: bool,
    }

    pub fn serialize<S: Serializer>(order: &Order, serializer: S) -> Result<S::Ok, S::Error> {
        Kept { by: name(order.by), descending: order.descending }.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Order, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        let by = value["by"].as_str().and_then(|by| Sort::ALL.into_iter().find(|s| name(*s) == by));
        Ok(Order { by: by.unwrap_or_default(), descending: value["descending"].as_bool().unwrap_or_default() })
    }
}

/// `%APPDATA%\Colet\<name>`, beside the other files Colet keeps.
pub fn kept_path(name: &str) -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Colet").join(name))
}

/// Why a file kept could not be used; Colet starts on the defaults either way.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadError {
    pub message: String,
    /// The file is still where it was, unread: writing the defaults over it would lose it, so nothing is
    /// saved until Colet is started again.
    pub kept: bool,
}

/// A file Colet keeps, as its words name it.
pub struct File {
    /// Its name without `.json`, which a broken one is put aside under.
    pub stem: &'static str,
    /// What it holds: "設定".
    pub what: &'static str,
    /// What it holds, as it is a file: "設定ファイル".
    pub file: &'static str,
    /// What Colet starts on without it: "既定の設定".
    pub fallback: &'static str,
}

pub const SETTINGS: File = File { stem: "settings", what: "設定", file: "設定ファイル", fallback: "既定の設定" };

type Done = mpsc::Sender<Result<(), String>>;

/// Writes a file kept on a thread of its own, one save after another in the order they were sent, so
/// an older save never lands over a newer one: the final save on close waits for those before it.
pub struct Saver<T>(mpsc::Sender<(T, Done)>);

impl<T: Send + 'static> Saver<T> {
    pub fn new(write: impl Fn(&T) -> Result<(), String> + Send + 'static) -> Self {
        let (saves, queued) = mpsc::channel::<(T, Done)>();
        // Ends once the saver is dropped and the saves sent before are written.
        std::thread::spawn(move || {
            for (kept, done) in queued {
                let _ = done.send(write(&kept));
            }
        });
        Self(saves)
    }

    /// Queues `kept` behind the saves before it; the receiver hears how its write went.
    pub fn save(&self, kept: T) -> mpsc::Receiver<Result<(), String>> {
        let (done, result) = mpsc::channel();
        let _ = self.0.send((kept, done));
        result
    }
}

/// The saves of a kept file: a save waiting out a burst of changes goes only for the last of them. None
/// are written when the file read at start could not be read and is still there, so it is never written
/// over this session.
pub struct Saves<T> {
    saver: Option<Saver<T>>,
    /// Counts the changes.
    changes: u64,
}

impl<T: Clone + Send + 'static> Saves<T> {
    pub fn new(saver: Option<Saver<T>>) -> Self {
        Self { saver, changes: 0 }
    }

    /// Notes a change: the number a save of it waits with, none when nothing is written.
    pub fn changed(&mut self) -> Option<u64> {
        self.changes += 1;
        self.saver.as_ref().map(|_| self.changes)
    }

    /// Queues `kept` unless a change came after `change`, which has its own save coming.
    pub fn save_if_last(&self, change: u64, kept: &T) -> Option<mpsc::Receiver<Result<(), String>>> {
        let saver = self.saver.as_ref().filter(|_| self.changes == change)?;
        Some(saver.save(kept.clone()))
    }

    /// Queues `kept` now, dropping the saves still waiting out a burst.
    pub fn save_now(&mut self, kept: &T) -> Option<mpsc::Receiver<Result<(), String>>> {
        self.changes += 1;
        self.saver.as_ref().map(|saver| saver.save(kept.clone()))
    }
}

/// How a save went, once it is written; a saver that stopped never says.
pub fn written(result: mpsc::Receiver<Result<(), String>>) -> Result<(), String> {
    result.recv().unwrap_or_else(|_| Err("設定を保存できません".into()))
}

/// `%APPDATA%\Colet\settings.json`.
pub fn path() -> Option<PathBuf> {
    kept_path("settings.json")
}

/// The settings kept, or the defaults when there are none yet.
pub fn load() -> Result<Settings, LoadError> {
    let Some(path) = path() else { return Ok(Settings::default()) };
    load_from(&path)
}

fn load_from(path: &Path) -> Result<Settings, LoadError> {
    read(path, &SETTINGS).map(|mut settings: Settings| {
        settings.normalize();
        settings
    })
}

/// What `path` holds, or the defaults when it is not there yet. A file that does not parse is put aside,
/// so the defaults written next do not take its place unseen.
pub fn read<T: serde::de::DeserializeOwned + Default>(path: &Path, file: &File) -> Result<T, LoadError> {
    let kept = |e: &dyn std::fmt::Display| LoadError {
        message: format!("{}を読み込めません: {e}。元の{}を残すため、変更は保存しません。", file.what, file.file),
        kept: true,
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(T::default()),
        Err(e) => return Err(kept(&e)),
    };
    match serde_json::from_slice::<T>(&bytes) {
        Ok(read) => Ok(read),
        Err(e) => {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let aside = path.with_file_name(format!("{}.broken-{stamp}.json", file.stem));
            match std::fs::rename(path, &aside) {
                Ok(()) => Err(LoadError {
                    message: format!(
                        "{}が壊れていたため、{}で始めました（{e}）。元のファイルは{}に残しています。",
                        file.file,
                        file.fallback,
                        aside.display()
                    ),
                    kept: false,
                }),
                Err(_) => Err(kept(&e)),
            }
        }
    }
}

/// Writes beside the file and then over it, so a failed write leaves the old one whole.
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or("設定の保存先がわかりません")?;
    save_to(settings, &path)
}

fn save_to(settings: &Settings, path: &Path) -> Result<(), String> {
    write(settings, path, &SETTINGS)
}

/// Writes `kept` beside `path` and then over it, so a failed write leaves the old one whole.
pub fn write<T: Serialize>(kept: &T, path: &Path, file: &File) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}を保存できません: {e}", file.what);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(fail)?;
    let temporary = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(kept).map_err(|e| e.to_string())?;
    std::fs::write(&temporary, json).map_err(fail)?;
    std::fs::rename(&temporary, path).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finishes_an_in_flight_save_before_the_final_one() {
        use std::sync::Mutex;
        use std::time::Duration;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (started, entered) = mpsc::channel();
        let (release, released) = mpsc::channel::<()>();
        let gate = Mutex::new((started, released));
        let target = path.clone();
        let saver = Saver::new(move |settings: &Settings| {
            if settings.favorites == [Favorite::new("older")] {
                let gate = gate.lock().unwrap();
                gate.0.send(()).unwrap();
                gate.1.recv().unwrap();
            }
            save_to(settings, &target)
        });
        let older = saver.save(Settings { favorites: vec!["older".into()], ..Settings::default() });
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        // The older save is in the middle of its write; the final one waits behind it.
        let latest = saver.save(Settings { favorites: vec!["latest".into()], ..Settings::default() });
        assert!(latest.try_recv().is_err());
        release.send(()).unwrap();
        written(older).unwrap();
        written(latest).unwrap();
        assert_eq!(load_from(&path).unwrap().favorites, [Favorite::new("latest")]);
    }

    #[test]
    fn fills_in_what_a_partial_file_lacks() {
        let settings: Settings = serde_json::from_str(r#"{"column_width":300,"favorites":["D:\\"]}"#).unwrap();
        assert_eq!(settings.column_width, 300.);
        assert_eq!(settings.sidebar_width, 240.);
        assert_eq!(settings.favorites, [Favorite::new(r"D:\")]);
        assert!(settings.saved_searches.is_empty());
        // One written before the theme could be chosen follows Windows.
        assert_eq!(settings.theme, Theme::System);
    }

    #[test]
    fn keeps_a_favorite_whose_path_is_not_text_as_it_is() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        let settings = Settings { favorites: vec![Favorite::new(r"D:\b"), odd.clone().into()], ..Settings::default() };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""favorites":[{"path":"D:\\b","name":""},{"path":{"utf16":[68,58,92,97,55296]},"name":""}]"#), "{json}");
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap().favorites, [Favorite::new(r"D:\b"), Favorite::new(odd)]);
        // One read neither way is left out alone.
        let settings: Settings = serde_json::from_str(r#"{"favorites":[1,"D:\\c"]}"#).unwrap();
        assert_eq!(settings.favorites, [Favorite::new(r"D:\c")]);
    }

    #[test]
    fn reads_a_favorite_written_as_its_path_alone_and_writes_each_whole() {
        let settings: Settings =
            serde_json::from_str(r#"{"favorites":["D:\\a",{"path":"D:\\b","name":"書類","narrow":"pdf"},{"name":"no path"}]}"#).unwrap();
        let narrowed = Favorite { path: r"D:\b".into(), name: "書類".into(), narrow: Some("pdf".into()) };
        assert_eq!(settings.favorites, [Favorite::new(r"D:\a"), narrowed]);
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""favorites":[{"path":"D:\\a","name":""},{"path":"D:\\b","name":"書類","narrow":"pdf"}]"#), "{json}");
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap().favorites, settings.favorites);
    }

    #[test]
    fn keeps_one_favorite_and_saved_search_of_each_narrowing_as_it_would_be_typed_again() {
        let favorite = |path: &str, narrow: Option<&str>| Favorite { narrow: narrow.map(Into::into), ..Favorite::new(path) };
        let search = |narrow: Option<&str>| SavedSearch { name: String::new(), query: "cat".into(), filter: None, narrow: narrow.map(Into::into) };
        let mut settings = Settings {
            favorites: vec![
                favorite(r"D:\Work", None),
                favorite(r"D:\Work", Some("  a   b ")),
                favorite(r"d:\work", Some("a b")),
                favorite(r"D:\Work", Some("A b")),
                favorite(r"D:\Work", Some("\"x  y\"")),
                favorite(r"D:\Work", Some("  ")),
            ],
            saved_searches: vec![search(Some(" a")), search(Some("a")), search(None), search(Some(""))],
            pinned: vec![
                favorite_key(Path::new(r"D:\Work"), Some("  a   b ")),
                favorite_key(Path::new(r"D:\Work"), Some("gone")),
                search_key("cat", None, Some(" a")),
            ],
            ..Settings::default()
        };
        settings.normalize();
        let narrows: Vec<Option<&str>> = settings.favorites.iter().map(|f| f.narrow.as_deref()).collect();
        assert_eq!(narrows, [None, Some("a b"), Some("A b"), Some("\"x  y\"")]);
        assert_eq!(settings.saved_searches, [search(Some("a")), search(None)]);
        // A pin follows its row's narrowing as it is kept, and goes with a narrowed favorite gone.
        assert_eq!(settings.pinned, [favorite_key(Path::new(r"D:\Work"), Some("a b")), search_key("cat", None, Some("a"))]);
        assert!(settings.is_favorite(&favorite_key(Path::new(r"d:\WORK"), Some("a b"))));
        assert!(!settings.is_favorite(&favorite_key(Path::new(r"D:\Work"), Some("a"))));
        assert!(settings.is_pinned(&favorite_key(Path::new(r"d:\work"), Some("a b"))));
    }

    #[test]
    fn reads_a_file_that_still_names_an_everything_instance() {
        let settings: Settings = serde_json::from_str(r#"{"instance":"1.5a","favorites":["D:\\"]}"#).unwrap();
        assert_eq!(settings.favorites, [Favorite::new(r"D:\")]);
        assert!(!serde_json::to_string(&settings).unwrap().contains("instance"));
    }

    #[test]
    fn keeps_the_theme_chosen() {
        let settings: Settings = serde_json::from_str(r#"{"theme":"dark"}"#).unwrap();
        assert_eq!(settings.theme, Theme::Dark);
        let written = serde_json::to_string(&Settings { theme: Theme::Light, ..Settings::default() }).unwrap();
        assert!(written.contains(r#""theme":"light""#), "{written}");
    }

    #[test]
    fn keeps_the_sort_order_chosen() {
        let order = Order { by: Sort::Modified, descending: true };
        let written = serde_json::to_string(&Settings { sort: order, ..Settings::default() }).unwrap();
        assert!(written.contains(r#""sort":{"by":"modified","descending":true}"#), "{written}");
        assert_eq!(serde_json::from_str::<Settings>(&written).unwrap().sort, order);
    }

    #[test]
    fn sorts_by_name_from_a_to_z_when_the_file_keeps_no_order() {
        let settings: Settings = serde_json::from_str(r#"{"favorites":["D:\\"]}"#).unwrap();
        assert_eq!(settings.sort, Order::default());
        assert_eq!(settings.favorites, [Favorite::new(r"D:\")]);
    }

    #[test]
    fn puts_back_only_what_it_cannot_read_of_the_sort_order() {
        let sort = |json: &str| serde_json::from_str::<Settings>(json).unwrap().sort;
        assert_eq!(sort(r#"{"sort":{"by":"size"}}"#), Order { by: Sort::Size, descending: false });
        assert_eq!(sort(r#"{"sort":{"descending":true}}"#), Order { by: Sort::Name, descending: true });
        assert_eq!(sort(r#"{"sort":{"by":"type","descending":true}}"#), Order { by: Sort::Name, descending: true });
        assert_eq!(sort(r#"{"sort":{"by":"size","descending":"yes"}}"#), Order { by: Sort::Size, descending: false });
        assert_eq!(sort(r#"{"sort":"size"}"#), Order::default());
        assert_eq!(sort(r#"{"sort":null}"#), Order::default());
        // The rest of the file is read as ever.
        let settings: Settings = serde_json::from_str(r#"{"sort":3,"theme":"dark"}"#).unwrap();
        assert_eq!((settings.sort, settings.theme), (Order::default(), Theme::Dark));
    }

    #[test]
    fn keeps_widths_whole_and_within_bounds() {
        let mut settings = Settings { column_width: 9000., search_width: 100.4, sidebar_width: 200.6, ..Settings::default() };
        settings.normalize();
        assert_eq!((settings.column_width, settings.search_width, settings.sidebar_width), (640., 260., 201.));
    }

    #[test]
    fn keeps_one_of_each_saved_search_and_favorite_and_none_empty() {
        let search = |name: &str, query: &str| SavedSearch { name: name.into(), query: query.into(), filter: None, narrow: None };
        let mut settings = Settings {
            saved_searches: vec![search("PDF", "ext:pdf"), search("", "  "), search("again", "ext:pdf"), search("", "dm:today")],
            favorites: vec![r"D:\Work".into(), r"d:\work".into(), Favorite::default()],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.saved_searches, [search("PDF", "ext:pdf"), search("", "dm:today")]);
        assert_eq!(settings.favorites, [Favorite::new(r"D:\Work")]);
        assert_eq!(settings.saved_searches[1].label(), "dm:today");
    }

    #[test]
    fn keeps_one_pin_of_each_row_and_none_for_a_search_not_saved() {
        let search = |query: &str, filter: Option<&str>| SavedSearch { name: String::new(), query: query.into(), filter: filter.map(Into::into), narrow: None };
        let mut settings = Settings {
            filters: vec![SearchFilter { name: "画像".into(), query: "ext:jpg".into() }],
            saved_searches: vec![search("cat", None), search("dog", Some("動画")), search("cow", Some("画像"))],
            pinned: vec![
                r"D:\Work".into(),
                r"d:\work".into(),
                search_key("gone", None, None),
                search_key("cat", None, None),
                search_key("CAT", None, None),
                // Its filter gone, the search it is now keeps the pin.
                search_key("dog", Some("動画"), None),
                search_key("cow", Some("画像"), None),
                PathBuf::new(),
            ],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(
            settings.pinned,
            [PathBuf::from(r"D:\Work"), search_key("cat", None, None), search_key("dog", None, None), search_key("cow", Some("画像"), None)]
        );
        assert!(settings.is_pinned(Path::new(r"d:\WORK")) && !settings.is_pinned(&search_key("Cat", None, None)));
        let written = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&written).unwrap().pinned, settings.pinned);
    }

    #[test]
    fn keeps_one_filter_of_each_name_and_lets_go_of_the_names_of_those_gone() {
        let filter = |name: &str, query: &str| SearchFilter { name: name.into(), query: query.into() };
        let search = |query: &str, filter: Option<&str>| SavedSearch { name: String::new(), query: query.into(), filter: filter.map(Into::into), narrow: None };
        let mut settings = Settings {
            filters: vec![filter("画像", "ext:jpg;png"), filter("", "ext:mp3"), filter("動画", " "), filter("画像", "ext:gif")],
            tabs: vec![KeptTab { filter: Some("動画".into()), ..KeptTab::default() }],
            saved_searches: vec![search("cat", Some("画像")), search("cat", None), search("cat", Some("動画")), search("cat", Some("画像"))],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.filters, [filter("画像", "ext:jpg;png")]);
        assert_eq!(settings.tabs[0].filter, None);
        // The one saved with a filter gone is the one saved with none, already there.
        assert_eq!(settings.saved_searches, [search("cat", Some("画像")), search("cat", None)]);
        assert!(settings.saved("cat", Some("画像"), None).is_some());
        assert_eq!(settings.filter_query("画像"), Some("ext:jpg;png"));
    }

    #[test]
    fn keeps_the_filters_and_the_one_chosen() {
        let settings = Settings {
            filters: vec![SearchFilter { name: "画像".into(), query: "ext:jpg".into() }],
            tabs: vec![KeptTab { filter: Some("画像".into()), ..KeptTab::default() }],
            saved_searches: vec![SavedSearch { name: String::new(), query: "cat".into(), filter: None, narrow: None }],
            ..Settings::default()
        };
        let written = serde_json::to_string(&settings).unwrap();
        // One saved with no filter is written as before.
        assert!(written.contains(r#"{"name":"","query":"cat"}"#), "{written}");
        assert_eq!(serde_json::from_str::<Settings>(&written).unwrap(), settings);
    }

    #[test]
    fn asks_everything_for_the_query_within_the_filter() {
        assert_eq!(everything_query("cat", None), "cat");
        assert_eq!(everything_query("cat | dog", Some("ext:jpg;png")), "<ext:jpg;png> <cat | dog>");
        assert_eq!(everything_query(" ", Some("ext:jpg")), "<ext:jpg>");
    }

    #[test]
    fn closes_a_quote_left_open_inside_its_group() {
        assert_eq!(everything_query("\"my doc", Some("ext:txt")), "<ext:txt> <\"my doc\">");
        assert_eq!(everything_query("\"my doc\"", Some("ext:txt")), "<ext:txt> <\"my doc\">");
        assert_eq!(everything_query("cat", Some("path:\"D:\\My Pictures")), "<path:\"D:\\My Pictures\"> <cat>");
        assert_eq!(everything_query("", Some("path:\"D:\\a")), "<path:\"D:\\a\">");
        // Without a filter Everything is asked what was typed, and closes the quote itself.
        assert_eq!(everything_query("\"my doc", None), "\"my doc");
    }

    #[test]
    fn narrows_the_search_to_names_holding_the_text_as_it_is() {
        assert_eq!(narrowed_query("cat | dog", "a\u{3000}b"), "<cat | dog> nopath:nowildcards:\"a\" nopath:nowildcards:\"b\"");
        assert_eq!(narrowed_query("\"my doc", "*"), "<\"my doc\"> nopath:nowildcards:\"*\"");
        assert_eq!(narrowed_query("cat", "\"a b\" c|d"), "<cat> nopath:nowildcards:\"a b\" nopath:nowildcards:\"c|d\"");
    }

    #[test]
    fn puts_a_broken_file_aside_rather_than_have_it_written_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(load_from(&path), Ok(Settings::default()));
        std::fs::write(&path, r#"{"favorites":["D:\\Work"],"#).unwrap();
        let error = load_from(&path).unwrap_err();
        assert!(!error.kept, "{error:?}");
        assert!(!path.exists());
        let aside: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().path()).collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(std::fs::read_to_string(&aside[0]).unwrap(), r#"{"favorites":["D:\\Work"],"#);
        assert!(error.message.contains(&aside[0].display().to_string()), "{}", error.message);
    }

    #[test]
    fn keeps_a_file_it_cannot_read_from_being_written_over() {
        let dir = tempfile::tempdir().unwrap();
        // A folder where the file should be cannot be read as one.
        let error = load_from(dir.path()).unwrap_err();
        assert!(error.kept, "{error:?}");
    }

    #[test]
    fn forgets_a_window_too_small_to_use() {
        let mut settings = Settings {
            window: Some(Placement { x: 0., y: 0., width: 10., height: 10., maximized: false }),
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.window, None);
    }
}
