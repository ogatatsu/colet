//! What the user does to files: copy, cut and paste through the clipboard, rename, make folders, send to
//! the Recycle Bin, open; one operation at a time, each kept in the history with how its items ended.
//!
//! [`Operations`] is the entity that runs them and keeps their history, the clipboard's and Send To's
//! state. Colet asks for each from its columns, one feature a module: why an action is off
//! ([`reasons`]), the clipboard ([`clipboard`]), naming ([`name`]), opening ([`open`]), what is handed to
//! Windows ([`handoff`]), and running an operation and what follows it ([`run`]). The history's records are
//! [`history`]'s.
use std::{
    borrow::Cow,
    collections::HashSet,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, atomic::Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use colet_core::{
    entry::Entry,
    operations::{Event, Kind, OperationError, Request},
    paths::{ancestors, child, folder_name, is_server, parent_folder, path_key, shown},
    shell::{self, Dropped, SendTarget},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    backend::{self, PathKind},
    clipboard::{Clipboard, Paste},
    dialogs::{Dialog, DialogParts, field, field_problem, label, push},
    metrics,
    notices::Tone,
    settings::Favorite,
    state::{Column, HomeKey, Kind as ColumnKind},
    theme::ActivePalette,
    wake::Wake,
};
mod clipboard;
mod handoff;
mod history;
mod name;
mod open;
mod reasons;
mod run;

pub use history::{History, ItemStatus, Status};
pub use reasons::{Reasons, needs_file, needs_folder, one_item};
#[cfg(test)]
pub use {
    open::ASK_TO_OPEN,
    reasons::{not_at_server, same_folder},
};

/// How long the clipboard waits before it is read again, after another app held it.
const CLIPBOARD_RETRY: std::time::Duration = std::time::Duration::from_millis(200);

/// The file operations and what they leave behind: the one under way and the sends to Windows' Send To,
/// the history, the clipboard as last read, what Send To offers, and what the latest operation sent away.
///
/// Its own entity, beside Colet's: it reads the clipboard and Send To, writes the clipboard and runs the
/// sends on its own, notifying as what it holds changes, and tells Colet through [`OperationsEvent`]
/// what to say and what to do next. A file operation changes Colet's columns, so Colet starts and ends
/// each one; this keeps their bookkeeping consistent.
pub struct Operations {
    backend: backend::Shared,
    /// The window the shell's dialogs belong to.
    owner: shell::Owner,
    /// Brings the window round to read the clipboard again, after another app held it.
    wake: Wake,
    /// The file operation under way; they go one at a time.
    running: Option<Running>,
    /// The sends to Windows' Send To under way: their drops run in this process, so the window stays
    /// until each returns.
    sending: usize,
    pub history: History,
    pub clipboard: Clipboard,
    /// What Windows' Send To offers, as last read: none until it is.
    send_targets: Vec<SendTarget>,
    /// What the latest operation sent away, by path key: a listing that finds it gone takes it as Colet's
    /// own doing.
    departed: HashSet<OsString>,
}

/// What [`Operations`] tells Colet of on its own, as what it set off ends.
pub enum OperationsEvent {
    /// A line to put up in the window.
    Notice(String, Tone),
    /// The clipboard writes a paste waited on are done: paste now, for the tab it was asked in.
    PasteReady(Paste),
    /// What Send To offers changed: a file menu open shows it afresh.
    SendTargetsChanged,
    /// A send to Send To ended: what it moved has left its folder, and what it let go of, by the number
    /// [`Colet::carry`] gave, may be watched again.
    Sent(Option<u64>),
}

/// The watches let go of for a send or a drag, by the number to put them down with, and what hears once
/// they are.
pub struct Carried {
    pub id: u64,
    pub released: Option<std::sync::mpsc::Receiver<()>>,
}

impl EventEmitter<OperationsEvent> for Operations {}

/// How an operation ended, for its entry in the history.
struct Ended {
    id: u64,
    kind: Kind,
    /// The user stopped it.
    cancelled: bool,
    /// Its own failure, apart from its items'.
    problem: Option<String>,
    /// Where a rename that told nothing of its item put it.
    renamed: Option<PathBuf>,
}

impl Operations {
    /// Watches the clipboard from now on, telling `wake` of each change, and reads what Send To offers.
    pub fn new(backend: backend::Shared, owner: shell::Owner, wake: Wake, cx: &mut Context<Self>) -> Self {
        let clipboard = Clipboard::default();
        let changed = clipboard.changed_flag();
        let told = wake.clone();
        backend.watch_clipboard(Box::new(move || {
            changed.store(true, Ordering::Relaxed);
            told.wake();
        }));
        let mut this = Self {
            backend,
            owner,
            wake,
            running: None,
            sending: 0,
            history: History::default(),
            clipboard,
            send_targets: Vec::new(),
            departed: HashSet::new(),
        };
        this.load_send_targets(cx);
        this
    }

    /// A file operation is under way.
    pub fn is_running(&self) -> bool {
        self.running.is_some()
    }

    /// A send to Windows' Send To is under way.
    pub fn is_sending(&self) -> bool {
        self.sending > 0
    }

    pub fn send_targets(&self) -> &[SendTarget] {
        &self.send_targets
    }

    /// Takes what Send To offers as read; tells whether it changed.
    pub fn set_send_targets(&mut self, targets: Vec<SendTarget>) -> bool {
        let changed = targets != self.send_targets;
        if changed {
            self.send_targets = targets;
        }
        changed
    }

    /// Lets go of the operations in the history that are over; the one under way stays.
    pub fn clear_history(&mut self, cx: &mut Context<Self>) {
        self.history.clear();
        cx.notify();
    }

    /// The history has been seen: no problems are left unseen.
    pub fn mark_history_seen(&mut self, cx: &mut Context<Self>) {
        if self.history.unseen != 0 {
            self.history.unseen = 0;
            cx.notify();
        }
    }

    /// What the latest operation sent away, by path key.
    pub fn departed(&self) -> &HashSet<OsString> {
        &self.departed
    }

    /// Starts `request` in tab `tab`, kept at the top of the history: the channel its items' ends come
    /// through. `None` while another runs.
    fn begin(&mut self, request: &Request, tab: crate::tabs::TabId, cx: &mut Context<Self>) -> Option<OperationEvents> {
        if self.running.is_some() {
            return None;
        }
        let id = self.history.start(request.clone());
        self.departed.clear();
        let events = OperationEvents::default();
        self.running = Some(Running { id, tab, events: events.clone(), made: None, placed: Vec::new() });
        cx.notify();
        Some(events)
    }

    /// Takes in how the items of the running operation ended so far; tells whether any did.
    fn take_events(&mut self) -> bool {
        let Some(running) = &mut self.running else { return false };
        let events = std::mem::take(&mut *running.events.lock().unwrap_or_else(|e| e.into_inner()));
        if events.is_empty() {
            return false;
        }
        let mut entry = self.history.entry_mut(running.id);
        for event in events {
            match &event {
                Event::Done { path } => running.made = Some(path.clone()),
                // A copy's source stays: only what left is expected to go from its listing.
                Event::Left { path } => {
                    self.departed.insert(path_key(path));
                }
                // What the shell writes out on its own is picked by its name once it is listed.
                Event::Placed { path } | Event::Handed { path } => running.placed.push(path.clone()),
                _ => {}
            }
            if let Some(entry) = &mut entry {
                entry.record(event);
            }
        }
        true
    }

    /// The running operation over: the last of its items' ends taken in, it is let go of.
    fn end(&mut self, cx: &mut Context<Self>) -> Option<Running> {
        self.take_events();
        cx.notify();
        self.running.take()
    }

    /// Puts down in its entry how the operation ended. What to say of its problems, counted as not yet seen.
    fn record_end(&mut self, ended: Ended, cx: &mut Context<Self>) -> Option<String> {
        let Ended { id, kind, cancelled, problem, renamed } = ended;
        let index = self.history.entries.iter().position(|e| e.id == id)?;
        let entry = &mut self.history.entries[index];
        let counts = entry.item_problems();
        // A cut pasted where it already was moved nothing, and a deletion turned down at the shell's
        // question deleted nothing: no trace, as in Explorer.
        let moved_nothing = kind == Kind::Paste && problem.is_none() && !cancelled && entry.items.is_empty() && entry.more == 0;
        let declined = matches!(kind, Kind::Recycle | Kind::Delete)
            && cancelled
            && entry.more == 0
            && entry.items.iter().all(|i| i.status == ItemStatus::Cancelled);
        if moved_nothing || declined {
            self.history.entries.remove(index);
        } else {
            if let Some(to) = renamed
                && let Some(item) = entry.items.first_mut().filter(|i| i.status == ItemStatus::Done)
            {
                item.from = Some(std::mem::replace(&mut item.path, to));
            }
            entry.status = if cancelled {
                Status::Cancelled
            } else if problem.is_some() {
                Status::Error
            } else {
                Status::Done
            };
            // The items say more than the summary the operation adds for them.
            entry.error = if counts.is_empty() { problem.clone() } else { None };
        }
        cx.notify();
        let problem = if counts.is_empty() { problem } else { Some(counts.join(words::separator())) };
        if problem.is_some() {
            self.history.unseen += 1;
        }
        problem
    }

    /// How the running operation's items ended so far, and what the clipboard now holds, read off the UI
    /// thread.
    pub fn take_changes(&mut self, cx: &mut Context<Self>) {
        if self.take_events() {
            cx.notify();
        }
        if self.clipboard.take_changed() {
            let backend = self.backend.clone();
            let read = cx.background_executor().spawn(async move { backend.clipboard_files() });
            cx.spawn(async move |this, cx| {
                match read.await {
                    Some(files) => this.update(cx, |this, cx| {
                        this.clipboard.set_files(files);
                        cx.notify();
                    }),
                    // None while another app holds it, which says nothing as it lets go: read again in a
                    // moment.
                    None => {
                        cx.background_executor().timer(CLIPBOARD_RETRY).await;
                        this.update(cx, |this, _| {
                            this.clipboard.read_again();
                            this.wake.wake();
                        })
                    }
                }
                .ok();
            })
            .detach();
        }
    }

    /// Reads what Windows' Send To offers off the UI thread, and tells if it changed.
    pub fn load_send_targets(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_executor().spawn(async move { backend.send_targets() });
        cx.spawn(async move |this, cx| {
            let targets = task.await;
            this.update(cx, |this, cx| {
                if this.set_send_targets(targets) {
                    cx.emit(OperationsEvent::SendTargetsChanged);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Puts `paths` on the clipboard, to be moved by the paste when `cut`.
    pub fn copy_files(&mut self, paths: Vec<PathBuf>, cut: bool, cx: &mut Context<Self>) {
        let (backend, owner) = (self.backend.clone(), self.owner);
        let done = words::ops::put_on_clipboard(paths.len(), cut);
        self.write_clipboard(move || backend.copy_files(&paths, cut, owner).map_err(|e| words::windows_failure(e.code())), done, cx);
    }

    /// Puts `text`, the paths of `count` items, on the clipboard.
    pub fn copy_text(&mut self, text: OsString, count: usize, cx: &mut Context<Self>) {
        let (backend, owner) = (self.backend.clone(), self.owner);
        let done = words::ops::paths_copied(count);
        self.write_clipboard(move || backend.copy_text(&text, owner).map_err(|e| words::windows_failure(e.code())), done, cx);
    }

    /// Puts something on the clipboard off the UI thread; a paste asked for meanwhile waits for it, so it
    /// never pastes what was there before.
    fn write_clipboard(&mut self, work: impl FnOnce() -> Result<(), String> + Send + 'static, done: String, cx: &mut Context<Self>) {
        let (which, result) = self.clipboard.write(work);
        // A lane that stopped, as after a write panicked, never says.
        let task = cx
            .background_executor()
            .spawn(async move { result.recv().unwrap_or_else(|_| Err(words::ops::cannot_write_clipboard().into())) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                let written = result.is_ok();
                // What was written is read back at once, rather than when the watch on the clipboard tells.
                if written {
                    this.clipboard.read_again();
                    this.wake.wake();
                }
                cx.emit(match result {
                    Ok(()) => OperationsEvent::Notice(done, Tone::Info),
                    Err(error) => OperationsEvent::Notice(error, Tone::Error),
                });
                if let Some(paste) = this.clipboard.write_ended(which, written) {
                    cx.emit(OperationsEvent::PasteReady(paste));
                }
            })
            .ok();
        })
        .detach();
    }

    /// Sends `paths` to `target`, one of Windows' Send To, as Explorer does: copied, or with `shift` moved
    /// where it allows it.
    pub fn send_to(&mut self, target: PathBuf, paths: Vec<PathBuf>, shift: bool, carried: Option<Carried>, cx: &mut Context<Self>) {
        let name = self.send_targets.iter().find(|t| t.path == target).map_or_else(|| shown(folder_name(&target)), |t| t.name.clone());
        let (backend, owner) = (self.backend.clone(), self.owner);
        let (carry, released) = carried.map_or((None, None), |c| (Some(c.id), c.released));
        let task = cx.background_executor().spawn(async move {
            if let Some(released) = released {
                let _ = released.recv();
            }
            backend.send_to(&target, &paths, shift, owner)
        });
        self.sending += 1;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                this.sending -= 1;
                match result {
                    Ok(Dropped::Refused) => cx.emit(OperationsEvent::Notice(words::ops::refused(&name), Tone::Info)),
                    Ok(_) => {}
                    Err(error) => {
                        cx.emit(OperationsEvent::Notice(words::ops::cannot_send(&name, words::windows_failure(error.code())), Tone::Error))
                    }
                }
                cx.emit(OperationsEvent::Sent(carry));
            })
            .ok();
        })
        .detach();
    }
}

/// How the running operation's events reach the window from the shell's thread.
type OperationEvents = Arc<Mutex<Vec<Event>>>;

/// The file operation under way; they go one at a time.
pub struct Running {
    id: u64,
    /// The tab it started in, which picks what it made once listed.
    tab: crate::tabs::TabId,
    events: OperationEvents,
    /// Where its last item ended, as a new folder's.
    made: Option<PathBuf>,
    /// Where its copied or moved items went.
    placed: Vec<PathBuf>,
}
