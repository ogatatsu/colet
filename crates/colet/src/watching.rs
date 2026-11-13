//! Watching the folders the columns show, letting go of those an operation moves.
use std::{
    collections::HashSet,
    ffi::OsString,
    path::PathBuf,
    sync::{Arc, Mutex, mpsc},
};

use colet_core::{
    paths::{ancestors, path_key},
    watch::{Changes, FolderWatcher},
};

#[derive(Default)]
pub struct Watching {
    /// Started with the first folder to watch.
    watcher: Option<FolderWatcher>,
    watched: Vec<PathBuf>,
    /// What the running operation moves or deletes, by path key: the folders at or inside it go unwatched
    /// until it ends, as Windows keeps a folder in place while a folder inside it is watched.
    pub(crate) released: HashSet<OsString>,
    /// Folders already said to go unwatched, each said once.
    unwatched: HashSet<OsString>,
    /// What the watcher saw change, collected until the UI takes it.
    pub(crate) changed: Arc<Mutex<Changes>>,
    /// Folders the watcher could list but not watch, until the UI says so.
    missed: Arc<Mutex<Vec<PathBuf>>>,
}

impl Watching {
    /// Watches `folders` but those `released` lets go. The watcher opens them on its own thread, so a
    /// share slow to answer never holds up the window; the receiver hears once it has, when there was
    /// anything to change. Errs when the watcher could not start.
    pub fn set(&mut self, folders: Vec<PathBuf>) -> Result<Option<mpsc::Receiver<()>>, String> {
        let folders = kept_watching(folders, &self.released);
        if folders == self.watched {
            return Ok(None);
        }
        self.watched = folders.clone();
        let watcher = match &mut self.watcher {
            Some(watcher) => watcher,
            None => {
                let missed = self.missed.clone();
                let watcher =
                    FolderWatcher::new(self.changed.clone(), move |paths| missed.lock().unwrap_or_else(|e| e.into_inner()).extend(paths))?;
                self.watcher.insert(watcher)
            }
        };
        Ok(Some(watcher.set(folders)))
    }

    /// The folders the watcher could list but not watch that were not taken before.
    pub fn take_missed(&mut self) -> Vec<PathBuf> {
        let missed = std::mem::take(&mut *self.missed.lock().unwrap_or_else(|e| e.into_inner()));
        missed.into_iter().filter(|p| self.unwatched.insert(path_key(p))).collect()
    }
}

/// `folders` but those at or inside one `released` holds, by path key.
pub(crate) fn kept_watching(folders: Vec<PathBuf>, released: &HashSet<OsString>) -> Vec<PathBuf> {
    if released.is_empty() {
        return folders;
    }
    folders.into_iter().filter(|folder| !ancestors(folder).iter().any(|a| released.contains(&path_key(a)))).collect()
}
