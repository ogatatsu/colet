//! Where listings come from (Everything, the file system and the drives) and where the settings are kept.
//! Tests hand the view their own.
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex, MutexGuard, OnceLock, PoisonError, TryLockError,
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};

use crate::words::{self, Spoken};
use crate::{
    kept_file::Loaded,
    settings::{self, Settings},
    sorts::{self, Sorts},
};
use colet_core::{
    entry::{Entry, Order},
    everything::{self, Connection, Page, SearchError, SearchSession, SlowProperties, Tell},
    folders::{self, ListError},
    operations::{self, Event, OperationError, Request},
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

/// What a search came back with.
pub struct Found {
    pub page: Page,
    /// The Everything that answered it.
    pub connection: Option<Connection>,
    /// What to warn of that Everything, once for each connection to it.
    pub warning: Option<SlowProperties>,
}

pub trait Backend: Send + Sync + 'static {
    fn roots(&self) -> Vec<Entry>;
    /// A server that will not let the user in as they are asks them to sign in, in a dialog over `owner`.
    fn list(&self, path: &Path, sort: Order, cancelled: &AtomicBool, owner: Owner) -> Result<Vec<Entry>, ListError>;
    /// `None` when a search asked after it came first, and it was not sent.
    fn search(&self, search: Search) -> Result<Option<Found>, SearchError>;
    /// How many results `query` finds, leaving the last search and its watch be.
    fn count(&self, query: &str) -> Result<usize, SearchError>;
    /// Whether the last search's results changed; so until it is searched afresh. `None` while a search
    /// under way holds it, perhaps waiting out a busy Everything: ask again in a moment.
    fn search_changed(&self) -> Result<Option<bool>, SearchError>;
    /// Tells `tell`, from another thread, whenever the watch on the search hears a change or fails, so
    /// [`Backend::search_changed`] is asked then rather than every so often.
    fn tell_search(&self, tell: Tell);
    /// Stops watching the last search, as none is in front, unless a search asked after `request` came
    /// first. It may wait for a search under way: never call it on the UI thread.
    fn stop_search(&self, request: u64);
    /// The language Colet speaks as it follows Windows's: Japanese where Windows shows itself in it, English
    /// elsewhere.
    fn windows_speaks(&self) -> Spoken;
    fn load_settings(&self) -> Loaded<Settings>;
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
    fn load_sorts(&self) -> Loaded<Sorts>;
    fn save_sorts(&self, sorts: &Sorts) -> Result<(), String>;
    /// Carries out a file operation, telling `report` how each item ends; a paste pastes `clipboard`, the
    /// files read from it, or when `None` reads them itself. Blocks until it is done.
    fn operate(
        &self,
        request: Request,
        clipboard: Option<ClipboardFiles>,
        owner: Owner,
        report: Box<dyn FnMut(Event) + Send>,
    ) -> Result<(), OperationError>;
    /// The files on the clipboard; `None` while another app holds it.
    fn clipboard_files(&self) -> Option<ClipboardFiles>;
    fn copy_files(&self, paths: &[PathBuf], cut: bool, owner: Owner) -> windows_core::Result<()>;
    fn copy_text(&self, text: &OsStr, owner: Owner) -> windows_core::Result<()>;
    /// Calls `changed` whenever any app changes the clipboard.
    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>);
    /// Opens a file in the app Windows associates with it.
    fn open(&self, path: &Path, owner: Owner) -> windows_core::Result<()>;
    /// Shows them, all in one folder, selected together in an Explorer window.
    fn reveal(&self, paths: &[PathBuf]) -> windows_core::Result<()>;
    /// Opens Windows' properties of `paths`, several together in one dialog, from any folders.
    fn properties(&self, paths: &[PathBuf], owner: Owner) -> windows_core::Result<()>;
    /// Whether `path` is a folder, a file, or gone. It touches the disk, which a share can keep waiting for
    /// seconds: never ask it on the UI thread.
    fn path_kind(&self, path: &Path) -> PathKind;
    /// `path` as it is named on disk, and whether it is a folder; `None` when it cannot be reached. A share
    /// that refuses the user asks them to sign in over `owner`, unless `cancelled` is set first. It touches
    /// the disk: never ask it on the UI thread.
    fn locate(&self, path: &Path, cancelled: &AtomicBool, owner: Owner) -> Option<(PathBuf, bool)>;
    /// The Windows context menu at screen point `at`, for `paths` or the folder `background`. It fails with
    /// `E_INVALIDARG` when they are not all in one folder, and with [`shell::MENU_BUSY`] while a command
    /// from the menu before still runs.
    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, at: (i32, i32), owner: Owner) -> windows_core::Result<()>;
    /// What Windows' Send To offers, from its folder; none while it cannot be read. It touches the disk.
    fn send_targets(&self) -> Vec<SendTarget>;
    /// Drops `paths`, from any folders, on `target`, one of [`Backend::send_targets`]: copied, or with
    /// `shift` moved where it allows it. Blocks until the drop returns.
    fn send_to(&self, target: &Path, paths: &[PathBuf], shift: bool, owner: Owner) -> windows_core::Result<Dropped>;
    /// Drags `paths`, from any folders, out of the window `owner` into another app, which copies, moves
    /// or links them. Call it on the window's own thread, out of any update: it runs the window's
    /// messages until the drag ends. What it returns tells, asked on the same thread, whether the app
    /// they were dropped in is still writing them out on a thread of its own.
    fn drag_out(&self, paths: &[PathBuf], owner: Owner) -> windows_core::Result<Box<dyn Fn() -> bool>>;
    /// Asks QuickLook to show `path`, in the order asked; what it returns tells how that went.
    fn quick_look(&self, message: quick_look::Message, path: PathBuf) -> mpsc::Receiver<Result<(), quick_look::Error>>;
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
    fn menu_thread(&self) -> windows_core::Result<MenuThread> {
        // What a panic left is a thread or none, either of which serves.
        let mut menu = self.menu.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(thread) = &*menu {
            return Ok(thread.clone());
        }
        Ok(menu.insert(MenuThread::spawn()?).clone())
    }

    /// The last search and its watch. One a panic left part way is let go of, and the next search
    /// connects afresh, rather than every search failing from then on.
    fn session(&self) -> MutexGuard<'_, SearchSession> {
        self.session.lock().unwrap_or_else(|poisoned| {
            let mut session = poisoned.into_inner();
            session.disconnect();
            self.session.clear_poison();
            session
        })
    }
}

impl Backend for Live {
    fn roots(&self) -> Vec<Entry> {
        folders::roots()
    }

    fn list(&self, path: &Path, sort: Order, cancelled: &AtomicBool, owner: Owner) -> Result<Vec<Entry>, ListError> {
        folders::list(path, sort, cancelled, owner)
    }

    fn search(&self, search: Search) -> Result<Option<Found>, SearchError> {
        self.latest.fetch_max(search.request, Ordering::Relaxed);
        let mut session = self.session();
        if self.latest.load(Ordering::Relaxed) != search.request {
            return Ok(None);
        }
        // A search that fails after connecting keeps the warning for the next.
        let page = session.search(&search.query, search.sort, search.offset, search.count, search.refresh)?;
        Ok(Some(Found { page, connection: session.connection().cloned(), warning: session.take_warning() }))
    }

    fn count(&self, query: &str) -> Result<usize, SearchError> {
        // Held so it never connects as a search does.
        let _session = self.session();
        everything::count(query)
    }

    fn search_changed(&self) -> Result<Option<bool>, SearchError> {
        match self.session.try_lock() {
            Ok(mut session) => session.changed().map(Some),
            // A search under way, perhaps waiting out a busy Everything: asked again in a moment.
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Poisoned(_)) => self.session().changed().map(Some),
        }
    }

    fn tell_search(&self, tell: Tell) {
        self.session().tell(tell);
    }

    fn stop_search(&self, request: u64) {
        // A search asked before it and not yet run is not run: it is no longer in front.
        self.latest.fetch_max(request, Ordering::Relaxed);
        let mut session = self.session();
        if self.latest.load(Ordering::Relaxed) == request {
            session.release();
        }
    }

    fn windows_speaks(&self) -> Spoken {
        if colet_core::language::windows_is_japanese() { Spoken::Japanese } else { Spoken::English }
    }

    fn load_settings(&self) -> Loaded<Settings> {
        settings::load()
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings::save(settings)
    }

    fn load_sorts(&self) -> Loaded<Sorts> {
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
    ) -> Result<(), OperationError> {
        operations::run(request, clipboard, owner, report)
    }

    fn clipboard_files(&self) -> Option<ClipboardFiles> {
        clipboard::read_files().ok()
    }

    fn copy_files(&self, paths: &[PathBuf], cut: bool, owner: Owner) -> windows_core::Result<()> {
        clipboard::write_files(paths, cut, owner)
    }

    fn copy_text(&self, text: &OsStr, owner: Owner) -> windows_core::Result<()> {
        clipboard::write_text(text, owner)
    }

    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>) {
        if let Err(error) = clipboard::watch(changed) {
            eprintln!("{}", words::ops::cannot_watch_clipboard(words::windows_failure(error.code())));
        }
    }

    fn open(&self, path: &Path, owner: Owner) -> windows_core::Result<()> {
        shell::open(path, owner)
    }

    fn reveal(&self, paths: &[PathBuf]) -> windows_core::Result<()> {
        shell::reveal(paths)
    }

    fn properties(&self, paths: &[PathBuf], owner: Owner) -> windows_core::Result<()> {
        self.menu_thread()?.properties(paths.to_vec(), owner)
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

    fn locate(&self, path: &Path, cancelled: &AtomicBool, owner: Owner) -> Option<(PathBuf, bool)> {
        folders::locate(path, owner, cancelled)
    }

    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, at: (i32, i32), owner: Owner) -> windows_core::Result<()> {
        self.menu_thread()?.show(ShowMenu { owner, paths, background, at }).map(drop)
    }

    fn send_targets(&self) -> Vec<SendTarget> {
        shell::send_targets().unwrap_or_default()
    }

    fn send_to(&self, target: &Path, paths: &[PathBuf], shift: bool, owner: Owner) -> windows_core::Result<Dropped> {
        shell::send_to(target, paths, shift, owner)
    }

    fn drag_out(&self, paths: &[PathBuf], owner: Owner) -> windows_core::Result<Box<dyn Fn() -> bool>> {
        let dragged = shell::drag_out(paths, owner)?;
        Ok(Box::new(move || dragged.writing()))
    }

    fn quick_look(&self, message: quick_look::Message, path: PathBuf) -> mpsc::Receiver<Result<(), quick_look::Error>> {
        self.quick_look.get_or_init(quick_look::Sender::spawn).send(message, path)
    }
}

pub type Shared = Arc<dyn Backend>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search_query::narrowed_query;
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
            let mut names: Vec<String> = live.search(search).unwrap().unwrap().page.entries.into_iter().map(|e| e.name).collect();
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
