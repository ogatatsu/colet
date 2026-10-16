//! Telling when what the open folders hold changes.
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    thread,
    time::Duration,
};

use notify::{RecursiveMode, Watcher as _};

/// The paths changed since the last batch was taken; none named means anything may have.
#[derive(Default)]
pub struct Changes {
    all: bool,
    paths: HashSet<String>,
}

impl Changes {
    /// Only the first change in a batch schedules it to be sent: true then. An empty `paths` stands for
    /// any change, even after named ones.
    pub fn add(&mut self, paths: Vec<String>) -> bool {
        let schedule = !self.all && self.paths.is_empty();
        if paths.is_empty() {
            self.all = true;
            self.paths.clear();
        } else if !self.all {
            self.paths.extend(paths);
        }
        schedule
    }

    /// The batch, empty when any change may have happened.
    pub fn take(&mut self) -> Vec<String> {
        self.all = false;
        self.paths.drain().collect()
    }
}

/// Watches folders, one level deep each, and hands `changed` each batch of changed paths a little after
/// the first in it. Dropped, it stops.
pub struct FolderWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl FolderWatcher {
    /// Also returns the folders it could list but not watch, as for a share that sends no change
    /// notifications. One it cannot read is left out: its column says so.
    pub fn new(
        paths: &[String],
        changed: impl Fn(Vec<String>) + Send + 'static,
    ) -> Result<(Self, Vec<String>), String> {
        let changes = Arc::new(Mutex::new(Changes::default()));
        let (wake, batches) = mpsc::channel::<()>();
        let queued = changes.clone();
        // Asleep when idle; dropping the watcher closes the channel once any batch is delivered.
        thread::spawn(move || {
            for () in batches {
                thread::sleep(Duration::from_millis(100));
                let paths = queued.lock().unwrap().take();
                changed(paths);
            }
        });
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let Ok(event) = event else { return };
            if matches!(event.kind, notify::EventKind::Access(_)) {
                return;
            }
            let paths = event.paths.iter().map(|p| p.display().to_string()).collect();
            if changes.lock().unwrap().add(paths) {
                let _ = wake.send(());
            }
        })
        .map_err(|e| e.to_string())?;
        let mut unwatched = Vec::new();
        for path in paths {
            let folder = Path::new(path);
            if watcher.watch(folder, RecursiveMode::NonRecursive).is_err() && std::fs::read_dir(folder).is_ok() {
                unwatched.push(path.clone());
            }
        }
        Ok((Self { _watcher: watcher }, unwatched))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_keeps_both_rename_paths_and_schedules_one_batch() {
        let mut changes = Changes::default();
        assert!(changes.add(vec!["old".into(), "new".into()]));
        for _ in 0..1000 {
            assert!(!changes.add(vec!["new".into()]));
        }
        let paths: HashSet<_> = changes.take().into_iter().collect();
        assert_eq!(paths, HashSet::from(["old".into(), "new".into()]));
        assert!(changes.add(vec!["later".into()]));
        assert_eq!(changes.take(), ["later"]);
    }

    #[test]
    fn an_unnamed_change_dominates_the_batch_but_not_the_next() {
        let mut changes = Changes::default();
        changes.add(vec!["before".into()]);
        changes.add(vec![]);
        changes.add(vec!["after".into()]);
        assert!(changes.take().is_empty());
        assert!(changes.add(vec!["next".into()]));
        assert_eq!(changes.take(), ["next"]);
    }

    #[test]
    fn reports_a_new_file_in_a_watched_folder() {
        let dir = tempfile::tempdir().unwrap();
        let (sent, received) = mpsc::channel();
        let (_watcher, unwatched) =
            FolderWatcher::new(&[dir.path().display().to_string()], move |paths| sent.send(paths).unwrap()).unwrap();
        assert!(unwatched.is_empty());
        let file = dir.path().join("new.txt");
        std::fs::write(&file, "x").unwrap();
        let paths = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(paths.iter().any(|p| p.ends_with("new.txt")), "{paths:?}");
    }
}
