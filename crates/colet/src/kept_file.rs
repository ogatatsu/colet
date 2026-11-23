//! The files Colet keeps between starts, as any of them is read and written: a field and an item at a
//! time, so what cannot be read is left out and the rest kept; a broken one put aside rather than written
//! over; and the saves written one after another on a lane of their own.
use std::{
    io::Write as _,
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};

use crate::lane::Lane;
use crate::words;

/// The folder in `%APPDATA%` Colet keeps its files in. A debug build, which runs beside a release one
/// (see `instance`), keeps its own: each holds what it read at start and saves the whole of it, so with one
/// file the last to save would take away what the other added.
#[cfg(not(debug_assertions))]
const KEPT_FOLDER: &str = "Colet";
#[cfg(debug_assertions)]
const KEPT_FOLDER: &str = "Colet-debug";

/// `%APPDATA%\Colet\<name>`, beside the other files Colet keeps; `Colet-debug` for a debug build.
pub fn kept_path(name: &str) -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join(KEPT_FOLDER).join(name))
}

/// Why a file kept could not be used, or not all of it.
#[derive(Clone, Debug, PartialEq)]
pub struct LoadError {
    pub message: String,
    /// The file is still where it was, not all read: writing over it would lose what was not, so nothing
    /// is saved until Colet is started again.
    pub kept: bool,
}

/// A file kept as it was read: what of it could be used, the defaults for the rest, and what to say of
/// what could not.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded<T> {
    pub value: T,
    pub problem: Option<LoadError>,
}

impl<T: Default> Loaded<T> {
    pub fn read(value: T) -> Self {
        Self { value, problem: None }
    }

    /// None of it: the defaults.
    pub fn failed(problem: LoadError) -> Self {
        Self { value: T::default(), problem: Some(problem) }
    }
}

#[cfg(test)]
impl<T> Loaded<T> {
    /// All of it, read with nothing to say.
    pub fn unwrap(self) -> T {
        if let Some(problem) = self.problem {
            panic!("{problem:?}");
        }
        self.value
    }

    pub fn unwrap_err(self) -> LoadError {
        self.problem.expect("read with nothing to say")
    }
}

/// A file Colet keeps, as its words name it.
pub struct File {
    /// Its name without `.json`, which a broken one is put aside under.
    pub stem: &'static str,
    /// What it holds: "設定".
    pub what: fn() -> &'static str,
    /// What it holds, as it is a file: "設定ファイル".
    pub file: fn() -> &'static str,
    /// What Colet starts on without it: "既定の設定".
    pub fallback: fn() -> &'static str,
}

type Write<T> = dyn Fn(&T) -> Result<(), String> + Send + Sync;

/// A save on its way.
pub struct Queued {
    result: mpsc::Receiver<Result<(), String>>,
    what: fn() -> &'static str,
}

impl Queued {
    /// How the save went, once it is written; a write that broke off never says, and is taken as failed.
    pub fn written(self) -> Result<(), String> {
        self.result.recv().unwrap_or_else(|_| Err(words::kept::cannot_save((self.what)())))
    }

    #[cfg(test)]
    pub fn pending(&self) -> bool {
        matches!(self.result.try_recv(), Err(mpsc::TryRecvError::Empty))
    }
}

/// The saves of a kept file, written on a lane of their own one after another in the order sent, so an
/// older save never lands over a newer one and the final save on close waits for those before it. A save
/// waiting out a burst of changes goes only for the last of them. None are written when the file read at
/// start could not be read and is still there, so it is never written over this session.
pub struct Saves<T> {
    lane: Lane,
    /// None while nothing is written.
    write: Option<Arc<Write<T>>>,
    /// What the file holds, for saying it could not be saved: "設定".
    what: fn() -> &'static str,
    /// Counts the changes.
    changes: u64,
}

impl<T: Clone + Send + 'static> Saves<T> {
    pub fn new(file: &File, write: Option<impl Fn(&T) -> Result<(), String> + Send + Sync + 'static>) -> Self {
        let write = write.map(|write| Arc::new(write) as Arc<Write<T>>);
        Self { lane: Lane::default(), write, what: file.what, changes: 0 }
    }

    /// Queues `kept` behind the saves before it; [`Queued::written`] says how its write went.
    fn save(&self, kept: &T) -> Option<Queued> {
        let write = self.write.clone()?;
        let kept = kept.clone();
        Some(Queued { result: self.lane.run(move || write(&kept)), what: self.what })
    }

    /// Notes a change: the number a save of it waits with, none when nothing is written.
    pub fn changed(&mut self) -> Option<u64> {
        self.changes += 1;
        self.write.as_ref().map(|_| self.changes)
    }

    /// Queues `kept` unless a change came after `change`, which has its own save coming.
    pub fn save_if_last(&self, change: u64, kept: &T) -> Option<Queued> {
        if self.changes != change {
            return None;
        }
        self.save(kept)
    }

    /// Queues `kept` now, dropping the saves still waiting out a burst.
    pub fn save_now(&mut self, kept: &T) -> Option<Queued> {
        self.changes += 1;
        self.save(kept)
    }
}

/// What `path` holds, or the defaults when it is not there yet. A file that is not JSON is put aside, so
/// the defaults written next do not take its place unseen; one with fields or items that cannot be read
/// is read without them, and copied aside, as the next save leaves them out.
pub fn read<T: Lenient + Default>(path: &Path, file: &File) -> Loaded<T> {
    let kept = |e: &dyn std::fmt::Display| LoadError { message: words::kept::unreadable((file.what)(), e, (file.file)()), kept: true };
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded::read(T::default()),
        Err(e) => return Loaded::failed(kept(&e)),
    };
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let aside = path.with_file_name(format!("{}.broken-{stamp}.json", file.stem));
    let fields = serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| e.to_string()).and_then(Fields::of);
    let mut fields = match fields {
        Ok(fields) => fields,
        Err(e) => {
            return Loaded::failed(match std::fs::rename(path, &aside) {
                Ok(()) => LoadError { message: words::kept::put_aside((file.file)(), (file.fallback)(), &e, aside.display()), kept: false },
                Err(_) => kept(&e),
            });
        }
    };
    let value = T::from_json(&mut fields);
    if fields.skipped == 0 {
        return Loaded::read(value);
    }
    let skipped = words::kept::skipped((file.file)(), fields.skipped);
    let problem = match std::fs::copy(path, &aside) {
        Ok(_) => LoadError { message: words::kept::copied_aside(&skipped, aside.display()), kept: false },
        Err(_) => LoadError { message: words::kept::not_saved(&skipped, (file.file)()), kept: true },
    };
    Loaded { value, problem: Some(problem) }
}

/// Writes `kept` beside `path` and then over it, so a failed write leaves the old one whole.
pub fn write<T: Serialize>(kept: &T, path: &Path, file: &File) -> Result<(), String> {
    let fail = |e: std::io::Error| words::kept::cannot_save_because((file.what)(), e);
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

/// A file Colet keeps, read a field and an item at a time: one that cannot be read is left out, the
/// rest kept, as a file edited by hand, or by another Colet, may hold one it does not know.
pub trait Lenient: Sized {
    fn from_json(fields: &mut Fields) -> Self;
}

/// `T` read as [`Lenient`] reads it, what it cannot read let go of unsaid.
pub fn lenient<'de, T: Lenient, D: serde::Deserializer<'de>>(deserializer: D) -> Result<T, D::Error> {
    let value = serde_json::Value::deserialize(deserializer)?;
    let mut fields = Fields::of(value).map_err(serde::de::Error::custom)?;
    Ok(T::from_json(&mut fields))
}

/// The fields of a file's object, as they are taken, with how many of them, or of their items, could
/// not be read.
pub struct Fields {
    map: serde_json::Map<String, serde_json::Value>,
    pub skipped: usize,
}

impl Fields {
    pub fn of(value: serde_json::Value) -> Result<Self, String> {
        match value {
            serde_json::Value::Object(map) => Ok(Self { map, skipped: 0 }),
            _ => Err(words::kept::not_an_object().into()),
        }
    }

    /// The field `key`; none when it is not there, or cannot be read, which is counted.
    pub fn one<T: serde::de::DeserializeOwned>(&mut self, key: &str) -> Option<T> {
        let value = self.map.remove(key)?;
        serde_json::from_value(value).inspect_err(|_| self.skipped += 1).ok()
    }

    /// The list `key`, an item at a time: none where one cannot be read, which is counted. One that is
    /// not a list counts as one.
    pub fn each<T: serde::de::DeserializeOwned>(&mut self, key: &str) -> Vec<Option<T>> {
        match self.map.remove(key) {
            None => Vec::new(),
            Some(serde_json::Value::Array(items)) => {
                items.into_iter().map(|item| serde_json::from_value(item).inspect_err(|_| self.skipped += 1).ok()).collect()
            }
            Some(_) => {
                self.skipped += 1;
                Vec::new()
            }
        }
    }

    /// The items of the list `key` that can be read.
    pub fn many<T: serde::de::DeserializeOwned>(&mut self, key: &str) -> Vec<T> {
        self.each(key).into_iter().flatten().collect()
    }
}
