//! What the user does to files: copy, cut and paste through the clipboard, rename, make folders, send to
//! the Recycle Bin, open; one operation at a time, each kept in the history with how its items ended.
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
    operations::{CANCELLED, Event, Kind, Request},
    paths::{child, folder_name, is_server, parent_folder, path_key, shown},
    shell::{self, Dropped, SendTarget},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{
    app::{Colet, Tone},
    backend::{self, PathKind},
    clipboard::{Clipboard, Paste},
    places::Dialog,
    settings::Favorite,
    state::{Column, ColumnId, HomeKey, Kind as ColumnKind},
};

/// The most operations kept, and the most items of each.
const MAX_ENTRIES: usize = 100;
const MAX_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Done,
    /// Deleted outright: asked so, or one the Recycle Bin could not take.
    Destroyed,
    Failed,
    Skipped,
    Cancelled,
    /// Handed to the shell to write out, which tells nothing of how it went.
    Handed,
}

impl ItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            ItemStatus::Done => "完了",
            ItemStatus::Destroyed => "完全に削除",
            ItemStatus::Failed => "失敗",
            ItemStatus::Skipped => "スキップ",
            ItemStatus::Cancelled => "キャンセル",
            ItemStatus::Handed => "シェルに渡した",
        }
    }
}

/// One item of an operation, as the history lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryItem {
    /// Where it is now; where it was, when it failed or was deleted.
    pub path: PathBuf,
    /// Where a paste or a rename took it from.
    pub from: Option<PathBuf>,
    pub status: ItemStatus,
    pub error: Option<String>,
    /// Sent to the Recycle Bin or deleted outright: nothing to open.
    pub gone: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Running,
    Done,
    Cancelled,
    Error,
}

/// A file operation Colet ran, kept while it is open.
#[derive(Clone, Debug)]
pub struct HistoryEntry {
    pub id: u64,
    /// What was asked: a paste's paths are left empty, read from the clipboard as it runs.
    pub request: Request,
    /// Milliseconds since the Unix epoch.
    pub started: u64,
    pub status: Status,
    /// The operation's own failure, when no item says it.
    pub error: Option<String>,
    pub items: Vec<HistoryItem>,
    /// Items past the most it keeps, counted only.
    pub more: usize,
    /// How many of those failed and how many were skipped: what it says of its problems counts them too.
    more_failed: usize,
    more_skipped: usize,
}

impl HistoryEntry {
    /// An item's end, added to its operation. Where a copy or move put it follows its result.
    pub fn record(&mut self, event: Event) {
        let (path, status, error) = match event {
            // Said by the item's own end.
            Event::Left { .. } => return,
            Event::Placed { path } => {
                if self.more == 0
                    && let Some(last) = self.items.last_mut()
                {
                    last.from = Some(std::mem::replace(&mut last.path, path));
                }
                return;
            }
            Event::Done { path } => (path, ItemStatus::Done, None),
            Event::Failed { path, error } => (path, ItemStatus::Failed, Some(error)),
            Event::Skipped { path } => (path, ItemStatus::Skipped, None),
            Event::Cancelled { path } => (path, ItemStatus::Cancelled, None),
            Event::Handed { path } => (path, ItemStatus::Handed, None),
            Event::Destroyed { .. } => {
                if self.more == 0
                    && let Some(last) = self.items.last_mut()
                {
                    last.status = ItemStatus::Destroyed;
                }
                return;
            }
        };
        let gone = matches!(self.request.kind, Kind::Recycle | Kind::Delete) && status == ItemStatus::Done;
        if self.items.len() < MAX_ITEMS {
            self.items.push(HistoryItem { path, from: None, status, error, gone });
        } else {
            self.more += 1;
            match status {
                ItemStatus::Failed => self.more_failed += 1,
                ItemStatus::Skipped => self.more_skipped += 1,
                _ => {}
            }
        }
    }

    /// How many of its items ended as `status`; of those past the most it keeps, only the failed and the
    /// skipped are told apart.
    pub fn count(&self, status: ItemStatus) -> usize {
        let more = match status {
            ItemStatus::Failed => self.more_failed,
            ItemStatus::Skipped => self.more_skipped,
            _ => 0,
        };
        self.items.iter().filter(|i| i.status == status).count() + more
    }

    /// What it acted on, in a few words.
    pub fn target(&self) -> String {
        let Request { kind, paths, destination, name } = &self.request;
        match kind {
            Kind::Paste => format!("{} 項目 → {}", self.items.len() + self.more, shown(folder_name(destination))),
            Kind::Recycle | Kind::Delete if paths.len() == 1 => shown(folder_name(&paths[0])),
            Kind::Recycle | Kind::Delete => format!("{} 項目", paths.len()),
            Kind::Rename => format!("{} → {name}", shown(folder_name(paths.first().map_or(Path::new(""), |p| p)))),
            Kind::NewFolder => format!("{name}（{}）", shown(folder_name(destination))),
        }
    }

    /// How it went: what failed or was skipped, its own failure, or that it ran to the end.
    pub fn outcome(&self) -> String {
        if self.status == Status::Running {
            return "処理中".into();
        }
        let mut parts = self.item_problems();
        if self.status == Status::Cancelled {
            parts.push("キャンセル".into());
        }
        if self.status == Status::Error && self.count(ItemStatus::Failed) == 0 {
            parts.push("失敗".into());
        }
        if parts.is_empty() { "完了".into() } else { parts.join("・") }
    }

    /// How many of its items failed and how many were skipped, those there were: `1 件失敗`, `1 件スキップ`.
    fn item_problems(&self) -> Vec<String> {
        let (failed, skipped) = (self.count(ItemStatus::Failed), self.count(ItemStatus::Skipped));
        let mut parts = Vec::new();
        if failed > 0 {
            parts.push(format!("{failed} 件失敗"));
        }
        if skipped > 0 {
            parts.push(format!("{skipped} 件スキップ"));
        }
        parts
    }

    pub fn has_problems(&self) -> bool {
        self.status == Status::Error || self.count(ItemStatus::Failed) + self.count(ItemStatus::Skipped) > 0
    }
}

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

impl Operations {
    /// Watches the clipboard from now on, and reads what Send To offers.
    pub fn new(backend: backend::Shared, owner: shell::Owner, cx: &mut Context<Self>) -> Self {
        let clipboard = Clipboard::default();
        let changed = clipboard.changed_flag();
        backend.watch_clipboard(Box::new(move || changed.store(true, Ordering::Relaxed)));
        let mut this = Self {
            backend,
            owner,
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

    /// Puts down in its entry how operation `id` ended: `problem` is its own failure, and `renamed` where a
    /// rename that told nothing of its item put it. What to say of its problems, counted as not yet seen.
    fn record_end(
        &mut self,
        id: u64,
        kind: Kind,
        cancelled: bool,
        problem: Option<String>,
        renamed: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Option<String> {
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
        let problem = if counts.is_empty() { problem } else { Some(counts.join("・")) };
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
                // None while another app holds it: try again next time.
                match read.await {
                    Some(files) => this.update(cx, |this, cx| {
                        this.clipboard.set_files(files);
                        cx.notify();
                    }),
                    None => this.update(cx, |this, _| this.clipboard.read_again()),
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
        let done = format!("{} 項目を{}しました", paths.len(), if cut { "切り取り" } else { "コピー" });
        self.write_clipboard(move || backend.copy_files(&paths, cut, owner), done, cx);
    }

    /// Puts `text`, the paths of `count` items, on the clipboard.
    pub fn copy_text(&mut self, text: OsString, count: usize, cx: &mut Context<Self>) {
        let (backend, owner) = (self.backend.clone(), self.owner);
        let done = format!("{count} 項目のパスをコピーしました");
        self.write_clipboard(move || backend.copy_text(&text, owner), done, cx);
    }

    /// Puts something on the clipboard off the UI thread; a paste asked for meanwhile waits for it, so it
    /// never pastes what was there before.
    fn write_clipboard(&mut self, work: impl FnOnce() -> Result<(), String> + Send + 'static, done: String, cx: &mut Context<Self>) {
        let (which, result) = self.clipboard.write(work);
        // A lane that stopped, as after a write panicked, never says.
        let task = cx
            .background_executor()
            .spawn(async move { result.recv().unwrap_or_else(|_| Err("クリップボードに書き込めません".into())) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| {
                let written = result.is_ok();
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
                    Ok(Dropped::Refused) => cx.emit(OperationsEvent::Notice(format!("「{name}」は受け付けませんでした"), Tone::Info)),
                    Ok(_) => {}
                    Err(error) => cx.emit(OperationsEvent::Notice(format!("「{name}」に送れません: {error}"), Tone::Error)),
                }
                cx.emit(OperationsEvent::Sent(carry));
            })
            .ok();
        })
        .detach();
    }
}

/// The file operations run since Colet opened.
#[derive(Default)]
pub struct History {
    last_id: u64,
    /// Newest first.
    pub entries: Vec<HistoryEntry>,
    /// Operations with problems since the history was last opened.
    pub unseen: usize,
}

impl History {
    /// Keeps an operation just started at the top, running; its id.
    pub fn start(&mut self, request: Request) -> u64 {
        self.last_id += 1;
        let id = self.last_id;
        let entry = HistoryEntry {
            id,
            request,
            started: now(),
            status: Status::Running,
            error: None,
            items: Vec::new(),
            more: 0,
            more_failed: 0,
            more_skipped: 0,
        };
        self.entries.insert(0, entry);
        self.entries.truncate(MAX_ENTRIES);
        id
    }

    pub fn entry_mut(&mut self, id: u64) -> Option<&mut HistoryEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// Lets go of the operations that are over.
    pub fn clear(&mut self) {
        self.entries.retain(|e| e.status == Status::Running);
    }
}

/// How the window shows the history: Colet's own, as it draws it.
#[derive(Default)]
pub struct HistoryPanel {
    pub open: bool,
    /// The operations it shows opened.
    pub expanded: HashSet<u64>,
    /// The item last pressed in it, while the disk is asked whether it is still there.
    pub(crate) revealing: Option<PathBuf>,
}

impl HistoryPanel {
    /// Opened, with `entry` shown opened; closed, a lookup still out for an item pressed is over, and
    /// opening it again does not revive it.
    pub fn set_open(&mut self, open: bool, entry: Option<u64>) {
        self.open = open;
        self.expanded = entry.into_iter().collect();
        if !open {
            self.revealing = None;
        }
    }

    /// Shows `entry` opened, or closed if it was.
    pub fn toggle(&mut self, entry: u64) {
        if !self.expanded.remove(&entry) {
            self.expanded.insert(entry);
        }
    }
}

/// Where a menu opened by a right-click or the menu key stands, and what it acts on.
pub struct Menu {
    pub column: ColumnId,
    /// `None` for the column's selection; empty from its empty space, for the folder itself.
    pub paths: Option<Vec<PathBuf>>,
    /// In the window.
    pub at: Point<Pixels>,
    /// On the screen, for the Windows menu.
    pub screen: (i32, i32),
}

/// Why an action is off, as its button and menu item say; `None` while it is on.
pub struct Reasons {
    pub copy: Option<&'static str>,
    pub cut: Option<&'static str>,
    pub paste: Option<&'static str>,
    pub rename: Option<&'static str>,
    pub delete: Option<&'static str>,
    pub explorer: Option<&'static str>,
    pub shell: Option<&'static str>,
    pub copy_path: Option<&'static str>,
    pub send_to: Option<&'static str>,
    pub properties: Option<&'static str>,
}

const NEEDS_SELECTION: &str = "項目を選択すると使えます";
pub const NEEDS_FOLDER: &str = "フォルダーのカラムで使えます";
const BUSY: &str = "ファイル操作の完了後に使えます";
const NOT_AT_HOME: &str = "ホームでは使えません";
pub const NOT_AT_SERVER: &str = "サーバーの共有一覧では使えません";
pub const ONE_ITEM: &str = "1項目を選択したときに使えます";
pub const NEEDS_FILE: &str = "ファイルを選択したときに使えます";
pub const SAME_FOLDER: &str = "同じフォルダーの項目を選んだときに使えます";
const NO_SEND_TARGETS: &str = "送るフォルダーに送り先がありません";
const NO_SEND_DRIVE: &str = "ドライブは送れません";
const NO_COPY_SAVED: &str = "保存した検索はコピーできません";
const NO_COPY_DRIVE: &str = "ドライブはコピーできません";
const NOT_FOR_SAVED: &str = "保存した検索には使えません";
/// The fewest files opened together that are asked about first, as Explorer stops offering to open them
/// past 15.
pub const ASK_TO_OPEN: usize = 16;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Colet {
    /// A menu keeps the column it opened on even if another column takes its place in the chain.
    fn action_column(&self) -> Option<&Column> {
        let index = match self.menu() {
            Some(menu) => self.state.index_of(menu.column)?,
            None => self.state.active,
        };
        self.state.columns.get(index)
    }

    /// What the file actions work on: the selection shown, or while the menu is open, what it was opened on.
    pub fn picked(&self) -> Vec<PathBuf> {
        self.picked_paths().into_owned()
    }

    /// [`picked`](Self::picked), lent rather than copied where it can be, for what is asked each frame.
    fn picked_paths(&self) -> Cow<'_, [PathBuf]> {
        let Some(column) = self.action_column() else { return Cow::Borrowed(&[]) };
        match self.menu() {
            Some(Menu { paths: Some(paths), .. }) => Cow::Borrowed(paths),
            _ => column.shown_selected(),
        }
    }

    pub fn picked_entries(&self) -> Vec<Entry> {
        let Some(c) = self.action_column() else { return Vec::new() };
        let picked: HashSet<PathBuf> = self.picked().into_iter().collect();
        c.entries.iter().filter(|e| picked.contains(&e.path)).map(|e| Entry::clone(e)).collect()
    }

    /// The folder column worked in's own folder, or server.
    pub fn folder_here(&self) -> Option<PathBuf> {
        let c = self.action_column()?;
        (c.kind == ColumnKind::Folder).then(|| c.path.clone())
    }

    /// Where a paste or a new folder goes: the folder column worked in, unless it lists a server's shares,
    /// which are only looked at.
    pub fn destination(&self) -> Option<PathBuf> {
        self.folder_here().filter(|path| !is_server(path))
    }

    /// The column worked in lists a server's shares.
    fn at_server(&self) -> bool {
        self.action_column().is_some_and(|c| c.kind == ColumnKind::Folder && is_server(&c.path))
    }

    /// Asked as the buttons are drawn, each frame: it walks the selection, which may be thousands, without
    /// making a string, and stops at the first answer.
    pub fn reasons(&self, cx: &App) -> Reasons {
        fn parent(p: &Path) -> &Path {
            parent_folder(p).unwrap_or(Path::new(""))
        }

        let operations = self.operations.read(cx);
        let picked = self.picked_paths();
        let at_home = self.action_column().is_none_or(|c| c.kind == ColumnKind::Home);
        let at_server = self.at_server();
        let search = picked.iter().any(|p| HomeKey::of(p).is_search());
        let none = picked.is_empty();
        let drive = picked.iter().any(|p| !p.as_os_str().is_empty() && parent_folder(&HomeKey::on_disk(p)).is_none());
        let apart = picked.first().map(|p| parent(p)).is_some_and(|first| picked.iter().any(|p| parent(p) != first));

        // What works on the items picked needs some, and not in home or a server's shares.
        let item = none.then_some(NEEDS_SELECTION).or(at_home.then_some(NOT_AT_HOME)).or(at_server.then_some(NOT_AT_SERVER));
        let busy = operations.is_running().then_some(BUSY);
        // What works on a place: with nothing picked, the folder column's own folder; else the items', all
        // in one folder.
        let place = if none { self.folder_here().is_none().then_some(NEEDS_FOLDER) } else { apart.then_some(SAME_FOLDER) };
        Reasons {
            copy: item.or(search.then_some(NO_COPY_SAVED)).or(drive.then_some(NO_COPY_DRIVE)),
            cut: item,
            paste: busy.or(at_server.then_some(NOT_AT_SERVER)).or_else(|| self.destination().is_none().then_some(NEEDS_FOLDER)),
            // One saved search picked alone can always be renamed.
            rename: if picked.len() == 1 && search { None } else { busy.or(item).or((picked.len() != 1).then_some(ONE_ITEM)) },
            delete: busy.or(item),
            explorer: place,
            shell: search.then_some(NOT_FOR_SAVED).or(at_server.then_some(NOT_AT_SERVER)).or(place),
            // With nothing picked, the folder column's own folder.
            copy_path: if none { place } else { None },
            // From several folders too, as a drag from them.
            send_to: item.or(drive.then_some(NO_SEND_DRIVE)).or(operations.send_targets().is_empty().then_some(NO_SEND_TARGETS)),
            // Only looked at: in home and while an operation runs too, and from several folders. With
            // nothing picked, the folder column's own folder, a server's not.
            properties: if none {
                at_server.then_some(NOT_AT_SERVER).or(self.folder_here().is_none().then_some(NEEDS_FOLDER))
            } else {
                search.then_some(NOT_FOR_SAVED)
            },
        }
    }

    /// Says why a shortcut did nothing, as the buttons and menu items say it.
    fn refuse(&mut self, title: &str, reason: &str, cx: &mut Context<Self>) {
        self.notify(format!("「{title}」は{reason}"), Tone::Info, cx);
    }

    pub fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        // What the menu was opened on, read before it closes: after, the selection would stand in.
        let (picked, reasons) = (self.picked(), self.reasons(cx));
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = if cut { reasons.cut } else { reasons.copy } {
            self.refuse(if cut { "切り取り" } else { "コピー" }, reason, cx);
            return;
        }
        let picked: Vec<PathBuf> = picked.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        self.operations.update(cx, |operations, cx| operations.copy_files(picked, cut, cx));
    }

    /// The paths as text, one a line; a saved search's is the query it runs, a narrowed favorite's its
    /// folder's. With nothing picked, the folder column's own.
    pub fn copy_paths(&mut self, cx: &mut Context<Self>) {
        let mut picked = self.picked();
        if picked.is_empty() {
            picked.extend(self.folder_here());
        }
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        // As the paths are, even where they are not text: a pasted one still names its item.
        let mut text = OsString::new();
        for (i, p) in picked.iter().enumerate() {
            if i > 0 {
                text.push("\r\n");
            }
            match HomeKey::of(p) {
                HomeKey::Search { query, .. } => text.push(query),
                HomeKey::Folder { .. } => text.push(HomeKey::on_disk(p).as_os_str()),
            }
        }
        let count = picked.len();
        self.operations.update(cx, |operations, cx| operations.copy_text(text, count, cx));
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        self.take_menu();
        match self.reasons(cx).paste {
            Some(reason) => self.refuse("貼り付け", reason, cx),
            None => {
                let paste = Paste { destination: self.destination().unwrap_or_default(), tab: self.current_tab() };
                if let Some(paste) = self.operations.update(cx, |operations, _| operations.clipboard.paste(paste)) {
                    self.run_paste(paste, cx);
                }
            }
        }
    }

    /// F2: a rename, or for a saved search or favorite in home, its edit.
    pub fn ask_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, reasons, entries) = (self.picked(), self.reasons(cx), self.picked_entries());
        let at_home = self.action_column().is_some_and(|c| c.kind == ColumnKind::Home);
        self.take_menu();
        if let [path] = picked.as_slice() {
            if HomeKey::of(path).is_search() {
                self.open_edit(path, window, cx);
                return;
            }
            if at_home && self.stored.settings().is_favorite(path) {
                self.open_favorite_edit(path, window, cx);
                return;
            }
        }
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = reasons.rename {
            self.refuse("名前を変更", reason, cx);
            return;
        }
        let Some(entry) = entries.into_iter().next() else { return };
        // Like Explorer, the extension stays out of the selection.
        let end = if entry.directory { entry.name.len() } else { entry.name.rfind('.').filter(|&d| d > 0).unwrap_or(entry.name.len()) };
        let request = Request { kind: Kind::Rename, paths: vec![entry.path], destination: PathBuf::new(), name: entry.name };
        self.ask_name(request, end, window, cx);
    }

    pub fn ask_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.take_menu();
        if let Some(reason) = self.reasons(cx).paste {
            self.refuse("新規フォルダー", reason, cx);
            return;
        }
        let destination = self.destination().unwrap_or_default();
        let name = "新しいフォルダー".to_owned();
        let end = name.len();
        self.ask_name(Request { kind: Kind::NewFolder, paths: Vec::new(), destination, name }, end, window, cx);
    }

    /// The name dialog for `request`, starting from its name with the first `select_end` bytes selected.
    fn ask_name(&mut self, request: Request, select_end: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        let input = cx.new(|cx| InputState::new(window, cx).default_value(request.name.clone()));
        let subscription = cx.subscribe_in(&input, window, |this: &mut Self, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => this.submit_name(window, cx),
            InputEvent::Change => {
                if let Some(Dialog::Name { tried, checking, .. }) = this.overlays.dialog_mut() {
                    *tried = false;
                    // The answer on its way is about a name no longer there.
                    *checking = None;
                }
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
        });
        self.palette.style_input(&input, cx);
        self.overlays.listen_in_dialog(subscription);
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(0..select_end, cx);
        });
        self.overlays.set_dialog(Dialog::Name { request, input, tried: false, taken: None, checking: None });
        cx.notify();
    }

    /// Why the name in the dialog cannot be used, as it stands.
    pub fn name_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Name { input, tried, taken, .. }) = self.overlays.dialog() else { return None };
        let name = input.read(cx).value();
        colet_core::operations::name_problem(&name, *tried)
            .or(taken.as_deref().filter(|&taken| taken == name.as_ref()).map(|_| "同じ名前の項目があります"))
    }

    pub fn submit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { request, input, tried, checking, .. }) = self.overlays.dialog_mut() else { return };
        let name = input.read(cx).value().to_string();
        // Pressed again while the folder is asked about this very name: the answer sends it.
        if name.trim().is_empty() || checking.as_deref() == Some(name.as_str()) {
            return;
        }
        // Like Explorer, an unchanged name just closes the dialog: one that is not all text, as it was
        // shown, too, rather than renamed to what stood in for what is not.
        if request.kind == Kind::Rename && request.paths.first().is_some_and(|p| shown(folder_name(p)) == name) {
            self.close_dialog(window, cx);
            return;
        }
        // Say what the name still lacks rather than send it.
        if colet_core::operations::name_problem(&name, true).is_some() {
            *tried = true;
            cx.notify();
            return;
        }
        // Nor a name its folder already has, unless it is the item's own in another case. The folder is asked
        // off the UI thread, as a share can keep it waiting; the dialog stays up until it answers.
        if request.kind == Kind::Rename
            && let Some(path) = request.paths.first()
            && shown(folder_name(path)).to_lowercase() != name.to_lowercase()
            && let Some(parent) = parent_folder(path)
        {
            let target = child(parent, &name);
            *checking = Some(name.clone());
            let (backend, dialog) = (self.backend.clone(), input.entity_id());
            let asked = cx.background_executor().spawn(async move { backend.path_kind(&target) != PathKind::Gone });
            cx.spawn_in(window, async move |this, cx| {
                let exists = asked.await;
                this.update_in(cx, |this, window, cx| this.name_checked(dialog, name, exists, window, cx)).ok();
            })
            .detach();
            cx.notify();
            return;
        }
        self.send_name(name, window, cx);
    }

    /// The folder answered whether it has `name`: taken, the dialog says so; free, it is sent. Only while the
    /// same dialog still waits on that name: closed, or typed on since, the answer is about nothing shown.
    fn name_checked(&mut self, dialog: EntityId, name: String, exists: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { input, taken, checking, .. }) = self.overlays.dialog_mut() else { return };
        if input.entity_id() != dialog || checking.as_deref() != Some(name.as_str()) {
            return;
        }
        *checking = None;
        if exists {
            *taken = Some(name);
            cx.notify();
            return;
        }
        self.send_name(name, window, cx);
    }

    fn send_name(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { request, .. }) = self.overlays.dialog() else { return };
        let request = Request { name, ..request.clone() };
        self.close_dialog(window, cx);
        self.run(request, cx);
    }

    /// Delete sends to the Recycle Bin and Shift+Delete deletes outright, as in Explorer: whether to ask
    /// first, and what to do with an item the bin cannot take, is the shell's.
    pub fn delete_picked(&mut self, permanent: bool, cx: &mut Context<Self>) {
        let (picked, reasons) = (self.picked(), self.reasons(cx));
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        let kind = if permanent { Kind::Delete } else { Kind::Recycle };
        if let Some(reason) = reasons.delete {
            let hint = if reason != NOT_AT_HOME {
                ""
            } else if picked.iter().any(|p| HomeKey::of(p).is_search()) {
                "保存した検索は右クリックメニューの「保存した検索を解除」で外せます。"
            } else {
                "お気に入りは右クリックメニューの「お気に入りを解除」で外せます。"
            };
            self.notify(format!("「{}」は{reason}。{hint}", kind.title()), Tone::Info, cx);
            return;
        }
        self.run(Request { kind, paths: picked, destination: PathBuf::new(), name: String::new() }, cx);
    }

    /// Opens the picked item: a folder beside it, a file in its app. Of several, the files each open in
    /// their apps and the folders are passed over.
    pub fn open_picked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.action_column().and_then(|c| self.state.index_of(c.id));
        let picked = self.picked_entries();
        self.take_menu();
        match (index, picked.as_slice()) {
            (Some(index), [entry]) => self.open(index, &entry.path, window, cx),
            _ => {
                let files: Vec<PathBuf> = picked.into_iter().filter(|e| !e.directory).map(|e| e.path).collect();
                if files.len() >= ASK_TO_OPEN {
                    self.begin_dialog(window, cx);
                    self.overlays.set_dialog(Dialog::Open { paths: files });
                    self.stop("confirm-open", cx).focus(window, cx);
                    cx.notify();
                } else {
                    self.open_files(files, cx);
                }
            }
        }
    }

    /// Opens the files asked about.
    pub fn confirm_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Open { paths }) = self.overlays.take_dialog() else { return };
        self.close_dialog(window, cx);
        self.open_files(paths, cx);
    }

    /// Opens `paths` one after another in their apps, in the order given; one that fails leaves the rest to
    /// open, and those that failed are told of together.
    fn open_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let (backend, owner) = (self.backend.clone(), self.owner);
        self.in_background(
            move || {
                let failed: Vec<(String, String)> = paths
                    .iter()
                    .filter_map(|path| backend.open(path, owner).err().map(|error| (shown(folder_name(path)), error)))
                    .collect();
                match failed.as_slice() {
                    [] => Ok(()),
                    [(name, error)] => Err(format!("「{name}」を開けません: {error}")),
                    [(_, error), ..] => {
                        let names: Vec<String> = failed.iter().take(3).map(|(name, _)| format!("「{name}」")).collect();
                        let more = if failed.len() > 3 { format!("ほか {} 項目", failed.len() - 3) } else { String::new() };
                        Err(format!("{} 項目を開けません: {}{more}（{error}）", failed.len(), names.join("")))
                    }
                }
            },
            None,
            cx,
        );
    }

    /// Shows the picked items, all in one folder, selected together in it; with nothing picked, the folder
    /// column's own folder, open on what it holds.
    pub fn reveal_in_explorer(&mut self, cx: &mut Context<Self>) {
        let (picked, folder, refused) = (self.picked_entries(), self.folder_here(), self.reasons(cx).explorer);
        self.take_menu();
        if refused.is_some() {
            return;
        }
        let backend = self.backend.clone();
        match folder {
            Some(folder) if picked.is_empty() => {
                let owner = self.owner;
                self.in_background(move || backend.open(&folder, owner).map_err(|error| format!("開けません: {error}")), None, cx);
            }
            _ if !picked.is_empty() => {
                let paths: Vec<PathBuf> = picked.into_iter().map(|e| HomeKey::on_disk(&e.path).into_owned()).collect();
                self.in_background(move || backend.reveal(&paths), None, cx);
            }
            _ => {}
        }
    }

    /// Windows' properties of what is picked, several together in one dialog; with nothing picked, of the
    /// folder column's own folder.
    pub fn show_properties(&mut self, cx: &mut Context<Self>) {
        // What the menu was opened on, read before it closes: after, the selection would stand in.
        let (mut picked, refused) = (self.picked(), self.reasons(cx).properties);
        self.take_menu();
        if let Some(reason) = refused {
            self.refuse("プロパティ", reason, cx);
            return;
        }
        if picked.is_empty() {
            picked.extend(self.folder_here());
        }
        let paths: Vec<PathBuf> = picked.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        let (backend, owner) = (self.backend.clone(), self.owner);
        self.in_background(move || backend.properties(&paths, owner), None, cx);
    }

    /// The Windows (Explorer) menu, where this one was opened. Dismisses Colet's menu and returns its
    /// keys even when the column it belonged to has gone.
    pub fn show_shell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Read while the menu is open: its column and paths are what the reasons are for.
        let refused = self.reasons(cx).shell;
        let Some(menu) = self.take_menu() else { return };
        let index = self.state.index_of(menu.column);
        if let Some(index) = index {
            self.state.set_active(index);
        }
        let to_column = !self.overlays.has_dialog() && self.menu_has_keys(window, cx);
        self.close_menus(to_column, window, cx);
        let Some(index) = index else { return };
        if let Some(reason) = refused {
            self.notify(reason, Tone::Info, cx);
            return;
        }
        let paths = match &menu.paths {
            Some(paths) => paths.clone(),
            None => self.state.columns[index].shown_selected().into_owned(),
        };
        let paths: Vec<PathBuf> = paths.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        let background = if paths.is_empty() { self.state.columns[index].path.clone() } else { PathBuf::new() };
        let (backend, owner, at) = (self.backend.clone(), self.owner, menu.screen);
        let task = cx.background_executor().spawn(async move { backend.shell_menu(paths, background, at, owner) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                this.update(cx, |this, cx| this.notify(error, Tone::Error, cx)).ok();
            }
            // What it ran may have changed the folders.
            this.update(cx, |this, cx| this.refresh(cx)).ok();
        })
        .detach();
    }

    /// What the file operations tell of on their own, as what they set off ends.
    pub(crate) fn on_operations_event(&mut self, event: &OperationsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            OperationsEvent::Notice(text, tone) => self.notify(text.clone(), *tone, cx),
            OperationsEvent::PasteReady(paste) => self.run_paste(paste.clone(), cx),
            OperationsEvent::SendTargetsChanged => {
                if self.menu().is_some() {
                    self.show_menu(window, cx);
                }
            }
            // What it moved has left its folder.
            OperationsEvent::Sent(Some(carried)) => self.put_down(*carried, cx),
            OperationsEvent::Sent(None) => self.refresh(cx),
        }
    }

    /// Sends the picked items to `target`, one of Windows' Send To, as Explorer does: copied, or with
    /// `shift` moved where it allows it.
    pub fn send_to(&mut self, target: PathBuf, shift: bool, cx: &mut Context<Self>) {
        let (picked, refused) = (self.picked(), self.reasons(cx).send_to);
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = refused {
            self.refuse("送る", reason, cx);
            return;
        }
        let paths: Vec<PathBuf> = picked.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        // What may be moved goes unwatched first, with the folders inside it, until the send returns.
        let carried = shift.then(|| self.carry(&paths, cx));
        self.operations.update(cx, |operations, cx| operations.send_to(target, paths, shift, carried, cx));
    }

    /// Lets go of the watches on the folders at or inside `paths`, which a send or a drag may move, until
    /// [`put_down`](Self::put_down) is told the number; what hears once they are let go of.
    pub(crate) fn carry(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) -> Carried {
        let id = self.loading.watching.carry(paths);
        Carried { id, released: self.watch(cx) }
    }

    /// What [`carry`](Self::carry) let go of is watched again, by the refresh this makes, which lists what
    /// changed while it was not; folders moved away meanwhile are closed rather than watched.
    pub(crate) fn put_down(&mut self, carry: u64, cx: &mut Context<Self>) {
        self.loading.watching.put_down(carry);
        self.refresh(cx);
    }

    /// Pastes as `paste` asks, for the tab it was asked in, whichever is in front now: that tab picks what
    /// it pastes, unless closed since.
    fn run_paste(&mut self, paste: Paste, cx: &mut Context<Self>) {
        let request = Request { kind: Kind::Paste, paths: Vec::new(), destination: paste.destination, name: String::new() };
        self.run_in(request, paste.tab, cx);
    }

    /// Runs one file operation and keeps it in the history; a second waits for none, it is refused.
    pub fn run(&mut self, request: Request, cx: &mut Context<Self>) {
        let tab = self.current_tab();
        self.run_in(request, tab, cx);
    }

    /// Runs `request` for the tab `tab`, which picks what it makes.
    fn run_in(&mut self, request: Request, tab: crate::tabs::TabId, cx: &mut Context<Self>) {
        let Some(events) = self.operations.update(cx, |operations, cx| operations.begin(&request, tab, cx)) else {
            self.refuse(request.kind.title(), BUSY, cx);
            return;
        };
        let (backend, owner, asked) = (self.backend.clone(), self.owner, request.clone());
        cx.spawn(async move |this, cx| {
            // A paste reads the clipboard once, and both lets go of and pastes what it read: what was last
            // seen on it may be a moment old.
            let clipboard = if asked.kind == Kind::Paste {
                let backend = backend.clone();
                cx.background_executor().spawn(async move { backend.clipboard_files() }).await
            } else {
                None
            };
            // What leaves its place goes unwatched first, with the folders inside it.
            let Ok(released) = this.update(cx, |this, cx| {
                let operations = this.operations.read(cx);
                let leaving = match asked.kind {
                    Kind::Recycle | Kind::Delete | Kind::Rename => asked.paths.as_slice(),
                    Kind::Paste => match &clipboard {
                        Some(files) if files.cut => files.paths.as_slice(),
                        Some(_) => &[],
                        // Unread, as while another app holds it: the paste reads it again, and most likely
                        // fails as this did.
                        None if operations.clipboard.files.cut => operations.clipboard.files.paths.as_slice(),
                        None => &[],
                    },
                    Kind::NewFolder => &[],
                };
                this.loading.watching.released = leaving.iter().map(|p| path_key(p)).collect();
                this.watch(cx)
            }) else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some(released) = released {
                        let _ = released.recv();
                    }
                    backend.operate(
                        asked,
                        clipboard,
                        owner,
                        Box::new(move |event| events.lock().unwrap_or_else(|e| e.into_inner()).push(event)),
                    )
                })
                .await;
            this.update(cx, |this, cx| this.finished(request, result, cx)).ok();
        })
        .detach();
    }

    /// Home's favorites and pins at `from` or inside it, whatever their case, move to `to`; a narrowed
    /// favorite's folder moves under its narrowing. Two that then name one row keep the first.
    pub(crate) fn follow_move(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let moved_path = |p: &Path| colet_core::paths::moved(p, from, to);
        let moved_key = |key: &PathBuf| {
            match HomeKey::of(key) {
                HomeKey::Folder { path, narrow } => moved_path(&path).map(|path| HomeKey::Folder { path: path.into(), narrow }.key()),
                HomeKey::Search { .. } => None,
            }
            .unwrap_or_else(|| key.clone())
        };
        let pinned: Vec<PathBuf> = self.stored.settings().pinned.iter().map(moved_key).collect();
        let pinned = (pinned != self.stored.settings().pinned).then_some(pinned);
        let favorites: Vec<Favorite> = self
            .stored
            .settings()
            .favorites
            .iter()
            .map(|f| Favorite { path: moved_path(&f.path).unwrap_or_else(|| f.path.clone()), ..f.clone() })
            .collect();
        let favorites = (favorites != self.stored.settings().favorites).then_some(favorites);
        if favorites.is_some() || pinned.is_some() {
            // Settling the settings keeps the first of any two that now name one row.
            self.update_settings(
                |s| {
                    if let Some(favorites) = favorites {
                        s.favorites = favorites;
                    }
                    if let Some(pinned) = pinned {
                        s.pinned = pinned;
                    }
                },
                cx,
            );
        }
    }

    fn finished(&mut self, request: Request, result: Result<(), String>, cx: &mut Context<Self>) {
        let Some(Running { id, tab, made, placed: pasted, .. }) = self.operations.update(cx, |operations, cx| operations.end(cx)) else {
            return;
        };
        // Watched again by the refresh below, which lists what changed while they were not.
        self.loading.watching.released.clear();
        let Request { kind, paths, destination, name } = request;
        let (cancelled, problem) = match &result {
            Ok(()) => (false, None),
            Err(error) if error == CANCELLED => (true, None),
            Err(error) => (false, Some(error.clone())),
        };
        // Where the shell says it put it, under the name it got, which may not be the one asked; one that
        // says nothing of it is taken at the name asked, once its item is done.
        let told = kind == Kind::Rename && !pasted.is_empty();
        let renamed = (kind == Kind::Rename && result.is_ok())
            .then(|| {
                pasted
                    .first()
                    .cloned()
                    .or_else(|| made.as_ref().and(paths.first()).map(|p| child(parent_folder(p).unwrap_or(Path::new("")), &name)))
            })
            .flatten();
        if let Some(to) = &renamed {
            // Carry the selection over to the new name before the listing drops the old one, in the tab it
            // was renamed from; a tab closed since has no selection to carry.
            if let Some(state) = self.state_in(tab) {
                state.renamed(&paths[0], to);
            }
            // Home's favorites and pins at it or inside it follow it, as the refresh below lists home again.
            self.follow_move(&paths[0], to, cx);
        }
        if kind == Kind::NewFolder && result.is_ok() {
            // The shell says where it made the folder, which a name clash may have changed.
            if let Some(state) = self.state_in(tab) {
                state.created(&destination, &made.unwrap_or_else(|| child(&destination, &name)));
            }
        }
        // Told where it went, the item already says so.
        let renamed = renamed.filter(|_| !told);
        let problem = self.operations.update(cx, |operations, cx| operations.record_end(id, kind, cancelled, problem, renamed, cx));
        if let Some(problem) = problem {
            self.notify_entry(format!("「{}」: {problem}", kind.title()), id, cx);
        }
        // Like Explorer, what was pasted is picked once listed, under the names it got.
        if kind == Kind::Paste
            && !pasted.is_empty()
            && let Some(state) = self.state_in(tab)
        {
            state.pasted(&destination, &pasted);
        }
        self.refresh(cx);
        self.changed(cx);
    }

    /// Opened, the history has been seen; closed, the keys go back to where they were as it opened.
    pub fn show_history(&mut self, open: bool, entry: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.history_panel.open;
        if open && !was_open {
            self.remember_keys(window, cx);
        }
        self.history_panel.set_open(open, entry);
        if open {
            self.operations.update(cx, |operations, cx| {
                operations.history.unseen = 0;
                cx.notify();
            });
        }
        if !open && was_open {
            self.restore_keys(window, cx);
        }
        cx.notify();
    }

    /// Only what is still there: an item moved on or deleted since says so. Whether it is, is asked off the UI
    /// thread, as a share can keep it waiting.
    pub fn reveal_from_history(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.history_panel.revealing = Some(path.clone());
        let backend = self.backend.clone();
        let asked = {
            let path = path.clone();
            cx.background_executor().spawn(async move { backend.path_kind(&path) })
        };
        cx.spawn_in(window, async move |this, cx| {
            let kind = asked.await;
            this.update_in(cx, |this, window, cx| this.history_item_found(path, kind, window, cx)).ok();
        })
        .detach();
    }

    /// The answer for the item last pressed in the history, while the history is still open: another pressed
    /// since, or the history closed, has moved on from it.
    fn history_item_found(&mut self, path: PathBuf, kind: PathKind, window: &mut Window, cx: &mut Context<Self>) {
        if self.history_panel.revealing.as_ref() != Some(&path) {
            return;
        }
        self.history_panel.revealing = None;
        if !self.history_panel.open {
            return;
        }
        match kind {
            PathKind::Gone => self.notify(format!("「{}」はもうありません", shown(folder_name(&path))), Tone::Info, cx),
            PathKind::Folder | PathKind::File => {
                self.show_history(false, None, window, cx);
                self.state.go_to_path(&path, kind == PathKind::Folder);
                self.sync_search_field(window, cx);
                self.focus_active(window, cx);
            }
        }
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

#[cfg(test)]
mod tests {
    use super::{Event, HistoryEntry, ItemStatus, Kind, Request, Status};

    fn entry(kind: Kind) -> HistoryEntry {
        HistoryEntry {
            id: 1,
            request: Request { kind, paths: vec![r"D:\a.txt".into()], destination: r"D:\dest".into(), name: String::new() },
            started: 0,
            status: Status::Running,
            error: None,
            items: Vec::new(),
            more: 0,
            more_failed: 0,
            more_skipped: 0,
        }
    }

    #[test]
    fn records_where_a_paste_put_each_item_and_what_failed() {
        let mut paste = entry(Kind::Paste);
        paste.record(Event::Done { path: r"D:\a.txt".into() });
        paste.record(Event::Placed { path: r"D:\dest\a.txt".into() });
        paste.record(Event::Failed { path: r"D:\b.txt".into(), error: "アクセスが拒否されました。".into() });
        paste.record(Event::Skipped { path: r"D:\c.txt".into() });
        assert_eq!(paste.items[0].path, std::path::Path::new(r"D:\dest\a.txt"));
        assert_eq!(paste.items[0].from.as_deref(), Some(std::path::Path::new(r"D:\a.txt")));
        assert_eq!(paste.outcome(), "処理中");
        paste.status = Status::Done;
        assert_eq!(paste.outcome(), "1 件失敗・1 件スキップ");
        assert!(paste.has_problems());
        assert_eq!(paste.target(), "3 項目 → dest");
        assert_eq!(paste.count(ItemStatus::Done), 1);
    }

    #[test]
    fn counts_what_failed_or_was_skipped_past_the_items_it_keeps() {
        let mut paste = entry(Kind::Paste);
        for i in 0..super::MAX_ITEMS {
            paste.record(Event::Done { path: format!(r"D:\{i}.txt").into() });
        }
        paste.record(Event::Skipped { path: r"D:\skipped.txt".into() });
        paste.record(Event::Failed { path: r"D:\failed.txt".into(), error: "アクセスが拒否されました。".into() });
        assert_eq!((paste.items.len(), paste.more), (super::MAX_ITEMS, 2));
        paste.status = Status::Done;
        assert_eq!(paste.outcome(), "1 件失敗・1 件スキップ");
        assert!(paste.has_problems());
    }

    #[test]
    fn lists_what_was_handed_to_the_shell_by_its_name_without_a_problem() {
        let mut paste = entry(Kind::Paste);
        paste.record(Event::Handed { path: r"D:\dest\添付.txt".into() });
        assert_eq!((paste.items[0].path.to_str().unwrap(), paste.items[0].status), (r"D:\dest\添付.txt", ItemStatus::Handed));
        paste.status = Status::Done;
        assert!(!paste.has_problems());
        assert_eq!(paste.target(), "1 項目 → dest");
    }

    #[test]
    fn marks_what_went_to_the_bin_as_gone() {
        let mut recycle = entry(Kind::Recycle);
        recycle.record(Event::Done { path: r"D:\a.txt".into() });
        assert!(recycle.items[0].gone);
        recycle.status = Status::Done;
        assert_eq!(recycle.outcome(), "完了");
        assert_eq!(recycle.target(), "a.txt");
    }
}
