//! What Colet keeps between runs: the widths, the favorites, the saved searches, what is pinned in home,
//! the search filters, the tabs with what each searched and the folders open, the theme, whether
//! the hidden items show, and where the window was. The way back and forth and what is picked besides are not kept.
use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use colet_core::paths::{folder_name, path_key, shown};
use serde::{Deserialize, Serialize};

use crate::{
    app::{COLUMN_WIDTHS, SEARCH_FIELD_WIDTH, SEARCH_FIELD_WIDTHS, SIDEBAR_WIDTHS},
    lane::Lane,
    state::{Fold, HomeKey, Kept, narrow_words, narrowing},
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
        HomeKey::search(&self.query, self.filter.as_deref(), self.narrow.as_deref()).key()
    }
}

/// A folder kept in home, narrowed to some names or not. An empty name shows the folder's own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Favorite {
    pub path: PathBuf,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    /// What its row in home stands for.
    pub fn home_key(&self) -> HomeKey<'_> {
        HomeKey::favorite(&self.path, self.narrow.as_deref())
    }

    /// Its row's key in home.
    pub fn key(&self) -> PathBuf {
        self.home_key().key()
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
    let words: String = narrow_words(text).iter().map(|word| format!(" nopath:nowildcards:\"{word}\"")).collect();
    format!("<{}>{words}", closed(query))
}

/// `text` with a quote left open closed, as Everything closes it at the end of the text: inside `< >` it
/// would run on over the `>` and take it for a letter, finding nothing while `"my doc` is being typed.
fn closed(text: &str) -> std::borrow::Cow<'_, str> {
    if text.matches('"').count() % 2 == 1 { format!("{text}\"").into() } else { text.into() }
}

/// The colors Colet takes: Windows's setting, or one of its own whatever Windows says.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
            Theme::System => "システム",
            Theme::Light => "ライト",
            Theme::Dark => "ダーク",
        }
    }
}

/// A tab kept between starts: where it was, and the filter its next search is narrowed by, by name.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(from = "TabFile", into = "TabFile")]
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
    /// The search field in the toolbar.
    pub search_field_width: f32,
    pub favorites: Vec<Favorite>,
    pub saved_searches: Vec<SavedSearch>,
    /// Home's rows pinned above the rest, in their order, by the path each row has: a folder's, or a
    /// narrowed favorite's or a saved search's key.
    pub pinned: Vec<PathBuf>,
    pub filters: Vec<SearchFilter>,
    /// The tabs, in their order, and the one in front.
    pub tabs: Vec<KeptTab>,
    pub tab: usize,
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
            search_field_width: SEARCH_FIELD_WIDTH,
            favorites: Vec::new(),
            saved_searches: Vec::new(),
            pinned: Vec::new(),
            filters: Vec::new(),
            tabs: Vec::new(),
            tab: 0,
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
        self.search_field_width = clamp(self.search_field_width, SEARCH_FIELD_WIDTHS);
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
            let was = favorite.home_key().pin();
            favorite.narrow = favorite.narrow.as_deref().and_then(narrowing);
            if favorite.home_key().pin() == was {
                continue;
            }
            let is = favorite.key();
            for pin in self.pinned.iter_mut().filter(|pin| HomeKey::of(pin).pin() == was) {
                *pin = is.clone();
            }
        }
        let mut seen = std::collections::HashSet::new();
        self.favorites.retain(|f| !f.path.as_os_str().is_empty() && seen.insert(f.home_key().pin()));
        // A pin is one of each row, and goes with the saved search or narrowed favorite it pins.
        let saved: std::collections::HashSet<PathBuf> = self.saved_searches.iter().map(SavedSearch::key).collect();
        let narrowed: std::collections::HashSet<std::ffi::OsString> =
            self.favorites.iter().filter(|f| f.narrow.is_some()).map(|f| f.home_key().pin()).collect();
        let mut seen = std::collections::HashSet::new();
        self.pinned.retain(|pin| {
            let key = HomeKey::of(pin);
            !pin.as_os_str().is_empty()
                && match &key {
                    HomeKey::Search { .. } => saved.contains(pin),
                    HomeKey::Folder { narrow: Some(_), .. } => narrowed.contains(&key.pin()),
                    HomeKey::Folder { narrow: None, .. } => true,
                }
                && seen.insert(key.pin())
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
        let key = HomeKey::of(path).pin();
        self.pinned.iter().any(|pin| HomeKey::of(pin).pin() == key)
    }

    /// Whether `key`, a folder's path or a narrowed favorite's key, is a favorite.
    pub fn is_favorite(&self, key: &Path) -> bool {
        match HomeKey::of(key) {
            HomeKey::Folder { path, narrow } => self.favorite(&path, narrow.as_deref()).is_some(),
            HomeKey::Search { .. } => false,
        }
    }

    /// The settings as they can be written: the file holds a path only as text, so a favorite, pin or tab
    /// with a path that is not (an unpaired surrogate, which Windows allows in a name) is left out.
    fn writable(&self) -> Settings {
        let text = |path: &Path| path.to_str().is_some();
        let tab_text = |tab: &KeptTab| {
            tab.place.folders.iter().all(|(path, highlight)| text(path) && highlight.as_deref().is_none_or(text))
                && tab.place.picks.iter().flatten().all(|pick| text(pick))
        };
        let mut settings = self.clone();
        settings.favorites.retain(|f| text(&f.path));
        settings.pinned.retain(|pin| text(pin));
        settings.tab -= self.tabs.iter().take(self.tab).filter(|tab| !tab_text(tab)).count();
        settings.tabs.retain(tab_text);
        settings.tab = settings.tab.min(settings.tabs.len().saturating_sub(1));
        settings
    }
}

/// A tab as the file holds it: `{"query": ..., "search_filter": ..., "filter": ..., "folders": [{"path": ...,
/// "highlight": ...}], "picks": [...], "active": ..., "fold": {"anchor": ..., "open": ...}}`, what it lacks
/// left out.
#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct TabFile {
    #[serde(skip_serializing_if = "String::is_empty")]
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    search_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    filter: Option<String>,
    folders: Vec<FolderFile>,
    picks: Vec<Option<PathBuf>>,
    active: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    fold: Option<Fold<usize>>,
}

#[derive(Serialize, Deserialize)]
struct FolderFile {
    path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    highlight: Option<PathBuf>,
}

impl From<TabFile> for KeptTab {
    fn from(file: TabFile) -> Self {
        let folders: Vec<_> = file.folders.into_iter().map(|f| (f.path, f.highlight)).collect();
        let place = Kept {
            query: file.query,
            search_filter: file.search_filter,
            fold: file.fold.filter(|fold| fold.anchor < folders.len()),
            folders,
            picks: file.picks,
            active: file.active,
        };
        KeptTab { place, filter: file.filter }
    }
}

impl From<KeptTab> for TabFile {
    fn from(tab: KeptTab) -> Self {
        let place = tab.place;
        TabFile {
            query: place.query,
            search_filter: place.search_filter,
            filter: tab.filter,
            folders: place.folders.into_iter().map(|(path, highlight)| FolderFile { path, highlight }).collect(),
            picks: place.picks,
            active: place.active,
            fold: place.fold,
        }
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

/// Writes a file kept on a lane of its own, one save after another in the order they were sent, so an
/// older save never lands over a newer one: the final save on close waits for those before it.
pub struct Saver<T> {
    lane: Lane,
    write: Arc<Write<T>>,
    /// What the file holds, for saying it could not be saved: "設定".
    what: &'static str,
}

type Write<T> = dyn Fn(&T) -> Result<(), String> + Send + Sync;

impl<T: Send + 'static> Saver<T> {
    pub fn new(file: &File, write: impl Fn(&T) -> Result<(), String> + Send + Sync + 'static) -> Self {
        Self { lane: Lane::default(), write: Arc::new(write), what: file.what }
    }

    /// Queues `kept` behind the saves before it; [`Queued::written`] says how its write went.
    pub fn save(&self, kept: T) -> Queued {
        let write = self.write.clone();
        Queued { result: self.lane.run(move || write(&kept)), what: self.what }
    }
}

/// A save on its way.
pub struct Queued {
    result: mpsc::Receiver<Result<(), String>>,
    what: &'static str,
}

impl Queued {
    /// How the save went, once it is written; a write that broke off never says, and is taken as failed.
    pub fn written(self) -> Result<(), String> {
        self.result.recv().unwrap_or_else(|_| Err(format!("{}を保存できません", self.what)))
    }

    #[cfg(test)]
    fn pending(&self) -> bool {
        matches!(self.result.try_recv(), Err(mpsc::TryRecvError::Empty))
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
    pub fn save_if_last(&self, change: u64, kept: &T) -> Option<Queued> {
        let saver = self.saver.as_ref().filter(|_| self.changes == change)?;
        Some(saver.save(kept.clone()))
    }

    /// Queues `kept` now, dropping the saves still waiting out a burst.
    pub fn save_now(&mut self, kept: &T) -> Option<Queued> {
        self.changes += 1;
        self.saver.as_ref().map(|saver| saver.save(kept.clone()))
    }
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
    write(&settings.writable(), path, &SETTINGS)
}

/// Writes `kept` beside `path` and then over it, so a failed write leaves the old one whole.
pub fn write<T: Serialize>(kept: &T, path: &Path, file: &File) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("{}を保存できません: {e}", file.what);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(fail)?;
    let temporary = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(kept).map_err(|e| e.to_string())?;
    // Flushed to the disk before it takes the old one's place, so a power cut leaves one of them whole.
    let mut written = std::fs::File::create(&temporary).map_err(fail)?;
    written.write_all(&json).map_err(fail)?;
    written.sync_all().map_err(fail)?;
    drop(written);
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
        let saver = Saver::new(&SETTINGS, move |settings: &Settings| {
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
        assert!(latest.pending());
        release.send(()).unwrap();
        older.written().unwrap();
        latest.written().unwrap();
        assert_eq!(load_from(&path).unwrap().favorites, [Favorite::new("latest")]);
    }

    #[test]
    fn fills_in_what_a_partial_file_lacks() {
        let settings: Settings = serde_json::from_str(r#"{"column_width":300,"favorites":[{"path":"D:\\"}]}"#).unwrap();
        assert_eq!(settings.column_width, 300.);
        assert_eq!(settings.sidebar_width, 240.);
        assert_eq!(settings.favorites, [Favorite::new(r"D:\")]);
        assert!(settings.saved_searches.is_empty());
        // One written before the theme could be chosen follows Windows.
        assert_eq!(settings.theme, Theme::System);
    }

    #[test]
    fn leaves_out_what_names_a_path_that_is_not_text_as_it_is_written() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        let tab = |path: &Path| KeptTab { place: Kept { folders: vec![(path.into(), None)], ..Kept::default() }, filter: None };
        let settings = Settings {
            favorites: vec![Favorite::new(r"D:\b"), odd.clone().into()],
            pinned: vec![odd.clone(), r"D:\b".into()],
            tabs: vec![tab(&odd), tab(Path::new(r"D:\b")), tab(Path::new(r"D:\c"))],
            tab: 2,
            ..Settings::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save_to(&settings, &path).unwrap();
        let read = load_from(&path).unwrap();
        assert_eq!(read.favorites, [Favorite::new(r"D:\b")]);
        assert_eq!(read.pinned, [PathBuf::from(r"D:\b")]);
        // The tab in front stays in front.
        assert_eq!(read.tabs, [tab(Path::new(r"D:\b")), tab(Path::new(r"D:\c"))]);
        assert_eq!(read.tab, 1);
    }

    #[test]
    fn writes_each_favorite_whole() {
        let settings: Settings =
            serde_json::from_str(r#"{"favorites":[{"path":"D:\\a"},{"path":"D:\\b","name":"書類","narrow":"pdf"}]}"#).unwrap();
        let narrowed = Favorite { path: r"D:\b".into(), name: "書類".into(), narrow: Some("pdf".into()) };
        assert_eq!(settings.favorites, [Favorite::new(r"D:\a"), narrowed]);
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains(r#""favorites":[{"path":"D:\\a","name":""},{"path":"D:\\b","name":"書類","narrow":"pdf"}]"#), "{json}");
        assert_eq!(serde_json::from_str::<Settings>(&json).unwrap().favorites, settings.favorites);
    }

    #[test]
    fn writes_a_tab_with_what_it_lacks_left_out() {
        let tab = KeptTab {
            place: Kept {
                query: "cat".into(),
                search_filter: None,
                folders: vec![(r"D:\a".into(), Some(r"D:\a\b".into())), (r"D:\a\b".into(), None)],
                picks: vec![None, Some(r"D:\a".into())],
                active: 2,
                fold: Some(Fold { anchor: 0, open: true }),
            },
            filter: Some("画像".into()),
        };
        let json = serde_json::to_string(&tab).unwrap();
        assert_eq!(
            json,
            r#"{"query":"cat","filter":"画像","folders":[{"path":"D:\\a","highlight":"D:\\a\\b"},{"path":"D:\\a\\b"}],"picks":[null,"D:\\a"],"active":2,"fold":{"anchor":0,"open":true}}"#
        );
        assert_eq!(serde_json::from_str::<KeptTab>(&json).unwrap(), tab);
        // A fold whose anchor is not among the folders goes.
        let read: KeptTab = serde_json::from_str(r#"{"folders":[{"path":"D:\\a"}],"fold":{"anchor":1,"open":false}}"#).unwrap();
        assert_eq!(read.place.fold, None);
    }

    #[test]
    fn keeps_one_favorite_and_saved_search_of_each_narrowing_as_it_would_be_typed_again() {
        let favorite = |path: &str, narrow: Option<&str>| Favorite { narrow: narrow.map(Into::into), ..Favorite::new(path) };
        let search =
            |narrow: Option<&str>| SavedSearch { name: String::new(), query: "cat".into(), filter: None, narrow: narrow.map(Into::into) };
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
                HomeKey::favorite(Path::new(r"D:\Work"), Some("  a   b ")).key(),
                HomeKey::favorite(Path::new(r"D:\Work"), Some("gone")).key(),
                HomeKey::search("cat", None, Some(" a")).key(),
            ],
            ..Settings::default()
        };
        settings.normalize();
        let narrows: Vec<Option<&str>> = settings.favorites.iter().map(|f| f.narrow.as_deref()).collect();
        assert_eq!(narrows, [None, Some("a b"), Some("A b"), Some("\"x  y\"")]);
        assert_eq!(settings.saved_searches, [search(Some("a")), search(None)]);
        // A pin follows its row's narrowing as it is kept, and goes with a narrowed favorite gone.
        assert_eq!(
            settings.pinned,
            [HomeKey::favorite(Path::new(r"D:\Work"), Some("a b")).key(), HomeKey::search("cat", None, Some("a")).key()]
        );
        assert!(settings.is_favorite(&HomeKey::favorite(Path::new(r"d:\WORK"), Some("a b")).key()));
        assert!(!settings.is_favorite(&HomeKey::favorite(Path::new(r"D:\Work"), Some("a")).key()));
        assert!(settings.is_pinned(&HomeKey::favorite(Path::new(r"d:\work"), Some("a b")).key()));
    }

    #[test]
    fn reads_a_file_that_still_names_an_everything_instance() {
        let settings: Settings = serde_json::from_str(r#"{"instance":"1.5a","favorites":[{"path":"D:\\"}]}"#).unwrap();
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
    fn drops_the_default_order_a_file_still_keeps() {
        let settings: Settings =
            serde_json::from_str(r#"{"sort":{"by":"modified","descending":true},"favorites":[{"path":"D:\\"}]}"#).unwrap();
        assert_eq!(settings.favorites, [Favorite::new(r"D:\")]);
        let written = serde_json::to_string(&settings).unwrap();
        assert!(!written.contains(r#""sort""#), "{written}");
    }

    #[test]
    fn keeps_widths_whole_and_within_bounds() {
        let mut settings =
            Settings { column_width: 9000., search_width: 100.4, sidebar_width: 200.6, search_field_width: 1000., ..Settings::default() };
        settings.normalize();
        assert_eq!((settings.column_width, settings.search_width, settings.sidebar_width), (640., 260., 201.));
        assert_eq!(settings.search_field_width, 640.);
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
        let search = |query: &str, filter: Option<&str>| SavedSearch {
            name: String::new(),
            query: query.into(),
            filter: filter.map(Into::into),
            narrow: None,
        };
        let mut settings = Settings {
            filters: vec![SearchFilter { name: "画像".into(), query: "ext:jpg".into() }],
            saved_searches: vec![search("cat", None), search("dog", Some("動画")), search("cow", Some("画像"))],
            pinned: vec![
                r"D:\Work".into(),
                r"d:\work".into(),
                HomeKey::search("gone", None, None).key(),
                HomeKey::search("cat", None, None).key(),
                HomeKey::search("CAT", None, None).key(),
                // Its filter gone, the search it is now keeps the pin.
                HomeKey::search("dog", Some("動画"), None).key(),
                HomeKey::search("cow", Some("画像"), None).key(),
                PathBuf::new(),
            ],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(
            settings.pinned,
            [
                PathBuf::from(r"D:\Work"),
                HomeKey::search("cat", None, None).key(),
                HomeKey::search("dog", None, None).key(),
                HomeKey::search("cow", Some("画像"), None).key()
            ]
        );
        assert!(settings.is_pinned(Path::new(r"d:\WORK")) && !settings.is_pinned(&HomeKey::search("Cat", None, None).key()));
        let written = serde_json::to_string(&settings).unwrap();
        assert_eq!(serde_json::from_str::<Settings>(&written).unwrap().pinned, settings.pinned);
    }

    #[test]
    fn keeps_one_filter_of_each_name_and_lets_go_of_the_names_of_those_gone() {
        let filter = |name: &str, query: &str| SearchFilter { name: name.into(), query: query.into() };
        let search = |query: &str, filter: Option<&str>| SavedSearch {
            name: String::new(),
            query: query.into(),
            filter: filter.map(Into::into),
            narrow: None,
        };
        let mut settings = Settings {
            filters: vec![filter("画像", "ext:jpg;png"), filter("", "ext:mp3"), filter("動画", " "), filter("画像", "ext:gif")],
            tabs: vec![KeptTab { filter: Some("動画".into()), ..KeptTab::default() }],
            saved_searches: vec![
                search("cat", Some("画像")),
                search("cat", None),
                search("cat", Some("動画")),
                search("cat", Some("画像")),
            ],
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
        let mut settings =
            Settings { window: Some(Placement { x: 0., y: 0., width: 10., height: 10., maximized: false }), ..Settings::default() };
        settings.normalize();
        assert_eq!(settings.window, None);
    }
}
