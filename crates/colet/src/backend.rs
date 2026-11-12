//! Where listings come from (Everything, the file system and the drives) and where the settings are kept.
//! Tests hand the view their own.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, OnceLock,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

use crate::{
    settings::{self, LoadError, Settings},
    sorts::{self, Sorts},
};
use colet_core::{
    entry::{Entry, Order},
    everything::{self, Page, SearchSession},
    folders,
    operations::{self, Event, Request},
    quick_look,
    shell::{
        self, Dropped, MenuThread, Owner, SendTarget, ShowMenu,
        clipboard::{self, ClipboardFiles},
    },
};

/// What a path names on disk.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathKind {
    Folder,
    File,
    Gone,
}

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
    /// A server that will not let the user in as they are asks them to sign in, in a dialog over `owner`.
    fn list(&self, path: &Path, sort: Order, cancelled: &AtomicBool, owner: Owner) -> Result<Vec<Entry>, String>;
    /// The results and the name of what they came from.
    fn search(&self, search: Search) -> Result<(Page, String), String>;
    /// How many results `query` finds, leaving the last search and its watch be.
    fn count(&self, query: &str) -> Result<usize, String>;
    /// Whether the last search's results changed; so until it is searched afresh.
    fn search_changed(&self) -> Result<bool, String>;
    fn load_settings(&self) -> Result<Settings, LoadError>;
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
    fn load_sorts(&self) -> Result<Sorts, LoadError>;
    fn save_sorts(&self, sorts: &Sorts) -> Result<(), String>;
    /// Carries out a file operation, telling `report` how each item ends; a paste pastes `clipboard`, the
    /// files read from it, or when `None` reads them itself. Blocks until it is done.
    fn operate(
        &self,
        request: Request,
        clipboard: Option<ClipboardFiles>,
        owner: Owner,
        report: Box<dyn FnMut(Event) + Send>,
    ) -> Result<(), String>;
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
    /// Opens Windows' properties of `paths`, several together in one dialog, from any folders.
    fn properties(&self, paths: &[PathBuf], owner: Owner) -> Result<(), String>;
    /// Whether `path` is a folder, a file, or gone. It touches the disk, which a share can keep waiting for
    /// seconds: never ask it on the UI thread.
    fn path_kind(&self, path: &Path) -> PathKind;
    /// `path` as it is named on disk, and whether it is a folder; `None` when it cannot be reached. It
    /// touches the disk: never ask it on the UI thread.
    fn locate(&self, path: &Path) -> Option<(PathBuf, bool)>;
    /// The Windows context menu at screen point `at`, for `paths` or the folder `background`.
    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, at: (i32, i32), owner: Owner) -> Result<(), String>;
    /// What Windows' Send To offers, from its folder; none while it cannot be read. It touches the disk.
    fn send_targets(&self) -> Vec<SendTarget>;
    /// Drops `paths`, from any folders, on `target`, one of [`Backend::send_targets`]: copied, or with
    /// `shift` moved where it allows it. Blocks until the drop returns.
    fn send_to(&self, target: &Path, paths: &[PathBuf], shift: bool, owner: Owner) -> Result<Dropped, String>;
    /// Drags `paths`, from any folders, out of the window `owner` into another app, which copies, moves
    /// or links them. Call it on the window's own thread, out of any update: it runs the window's
    /// messages until the drag ends.
    fn drag_out(&self, paths: &[PathBuf], owner: Owner) -> Result<(), String>;
    /// Asks QuickLook to show `path`, in the order asked; what it returns tells how that went.
    fn quick_look(&self, message: quick_look::Message, path: PathBuf) -> mpsc::Receiver<Result<(), quick_look::Error>>;
}

fn said(error: windows_core::Error) -> String {
    operations::message(error.code())
}

/// Everything 1.5, the real disks, the clipboard and the shell.
#[derive(Default)]
pub struct Live {
    session: Mutex<SearchSession>,
    latest: AtomicU64,
    /// Started at the first menu or properties, and kept for the life of the app.
    menu: Mutex<Option<MenuThread>>,
    /// Started at the first preview, and kept for the life of the app.
    quick_look: OnceLock<quick_look::Sender>,
}

impl Live {
    /// The thread the shell's menus and properties are shown on, started the first time it is wanted.
    fn menu_thread(&self) -> Result<MenuThread, String> {
        let mut menu = self.menu.lock().map_err(|e| e.to_string())?;
        if menu.is_none() {
            *menu = Some(MenuThread::spawn().map_err(said)?);
        }
        Ok(menu.clone().unwrap())
    }
}

impl Backend for Live {
    fn roots(&self) -> Vec<Entry> {
        folders::roots()
    }

    fn list(&self, path: &Path, sort: Order, cancelled: &AtomicBool, owner: Owner) -> Result<Vec<Entry>, String> {
        folders::list(path, sort, cancelled, owner)
    }

    fn search(&self, search: Search) -> Result<(Page, String), String> {
        self.latest.fetch_max(search.request, Ordering::Relaxed);
        let mut session = self.session.lock().map_err(|e| e.to_string())?;
        if self.latest.load(Ordering::Relaxed) != search.request {
            return Err("古い検索要求です".into());
        }
        let page = session.search(&search.query, search.sort, search.offset, search.count, search.refresh)?;
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

    fn load_settings(&self) -> Result<Settings, LoadError> {
        settings::load()
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings::save(settings)
    }

    fn load_sorts(&self) -> Result<Sorts, LoadError> {
        sorts::load()
    }

    fn save_sorts(&self, sorts: &Sorts) -> Result<(), String> {
        sorts::save(sorts)
    }

    fn operate(
        &self,
        request: Request,
        clipboard: Option<ClipboardFiles>,
        owner: Owner,
        report: Box<dyn FnMut(Event) + Send>,
    ) -> Result<(), String> {
        operations::run(request, clipboard, owner, report)
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

    fn properties(&self, paths: &[PathBuf], owner: Owner) -> Result<(), String> {
        self.menu_thread()?.properties(paths.to_vec(), owner).map_err(said)
    }

    fn path_kind(&self, path: &Path) -> PathKind {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        // Asked once, of the item itself: a junction or link to a folder is one, even when what it points
        // to is gone, and a share slow to answer is waited on once rather than twice.
        match std::fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0 => PathKind::Folder,
            Ok(_) => PathKind::File,
            Err(_) => PathKind::Gone,
        }
    }

    fn locate(&self, path: &Path) -> Option<(PathBuf, bool)> {
        folders::locate(path)
    }

    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, at: (i32, i32), owner: Owner) -> Result<(), String> {
        match self.menu_thread()?.show(ShowMenu { owner, paths, background, at }) {
            Ok(_) => Ok(()),
            // The items are not all in one folder.
            Err(error) if error.code() == windows::Win32::Foundation::E_INVALIDARG => {
                Err("同じフォルダーの項目を選んだときに使えます".into())
            }
            // A command from the menu before is still running, as a dialog it opened.
            Err(error) if error.code() == shell::MENU_BUSY => Err("その他のオプションで開いた操作が終わってから使えます".into()),
            Err(error) => Err(said(error)),
        }
    }

    fn send_targets(&self) -> Vec<SendTarget> {
        shell::send_targets().unwrap_or_default()
    }

    fn send_to(&self, target: &Path, paths: &[PathBuf], shift: bool, owner: Owner) -> Result<Dropped, String> {
        shell::send_to(target, paths, shift, owner).map_err(said)
    }

    fn drag_out(&self, paths: &[PathBuf], owner: Owner) -> Result<(), String> {
        shell::drag_out(paths, owner).map_err(said)
    }

    fn quick_look(&self, message: quick_look::Message, path: PathBuf) -> mpsc::Receiver<Result<(), quick_look::Error>> {
        self.quick_look.get_or_init(quick_look::Sender::spawn).send(message, path)
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
        let made = std::process::Command::new("cmd").args(["/C", "mklink", "/J"]).arg(&junction).arg(&folder).output().unwrap();
        assert!(made.status.success(), "{made:?}");
        std::fs::remove_dir(&folder).unwrap();
        let live = Live::default();
        let kind = |path: &Path| live.path_kind(path);
        assert_eq!(kind(&file), PathKind::File);
        assert_eq!(kind(dir.path()), PathKind::Folder);
        // Still there, and still a folder, though what it points to is gone.
        assert_eq!(kind(&junction), PathKind::Folder);
        assert_eq!(kind(&folder), PathKind::Gone);
    }
}
