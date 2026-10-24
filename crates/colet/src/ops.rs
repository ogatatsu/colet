//! What the user does to files: copy, cut and paste through the clipboard, rename, make folders, send to
//! the Recycle Bin, open; one operation at a time, each kept in the history with how its items ended.
use std::{
    collections::HashSet,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use colet_core::{
    entry::Entry,
    operations::{CANCELLED, Event, Kind, Request},
    paths::{ancestors, child, folder_name, parent_folder, path_key},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{
    app::{Colet, Tone},
    places::Dialog,
    state::{Column, ColumnId, Kind as ColumnKind, saved_query},
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
    pub path: String,
    /// Where a paste or a rename took it from.
    pub from: Option<String>,
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
        }
    }

    pub fn count(&self, status: ItemStatus) -> usize {
        self.items.iter().filter(|i| i.status == status).count()
    }

    /// What it acted on, in a few words.
    pub fn target(&self) -> String {
        let Request { kind, paths, destination, name } = &self.request;
        match kind {
            Kind::Paste => format!("{} 項目 → {}", self.items.len() + self.more, folder_name(destination)),
            Kind::Recycle | Kind::Delete if paths.len() == 1 => folder_name(&paths[0]).to_owned(),
            Kind::Recycle | Kind::Delete => format!("{} 項目", paths.len()),
            Kind::Rename => format!("{} → {name}", folder_name(paths.first().map_or("", |p| p))),
            Kind::NewFolder => format!("{name}（{}）", folder_name(destination)),
        }
    }

    /// How it went: what failed or was skipped, its own failure, or that it ran to the end.
    pub fn outcome(&self) -> String {
        if self.status == Status::Running {
            return "処理中".into();
        }
        let (failed, skipped) = (self.count(ItemStatus::Failed), self.count(ItemStatus::Skipped));
        let mut parts = Vec::new();
        if failed > 0 {
            parts.push(format!("{failed} 件失敗"));
        }
        if skipped > 0 {
            parts.push(format!("{skipped} 件スキップ"));
        }
        if self.status == Status::Cancelled {
            parts.push("キャンセル".into());
        }
        if self.status == Status::Error && failed == 0 {
            parts.push("失敗".into());
        }
        if parts.is_empty() { "完了".into() } else { parts.join("・") }
    }

    pub fn has_problems(&self) -> bool {
        self.status == Status::Error || self.items.iter().any(|i| matches!(i.status, ItemStatus::Failed | ItemStatus::Skipped))
    }
}

/// Where a menu opened by a right-click or the menu key stands, and what it acts on.
pub struct Menu {
    pub column: ColumnId,
    /// `None` for the column's selection; empty from its empty space, for the folder itself.
    pub paths: Option<Vec<String>>,
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
}

const NEEDS_SELECTION: &str = "項目を選択すると使えます";
pub const NEEDS_FOLDER: &str = "フォルダーのカラムで使えます";
const BUSY: &str = "ファイル操作の完了後に使えます";
const NOT_AT_HOME: &str = "ホームでは使えません";
pub const ONE_ITEM: &str = "1項目を選択したときに使えます";
pub const NEEDS_FILE: &str = "ファイルを選択したときに使えます";
pub const SAME_FOLDER: &str = "同じフォルダーの項目を選んだときに使えます";
/// The fewest files opened together that are asked about first, as Explorer stops offering to open them
/// past 15.
pub const ASK_TO_OPEN: usize = 16;

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Colet {
    /// A menu keeps the column it opened on even if another column takes its place in the chain.
    fn action_column(&self) -> Option<&Column> {
        let index = match &self.menu {
            Some(menu) => self.state.index_of(menu.column)?,
            None => self.state.active,
        };
        self.state.columns.get(index)
    }

    /// What the file actions work on: the selection, or while the menu is open, what it was opened on.
    pub fn picked(&self) -> Vec<String> {
        self.picked_paths().to_vec()
    }

    /// [`picked`](Self::picked), lent rather than copied, for what is asked each frame.
    fn picked_paths(&self) -> &[String] {
        let Some(column) = self.action_column() else { return &[] };
        match &self.menu {
            Some(Menu { paths: Some(paths), .. }) => paths,
            _ => &column.selected,
        }
    }

    pub fn picked_entries(&self) -> Vec<Entry> {
        let Some(c) = self.action_column() else { return Vec::new() };
        let picked: HashSet<String> = self.picked().into_iter().collect();
        c.entries.iter().filter(|e| picked.contains(&e.path)).cloned().collect()
    }

    /// Where a paste or a new folder goes: the folder column worked in.
    pub fn destination(&self) -> Option<String> {
        let c = self.action_column()?;
        (c.kind == ColumnKind::Folder).then(|| c.path.clone())
    }

    /// Asked as the buttons are drawn, each frame: it walks the selection, which may be thousands, without
    /// making a string, and stops at the first answer.
    pub fn reasons(&self) -> Reasons {
        let picked = self.picked_paths();
        let at_home = self.action_column().is_none_or(|c| c.kind == ColumnKind::Home);
        let search = picked.iter().any(|p| saved_query(p).is_some());
        let none = picked.is_empty();
        let item = if none { Some(NEEDS_SELECTION) } else if at_home { Some(NOT_AT_HOME) } else { None };
        let busy = self.running.is_some().then_some(BUSY);
        let folder = busy.or(self.destination().is_none().then_some(NEEDS_FOLDER));
        let saved_pick = picked.len() == 1 && search;
        let first = picked.first().map(|p| parent_folder(p).unwrap_or_default());
        let apart = first.is_some_and(|first| picked.iter().any(|p| parent_folder(p).unwrap_or_default() != first));
        Reasons {
            copy: if none {
                Some(NEEDS_SELECTION)
            } else if search {
                Some("保存した検索はコピーできません")
            } else if picked.iter().any(|p| !p.is_empty() && parent_folder(p).is_none()) {
                Some("ドライブはコピーできません")
            } else {
                None
            },
            cut: item,
            paste: folder,
            rename: if saved_pick { None } else { busy.or(item).or((picked.len() != 1).then_some(ONE_ITEM)) },
            delete: busy.or(item),
            // With nothing picked, the folder column's own folder.
            explorer: if none {
                self.destination().is_none().then_some(NEEDS_FOLDER)
            } else if at_home && picked.len() > 1 {
                Some(ONE_ITEM)
            } else if apart {
                Some(SAME_FOLDER)
            } else {
                None
            },
            shell: if search {
                Some("保存した検索には使えません")
            } else if none {
                self.destination().is_none().then_some(NEEDS_FOLDER)
            } else if apart {
                Some(SAME_FOLDER)
            } else {
                None
            },
            copy_path: none.then_some(NEEDS_SELECTION),
        }
    }

    /// Says why a shortcut did nothing, as the buttons and menu items say it.
    fn refuse(&mut self, title: &str, reason: &str, cx: &mut Context<Self>) {
        self.notify(format!("「{title}」は{reason}"), Tone::Info, cx);
    }

    pub fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        // What the menu was opened on, read before it closes: after, the selection would stand in.
        let (picked, reasons) = (self.picked(), self.reasons());
        self.menu = None;
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = if cut { reasons.cut } else { reasons.copy } {
            self.refuse(if cut { "切り取り" } else { "コピー" }, reason, cx);
            return;
        }
        let (backend, owner) = (self.backend.clone(), self.owner);
        let done = format!("{} 項目を{}しました", picked.len(), if cut { "切り取り" } else { "コピー" });
        self.in_background(move || backend.copy_files(&picked, cut, owner), Some(done), cx);
    }

    /// The paths as text, one a line; a saved search's is the query it runs.
    pub fn copy_paths(&mut self, cx: &mut Context<Self>) {
        let picked = self.picked();
        self.menu = None;
        if picked.is_empty() {
            return;
        }
        let text = picked.iter().map(|p| saved_query(p).map_or(p.as_str(), |(query, _)| query)).collect::<Vec<_>>().join("\r\n");
        let (backend, owner) = (self.backend.clone(), self.owner);
        let done = format!("{} 項目のパスをコピーしました", picked.len());
        self.in_background(move || backend.copy_text(&text, owner), Some(done), cx);
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        match self.reasons().paste {
            Some(reason) => self.refuse("貼り付け", reason, cx),
            None => {
                let destination = self.destination().unwrap_or_default();
                self.run(Request { kind: Kind::Paste, paths: Vec::new(), destination, name: String::new() }, cx);
            }
        }
    }

    /// F2: a rename, or for a saved search in home, its edit.
    pub fn ask_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, reasons, entries) = (self.picked(), self.reasons(), self.picked_entries());
        self.menu = None;
        if let [path] = picked.as_slice()
            && let Some((query, filter)) = saved_query(path)
        {
            let (query, filter) = (query.to_owned(), filter.map(str::to_owned));
            self.open_edit(&query, filter.as_deref(), window, cx);
            return;
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
        let request = Request { kind: Kind::Rename, paths: vec![entry.path], destination: String::new(), name: entry.name };
        self.ask_name(request, end, window, cx);
    }

    pub fn ask_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        if let Some(reason) = self.reasons().paste {
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
        let input = cx.new(|cx| InputState::new(window, cx).default_value(request.name.clone()));
        let subscription = cx.subscribe_in(&input, window, |this: &mut Self, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => this.submit_name(window, cx),
            InputEvent::Change => {
                if let Some(Dialog::Name { tried, checking, .. }) = &mut this.dialog {
                    *tried = false;
                    // The answer on its way is about a name no longer there.
                    *checking = None;
                }
                cx.notify();
            }
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
        });
        self.palette.style_input(&input, cx);
        self.dialog_subscriptions.push(subscription);
        input.update(cx, |input, cx| {
            input.focus(window, cx);
            input.set_selected_range(0..select_end, cx);
        });
        self.dialog = Some(Dialog::Name { request, input, tried: false, taken: None, checking: None });
        cx.notify();
    }

    /// Why the name in the dialog cannot be used, as it stands.
    pub fn name_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Name { input, tried, taken, .. }) = &self.dialog else { return None };
        let name = input.read(cx).value();
        colet_core::operations::name_problem(&name, *tried)
            .or(taken.as_deref().filter(|&taken| taken == name.as_ref()).map(|_| "同じ名前の項目があります"))
    }

    pub fn submit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { request, input, tried, checking, .. }) = &mut self.dialog else { return };
        let name = input.read(cx).value().to_string();
        // Pressed again while the folder is asked about this very name: the answer sends it.
        if name.trim().is_empty() || checking.as_deref() == Some(name.as_str()) {
            return;
        }
        // Like Explorer, an unchanged name just closes the dialog.
        if request.kind == Kind::Rename && request.paths.first().is_some_and(|p| folder_name(p) == name) {
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
            && folder_name(path).to_lowercase() != name.to_lowercase()
            && let Some(parent) = ancestors(path).into_iter().rev().nth(1)
        {
            let target = child(&parent, &name);
            *checking = Some(name.clone());
            let (backend, dialog) = (self.backend.clone(), input.entity_id());
            let asked = cx.background_executor().spawn(async move { backend.path_kind(&target).is_some() });
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
        let Some(Dialog::Name { input, taken, checking, .. }) = &mut self.dialog else { return };
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
        let Some(Dialog::Name { request, .. }) = &self.dialog else { return };
        let request = Request { name, ..request.clone() };
        self.close_dialog(window, cx);
        self.run(request, cx);
    }

    /// Delete sends to the Recycle Bin and Shift+Delete deletes outright, as in Explorer: whether to ask
    /// first, and what to do with an item the bin cannot take, is the shell's.
    pub fn delete_picked(&mut self, permanent: bool, cx: &mut Context<Self>) {
        let (picked, reasons) = (self.picked(), self.reasons());
        self.menu = None;
        if picked.is_empty() {
            return;
        }
        let kind = if permanent { Kind::Delete } else { Kind::Recycle };
        if let Some(reason) = reasons.delete {
            let hint = if reason != NOT_AT_HOME {
                ""
            } else if picked.iter().any(|p| saved_query(p).is_some()) {
                "保存した検索は右クリックメニューの「保存した検索を解除」で外せます。"
            } else {
                "お気に入りは右クリックメニューの「お気に入りを解除」で外せます。"
            };
            self.notify(format!("「{}」は{reason}。{hint}", kind.title()), Tone::Info, cx);
            return;
        }
        self.run(Request { kind, paths: picked, destination: String::new(), name: String::new() }, cx);
    }

    /// Opens the picked item: a folder beside it, a file in its app. Of several, the files each open in
    /// their apps and the folders are passed over.
    pub fn open_picked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.action_column().and_then(|c| self.state.index_of(c.id));
        let picked = self.picked_entries();
        self.menu = None;
        match (index, picked.as_slice()) {
            (Some(index), [entry]) => self.open(index, &entry.path, window, cx),
            _ => {
                let files: Vec<String> = picked.into_iter().filter(|e| !e.directory).map(|e| e.path).collect();
                if files.len() >= ASK_TO_OPEN {
                    self.dialog_subscriptions.clear();
                    self.dialog = Some(Dialog::Open { paths: files });
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
        let Some(Dialog::Open { paths }) = self.dialog.take() else { return };
        self.close_dialog(window, cx);
        self.open_files(paths, cx);
    }

    /// Opens `paths` one after another in their apps, in the order given; one that fails leaves the rest to
    /// open, and those that failed are told of together.
    fn open_files(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let (backend, owner) = (self.backend.clone(), self.owner);
        self.in_background(
            move || {
                let failed: Vec<(&str, String)> =
                    paths.iter().filter_map(|path| backend.open(path, owner).err().map(|error| (folder_name(path), error))).collect();
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
        let (picked, folder, refused) = (self.picked_entries(), self.destination(), self.reasons().explorer);
        self.menu = None;
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
                let paths: Vec<String> = picked.into_iter().map(|e| e.path).collect();
                self.in_background(move || backend.reveal(&paths), None, cx);
            }
            _ => {}
        }
    }

    /// The Windows (Explorer) menu, where this one was opened. Dismisses Colet's menu and returns its
    /// keys even when the column it belonged to has gone.
    pub fn show_shell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Read while the menu is open: its column and paths are what the reasons are for.
        let refused = self.reasons().shell;
        let Some(menu) = self.menu.take() else { return };
        let index = self.state.index_of(menu.column);
        if let Some(index) = index {
            self.state.set_active(index);
        }
        let to_column = self.dialog.is_none() && self.menu_focus.is_focused(window);
        self.close_menus(to_column, window, cx);
        let Some(index) = index else { return };
        if let Some(reason) = refused {
            self.notify(reason, Tone::Info, cx);
            return;
        }
        let paths = match &menu.paths {
            Some(paths) => paths.clone(),
            None => self.state.columns[index].selected.clone(),
        };
        let background = if paths.is_empty() { self.state.columns[index].path.clone() } else { String::new() };
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

    /// Runs one file operation and keeps it in the history; a second waits for none, it is refused.
    pub fn run(&mut self, request: Request, cx: &mut Context<Self>) {
        if self.running.is_some() {
            self.refuse(request.kind.title(), BUSY, cx);
            return;
        }
        self.next_operation += 1;
        let id = self.next_operation;
        self.departed.clear();
        let events = OperationEvents::default();
        self.running = Some(Running { id, events: events.clone(), made: None, placed: Vec::new() });
        self.history.insert(
            0,
            HistoryEntry {
                id,
                request: request.clone(),
                started: now(),
                status: Status::Running,
                error: None,
                items: Vec::new(),
                more: 0,
            },
        );
        self.history.truncate(MAX_ENTRIES);
        let (backend, owner, asked) = (self.backend.clone(), self.owner, request.clone());
        let task = cx.background_executor().spawn(async move {
            backend.operate(asked, owner, Box::new(move |event| events.lock().unwrap().push(event)))
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.finished(request, result, cx)).ok();
        })
        .detach();
    }

    /// Takes in how the items of the running operation ended so far.
    pub fn take_operation_events(&mut self) -> bool {
        let Some(running) = &mut self.running else { return false };
        let events = std::mem::take(&mut *running.events.lock().unwrap());
        if events.is_empty() {
            return false;
        }
        let mut entry = self.history.iter_mut().find(|e| e.id == running.id);
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

    fn finished(&mut self, request: Request, result: Result<(), String>, cx: &mut Context<Self>) {
        self.take_operation_events();
        let Some(Running { id, made, placed: pasted, .. }) = self.running.take() else { return };
        let Request { kind, paths, destination, name } = request;
        let (cancelled, problem) = match &result {
            Ok(()) => (false, None),
            Err(error) if error == CANCELLED => (true, None),
            Err(error) => (false, Some(error.clone())),
        };
        let renamed = (kind == Kind::Rename && result.is_ok())
            .then(|| paths.first().map(|p| child(ancestors(p).iter().rev().nth(1).map_or("", |s| s), &name)))
            .flatten();
        if let Some(to) = &renamed {
            // Carry the selection over to the new name before the listing drops the old one.
            self.state.renamed(&paths[0], to);
        }
        if kind == Kind::NewFolder && result.is_ok() {
            // The shell says where it made the folder, which a name clash may have changed.
            self.state.created(&destination, &made.unwrap_or_else(|| child(&destination, &name)));
        }
        let Some(index) = self.history.iter().position(|e| e.id == id) else { return };
        let entry = &mut self.history[index];
        let (failed, skipped) = (entry.count(ItemStatus::Failed), entry.count(ItemStatus::Skipped));
        // A cut pasted where it already was moved nothing, and a deletion turned down at the shell's
        // question deleted nothing: no trace, as in Explorer.
        let moved_nothing = kind == Kind::Paste && problem.is_none() && !cancelled && entry.items.is_empty() && entry.more == 0;
        let declined = matches!(kind, Kind::Recycle | Kind::Delete)
            && cancelled
            && entry.more == 0
            && entry.items.iter().all(|i| i.status == ItemStatus::Cancelled);
        if moved_nothing || declined {
            self.history.remove(index);
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
            entry.error = if failed + skipped > 0 { None } else { problem.clone() };
        }
        let mut counts = Vec::new();
        if failed > 0 {
            counts.push(format!("{failed} 件失敗"));
        }
        if skipped > 0 {
            counts.push(format!("{skipped} 件スキップ"));
        }
        let problem = if counts.is_empty() { problem } else { Some(counts.join("・")) };
        if let Some(problem) = problem {
            self.unseen += 1;
            self.notify_entry(format!("「{}」: {problem}", kind.title()), id, cx);
        }
        // Like Explorer, what was pasted is picked once listed, under the names it got.
        if kind == Kind::Paste && !pasted.is_empty() {
            self.state.pasted(&destination, &pasted);
        }
        self.refresh(cx);
        self.changed(cx);
    }

    /// Opened, the history has been seen; closed, the keys go back to the column.
    pub fn show_history(&mut self, open: bool, entry: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        self.history_open = open;
        self.expanded = entry.into_iter().collect();
        if open {
            self.unseen = 0;
        } else {
            let handle = if self.state.active == 0 { &self.home_focus } else { &self.columns_focus };
            handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Only what is still there: an item moved on or deleted since says so. Whether it is, is asked off the UI
    /// thread, as a share can keep it waiting.
    pub fn reveal_from_history(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.revealing = Some(path.clone());
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
    fn history_item_found(&mut self, path: String, kind: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
        if self.revealing.as_ref() != Some(&path) {
            return;
        }
        self.revealing = None;
        if !self.history_open {
            return;
        }
        match kind {
            None => self.notify(format!("「{}」はもうありません", folder_name(&path)), Tone::Info, cx),
            Some(directory) => {
                self.show_history(false, None, window, cx);
                self.state.reveal(&path, directory);
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
    events: OperationEvents,
    /// Where its last item ended, as a new folder's.
    made: Option<String>,
    /// Where its copied or moved items went.
    placed: Vec<String>,
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
        }
    }

    #[test]
    fn records_where_a_paste_put_each_item_and_what_failed() {
        let mut paste = entry(Kind::Paste);
        paste.record(Event::Done { path: r"D:\a.txt".into() });
        paste.record(Event::Placed { path: r"D:\dest\a.txt".into() });
        paste.record(Event::Failed { path: r"D:\b.txt".into(), error: "アクセスが拒否されました。".into() });
        paste.record(Event::Skipped { path: r"D:\c.txt".into() });
        assert_eq!(paste.items[0].path, r"D:\dest\a.txt");
        assert_eq!(paste.items[0].from.as_deref(), Some(r"D:\a.txt"));
        assert_eq!(paste.outcome(), "処理中");
        paste.status = Status::Done;
        assert_eq!(paste.outcome(), "1 件失敗・1 件スキップ");
        assert!(paste.has_problems());
        assert_eq!(paste.target(), "3 項目 → dest");
        assert_eq!(paste.count(ItemStatus::Done), 1);
    }

    #[test]
    fn lists_what_was_handed_to_the_shell_by_its_name_without_a_problem() {
        let mut paste = entry(Kind::Paste);
        paste.record(Event::Handed { path: r"D:\dest\添付.txt".into() });
        assert_eq!((paste.items[0].path.as_str(), paste.items[0].status), (r"D:\dest\添付.txt", ItemStatus::Handed));
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
