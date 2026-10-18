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
    paths::{ancestors, child, folder_name, path_key},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{
    app::{Colet, Tone},
    places::Dialog,
    state::{Kind as ColumnKind, saved_query},
};

/// The most operations kept, and the most items of each.
const MAX_ENTRIES: usize = 100;
const MAX_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemStatus {
    Done,
    Failed,
    Skipped,
    Cancelled,
}

impl ItemStatus {
    pub fn label(self) -> &'static str {
        match self {
            ItemStatus::Done => "完了",
            ItemStatus::Failed => "失敗",
            ItemStatus::Skipped => "スキップ",
            ItemStatus::Cancelled => "キャンセル",
        }
    }
}

/// One item of an operation, as the history lists it.
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryItem {
    /// Where it is now; where it was, when it failed or went to the Recycle Bin.
    pub path: String,
    /// Where a paste or a rename took it from.
    pub from: Option<String>,
    pub status: ItemStatus,
    pub error: Option<String>,
    /// Sent to the Recycle Bin: nothing to open.
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
    pub kind: Kind,
    /// Milliseconds since the Unix epoch.
    pub started: u64,
    pub paths: Vec<String>,
    pub destination: String,
    pub name: String,
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
        };
        let gone = self.kind == Kind::Recycle && status == ItemStatus::Done;
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
        let n = self.paths.len();
        match self.kind {
            Kind::Paste => format!("{} 項目 → {}", self.items.len() + self.more, folder_name(&self.destination)),
            Kind::Recycle if n == 1 => folder_name(&self.paths[0]).to_owned(),
            Kind::Recycle => format!("{n} 項目"),
            Kind::Rename => format!("{} → {}", folder_name(self.paths.first().map_or("", |p| p)), self.name),
            Kind::NewFolder => format!("{}（{}）", self.name, folder_name(&self.destination)),
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
    pub index: usize,
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
    pub open: Option<&'static str>,
    pub shell: Option<&'static str>,
    pub copy_path: Option<&'static str>,
}

const NEEDS_SELECTION: &str = "項目を選択すると使えます";
const NEEDS_FOLDER: &str = "フォルダーのカラムで使えます";
const BUSY: &str = "ファイル操作の完了後に使えます";
const NOT_AT_HOME: &str = "ホームでは使えません";
const ONE_ITEM: &str = "1項目を選択したときに使えます";

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

impl Colet {
    /// What the file actions work on: the selection, or while the menu is open, what it was opened on.
    pub fn picked(&self) -> Vec<String> {
        match &self.menu {
            Some(Menu { paths: Some(paths), .. }) => paths.clone(),
            _ => self.state.columns[self.state.active].selected.clone(),
        }
    }

    pub fn picked_entries(&self) -> Vec<Entry> {
        let c = &self.state.columns[self.state.active];
        let picked: HashSet<String> = self.picked().into_iter().collect();
        c.entries.iter().filter(|e| picked.contains(&e.path)).cloned().collect()
    }

    /// Where a paste or a new folder goes: the folder column worked in.
    pub fn destination(&self) -> Option<String> {
        let c = &self.state.columns[self.state.active];
        (c.kind == ColumnKind::Folder).then(|| c.path.clone())
    }

    pub fn reasons(&self) -> Reasons {
        let picked = self.picked();
        let at_home = self.state.columns[self.state.active].kind == ColumnKind::Home;
        let search = picked.iter().any(|p| saved_query(p).is_some());
        let none = picked.is_empty();
        let item = if none { Some(NEEDS_SELECTION) } else if at_home { Some(NOT_AT_HOME) } else { None };
        let busy = self.running.then_some(BUSY);
        let folder = busy.or(self.destination().is_none().then_some(NEEDS_FOLDER));
        let saved_pick = picked.len() == 1 && search;
        let parents: HashSet<String> =
            picked.iter().map(|p| ancestors(p).iter().rev().nth(1).cloned().unwrap_or_default()).collect();
        Reasons {
            copy: if none {
                Some(NEEDS_SELECTION)
            } else if search {
                Some("保存した検索はコピーできません")
            } else if picked.iter().any(|p| ancestors(p).len() == 1) {
                Some("ドライブはコピーできません")
            } else {
                None
            },
            cut: item,
            paste: folder,
            rename: if saved_pick { None } else { busy.or(item).or((picked.len() != 1).then_some(ONE_ITEM)) },
            delete: busy.or(item),
            open: (picked.len() > 1).then_some(ONE_ITEM),
            shell: if search {
                Some("保存した検索には使えません")
            } else if none {
                self.destination().is_none().then_some(NEEDS_FOLDER)
            } else if parents.len() > 1 {
                Some("同じフォルダーの項目を選んだときに使えます")
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
        let count = picked.len();
        let task = cx.background_executor().spawn(async move { backend.copy_files(&picked, cut, owner) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.notify(format!("{count} 項目を{}しました", if cut { "切り取り" } else { "コピー" }), Tone::Info, cx),
                Err(error) => this.notify(error, Tone::Error, cx),
            })
            .ok();
        })
        .detach();
    }

    /// The paths as text, one a line; a saved search's is the query it runs.
    pub fn copy_paths(&mut self, cx: &mut Context<Self>) {
        let picked = self.picked();
        self.menu = None;
        if picked.is_empty() {
            return;
        }
        let text = picked.iter().map(|p| saved_query(p).unwrap_or(p)).collect::<Vec<_>>().join("\r\n");
        let (backend, owner) = (self.backend.clone(), self.owner);
        let count = picked.len();
        let task = cx.background_executor().spawn(async move { backend.copy_text(&text, owner) });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(()) => this.notify(format!("{count} 項目のパスをコピーしました"), Tone::Info, cx),
                Err(error) => this.notify(error, Tone::Error, cx),
            })
            .ok();
        })
        .detach();
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        self.menu = None;
        match self.reasons().paste {
            Some(reason) => self.refuse("貼り付け", reason, cx),
            None => {
                let destination = self.destination().unwrap_or_default();
                self.run(Kind::Paste, Vec::new(), destination, String::new(), cx);
            }
        }
    }

    /// F2: a rename, or for a saved search in home, its edit.
    pub fn ask_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, reasons, entries) = (self.picked(), self.reasons(), self.picked_entries());
        self.menu = None;
        if let [path] = picked.as_slice()
            && let Some(query) = saved_query(path)
        {
            let query = query.to_owned();
            self.open_edit(&query, window, cx);
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
        self.ask_name(Kind::Rename, vec![entry.path.clone()], String::new(), &entry.name, end, window, cx);
    }

    pub fn ask_new_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        if let Some(reason) = self.reasons().paste {
            self.refuse("新規フォルダー", reason, cx);
            return;
        }
        let destination = self.destination().unwrap_or_default();
        let name = "新しいフォルダー";
        self.ask_name(Kind::NewFolder, Vec::new(), destination, name, name.len(), window, cx);
    }

    #[allow(clippy::too_many_arguments)]
    fn ask_name(
        &mut self,
        kind: Kind,
        paths: Vec<String>,
        destination: String,
        name: &str,
        select_end: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| InputState::new(window, cx).default_value(name.to_owned()));
        let subscription = cx.subscribe_in(&input, window, |this: &mut Self, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => this.submit_name(window, cx),
            InputEvent::Change => {
                if let Some(Dialog::Name { tried, .. }) = &mut this.dialog {
                    *tried = false;
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
        self.dialog = Some(Dialog::Name { kind, paths, destination, input, tried: false });
        cx.notify();
    }

    /// Why the name in the dialog cannot be used, as it stands.
    pub fn name_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Name { input, tried, .. }) = &self.dialog else { return None };
        colet_core::operations::name_problem(&input.read(cx).value(), *tried)
    }

    pub fn submit_name(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Name { kind, paths, destination, input, tried }) = &mut self.dialog else { return };
        let name = input.read(cx).value().to_string();
        if name.trim().is_empty() {
            return;
        }
        // Like Explorer, an unchanged name just closes the dialog.
        if *kind == Kind::Rename && paths.first().is_some_and(|p| folder_name(p) == name) {
            self.close_dialog(window, cx);
            return;
        }
        // Say what the name still lacks rather than send it.
        if colet_core::operations::name_problem(&name, true).is_some() {
            *tried = true;
            cx.notify();
            return;
        }
        let (kind, paths, destination) = (*kind, paths.clone(), destination.clone());
        self.close_dialog(window, cx);
        self.run(kind, paths, destination, name, cx);
    }

    /// Delete and Shift+Delete ask first; Colet never deletes for good.
    pub fn ask_recycle(&mut self, permanent: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (picked, reasons) = (self.picked(), self.reasons());
        self.menu = None;
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = reasons.delete {
            let hint = if reason != NOT_AT_HOME {
                ""
            } else if picked.iter().any(|p| saved_query(p).is_some()) {
                "保存した検索は右クリックメニューの「保存した検索を解除」で外せます。"
            } else {
                "お気に入りは右クリックメニューの「お気に入りを解除」で外せます。"
            };
            self.notify(format!("「ごみ箱へ移動」は{reason}。{hint}"), Tone::Info, cx);
            return;
        }
        self.dialog_focus.focus(window, cx);
        self.dialog = Some(Dialog::Recycle { paths: picked, permanent });
        cx.notify();
    }

    pub fn confirm_recycle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Recycle { paths, .. }) = &self.dialog else { return };
        let paths = paths.clone();
        self.close_dialog(window, cx);
        self.run(Kind::Recycle, paths, String::new(), String::new(), cx);
    }

    /// Opens the picked item: a folder beside it, a file in its app.
    pub fn open_picked(&mut self, cx: &mut Context<Self>) {
        let picked = self.picked_entries();
        self.menu = None;
        if let [entry] = picked.as_slice() {
            let index = self.state.active;
            self.open(index, &entry.path.clone(), cx);
        }
    }

    pub fn reveal_in_explorer(&mut self, cx: &mut Context<Self>) {
        let picked = self.picked_entries();
        self.menu = None;
        let [entry] = picked.as_slice() else { return };
        let (backend, path) = (self.backend.clone(), entry.path.clone());
        let task = cx.background_executor().spawn(async move { backend.reveal(&path) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                this.update(cx, |this, cx| this.notify(error, Tone::Error, cx)).ok();
            }
        })
        .detach();
    }

    /// The Windows (Explorer) menu, where this one was opened.
    pub fn show_shell_menu(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.take() else { return };
        if let Some(reason) = self.reasons_for(&menu).shell {
            self.notify(reason, Tone::Info, cx);
            return;
        }
        let paths = match &menu.paths {
            Some(paths) => paths.clone(),
            None => self.state.columns[menu.index].selected.clone(),
        };
        let background = if paths.is_empty() { self.destination().unwrap_or_default() } else { String::new() };
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
        cx.notify();
    }

    fn reasons_for(&mut self, menu: &Menu) -> Reasons {
        let paths = menu.paths.clone();
        let saved = self.menu.replace(Menu { index: menu.index, paths, at: menu.at, screen: menu.screen });
        let reasons = self.reasons();
        self.menu = saved;
        reasons
    }

    /// Runs one file operation and keeps it in the history; a second waits for none, it is refused.
    pub fn run(&mut self, kind: Kind, paths: Vec<String>, destination: String, name: String, cx: &mut Context<Self>) {
        if self.running {
            self.refuse(kind.title(), BUSY, cx);
            return;
        }
        self.next_operation += 1;
        let id = self.next_operation;
        self.departed.clear();
        self.running = true;
        self.history.insert(
            0,
            HistoryEntry {
                id,
                kind,
                started: now(),
                paths: paths.clone(),
                destination: destination.clone(),
                name: name.clone(),
                status: Status::Running,
                error: None,
                items: Vec::new(),
                more: 0,
            },
        );
        self.history.truncate(MAX_ENTRIES);
        let events = self.operation_events.clone();
        let request = Request { kind, paths: paths.clone(), destination: destination.clone(), name: name.clone() };
        let (backend, owner) = (self.backend.clone(), self.owner);
        let task = cx.background_executor().spawn(async move {
            backend.operate(request, owner, Box::new(move |event| events.lock().unwrap().push((id, event))))
        });
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.finished(id, kind, paths, destination, name, result, cx)).ok();
        })
        .detach();
    }

    /// Takes in how the items of the running operation ended so far.
    pub fn take_operation_events(&mut self) -> bool {
        let events: Vec<(u64, Event)> = std::mem::take(&mut *self.operation_events.lock().unwrap());
        if events.is_empty() {
            return false;
        }
        for (id, event) in events {
            if let Event::Done { path } = &event {
                self.made.insert(id, path.clone());
            }
            // A copy's source stays: only what left is expected to go from its listing.
            if let Event::Left { path } = &event {
                self.departed.insert(path_key(path));
            }
            if let Event::Placed { path } = &event {
                self.placed.entry(id).or_default().push(path.clone());
            }
            if let Some(entry) = self.history.iter_mut().find(|e| e.id == id) {
                entry.record(event);
            }
        }
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn finished(
        &mut self,
        id: u64,
        kind: Kind,
        paths: Vec<String>,
        destination: String,
        name: String,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.take_operation_events();
        self.running = false;
        let made = self.made.remove(&id);
        let pasted = self.placed.remove(&id).unwrap_or_default();
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
        // A cut pasted where it already was moved nothing: no trace.
        if kind == Kind::Paste && problem.is_none() && !cancelled && entry.items.is_empty() && entry.more == 0 {
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

    /// Only what is still there: an item moved on or deleted since says so.
    pub fn reveal_from_history(&mut self, path: String, window: &mut Window, cx: &mut Context<Self>) {
        match self.backend.path_kind(&path) {
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

/// How the running operation's events reach the window.
pub type OperationEvents = Arc<Mutex<Vec<(u64, Event)>>>;

#[cfg(test)]
mod tests {
    use super::{Event, HistoryEntry, ItemStatus, Kind, Status};

    fn entry(kind: Kind) -> HistoryEntry {
        HistoryEntry {
            id: 1,
            kind,
            started: 0,
            paths: vec![r"D:\a.txt".into()],
            destination: r"D:\dest".into(),
            name: String::new(),
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
    fn marks_what_went_to_the_bin_as_gone() {
        let mut recycle = entry(Kind::Recycle);
        recycle.record(Event::Done { path: r"D:\a.txt".into() });
        assert!(recycle.items[0].gone);
        recycle.status = Status::Done;
        assert_eq!(recycle.outcome(), "完了");
        assert_eq!(recycle.target(), "a.txt");
    }
}
