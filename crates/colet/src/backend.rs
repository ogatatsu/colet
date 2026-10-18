//! Where listings come from (Everything, the file system and the drives) and where the settings are kept.
//! Tests hand the view their own.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::settings::{self, LoadError, Settings};
use colet_core::{
    entry::{Entry, Sort},
    everything::{Page, SearchSession},
    folders,
    operations::{self, Event, Request},
    shell::{self, MenuThread, Owner, ShowMenu, clipboard::{self, ClipboardFiles}},
};

/// A search: the query, the sort, and from where how many results to load; `refresh` searches afresh.
pub struct Search {
    pub query: String,
    pub sort: Sort,
    pub offset: usize,
    pub count: usize,
    pub refresh: bool,
    /// The Everything instance to ask; empty tries the default, then `1.5a`.
    pub instance: String,
    /// The search's request; one superseded before it runs is not sent.
    pub request: u64,
}

pub trait Backend: Send + Sync + 'static {
    fn roots(&self) -> Vec<Entry>;
    fn list(&self, path: &str, sort: Sort, cancelled: &AtomicBool) -> Result<Vec<Entry>, String>;
    /// The results and the name of what they came from.
    fn search(&self, search: Search) -> Result<(Page, String), String>;
    /// Whether the last search's results changed since asked.
    fn search_changed(&self) -> Result<bool, String>;
    /// Lets go of Everything, so the next search connects afresh.
    fn reconnect(&self);
    fn load_settings(&self) -> Result<Settings, LoadError>;
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
    /// Carries out a file operation, telling `report` how each item ends. Blocks until it is done.
    fn operate(&self, request: Request, owner: Owner, report: Box<dyn FnMut(Event) + Send>) -> Result<(), String>;
    /// The files on the clipboard; `None` while another app holds it.
    fn clipboard_files(&self) -> Option<ClipboardFiles>;
    fn copy_files(&self, paths: &[String], cut: bool, owner: Owner) -> Result<(), String>;
    fn copy_text(&self, text: &str, owner: Owner) -> Result<(), String>;
    /// Calls `changed` whenever any app changes the clipboard.
    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>);
    /// Opens a file in the app Windows associates with it.
    fn open(&self, path: &str, owner: Owner) -> Result<(), String>;
    /// Shows it selected in an Explorer window.
    fn reveal(&self, path: &str) -> Result<(), String>;
    /// Whether `path` is a folder, a file, or gone.
    fn path_kind(&self, path: &str) -> Option<bool>;
    /// The Windows context menu at screen point `at`, for `paths` or the folder `background`.
    fn shell_menu(&self, paths: Vec<String>, background: String, at: (i32, i32), owner: Owner) -> Result<(), String>;
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

    fn list(&self, path: &str, sort: Sort, cancelled: &AtomicBool) -> Result<Vec<Entry>, String> {
        folders::list(path, sort, cancelled)
    }

    fn search(&self, search: Search) -> Result<(Page, String), String> {
        self.latest.fetch_max(search.request, Ordering::Relaxed);
        let mut session = self.session.lock().map_err(|e| e.to_string())?;
        if self.latest.load(Ordering::Relaxed) != search.request {
            return Err("古い検索要求です".into());
        }
        let page =
            session.search(&search.query, search.sort, &search.instance, search.offset, search.count, search.refresh)?;
        Ok((page, session.connection.clone()))
    }

    fn search_changed(&self) -> Result<bool, String> {
        self.session.lock().map_err(|e| e.to_string())?.changed()
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

    fn copy_files(&self, paths: &[String], cut: bool, owner: Owner) -> Result<(), String> {
        clipboard::write_files(paths, cut, owner).map_err(said)
    }

    fn copy_text(&self, text: &str, owner: Owner) -> Result<(), String> {
        clipboard::write_text(text, owner).map_err(said)
    }

    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>) {
        if let Err(error) = clipboard::watch(changed) {
            eprintln!("クリップボードを監視できません: {}", said(error));
        }
    }

    fn open(&self, path: &str, owner: Owner) -> Result<(), String> {
        shell::open(path, owner).map_err(said)
    }

    fn reveal(&self, path: &str) -> Result<(), String> {
        shell::reveal(path).map_err(said)
    }

    fn path_kind(&self, path: &str) -> Option<bool> {
        std::fs::symlink_metadata(path).ok().map(|_| std::path::Path::new(path).is_dir())
    }

    fn shell_menu(&self, paths: Vec<String>, background: String, at: (i32, i32), owner: Owner) -> Result<(), String> {
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
