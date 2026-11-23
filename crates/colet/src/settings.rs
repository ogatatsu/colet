//! What Colet keeps between runs: the widths, the favorites, the saved searches, what is pinned in home,
//! the search filters, the tabs with what each searched and the folders open, the theme, the language,
//! whether the hidden items show, and where the window was. The way back and forth and what is picked
//! besides are not kept.
//!
//! How a kept file is read and written is [`crate::kept_file`]'s; a tab's place in the file is [`tab`]'s.
mod tab;

use std::path::{Path, PathBuf};

use colet_core::{
    entry::Order,
    paths::{folder_name, path_key, shown},
};
use serde::{Deserialize, Serialize};

pub use self::tab::KeptTab;
use crate::words::{self, Spoken};
use crate::{
    app::{COLUMN_WIDTHS, SEARCH_FIELD_WIDTH, SEARCH_FIELD_WIDTHS, SIDEBAR_WIDTHS},
    kept_file::{self, Fields, File, Lenient, Loaded, kept_path, lenient},
    kept_path::KeptPath,
    state::{HomeKey, narrowing},
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
    /// The order its results were listed in as it was saved; the default is left out, so one saved before
    /// orders were reads as listed by it.
    #[serde(skip_serializing_if = "is_default_order")]
    pub order: Order,
}

fn is_default_order(order: &Order) -> bool {
    *order == Order::default()
}

impl SavedSearch {
    pub fn label(&self) -> &str {
        if self.name.is_empty() { &self.query } else { &self.name }
    }

    /// Its row's key in home.
    pub fn key(&self) -> PathBuf {
        HomeKey::search(&self.query, self.filter.as_deref(), self.narrow.as_deref(), self.order).key()
    }
}

/// A folder kept in home, narrowed to some names or not. An empty name shows the folder's own.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Favorite {
    #[serde(with = "crate::kept_path")]
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
            Theme::System => words::preferences::system(),
            Theme::Light => words::preferences::theme_light(),
            Theme::Dark => words::preferences::theme_dark(),
        }
    }
}

/// The language Colet speaks: Windows's, Japanese where Windows shows itself in it and English elsewhere,
/// or one of its own whatever Windows shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "ja")]
    Japanese,
    #[serde(rename = "en")]
    English,
}

impl Language {
    /// In the order the settings offer them.
    pub const ALL: [Language; 3] = [Language::System, Language::Japanese, Language::English];

    /// Each language by its own name, so one picked by mistake can be found again without reading it.
    pub fn label(self) -> &'static str {
        match self {
            Language::System => words::preferences::system(),
            Language::Japanese => "日本語",
            Language::English => "English",
        }
    }

    /// What it speaks, `windows` being Windows's.
    pub fn spoken(self, windows: Spoken) -> Spoken {
        match self {
            Language::System => windows,
            Language::Japanese => Spoken::Japanese,
            Language::English => Spoken::English,
        }
    }
}

/// The sizes the settings offer to draw everything at, in percent, in the order they offer them.
pub const ZOOMS: [u16; 7] = [90, 100, 110, 125, 150, 175, 200];

/// The offered size nearest `zoom`, the smaller of two as near, for one written by hand.
pub fn nearest_zoom(zoom: u16) -> u16 {
    ZOOMS.into_iter().min_by_key(|offered| offered.abs_diff(zoom)).unwrap_or(100)
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

/// Read as [`Lenient::from_json`] reads it, one field and one item at a time.
#[derive(Clone, Debug, PartialEq, Serialize)]
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
    #[serde(serialize_with = "crate::kept_path::serialize_each")]
    pub pinned: Vec<PathBuf>,
    pub filters: Vec<SearchFilter>,
    /// The filter, by name, a tab opened afresh is to search within; with none, none.
    pub default_filter: Option<String>,
    /// The tabs, in their order, and the one in front.
    pub tabs: Vec<KeptTab>,
    pub tab: usize,
    pub theme: Theme,
    pub language: Language,
    /// How large everything is drawn, in percent: one of [`ZOOMS`].
    pub zoom: u16,
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
            default_filter: None,
            tabs: Vec::new(),
            tab: 0,
            theme: Theme::System,
            language: Language::System,
            zoom: 100,
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
        self.zoom = nearest_zoom(self.zoom);
        // A filter is found by its name, a search by its query, filter and narrowing, and a favorite by its
        // path and narrowing: one of each, none empty. What names a filter gone goes without it. A
        // narrowing is kept as one is typed again, with no word none.
        let mut names = std::collections::HashSet::new();
        self.filters.retain(|f| !f.name.trim().is_empty() && !f.query.trim().is_empty() && names.insert(f.name.clone()));
        let known = |name: &Option<String>| name.as_ref().is_none_or(|name| names.contains(name));
        if !known(&self.default_filter) {
            self.default_filter = None;
        }
        for tab in &mut self.tabs {
            tab.place.without_filter_alone();
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
        self.saved_searches.retain(|s| !s.query.trim().is_empty() && seen.insert(s.key()));
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

    pub fn saved(&self, query: &str, filter: Option<&str>, narrow: Option<&str>, order: Order) -> Option<&SavedSearch> {
        self.saved_searches
            .iter()
            .find(|s| s.query == query && s.filter.as_deref() == filter && s.narrow.as_deref() == narrow && s.order == order)
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
}

impl Lenient for Settings {
    fn from_json(fields: &mut Fields) -> Self {
        let d = Settings::default();
        let tabs = fields.each::<KeptTab>("tabs");
        // The tab in front stays in front of those that are left.
        let tab = fields.one("tab").unwrap_or(d.tab);
        let tab = tab - tabs.iter().take(tab).filter(|tab| tab.is_none()).count();
        Settings {
            column_width: fields.one("column_width").unwrap_or(d.column_width),
            search_width: fields.one("search_width").unwrap_or(d.search_width),
            sidebar_width: fields.one("sidebar_width").unwrap_or(d.sidebar_width),
            search_field_width: fields.one("search_field_width").unwrap_or(d.search_field_width),
            favorites: fields.many("favorites"),
            saved_searches: fields.many("saved_searches"),
            pinned: fields.many::<KeptPath>("pinned").into_iter().map(|pin| pin.0).collect(),
            filters: fields.many("filters"),
            default_filter: fields.one("default_filter").unwrap_or(d.default_filter),
            tabs: tabs.into_iter().flatten().collect(),
            tab,
            theme: fields.one("theme").unwrap_or(d.theme),
            language: fields.one("language").unwrap_or(d.language),
            zoom: fields.one("zoom").unwrap_or(d.zoom),
            show_hidden: fields.one("show_hidden").unwrap_or(d.show_hidden),
            show_protected: fields.one("show_protected").unwrap_or(d.show_protected),
            window: fields.one("window").unwrap_or(d.window),
        }
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        lenient(deserializer)
    }
}

pub const SETTINGS: File =
    File { stem: "settings", what: words::kept::settings, file: words::kept::settings_file, fallback: words::kept::settings_fallback };

/// `%APPDATA%\Colet\settings.json`.
pub fn path() -> Option<PathBuf> {
    kept_path("settings.json")
}

/// The settings kept, or the defaults when there are none yet.
pub fn load() -> Loaded<Settings> {
    let Some(path) = path() else { return Loaded::read(Settings::default()) };
    load_from(&path)
}

fn load_from(path: &Path) -> Loaded<Settings> {
    let mut loaded = kept_file::read::<Settings>(path, &SETTINGS);
    loaded.value.normalize();
    loaded
}

/// Writes beside the file and then over it, so a failed write leaves the old one whole.
pub fn save(settings: &Settings) -> Result<(), String> {
    let path = path().ok_or(words::kept::no_settings_place())?;
    save_to(settings, &path)
}

fn save_to(settings: &Settings, path: &Path) -> Result<(), String> {
    kept_file::write(settings, path, &SETTINGS)
}

#[cfg(test)]
mod tests {
    use colet_core::entry::Sort;

    use std::sync::mpsc;

    use super::*;
    use crate::{
        kept_file::Saves,
        state::{Fold, Kept},
    };

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
        let mut saves = Saves::new(
            &SETTINGS,
            Some(move |settings: &Settings| {
                if settings.favorites == [Favorite::new("older")] {
                    let gate = gate.lock().unwrap();
                    gate.0.send(()).unwrap();
                    gate.1.recv().unwrap();
                }
                save_to(settings, &target)
            }),
        );
        let older = saves.save_now(&Settings { favorites: vec!["older".into()], ..Settings::default() }).unwrap();
        entered.recv_timeout(Duration::from_secs(5)).unwrap();
        // The older save is in the middle of its write; the final one waits behind it.
        let latest = saves.save_now(&Settings { favorites: vec!["latest".into()], ..Settings::default() }).unwrap();
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
    fn keeps_what_names_a_path_that_is_not_text_by_its_units() {
        use std::os::windows::ffi::OsStringExt;
        let wide: Vec<u16> = r"D:\a".encode_utf16().chain([0xD800]).collect();
        let odd = PathBuf::from(std::ffi::OsString::from_wide(&wide));
        let narrowed = HomeKey::favorite(&odd, Some("x")).key();
        let place = Kept {
            folders: vec![(r"D:\".into(), Some(odd.clone())), (odd.clone(), None)],
            picks: vec![Some(odd.clone()), None],
            ..Kept::default()
        };
        let settings = Settings {
            favorites: vec![Favorite::new(r"D:\b"), odd.clone().into(), Favorite { narrow: Some("x".into()), ..odd.clone().into() }],
            pinned: vec![odd.clone(), r"D:\b".into(), narrowed],
            tabs: vec![KeptTab { place, filter: None }, KeptTab::default()],
            tab: 1,
            ..Settings::default()
        };
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        save_to(&settings, &path).unwrap();
        let written = std::fs::read_to_string(&path).unwrap();
        // What is text is written as before.
        assert!(written.contains(r#""path": "D:\\b""#), "{written}");
        assert_eq!(load_from(&path).unwrap(), settings);
    }

    #[test]
    fn reads_a_file_without_what_of_it_cannot_be_read_and_copies_it_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let file = r#"{
            "column_width": "wide", "theme": "purple", "sidebar_width": 300,
            "favorites": ["D:\\old", {"path": "D:\\a"}, {"path": 123}, {"path": {"utf16": "x"}}],
            "pinned": ["D:\\a", {"utf16": [70000]}],
            "saved_searches": [{"query": "cat"}, {"query": 5}],
            "filters": {"name": "画像"},
            "tabs": [{"folders": [{"path": 5}]}, {"query": "dog"}, {"query": "cow"}],
            "tab": 2
        }"#;
        std::fs::write(&path, file).unwrap();
        let loaded = load_from(&path);
        let problem = loaded.problem.clone().unwrap();
        assert!(!problem.kept, "{problem:?}");
        assert!(problem.message.starts_with("設定ファイルの読めない項目を9件飛ばしました"), "{}", problem.message);
        let read = loaded.value;
        // A field that cannot be read takes its default, alone.
        assert_eq!((read.column_width, read.sidebar_width, read.theme), (320., 300., Theme::System));
        // The favorite an older Colet wrote as a path alone is one that cannot be read.
        assert_eq!(read.favorites, [Favorite::new(r"D:\a")]);
        assert_eq!(read.pinned, [PathBuf::from(r"D:\a")]);
        assert_eq!(read.saved_searches.iter().map(|s| s.query.as_str()).collect::<Vec<_>>(), ["cat"]);
        assert!(read.filters.is_empty());
        // The tab in front stays in front of those left.
        assert_eq!(read.tabs.iter().map(|t| t.place.query.as_str()).collect::<Vec<_>>(), ["dog", "cow"]);
        assert_eq!(read.tab, 1);
        // The file is left as it was, and a copy kept aside, which the message names.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), file);
        let aside: Vec<_> = std::fs::read_dir(dir.path()).unwrap().map(|e| e.unwrap().path()).filter(|p| *p != path).collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(std::fs::read_to_string(&aside[0]).unwrap(), file);
        assert!(problem.message.contains(&aside[0].display().to_string()), "{}", problem.message);
    }

    #[test]
    fn puts_aside_a_file_that_is_not_an_object() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "[]").unwrap();
        let loaded = load_from(&path);
        assert_eq!(loaded.value, Settings::default());
        assert!(!loaded.problem.unwrap().kept);
        assert!(!path.exists());
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
                search_order: Order { by: Sort::Size, descending: true, ..Order::default() },
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
            r#"{"query":"cat","search_order":{"by":"size","descending":true},"filter":"画像","folders":[{"path":"D:\\a","highlight":"D:\\a\\b"},{"path":"D:\\a\\b"}],"picks":[null,"D:\\a"],"active":2,"fold":{"anchor":0,"open":true}}"#
        );
        assert_eq!(serde_json::from_str::<KeptTab>(&json).unwrap(), tab);
        // A tab an older Colet kept searching with a filter alone opens on home, the filter still chosen.
        let mut settings = Settings {
            filters: vec![SearchFilter { name: "画像".into(), query: "ext:jpg".into() }],
            tabs: vec![serde_json::from_str(r#"{"search_filter":"画像","filter":"画像","picks":[null,"D:\\a"],"active":1}"#).unwrap()],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!((settings.tabs[0].place.search_filter.as_deref(), settings.tabs[0].filter.as_deref()), (None, Some("画像")));
        assert_eq!((settings.tabs[0].place.picks.as_slice(), settings.tabs[0].place.active), ([None].as_slice(), 0));
        // A fold whose anchor is not among the folders goes.
        let read: KeptTab = serde_json::from_str(r#"{"folders":[{"path":"D:\\a"}],"fold":{"anchor":1,"open":false}}"#).unwrap();
        assert_eq!(read.place.fold, None);
    }

    #[test]
    fn keeps_one_favorite_and_saved_search_of_each_narrowing_as_it_would_be_typed_again() {
        let favorite = |path: &str, narrow: Option<&str>| Favorite { narrow: narrow.map(Into::into), ..Favorite::new(path) };
        let search = |narrow: Option<&str>| SavedSearch {
            name: String::new(),
            query: "cat".into(),
            filter: None,
            narrow: narrow.map(Into::into),
            order: Order::default(),
        };
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
                HomeKey::search("cat", None, Some(" a"), Order::default()).key(),
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
            [HomeKey::favorite(Path::new(r"D:\Work"), Some("a b")).key(), HomeKey::search("cat", None, Some("a"), Order::default()).key()]
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
        assert_eq!((settings.column_width, settings.search_width, settings.sidebar_width), (960., 260., 201.));
        assert_eq!(settings.search_field_width, 640.);
    }

    #[test]
    fn keeps_the_size_chosen_and_takes_the_nearest_offered_for_one_written_by_hand() {
        // One written before the size could be chosen is drawn at 100%.
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert_eq!(settings.zoom, 100);
        let settings: Settings = serde_json::from_str(r#"{"zoom":150}"#).unwrap();
        assert_eq!(settings.zoom, 150);
        // Not a number: the rest of the file is still read.
        let settings: Settings = serde_json::from_str(r#"{"zoom":"big","sidebar_width":300}"#).unwrap();
        assert_eq!((settings.zoom, settings.sidebar_width), (100, 300.));
        for (written, kept) in [(0, 90), (95, 90), (130, 125), (138, 150), (1000, 200), (175, 175)] {
            let mut settings = Settings { zoom: written, ..Settings::default() };
            settings.normalize();
            assert_eq!(settings.zoom, kept, "{written}");
        }
    }

    #[test]
    fn keeps_one_of_each_saved_search_and_favorite_and_none_empty() {
        let search = |name: &str, query: &str| SavedSearch {
            name: name.into(),
            query: query.into(),
            filter: None,
            narrow: None,
            order: Order::default(),
        };
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
            order: Order::default(),
        };
        let mut settings = Settings {
            filters: vec![SearchFilter { name: "画像".into(), query: "ext:jpg".into() }],
            saved_searches: vec![search("cat", None), search("dog", Some("動画")), search("cow", Some("画像"))],
            pinned: vec![
                r"D:\Work".into(),
                r"d:\work".into(),
                HomeKey::search("gone", None, None, Order::default()).key(),
                HomeKey::search("cat", None, None, Order::default()).key(),
                HomeKey::search("CAT", None, None, Order::default()).key(),
                // Its filter gone, the search it is now keeps the pin.
                HomeKey::search("dog", Some("動画"), None, Order::default()).key(),
                HomeKey::search("cow", Some("画像"), None, Order::default()).key(),
                PathBuf::new(),
            ],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(
            settings.pinned,
            [
                PathBuf::from(r"D:\Work"),
                HomeKey::search("cat", None, None, Order::default()).key(),
                HomeKey::search("dog", None, None, Order::default()).key(),
                HomeKey::search("cow", Some("画像"), None, Order::default()).key()
            ]
        );
        assert!(
            settings.is_pinned(Path::new(r"d:\WORK")) && !settings.is_pinned(&HomeKey::search("Cat", None, None, Order::default()).key())
        );
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
            order: Order::default(),
        };
        let mut settings = Settings {
            filters: vec![filter("画像", "ext:jpg;png"), filter("", "ext:mp3"), filter("動画", " "), filter("画像", "ext:gif")],
            default_filter: Some("動画".into()),
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
        assert_eq!(settings.default_filter, None);
        assert_eq!(settings.tabs[0].filter, None);
        // The one saved with a filter gone is the one saved with none, already there.
        assert_eq!(settings.saved_searches, [search("cat", Some("画像")), search("cat", None)]);
        assert!(settings.saved("cat", Some("画像"), None, Order::default()).is_some());
        assert_eq!(settings.filter_query("画像"), Some("ext:jpg;png"));
    }

    #[test]
    fn keeps_the_filters_and_the_one_chosen() {
        let settings = Settings {
            filters: vec![SearchFilter { name: "画像".into(), query: "ext:jpg".into() }],
            tabs: vec![KeptTab { filter: Some("画像".into()), ..KeptTab::default() }],
            saved_searches: vec![SavedSearch {
                name: String::new(),
                query: "cat".into(),
                filter: None,
                narrow: None,
                order: Order::default(),
            }],
            ..Settings::default()
        };
        let written = serde_json::to_string(&settings).unwrap();
        // One saved with no filter is written as before.
        assert!(written.contains(r#"{"name":"","query":"cat"}"#), "{written}");
        assert_eq!(serde_json::from_str::<Settings>(&written).unwrap(), settings);
    }

    #[test]
    fn puts_a_broken_file_aside_rather_than_have_it_written_over() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        assert_eq!(load_from(&path).unwrap(), Settings::default());
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
    fn reads_the_language_by_its_code_and_follows_windows_otherwise() {
        let read = |json: &str| serde_json::from_str::<Settings>(json).unwrap().language;
        assert_eq!(read(r#"{"language":"en"}"#), Language::English);
        assert_eq!(read(r#"{"language":"ja"}"#), Language::Japanese);
        assert_eq!(read(r#"{"language":"fr"}"#), Language::System);
        assert_eq!(read("{}"), Language::System);
        assert_eq!(serde_json::to_value(Language::English).unwrap(), "en");
        assert_eq!(Language::System.spoken(Spoken::English), Spoken::English);
        assert_eq!(Language::Japanese.spoken(Spoken::English), Spoken::Japanese);
    }

    #[test]
    fn forgets_a_window_too_small_to_use() {
        let mut settings =
            Settings { window: Some(Placement { x: 0., y: 0., width: 10., height: 10., maximized: false }), ..Settings::default() };
        settings.normalize();
        assert_eq!(settings.window, None);
    }
}
