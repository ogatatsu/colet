//! Where listings come from (Everything, the file system and the drives) and where the settings are kept.
//! Tests hand the view their own.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::settings::{self, LoadError, Settings};
use colet_core::{
    entry::{Entry, Order},
    everything::{self, Page, SearchSession},
    folders,
    operations::{self, Event, Request},
    shell::{self, MenuThread, Owner, ShowMenu, clipboard::{self, ClipboardFiles}},
};

/// A search: the query, the sort, and from where how many results to load; `refresh` searches afresh.
pub struct Search {
    pub query: String,
    pub sort: Order,
    pub offset: usize,
    pub count: usize,
    pub refresh: bool,
    /// The search's request; one superseded before it runs is not sent.
    pub request: u64,
}

pub trait Backend: Send + Sync + 'static {
    fn roots(&self) -> Vec<Entry>;
    /// The folders Everything's index starts from; none while it cannot be asked.
    fn indexed_roots(&self) -> Vec<PathBuf>;
    fn list(&self, path: &Path, sort: Order, cancelled: &AtomicBool) -> Result<Vec<Entry>, String>;
    /// The results and the name of what they came from.
    fn search(&self, search: Search) -> Result<(Page, String), String>;
    /// How many results `query` finds, leaving the last search and its watch be.
    fn count(&self, query: &str) -> Result<usize, String>;
    /// Whether the last search's results changed; so until it is searched afresh.
    fn search_changed(&self) -> Result<bool, String>;
    /// Lets go of Everything, so the next search connects afresh.
    fn reconnect(&self);
    fn load_settings(&self) -> Result<Settings, LoadError>;
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
    /// Carries out a file operation, telling `report` how each item ends. Blocks until it is done.
    fn operate(&self, request: Request, owner: Owner, report: Box<dyn FnMut(Event) + Send>) -> Result<(), String>;
    /// The files on the clipboard; `None` while another app holds it.
    fn clipboard_files(&self) -> Option<ClipboardFiles>;
    fn copy_files(&self, paths: &[PathBuf], cut: bool, owner: Owner) -> Result<(), String>;
    fn copy_text(&self, text: &OsStr, owner: Owner) -> Result<(), String>;
    /// Calls `changed` whenever any app changes the clipboard.
    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>);
    /// Opens a file in the app Windows associates with it.
    fn open(&self, path: &Path, owner: Owner) -> Result<(), String>;
    /// Shows them, all in one folder, selected together in an Explorer window.
    fn reveal(&self, paths: &[PathBuf]) -> Result<(), String>;
    /// Whether `path` is a folder, a file, or gone. It touches the disk, which a share can keep waiting for
    /// seconds: never ask it on the UI thread.
    fn path_kind(&self, path: &Path) -> Option<bool>;
    /// The Windows context menu at screen point `at`, for `paths` or the folder `background`.
    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, at: (i32, i32), owner: Owner) -> Result<(), String>;
}

fn said(error: windows_core::Error) -> String {
    operations::message(error.code())
}

/// Everything 1.5, the real disks, the clipboard and the shell.
#[derive(Default)]
pub struct Live {
    session: Mutex<SearchSession>,
    latest: AtomicU64,
    /// Started at the first menu, and kept for the life of the app.
    menu: Mutex<Option<MenuThread>>,
}

impl Backend for Live {
    fn roots(&self) -> Vec<Entry> {
        folders::roots()
    }

    fn indexed_roots(&self) -> Vec<PathBuf> {
        // Held so it never connects as a search does.
        let Ok(_session) = self.session.lock() else { return Vec::new() };
        everything::roots().unwrap_or_default()
    }

    fn list(&self, path: &Path, sort: Order, cancelled: &AtomicBool) -> Result<Vec<Entry>, String> {
        folders::list(path, sort, cancelled)
    }

    fn search(&self, search: Search) -> Result<(Page, String), String> {
        self.latest.fetch_max(search.request, Ordering::Relaxed);
        let mut session = self.session.lock().map_err(|e| e.to_string())?;
        if self.latest.load(Ordering::Relaxed) != search.request {
            return Err("古い検索要求です".into());
        }
        let page =
            session.search(&search.query, search.sort, search.offset, search.count, search.refresh)?;
        Ok((page, session.connection.clone()))
    }

    fn count(&self, query: &str) -> Result<usize, String> {
        // Held so it never connects as a search does.
        let _session = self.session.lock().map_err(|e| e.to_string())?;
        everything::count(query)
    }

    fn search_changed(&self) -> Result<bool, String> {
        match self.session.try_lock() {
            Ok(mut session) => session.changed(),
            // A search under way, perhaps waiting out a busy Everything, is asked next time.
            Err(std::sync::TryLockError::WouldBlock) => Ok(false),
            Err(error) => Err(error.to_string()),
        }
    }

    fn reconnect(&self) {
        if let Ok(mut session) = self.session.lock() {
            session.disconnect();
        }
    }

    fn load_settings(&self) -> Result<Settings, LoadError> {
        settings::load()
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings::save(settings)
    }

    fn operate(&self, request: Request, owner: Owner, report: Box<dyn FnMut(Event) + Send>) -> Result<(), String> {
        operations::run(request, owner, report)
    }

    fn clipboard_files(&self) -> Option<ClipboardFiles> {
        clipboard::read_files().ok()
    }

    fn copy_files(&self, paths: &[PathBuf], cut: bool, owner: Owner) -> Result<(), String> {
        clipboard::write_files(paths, cut, owner).map_err(said)
    }

    fn copy_text(&self, text: &OsStr, owner: Owner) -> Result<(), String> {
        clipboard::write_text(text, owner).map_err(said)
    }

    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>) {
        if let Err(error) = clipboard::watch(changed) {
            eprintln!("クリップボードを監視できません: {}", said(error));
        }
    }

    fn open(&self, path: &Path, owner: Owner) -> Result<(), String> {
        shell::open(path, owner).map_err(said)
    }

    fn reveal(&self, paths: &[PathBuf]) -> Result<(), String> {
        shell::reveal(paths).map_err(said)
    }

    fn path_kind(&self, path: &Path) -> Option<bool> {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        // Asked once, of the item itself: a junction or link to a folder is one, even when what it points
        // to is gone, and a share slow to answer is waited on once rather than twice.
        let metadata = std::fs::symlink_metadata(path).ok()?;
        Some(metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0)
    }

    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, at: (i32, i32), owner: Owner) -> Result<(), String> {
        let menu = {
            let mut menu = self.menu.lock().map_err(|e| e.to_string())?;
            if menu.is_none() {
                *menu = Some(MenuThread::spawn().map_err(said)?);
            }
            menu.clone().unwrap()
        };
        match menu.show(ShowMenu { owner, paths, background, at }) {
            Ok(_) => Ok(()),
            // The items are not all in one folder.
            Err(error) if error.code() == windows::Win32::Foundation::E_INVALIDARG => {
                Err("同じフォルダーの項目を選んだときに使えます".into())
            }
            // A command from the menu before is still running, as a dialog it opened.
            Err(error) if error.code() == shell::MENU_BUSY => {
                Err("その他のオプションで開いた操作が終わってから使えます".into())
            }
            Err(error) => Err(said(error)),
        }
    }
}

pub type Shared = Arc<dyn Backend>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::narrowed_query;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "needs a running Everything 1.5 that indexes the temporary folder"]
    fn narrows_the_results_to_names_holding_the_text_as_it_is_and_counts_them_all() {
        let fixture = tempfile::Builder::new().prefix("colet-narrow-").tempdir().unwrap();
        std::fs::write(fixture.path().join("alpha beta.txt"), b"x").unwrap();
        std::fs::write(fixture.path().join("alphabet.txt"), b"x").unwrap();
        std::fs::create_dir(fixture.path().join("Beta")).unwrap();
        std::fs::write(fixture.path().join(r"Beta\x.txt"), b"x").unwrap();
        let live = Live::default();
        let base = format!("\"{}\\\"", fixture.path().display());
        let names = |query: String| {
            let search = Search { query, sort: Order::default(), offset: 0, count: 100, refresh: true, request: 0 };
            let mut names: Vec<String> = live.search(search).unwrap().0.entries.into_iter().map(|e| e.name).collect();
            names.sort();
            names
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        while live.count(&base).unwrap() < 4 {
            assert!(Instant::now() < deadline, "Everything did not index the fixture");
            std::thread::sleep(Duration::from_millis(100));
        }
        assert_eq!(live.count(&base).unwrap(), 4);
        // Each word, in any order, in the name only: not the file in a folder that holds them.
        assert_eq!(names(narrowed_query(&base, "bet alpha")), ["alpha beta.txt", "alphabet.txt"]);
        assert_eq!(names(narrowed_query(&base, "\"a b\"")), ["alpha beta.txt"]);
        assert_eq!(names(narrowed_query(&base, "BETA")), ["Beta", "alpha beta.txt"]);
        // No wildcards.
        assert!(names(narrowed_query(&base, "a*t")).is_empty());
        assert_eq!(live.count(&narrowed_query(&base, "alpha")).unwrap(), 2);
    }

    #[test]
    fn tells_a_folder_from_a_file_and_a_junction_to_one_gone_from_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (folder, file, junction) = (dir.path().join("folder"), dir.path().join("a.txt"), dir.path().join("link"));
        std::fs::create_dir(&folder).unwrap();
        std::fs::write(&file, "a").unwrap();
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&folder)
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        std::fs::remove_dir(&folder).unwrap();
        let live = Live::default();
        let kind = |path: &Path| live.path_kind(path);
        assert_eq!(kind(&file), Some(false));
        assert_eq!(kind(dir.path()), Some(true));
        // Still there, and still a folder, though what it points to is gone.
        assert_eq!(kind(&junction), Some(true));
        assert_eq!(kind(&folder), None);
    }
}
