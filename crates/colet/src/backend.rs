//! Where listings come from (Everything, the file system and the drives) and where the settings are kept.
//! Tests hand the view their own.
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

use crate::settings::{self, Settings};
use colet_core::{
    entry::{Entry, Sort},
    everything::{Page, SearchSession},
    folders,
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
    fn load_settings(&self) -> Result<Settings, String>;
    fn save_settings(&self, settings: &Settings) -> Result<(), String>;
}

/// Everything 1.5 and the real disks.
#[derive(Default)]
pub struct Live {
    session: Mutex<SearchSession>,
    latest: AtomicU64,
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

    fn load_settings(&self) -> Result<Settings, String> {
        settings::load()
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        settings::save(settings)
    }
}

pub type Shared = Arc<dyn Backend>;
