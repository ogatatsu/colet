//! What Colet keeps between runs: the widths, the favorites, the saved searches, the sort order, which
//! Everything to ask, the theme, and where the window was. Searches typed, history and the columns open
//! are not kept.
use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    time::{SystemTime, UNIX_EPOCH},
};

use colet_core::entry::{Order, Sort};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::app::{COLUMN_WIDTHS, SIDEBAR_WIDTHS};

/// A search kept in home. An empty name shows the query itself.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedSearch {
    pub name: String,
    pub query: String,
}

impl SavedSearch {
    pub fn label(&self) -> &str {
        if self.name.is_empty() { &self.query } else { &self.name }
    }
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
    pub favorites: Vec<String>,
    pub saved_searches: Vec<SavedSearch>,
    /// The one order the folders and the search results share.
    #[serde(with = "order")]
    pub sort: Order,
    pub theme: Theme,
    pub window: Option<Placement>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            column_width: 288.,
            search_width: 288.,
            sidebar_width: 240.,
            favorites: Vec::new(),
            saved_searches: Vec::new(),
            sort: Order::default(),
            theme: Theme::System,
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
        // A search is found by its query, and a favorite by its path: one of each, none empty.
        let mut seen = std::collections::HashSet::new();
        self.saved_searches.retain(|s| !s.query.trim().is_empty() && seen.insert(s.query.clone()));
        let mut seen = std::collections::HashSet::new();
        self.favorites.retain(|f| !f.is_empty() && seen.insert(colet_core::paths::path_key(f)));
        if self.window.is_some_and(|w| !(w.width >= 200. && w.height >= 150.)) {
            self.window = None;
        }
    }

    pub fn saved(&self, query: &str) -> Option<&SavedSearch> {
        self.saved_searches.iter().find(|s| s.query == query)
    }

    pub fn is_favorite(&self, path: &str) -> bool {
        let key = colet_core::paths::path_key(path);
        self.favorites.iter().any(|f| colet_core::paths::path_key(f) == key)
    }
}

/// The sort order as `{"by": "size", "descending": true}`, read leniently: what is missing or not known
/// (a file edited by hand, or written by a later Colet) goes back to its default alone, rather than have
/// the whole file taken for broken.
mod order {
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

/// `%APPDATA%\Colet\settings.json`.
pub fn path() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("Colet").join("settings.json"))
}

/// Why the settings kept could not be used; Colet starts on the defaults either way.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadError {
    pub message: String,
    /// The file is still where it was, unread: writing the defaults over it would lose it, so nothing is
    /// saved until Colet is started again.
    pub kept: bool,
}

type Done = mpsc::Sender<Result<(), String>>;

/// Writes the settings on a thread of its own, one save after another in the order they were sent, so
/// an older save never lands over a newer one: the final save on close waits for those before it.
pub struct Saver(mpsc::Sender<(Settings, Done)>);

impl Saver {
    pub fn new(write: impl Fn(&Settings) -> Result<(), String> + Send + 'static) -> Self {
        let (saves, queued) = mpsc::channel::<(Settings, Done)>();
        // Ends once the saver is dropped and the saves sent before are written.
        std::thread::spawn(move || {
            for (settings, done) in queued {
                let _ = done.send(write(&settings));
            }
        });
        Self(saves)
    }

    /// Queues `settings` behind the saves before it; the receiver hears how its write went.
    pub fn save(&self, settings: Settings) -> mpsc::Receiver<Result<(), String>> {
        let (done, result) = mpsc::channel();
        let _ = self.0.send((settings, done));
        result
    }
}

/// How a save went, once it is written; a saver that stopped never says.
pub fn written(result: mpsc::Receiver<Result<(), String>>) -> Result<(), String> {
    result.recv().unwrap_or_else(|_| Err("設定を保存できません".into()))
}

/// The settings kept, or the defaults when there are none yet.
pub fn load() -> Result<Settings, LoadError> {
    let Some(path) = path() else { return Ok(Settings::default()) };
    load_from(&path)
}

/// A file that does not parse is put aside, so the defaults written next do not take its place unseen.
fn load_from(path: &Path) -> Result<Settings, LoadError> {
    let kept = |e: &dyn std::fmt::Display| LoadError {
        message: format!("設定を読み込めません: {e}。元の設定ファイルを残すため、変更は保存しません。"),
        kept: true,
    };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(e) => return Err(kept(&e)),
    };
    match serde_json::from_slice::<Settings>(&bytes) {
        Ok(mut settings) => {
            settings.normalize();
            Ok(settings)
        }
        Err(e) => {
            let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
            let aside = path.with_file_name(format!("settings.broken-{stamp}.json"));
            match std::fs::rename(path, &aside) {
                Ok(()) => Err(LoadError {
                    message: format!(
                        "設定ファイルが壊れていたため、既定の設定で始めました（{e}）。元のファイルは{}に残しています。",
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
    let fail = |e: std::io::Error| format!("設定を保存できません: {e}");
    std::fs::create_dir_all(path.parent().unwrap()).map_err(fail)?;
    let temporary = path.with_extension("json.tmp");
    let json = serde_json::to_vec_pretty(settings).map_err(|e| e.to_string())?;
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
        let saver = Saver::new(move |settings| {
            if settings.favorites == ["older"] {
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
        assert_eq!(load_from(&path).unwrap().favorites, ["latest"]);
    }

    #[test]
    fn fills_in_what_a_partial_file_lacks() {
        let settings: Settings = serde_json::from_str(r#"{"column_width":300,"favorites":["D:\\"]}"#).unwrap();
        assert_eq!(settings.column_width, 300.);
        assert_eq!(settings.sidebar_width, 240.);
        assert_eq!(settings.favorites, [r"D:\"]);
        assert!(settings.saved_searches.is_empty());
        // One written before the theme could be chosen follows Windows.
        assert_eq!(settings.theme, Theme::System);
    }

    #[test]
    fn reads_a_file_that_still_names_an_everything_instance() {
        let settings: Settings = serde_json::from_str(r#"{"instance":"1.5a","favorites":["D:\\"]}"#).unwrap();
        assert_eq!(settings.favorites, [r"D:\"]);
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
        assert_eq!(settings.favorites, [r"D:\"]);
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
        assert_eq!((settings.column_width, settings.search_width, settings.sidebar_width), (640., 192., 201.));
    }

    #[test]
    fn keeps_one_of_each_saved_search_and_favorite_and_none_empty() {
        let search = |name: &str, query: &str| SavedSearch { name: name.into(), query: query.into() };
        let mut settings = Settings {
            saved_searches: vec![search("PDF", "ext:pdf"), search("", "  "), search("again", "ext:pdf"), search("", "dm:today")],
            favorites: vec![r"D:\Work".into(), r"d:\work".into(), String::new()],
            ..Settings::default()
        };
        settings.normalize();
        assert_eq!(settings.saved_searches, [search("PDF", "ext:pdf"), search("", "dm:today")]);
        assert_eq!(settings.favorites, [r"D:\Work"]);
        assert_eq!(settings.saved_searches[1].label(), "dm:today");
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
