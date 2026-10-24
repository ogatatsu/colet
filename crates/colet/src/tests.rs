//! The window driven as the user would: keys, clicks and drags, against listings the test makes up.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use colet_core::{
    entry::{Entry, Marks, Order, Sort},
    everything::Page,
    operations::{CANCELLED, Event, Kind as Operation, Request},
    paths::{ancestors, child, folder_name},
    shell::{Owner, clipboard::ClipboardFiles},
    watch::Changes,
};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ElementId, SharedString, Entity, Focusable, ScrollDelta, ScrollStrategy, TestAppContext, Window,
    WindowBounds, WindowOptions, point, px, size,
};

use crate::{
    app::{Colet, Engine},
    backend::{Backend, Search},
    ops::ItemStatus,
    places::Dialog,
    settings::{LoadError, SavedSearch, SearchFilter, Settings, Theme},
    theme::Palette,
    state::{ColumnId, Kind, search_key},
};

#[derive(Default)]
struct Fake {
    roots: Vec<Entry>,
    /// The folders Everything's index starts from, none while searches fail.
    indexed: Mutex<Vec<String>>,
    folders: Mutex<HashMap<String, Vec<Entry>>>,
    results: Mutex<Vec<Entry>>,
    /// Each search sent: its query and where its page starts.
    searches: Mutex<Vec<(String, usize)>>,
    /// Each folder listed.
    lists: Mutex<Vec<String>>,
    /// The order each listing and search was asked in.
    orders: Mutex<Vec<Order>>,
    failing: Mutex<Option<String>>,
    changed: AtomicBool,
    /// The settings as last written, starting from those it is made with.
    settings: Mutex<Settings>,
    saves: AtomicUsize,
    /// What reading the settings fails with, in place of them.
    unreadable: Mutex<Option<LoadError>>,
    reconnects: AtomicUsize,
    clipboard: Mutex<ClipboardFiles>,
    text: Mutex<String>,
    /// What each operation was asked, and the paths that refuse to go.
    operations: Mutex<Vec<Request>>,
    stuck: Mutex<Vec<String>>,
    /// Paths the Recycle Bin cannot take, deleted outright.
    unbinnable: Mutex<Vec<String>>,
    /// The shell's question on deleting is turned down.
    declined: AtomicBool,
    opened: Mutex<Vec<String>>,
    menus: Mutex<Vec<(Vec<String>, String)>>,
    /// Told of every clipboard change, as the real watcher is.
    watcher: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl Fake {
    fn remove(&self, path: &str) -> Option<Entry> {
        let parent = ancestors(path).into_iter().rev().nth(1)?;
        let mut folders = self.folders.lock().unwrap();
        let list = folders.get_mut(&parent)?;
        let at = list.iter().position(|e| e.path == path)?;
        Some(list.remove(at))
    }

    fn add(&self, parent: &str, entry: Entry) {
        self.folders.lock().unwrap().entry(parent.into()).or_default().push(entry);
    }
}

impl Backend for Fake {
    fn roots(&self) -> Vec<Entry> {
        self.roots.clone()
    }

    fn indexed_roots(&self) -> Vec<String> {
        if self.failing.lock().unwrap().is_some() { Vec::new() } else { self.indexed.lock().unwrap().clone() }
    }

    fn list(&self, path: &str, sort: Order, _: &AtomicBool) -> Result<Vec<Entry>, String> {
        self.lists.lock().unwrap().push(path.into());
        self.orders.lock().unwrap().push(sort);
        self.folders.lock().unwrap().get(path).cloned().ok_or_else(|| "フォルダーを開けません".into())
    }

    fn search(&self, search: Search) -> Result<(Page, String), String> {
        self.searches.lock().unwrap().push((search.query.clone(), search.offset));
        self.orders.lock().unwrap().push(search.sort);
        // As Everything's watch, it says the results changed until they are searched afresh.
        if search.refresh {
            self.changed.store(false, Ordering::Relaxed);
        }
        if let Some(error) = self.failing.lock().unwrap().clone() {
            return Err(error);
        }
        let results = self.results.lock().unwrap();
        let end = (search.offset + search.count).min(results.len());
        let entries = results[search.offset.min(end)..end].to_vec();
        Ok((Page { entries, total: results.len(), offset: search.offset }, "Everything 1.5".into()))
    }

    fn search_changed(&self) -> Result<bool, String> {
        Ok(self.changed.load(Ordering::Relaxed))
    }

    fn reconnect(&self) {
        self.reconnects.fetch_add(1, Ordering::Relaxed);
    }

    fn load_settings(&self) -> Result<Settings, LoadError> {
        match self.unreadable.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(self.settings.lock().unwrap().clone()),
        }
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        self.saves.fetch_add(1, Ordering::Relaxed);
        *self.settings.lock().unwrap() = settings.clone();
        Ok(())
    }

    /// Acts on the listings as the shell would on the disk.
    fn operate(&self, request: Request, _: Owner, mut report: Box<dyn FnMut(Event) + Send>) -> Result<(), String> {
        self.operations.lock().unwrap().push(request.clone());
        let stuck = self.stuck.lock().unwrap().clone();
        let mut failed = false;
        match request.kind {
            Operation::Recycle | Operation::Delete if self.declined.load(Ordering::Relaxed) => return Err(CANCELLED.into()),
            Operation::Recycle | Operation::Delete => {
                let unbinnable = self.unbinnable.lock().unwrap().clone();
                for path in &request.paths {
                    if stuck.contains(path) {
                        failed = true;
                        report(Event::Failed { path: path.clone(), error: "アクセスが拒否されました。".into() });
                    } else {
                        self.remove(path);
                        report(Event::Done { path: path.clone() });
                        if request.kind == Operation::Delete || unbinnable.contains(path) {
                            report(Event::Destroyed { path: path.clone() });
                        }
                        report(Event::Left { path: path.clone() });
                    }
                }
            }
            Operation::Rename => {
                let from = &request.paths[0];
                let mut entry = self.remove(from).unwrap();
                entry.path = child(&entry.parent, &request.name);
                entry.name = request.name.clone();
                let parent = entry.parent.clone();
                self.add(&parent, entry);
                report(Event::Done { path: from.clone() });
                report(Event::Left { path: from.clone() });
            }
            Operation::NewFolder => {
                let path = child(&request.destination, &request.name);
                self.add(&request.destination, Entry { parent: request.destination.clone(), ..folder(&path) });
                self.folders.lock().unwrap().insert(path.clone(), vec![]);
                report(Event::Done { path });
            }
            Operation::Paste => {
                let files = self.clipboard.lock().unwrap().clone();
                for path in files.paths {
                    let name = folder_name(&path).to_owned();
                    let parent = ancestors(&path).into_iter().rev().nth(1).unwrap();
                    let name = if parent == request.destination { format!("{name} - コピー") } else { name };
                    let made = child(&request.destination, &name);
                    if files.cut {
                        self.remove(&path);
                    }
                    self.add(&request.destination, file(&made));
                    report(Event::Done { path: path.clone() });
                    if files.cut {
                        report(Event::Left { path: path.clone() });
                    }
                    report(Event::Placed { path: made });
                }
                // Written out by the shell later, as it says nothing back.
                for name in &files.contents {
                    report(Event::Handed { path: child(&request.destination, name) });
                }
            }
        }
        if failed { Err("一部の項目を操作できませんでした。結果を確認してください。".into()) } else { Ok(()) }
    }

    fn clipboard_files(&self) -> Option<ClipboardFiles> {
        Some(self.clipboard.lock().unwrap().clone())
    }

    fn copy_files(&self, paths: &[String], cut: bool, _: Owner) -> Result<(), String> {
        *self.clipboard.lock().unwrap() = ClipboardFiles { paths: paths.to_vec(), cut, sequence: 1, ..Default::default() };
        if let Some(changed) = &*self.watcher.lock().unwrap() {
            changed();
        }
        Ok(())
    }

    fn copy_text(&self, text: &str, _: Owner) -> Result<(), String> {
        *self.text.lock().unwrap() = text.into();
        Ok(())
    }

    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>) {
        *self.watcher.lock().unwrap() = Some(changed);
    }

    fn open(&self, path: &str, _: Owner) -> Result<(), String> {
        self.opened.lock().unwrap().push(path.into());
        Ok(())
    }

    fn reveal(&self, _: &str) -> Result<(), String> {
        Ok(())
    }

    fn path_kind(&self, path: &str) -> Option<bool> {
        let folders = self.folders.lock().unwrap();
        folders.values().flatten().find(|e| e.path == path).map(|e| e.directory)
    }

    fn shell_menu(&self, paths: Vec<String>, background: String, _: (i32, i32), _: Owner) -> Result<(), String> {
        self.menus.lock().unwrap().push((paths, background));
        Ok(())
    }
}

fn entry(path: &str, directory: bool) -> Entry {
    let chain = ancestors(path);
    Entry {
        path: path.into(),
        name: colet_core::paths::folder_name(path).into(),
        parent: chain.len().checked_sub(2).map(|i| chain[i].clone()).unwrap_or_default(),
        directory,
        size: (!directory).then_some(1),
        modified: None,
        hidden: false,
        marks: Marks::default(),
    }
}

fn file(path: &str) -> Entry {
    entry(path, false)
}

fn folder(path: &str) -> Entry {
    entry(path, true)
}

impl Fake {
    fn with_folders(self, folders: &[(&str, Vec<Entry>)]) -> Self {
        *self.folders.lock().unwrap() = folders.iter().map(|(p, e)| (p.to_string(), e.clone())).collect();
        self
    }

    fn with_results(self, results: Vec<Entry>) -> Self {
        *self.results.lock().unwrap() = results;
        self
    }

    fn queries(&self) -> Vec<String> {
        self.searches.lock().unwrap().iter().map(|(q, _)| q.clone()).collect()
    }
}

struct Ui {
    window: AnyWindowHandle,
    view: Entity<Colet>,
}

fn open(cx: &mut TestAppContext, fake: Arc<Fake>) -> Ui {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::app::bind_keys(cx);
    });
    let (window, view) = cx
        .update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        Default::default(),
                        size(px(1200.), px(800.)),
                    ))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let settings = fake.load_settings();
                    cx.new(|cx| Colet::new(fake, settings, window, cx))
                },
            )
        })
        .expect("open the window");
    settle(cx);
    Ui { window, view }
}

/// Lets the timers that wait on typing, listings and changes run out, and draws.
fn settle(cx: &mut TestAppContext) {
    for _ in 0..3 {
        cx.executor().advance_clock(Duration::from_millis(250));
        cx.run_until_parked();
    }
}

impl Ui {
    fn act<R>(&self, cx: &mut TestAppContext, f: impl FnOnce(&mut Window, &mut App) -> R) -> R {
        cx.update_window(self.window, |_, window, cx| {
            window.render_frame(cx);
            f(window, cx)
        })
        .unwrap()
    }

    fn press(&self, cx: &mut TestAppContext, key: &str) {
        self.act(cx, |window, cx| window.press(key, cx));
        settle(cx);
    }

    fn input(&self, cx: &mut TestAppContext, text: &str) {
        self.act(cx, |window, cx| window.input(text, cx));
        settle(cx);
    }

    fn click(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) {
        let id = id.into();
        self.act(cx, |window, cx| window.click(id, cx));
        settle(cx);
    }

    /// Clicks `id` inside `scope`, for a row shown in two columns.
    fn click_in(&self, cx: &mut TestAppContext, scope: impl Into<ElementId>, id: impl Into<ElementId>) {
        let (scope, id) = (scope.into(), id.into());
        self.act(cx, |window, cx| window.within(scope).click(id, cx));
        settle(cx);
    }

    fn read<R>(&self, cx: &mut TestAppContext, f: impl FnOnce(&Colet) -> R) -> R {
        self.view.read_with(cx, |view, _| f(view))
    }

    fn selected(&self, cx: &mut TestAppContext, index: usize) -> Vec<String> {
        self.read(cx, |v| v.state.columns[index].selected.clone())
    }

    fn active(&self, cx: &mut TestAppContext) -> usize {
        self.read(cx, |v| v.state.active)
    }

    fn home_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).home_focus.is_focused(window))
    }

    fn field(&self, cx: &mut TestAppContext) -> String {
        let field = self.view.read_with(cx, |v, _| v.search.clone());
        field.read_with(cx, |input, _| input.value().to_string())
    }

    fn toasts(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |v| v.toasts.iter().map(|t| t.text.to_string()).collect())
    }

    fn columns_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).columns_focus.is_focused(window))
    }
}

fn row(path: &str) -> ElementId {
    ElementId::Name(path.to_owned().into())
}

#[gpui_kit::test]
fn searches_only_for_what_is_typed_once_typing_pauses(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\abc.txt")]));
    let ui = open(cx, fake.clone());
    // Keys a few milliseconds apart: none of the queries on the way runs.
    for key in ["a", "b", "c"] {
        ui.act(cx, |window, cx| window.input(key, cx));
        cx.executor().advance_clock(Duration::from_millis(30));
    }
    settle(cx);
    assert_eq!(fake.queries(), ["abc"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
}

#[gpui_kit::test]
fn moves_from_the_search_field_into_the_results_with_down(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\alpha.txt"), file(r"D:\beta.txt")]));
    let ui = open(cx, fake);
    ui.input(cx, "a");
    ui.press(cx, "down");
    assert_eq!(ui.active(cx), 1);
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha.txt"]);
    // Picked, the file shows in its folder.
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().highlight.clone()).as_deref(), Some(r"D:\alpha.txt"));
}

#[gpui_kit::test]
fn opens_a_deep_search_file_on_its_own_folder_and_right_jumps_to_it(cx: &mut TestAppContext) {
    let deep = file(r"D:\a\b\zeta.txt");
    let fake = Arc::new(
        Fake::default()
            .with_results(vec![deep.clone()])
            .with_folders(&[(r"D:\", vec![folder(r"D:\a")]), (r"D:\a", vec![folder(r"D:\a\b")]), (r"D:\a\b", vec![deep.clone()])]),
    );
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    ui.click(cx, row(&deep.path));
    // The folders above it are folded away; their crumbs are still there.
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 2);
    assert!(ui.act(cx, |window, _| window.try_find(("crumb", 2usize)).is_some()));
    ui.press(cx, "right");
    assert_eq!(ui.active(cx), 4);
    assert_eq!(ui.selected(cx, 4), std::slice::from_ref(&deep.path));
    // ← passes over them, back to the results.
    ui.press(cx, "left");
    assert_eq!(ui.active(cx), 1);
    // The strip beside the results says how many it holds, brings them out for good, and goes.
    assert!(ui.act(cx, |window, _| window.try_find("unfold-count").is_some()));
    // Its heading's line runs on from the columns'.
    let deepest = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    let (strip, column) = ui.act(cx, |window, _| (window.find("unfold-head").bounds(), window.find(("head", deepest)).bounds()));
    assert_eq!((strip.top(), strip.bottom()), (column.top(), column.bottom()));
    ui.click(cx, "unfold");
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
    assert!(ui.act(cx, |window, _| window.try_find("unfold").is_none()));
    // Clicked, it leaves the keys in the columns: → still jumps to the file.
    assert!(ui.columns_focused(cx));
    ui.press(cx, "right");
    assert_eq!(ui.active(cx), 4);
    // Picked afresh, folded again: a crumb brings its column out and takes the keys.
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click_in(cx, ("column", results), row(&deep.path));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 2);
    ui.click(cx, ("crumb", 3usize));
    assert_eq!(ui.active(cx), 3);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
}

#[gpui_kit::test]
fn walks_folders_by_keys_and_type_ahead_and_goes_back(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), folder(r"D:\beta"), file(r"D:\gamma.txt")])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    ui.press(cx, "right");
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha"]);
    ui.input(cx, "g");
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma.txt"]);
    ui.press(cx, "up");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta"]);
    ui.press(cx, "alt-left");
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma.txt"]);
    ui.press(cx, "alt-right");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta"]);
}

#[gpui_kit::test]
fn leaves_the_search_for_a_place_picked_in_home_clearing_the_field(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![])]).with_results(vec![file(r"D:\x.txt")])
    });
    let ui = open(cx, fake);
    ui.input(cx, "x");
    ui.click(cx, row(r"D:\"));
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "");
    let field = ui.view.read_with(cx, |v, _| v.search.clone());
    assert_eq!(field.read_with(cx, |input, _| input.value().to_string()), "");
    // One step back returns to the results, and the field says so.
    ui.press(cx, "alt-left");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "x");
    assert_eq!(field.read_with(cx, |input, _| input.value().to_string()), "x");
}

#[gpui_kit::test]
fn reports_a_failed_search_as_one_and_retries(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    *fake.failing.lock().unwrap() = Some("Everything 1.5に接続できません。".into());
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    assert!(ui.read(cx, |v| v.state.columns[1].error.is_some()));
    assert!(ui.read(cx, |v| matches!(v.engine, Engine::Failed(_))));
    *fake.failing.lock().unwrap() = None;
    let id = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("retry", id));
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
    assert!(ui.read(cx, |v| matches!(v.engine, Engine::Connected(_))));
}

#[gpui_kit::test]
fn adds_the_index_roots_to_home_once_a_search_reaches_everything(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        indexed: Mutex::new(vec!["D:".into(), r"\\nas\share".into()]),
        ..Fake::default().with_results(vec![file(r"D:\x.txt")])
    });
    *fake.failing.lock().unwrap() = Some("Everything 1.5に接続できません。".into());
    let ui = open(cx, fake.clone());
    let home = |cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[0].entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    assert_eq!(home(cx), [r"D:\"]);
    *fake.failing.lock().unwrap() = None;
    ui.input(cx, "x");
    assert!(ui.read(cx, |v| matches!(v.engine, Engine::Connected(_))));
    assert_eq!(home(cx), [r"D:\", r"\\nas\share"]);
}

#[gpui_kit::test]
fn loads_the_next_page_of_results_as_the_list_nears_its_end(cx: &mut TestAppContext) {
    let results: Vec<Entry> = (0..1200).map(|i| file(&format!(r"D:\file{i:04}.txt"))).collect();
    let fake = Arc::new(Fake::default().with_results(results));
    let ui = open(cx, fake.clone());
    ui.input(cx, "file");
    assert_eq!(ui.read(cx, |v| (v.state.columns[1].entries.len(), v.state.columns[1].total)), (500, 1200));
    let id = ui.read(cx, |v| v.state.columns[1].id);
    let handle = ui.view.update(cx, |v, _| v.list_handle(id));
    handle.scroll_to_item(499, ScrollStrategy::Bottom);
    settle(cx);
    ui.act(cx, |_, _| ());
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1000);
    assert!(fake.searches.lock().unwrap().iter().any(|(_, offset)| *offset == 500));
}

#[gpui_kit::test]
fn reloads_only_the_folders_a_change_names(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\a")]), (r"D:\a", vec![file(r"D:\a\x.txt")])])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    fake.lists.lock().unwrap().clear();
    fake.folders.lock().unwrap().get_mut(r"D:\a").unwrap().push(file(r"D:\a\new.txt"));
    let changed = ui.read(cx, |v| v.changed.clone());
    *changed.lock().unwrap() = Changes::Paths(HashSet::from([r"D:\a\new.txt".to_owned()]));
    settle(cx);
    assert_eq!(*fake.lists.lock().unwrap(), [r"D:\a"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].entries.len()), 2);
    // Any change, unnamed, reloads them all.
    *changed.lock().unwrap() = Changes::All;
    settle(cx);
    let lists: HashSet<String> = fake.lists.lock().unwrap().iter().cloned().collect();
    assert!(lists.contains(r"D:\") && lists.contains(r"D:\a"), "{lists:?}");
}

#[gpui_kit::test]
fn reloads_the_results_when_everything_says_they_changed(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    fake.results.lock().unwrap().push(file(r"D:\x2.txt"));
    fake.changed.store(true, Ordering::Relaxed);
    for _ in 0..4 {
        settle(cx);
    }
    assert_eq!(fake.queries().len(), before + 1);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 2);
}

#[gpui_kit::test]
fn sorts_from_the_menu_reloading_what_is_open(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    assert_eq!(ui.read(cx, |v| v.settings.sort), Order { by: Sort::Size, descending: false });
    assert_eq!(fake.queries().len(), before + 1);
    assert!(!ui.read(cx, |v| v.sort_open));
}

#[gpui_kit::test]
fn turns_the_sort_round_from_the_menu_and_keeps_it_for_another_sort(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    ui.click(cx, "sort");
    ui.click(cx, "降順");
    assert_eq!(ui.read(cx, |v| v.settings.sort), Order { by: Sort::Name, descending: true });
    assert_eq!(fake.queries().len(), before + 1);
    assert!(!ui.read(cx, |v| v.sort_open));
    ui.click(cx, "sort");
    ui.click(cx, "更新日時");
    assert_eq!(ui.read(cx, |v| v.settings.sort), Order { by: Sort::Modified, descending: true });
    // Picking the way it already runs loads nothing again.
    ui.click(cx, "sort");
    ui.click(cx, "降順");
    assert_eq!(fake.queries().len(), before + 2);
}

#[gpui_kit::test]
fn keeps_the_sort_order_for_the_next_start(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    let saves = fake.saves.load(Ordering::Relaxed);
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().sort, Order { by: Sort::Size, descending: false });
    ui.click(cx, "sort");
    ui.click(cx, "降順");
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().sort, Order { by: Sort::Size, descending: true });
    assert_eq!(fake.saves.load(Ordering::Relaxed), saves + 2);
    // Picking the order already in use writes nothing.
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    settle(cx);
    assert_eq!(fake.saves.load(Ordering::Relaxed), saves + 2);
}

#[gpui_kit::test]
fn opens_in_the_sort_order_kept(cx: &mut TestAppContext) {
    let kept = Order { by: Sort::Modified, descending: true };
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    fake.settings.lock().unwrap().sort = kept;
    let ui = open(cx, fake.clone());
    assert_eq!(ui.read(cx, |v| v.settings.sort), kept);
    ui.input(cx, "x");
    let orders = fake.orders.lock().unwrap().clone();
    assert!(!orders.is_empty());
    assert!(orders.iter().all(|o| *o == kept), "{orders:?}");
}

#[gpui_kit::test]
fn widens_home_by_its_edge_within_its_bounds(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default()));
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(100.5), edge.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 341.);
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(px(0.), edge.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 160.);
    assert_eq!(ui.read(cx, |v| v.state.columns[0].kind), Kind::Home);
}

#[gpui_kit::test]
fn widens_a_column_by_an_edge_astride_its_border_even_past_it(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    let drive = ui.read(cx, |v| v.state.columns[1].id);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3, "a column past the drive's");
    let before = ui.read(cx, |v| v.widths.column);
    let (column, edge) = ui.act(cx, |window, _| {
        (window.find(("column", drive)).bounds(), window.within(("column", drive)).find("column-resizer").bounds())
    });
    // Half in the column, clear of its scrollbar; half over the next, which it is drawn above.
    assert!(edge.left() < column.right() && column.right() < edge.right(), "{edge:?} {column:?}");
    // Above the columns' scrollbar along the bottom, which keeps its presses.
    assert!(edge.bottom() <= column.bottom() - px(8.), "{edge:?} {column:?}");
    let past = point(column.right() + px(1.), edge.center().y);
    ui.act(cx, |window, cx| window.drag(past, point(past.x + px(40.), past.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.column), before + 40.);
}

#[gpui_kit::test]
fn closes_a_toast_whose_close_button_lies_over_a_columns_edge(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    ui.view.update(cx, |v, cx| v.notify("失敗しました", crate::app::Tone::Error, cx));
    let (drive, work) = ui.read(cx, |v| (v.state.columns[1].id, v.state.columns[2].id));
    // The drive's and the folder's edges brought under the close button.
    let (left, close) = ui.act(cx, |window, _| (window.find(("column", drive)).bounds().left(), window.find(("close-toast", 1usize)).bounds().center()));
    ui.view.update(cx, |v, cx| {
        v.widths.column = (f32::from(close.x - left) / 2.).round();
        cx.notify();
    });
    settle(cx);
    let (edge, close) = ui.act(cx, |window, _| {
        (window.within(("column", work)).find("column-resizer").bounds(), window.find(("close-toast", 1usize)).bounds().center())
    });
    assert!(edge.contains(&close), "{edge:?} {close:?}");
    ui.click(cx, ("close-toast", 1usize));
    assert!(ui.toasts(cx).is_empty(), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn leaves_the_edge_of_a_column_scrolled_out_of_view_unpressable(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    let before = ui.read(cx, |v| v.widths.column);
    let ids: Vec<_> = ui.read(cx, |v| v.state.columns.iter().map(|c| c.id).collect());
    // The edge of a column the view has scrolled past, now under what lies left of the columns.
    let hidden = ui.act(cx, |window, _| {
        let view = window.find("columns-scroll").bounds();
        ids.iter().find_map(|&id| {
            window.try_find(("column", id))?;
            let edge = window.within(("column", id)).try_find("column-resizer")?.bounds();
            (edge.right() < view.left() && edge.left() > px(0.)).then(|| edge.center())
        })
    });
    let at = hidden.expect("an edge scrolled out of view");
    ui.act(cx, |window, cx| window.drag(at, point(at.x + px(40.), at.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.column), before);
}

fn drive_d() -> Fake {
    Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![])])
    }
}

#[gpui_kit::test]
fn makes_a_folder_a_favorite_with_its_star_and_ctrl_d_and_lists_it_at_home(cx: &mut TestAppContext) {
    let fake = Arc::new(drive_d());
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("star", work));
    assert_eq!(fake.settings.lock().unwrap().favorites, [r"D:\work"]);
    // Home lists it first, under its own heading.
    assert_eq!(ui.read(cx, |v| v.state.columns[0].entries[0].path.clone()), r"D:\work");
    ui.press(cx, "ctrl-d");
    assert!(fake.settings.lock().unwrap().favorites.is_empty());
    ui.press(cx, "ctrl-d");
    assert_eq!(fake.settings.lock().unwrap().favorites, [r"D:\work"]);
    // Taken off from its menu in home.
    ui.act(cx, |window, cx| window.within("home").right_click(row(r"D:\work"), cx));
    settle(cx);
    ui.click(cx, "menu-favorite");
    assert!(fake.settings.lock().unwrap().favorites.is_empty());
    assert!(ui.read(cx, |v| v.menu.is_none()));
}

#[gpui_kit::test]
fn works_in_a_favorite_by_mouse_and_tab_leaving_the_folders_above_it_folded(cx: &mut TestAppContext) {
    let sub = folder(r"D:\work\sub");
    let fake = Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![sub.clone(), file(r"D:\work\a.txt")]),
            (r"D:\work\sub", vec![]),
        ])
    };
    fake.settings.lock().unwrap().favorites = vec![r"D:\work".into()];
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\work"));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    // A row in its own column is picked there; the folder above stays folded.
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click_in(cx, ("column", work), row(r"D:\work\a.txt"));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    // A click off the rows takes the keys to the column clicked, folding nothing out.
    ui.click_in(cx, "home", row(r"D:\work"));
    ui.click(cx, ("column", work));
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    // The keys tabbed over from home land where → would, past the fold.
    ui.click_in(cx, "home", row(r"D:\work"));
    let view = ui.view.clone();
    ui.act(cx, |window, cx| view.read(cx).columns_focus.clone().focus(window, cx));
    settle(cx);
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
}

#[gpui_kit::test]
fn leaves_the_keys_in_home_when_the_room_past_the_columns_is_pressed(cx: &mut TestAppContext) {
    let fake = Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![folder(r"D:\work\sub")])])
    };
    fake.settings.lock().unwrap().favorites = vec![r"D:\work".into()];
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\work"));
    let columns = ui.read(cx, |v| v.state.columns.len());
    ui.click(cx, "columns-rest");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.active(cx), 0);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), columns);
    assert!(ui.selected(cx, columns - 1).is_empty());
}

#[gpui_kit::test]
fn lists_the_folder_tabbing_into_the_columns_opens(cx: &mut TestAppContext) {
    let fake = Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![folder(r"D:\work\sub")]),
            (r"D:\work\sub", vec![file(r"D:\work\sub\a.txt")]),
        ])
    };
    fake.settings.lock().unwrap().favorites = vec![r"D:\work".into()];
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\work"));
    let view = ui.view.clone();
    ui.act(cx, |window, cx| view.read(cx).columns_focus.clone().focus(window, cx));
    settle(cx);
    let last = ui.read(cx, |v| v.state.columns.last().map(|c| (c.path.clone(), c.loading, c.entries.len())).unwrap());
    assert_eq!(last, (r"D:\work\sub".to_owned(), false, 1));
}

#[gpui_kit::test]
fn leaves_no_room_to_scroll_once_the_columns_slide_back(cx: &mut TestAppContext) {
    // A chain deep enough to scroll the columns, and a favorite of one column beside it.
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0]), folder(r"D:\fav")]), (r"D:\fav", vec![])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    fake.settings.lock().unwrap().favorites = vec![r"D:\fav".into()];
    let ui = open(cx, Arc::new(fake));
    // The slide runs on the clock: frames a screen's refresh apart until it rests.
    let rest = |ui: &Ui, cx: &mut TestAppContext| {
        for _ in 0..120 {
            std::thread::sleep(Duration::from_millis(16));
            ui.act(cx, |_, _| ());
            settle(cx);
            if ui.read(cx, |v| v.slide.is_none()) {
                break;
            }
        }
    };
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    assert!(ui.read(cx, |v| v.columns_scroll.offset().x) < px(0.), "the chain scrolls the columns");
    ui.click_in(cx, "home", row(r"D:\fav"));
    rest(&ui, cx);
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.columns_scroll.offset().x), px(0.));
    assert_eq!(ui.read(cx, |v| v.columns_scroll.max_offset().x), px(0.));
    assert!(!ui.read(cx, |v| v.on_screen().hidden_left()));
}

#[gpui_kit::test]
fn turns_the_wheel_down_a_column_without_moving_the_columns_or_shading_their_edges(cx: &mut TestAppContext) {
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0]), folder(r"D:\fav")]), (r"D:\fav", vec![])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    let ui = open(cx, Arc::new(fake));
    let rest = |ui: &Ui, cx: &mut TestAppContext| {
        for _ in 0..120 {
            std::thread::sleep(Duration::from_millis(16));
            ui.act(cx, |_, _| ());
            settle(cx);
            if ui.read(cx, |v| v.slide.is_none()) {
                break;
            }
        }
    };
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    let at = ui.read(cx, |v| v.columns_scroll.offset().x);
    assert!(at < px(0.), "the chain scrolls the columns");
    // Down and up over a column scroll its rows; the columns stay where they rest.
    let column = ui.read(cx, |v| v.state.columns[v.state.active].id);
    for dy in [-120., 120.] {
        ui.act(cx, |window, cx| window.scroll(("column", column), ScrollDelta::Pixels(point(px(0.), px(dy))), cx));
        settle(cx);
        assert_eq!(ui.read(cx, |v| v.columns_scroll.offset().x), at);
    }
    // An offset past the bounds, as a wheel leaves it until the next layout, reads as where it will be drawn.
    ui.click_in(cx, "home", row(r"D:\"));
    rest(&ui, cx);
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.columns_scroll.max_offset().x), px(0.));
    ui.read(cx, |v| v.columns_scroll.set_offset(point(px(-300.), px(0.))));
    assert!(!ui.read(cx, |v| v.on_screen().hidden_left()));
}

/// Eight folders from `D:\` down, each in the one before: enough to scroll the columns.
fn deep_chain() -> (Fake, Vec<String>) {
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0])])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    (fake, chain)
}

/// Frames a screen's refresh apart until the columns rest: the slide runs on the clock.
fn rest(ui: &Ui, cx: &mut TestAppContext) {
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(16));
        ui.act(cx, |_, _| ());
        settle(cx);
        if ui.read(cx, |v| v.slide.is_none()) {
            break;
        }
    }
}

#[gpui_kit::test]
fn slides_a_column_on_screen_once_a_press_in_its_header_or_its_empty_room_is_let_go(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    // Brought back to the start by hand: the column cut by the right edge, and how far it reaches past it.
    let cut = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read(cx, |v| v.columns_scroll.set_offset(point(px(0.), px(0.))));
        ui.act(cx, |_, _| ());
        let ids: Vec<ColumnId> = ui.read(cx, |v| v.state.columns.iter().map(|c| c.id).collect());
        ui.act(cx, |window, _| {
            let edge = window.find("columns-scroll").bounds().right();
            ids.into_iter().find_map(|id| {
                let b = window.try_find(("column", id))?.bounds();
                (b.left() + px(40.) < edge && b.right() > edge).then_some(id)
            })
        })
        .expect("a column cut by the right edge")
    };
    for target in ["head", "column"] {
        let id = cut(&ui, cx);
        let index = ui.read(cx, |v| v.state.index_of(id).unwrap());
        let selected = ui.selected(cx, index);
        let at = if target == "head" { point(px(20.), px(10.)) } else { point(px(20.), px(300.)) };
        ui.act(cx, |window, cx| window.click_at((target, id), at, cx));
        settle(cx);
        rest(&ui, cx);
        assert_eq!(ui.active(cx), index, "{target}");
        let (column, view) = ui.act(cx, |window, _| (window.find(("column", id)).bounds(), window.find("columns-scroll").bounds()));
        assert!(column.right() <= view.right() + px(1.), "{target}: the column slid on screen");
        // Only gone to: what it had picked stays as it was.
        assert_eq!(ui.selected(cx, index), selected, "{target}");
    }
}

#[gpui_kit::test]
fn goes_to_a_crumb_s_column_picking_nothing_in_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), folder(r"D:\beta")])])
    });
    let ui = open(cx, fake);
    ui.click_in(cx, "home", row(r"D:\"));
    assert_eq!(ui.active(cx), 0);
    ui.click(cx, ("crumb", 1usize));
    assert_eq!(ui.active(cx), 1);
    assert!(ui.columns_focused(cx));
    assert!(ui.selected(cx, 1).is_empty());
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
}

/// How far the columns' view reaches past the last column, as the frame drawn now shows it: none when
/// it is flush with the right edge.
fn room_past_last(ui: &Ui, cx: &mut TestAppContext) -> f32 {
    let last = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    ui.act(cx, |window, _| f32::from(window.find("columns-scroll").bounds().right() - window.find(("column", last)).bounds().right()))
}

#[gpui_kit::test]
fn keeps_the_last_column_at_the_right_edge_as_the_window_is_resized(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    assert!(room_past_last(&ui, cx).abs() < 1.);
    for width in [1500., 1000., 1300.] {
        cx.simulate_window_resize(ui.window, size(px(width), px(800.)));
        // Already in the first frame laid out at the new size.
        let room = room_past_last(&ui, cx);
        assert!(room.abs() < 1., "{width}: {room}");
        settle(cx);
        assert!(room_past_last(&ui, cx).abs() < 1.);
        assert!(ui.read(cx, |v| v.columns_scroll.offset().x) < px(0.), "{width}: the columns still scroll");
    }
    // Brought short of the end by hand, the left edge stays as the window narrows.
    ui.read(cx, |v| v.columns_scroll.set_offset(point(px(-100.), px(0.))));
    ui.act(cx, |_, _| ());
    cx.simulate_window_resize(ui.window, size(px(1100.), px(800.)));
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.columns_scroll.offset().x), px(-100.));
}

#[gpui_kit::test]
fn keeps_the_last_column_at_the_right_edge_as_a_width_is_dragged(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    let last = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    // The columns' width, then home's both ways, which changes the columns' view.
    for (resizer, by) in [("column-resizer", 60.), ("sidebar-resizer", 120.), ("sidebar-resizer", -120.)] {
        let edge = ui.act(cx, |window, _| {
            if resizer == "column-resizer" { window.within(("column", last)).find(resizer) } else { window.find(resizer) }.bounds().center()
        });
        ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(by), edge.y), cx));
        let room = room_past_last(&ui, cx);
        assert!(room.abs() < 1., "{resizer} {by}: {room}");
        settle(cx);
    }
}

#[gpui_kit::test]
fn leaves_no_room_past_the_last_column_once_the_folders_folded_away_come_out(cx: &mut TestAppContext) {
    let deep = file(r"D:\a\b\c\d\zeta.txt");
    let fake = Arc::new(Fake::default().with_results(vec![deep.clone()]).with_folders(&[
        (r"D:\", vec![folder(r"D:\a")]),
        (r"D:\a", vec![folder(r"D:\a\b")]),
        (r"D:\a\b", vec![folder(r"D:\a\b\c")]),
        (r"D:\a\b\c", vec![folder(r"D:\a\b\c\d")]),
        (r"D:\a\b\c\d", vec![deep.clone()]),
    ]));
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    ui.click(cx, row(&deep.path));
    rest(&ui, cx);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 4);
    ui.click(cx, "unfold");
    rest(&ui, cx);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
    assert!(ui.read(cx, |v| v.columns_scroll.offset().x) < px(0.), "the folders that came out scroll the columns");
    let room = room_past_last(&ui, cx);
    assert!(room.abs() < 1., "{room}");
}

#[gpui_kit::test]
fn turns_the_wheel_with_shift_to_send_the_columns_sideways_leaving_the_rows_under_it(cx: &mut TestAppContext) {
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0])])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    // The deepest folder holds more rows than its column shows.
    let files: Vec<Entry> = (0..100).map(|i| file(&format!(r"{}\f{i:03}.txt", chain[7]))).collect();
    folders.last_mut().unwrap().1 = files;
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(16));
        ui.act(cx, |_, _| ());
        settle(cx);
        if ui.read(cx, |v| v.slide.is_none()) {
            break;
        }
    }
    let column = ui.read(cx, |v| v.state.columns.iter().find(|c| c.path == chain[7]).unwrap().id);
    let rows = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.lists[&column].0.borrow().base_handle.offset().y);
    let columns = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.columns_scroll.offset().x);
    let wheel = |ui: &Ui, cx: &mut TestAppContext, x: f32, y: f32| {
        ui.act(cx, |window, cx| window.scroll(("column", column), ScrollDelta::Lines(point(x, y)), cx));
        settle(cx);
    };
    // The wheel alone scrolls the rows, leaving the columns.
    let (top, at) = (rows(&ui, cx), columns(&ui, cx));
    assert!(at < px(0.), "the chain scrolls the columns");
    wheel(&ui, cx, 0., -3.);
    let down = rows(&ui, cx);
    assert!(down < top, "the rows scroll");
    assert_eq!(columns(&ui, cx), at);
    // With shift, as Windows sends it, the columns go sideways and the rows stay.
    wheel(&ui, cx, 3., 0.);
    assert!(columns(&ui, cx) > at, "the columns scroll");
    assert_eq!(rows(&ui, cx), down);
}

#[gpui_kit::test]
fn saves_a_search_and_runs_it_from_home_keeping_the_keys_there(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake { roots: vec![Entry::folder(r"D:\", "D:")], ..Fake::default() }.with_results(vec![file(r"D:\a.pdf")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("star", results));
    assert_eq!(fake.settings.lock().unwrap().saved_searches, [SavedSearch { name: String::new(), query: "ext:pdf".into(), filter: None }]);
    ui.press(cx, "ctrl-f");
    ui.press(cx, "escape");
    assert_eq!(ui.field(cx), "");
    ui.click(cx, row(&search_key("ext:pdf", None)));
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf");
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
    // ↓ in home moves on to the next place, leaving the search.
    ui.press(cx, "down");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "");
    assert_eq!(ui.field(cx), "");
}

#[gpui_kit::test]
fn unsaves_a_search_with_ctrl_d_and_says_so(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches = vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None }];
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    ui.press(cx, "ctrl-d");
    assert!(fake.settings.lock().unwrap().saved_searches.is_empty());
    assert!(ui.toasts(cx).iter().any(|t| t.contains("「PDF」を保存した検索から外しました")), "{:?}", ui.toasts(cx));
    // Nothing typed: nothing to save, and it says why.
    ui.press(cx, "escape");
    ui.press(cx, "ctrl-d");
    assert!(ui.toasts(cx).iter().any(|t| t.contains("検索語を入力すると保存できます")));
}

#[gpui_kit::test]
fn never_writes_over_settings_it_could_not_read(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    *fake.unreadable.lock().unwrap() = Some(LoadError { message: "設定を読み込めません".into(), kept: true });
    let ui = open(cx, fake.clone());
    assert!(ui.toasts(cx).iter().any(|t| t == "設定を読み込めません"), "{:?}", ui.toasts(cx));
    ui.input(cx, "ext:pdf");
    ui.press(cx, "ctrl-d");
    // Kept for the session, but the file stays as it was.
    assert_eq!(ui.read(cx, |v| v.settings.saved_searches.len()), 1);
    // Nor as the window closes.
    assert!(ui.view.update(cx, |v, _| v.flush_settings()).is_ok());
    assert_eq!(fake.saves.load(Ordering::Relaxed), 0);
}

#[gpui_kit::test]
fn saves_afresh_once_a_broken_file_is_put_aside(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    *fake.unreadable.lock().unwrap() = Some(LoadError { message: "壊れていました".into(), kept: false });
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    ui.press(cx, "ctrl-d");
    assert_eq!(fake.settings.lock().unwrap().saved_searches.len(), 1);
}

#[gpui_kit::test]
fn edits_a_saved_searchs_name_and_query_with_f2(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches = vec![
        SavedSearch { name: String::new(), query: "ext:pdf".into(), filter: None },
        SavedSearch { name: String::new(), query: "dm:today".into(), filter: None },
    ];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(&search_key("ext:pdf", None)));
    ui.press(cx, "f2");
    let (name, query) = ui.read(cx, |v| match &v.dialog {
        Some(crate::places::Dialog::EditSearch { name, query, .. }) => (name.clone(), query.clone()),
        _ => panic!("no dialog"),
    });
    // A query already saved cannot be taken.
    ui.act(cx, |window, cx| query.update(cx, |input, cx| input.set_value("dm:today", window, cx)));
    ui.click(cx, "save-search");
    assert!(ui.read(cx, |v| v.dialog.is_some()));
    ui.act(cx, |window, cx| {
        name.update(cx, |input, cx| input.set_value("PDF", window, cx));
        query.update(cx, |input, cx| input.set_value("ext:pdf dm:thisweek", window, cx));
    });
    ui.click(cx, "save-search");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert_eq!(
        fake.settings.lock().unwrap().saved_searches[0],
        SavedSearch { name: "PDF".into(), query: "ext:pdf dm:thisweek".into(), filter: None }
    );
    // The search showing follows its new query.
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf dm:thisweek");
    assert_eq!(ui.field(cx), "ext:pdf dm:thisweek");
    // Escape closes the dialog without a change.
    ui.click(cx, row(&search_key("ext:pdf dm:thisweek", None)));
    ui.press(cx, "f2");
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
}

#[gpui_kit::test]
fn keeps_the_widths_it_is_left_with_and_starts_with_them(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().sidebar_width = 300.;
    let ui = open(cx, fake.clone());
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 300.);
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(20.), edge.y), cx));
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().sidebar_width, 320.);
}

#[gpui_kit::test]
fn reconnects_from_the_preferences_making_the_search_afresh(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = ui.read(cx, |v| v.state.columns[1].id);
    let searches = fake.queries().len();
    ui.click(cx, "preferences");
    assert!(ui.read(cx, |v| matches!(v.dialog, Some(crate::places::Dialog::Preferences))));
    // The theme chosen takes the keys, there being no field to type in.
    assert!(ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| v.stop("theme:System", cx).is_focused(window))));
    ui.click(cx, "reconnect");
    assert_eq!(fake.reconnects.load(Ordering::Relaxed), 1);
    assert_ne!(ui.read(cx, |v| v.state.columns[1].id), before);
    assert_eq!(fake.queries().len(), searches + 1);
    assert!(matches!(ui.read(cx, |v| v.engine.clone()), Engine::Connected(_)));
    ui.click(cx, "close-preferences");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
}

#[gpui_kit::test]
fn fixes_the_theme_from_the_preferences_at_once_and_keeps_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    // A test window's appearance is light, as Windows set light.
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::light().bg);
    ui.click(cx, "preferences");
    ui.click(cx, ("theme", Theme::Dark as usize));
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::dark().bg);
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::Dark);
    ui.click(cx, ("theme", Theme::Light as usize));
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::light().bg);
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::Light);
    // Back to following Windows, light here.
    ui.click(cx, ("theme", Theme::Dark as usize));
    ui.click(cx, ("theme", Theme::System as usize));
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::light().bg);
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::System);
    // From the keys, as a stop for Tab.
    let dark = ui.read(cx, |v| v.stops[&format!("theme:{:?}", Theme::Dark)].clone());
    ui.act(cx, |window, cx| dark.focus(window, cx));
    ui.press(cx, "space");
    assert_eq!(ui.read(cx, |v| v.settings.theme), Theme::Dark);
    let light = ui.read(cx, |v| v.stops[&format!("theme:{:?}", Theme::Light)].clone());
    ui.act(cx, |window, cx| light.focus(window, cx));
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.settings.theme), Theme::Light);
}

#[gpui_kit::test]
fn opens_in_the_theme_kept_whatever_windows_is_set_to(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().theme = Theme::Dark;
    let ui = open(cx, fake);
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::dark().bg);
}

/// D:\work open in its column, listing a.txt, b.txt and c.txt, worked in.
fn in_work(cx: &mut TestAppContext) -> (Arc<Fake>, Ui) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\other"), folder(r"D:\work")]),
            (r"D:\work", vec![file(r"D:\work\a.txt"), file(r"D:\work\b.txt"), file(r"D:\work\c.txt")]),
            (r"D:\other", vec![]),
        ])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "down");
    ui.press(cx, "right");
    assert_eq!(ui.read(cx, |v| v.state.columns[2].path.clone()), r"D:\work");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    (fake, ui)
}

#[gpui_kit::test]
fn picks_several_rows_with_ctrl_shift_and_ctrl_a(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "shift-down");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\b.txt"]);
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.selected(cx, 2).len(), 3);
    ui.click(cx, row(r"D:\work\b.txt"));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\b.txt"]);
    // Shift+↑ runs from the row last picked on its own.
    ui.press(cx, "shift-up");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\b.txt"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
}

#[gpui_kit::test]
fn copies_and_pastes_picking_what_was_pasted(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-c");
    assert_eq!(fake.clipboard.lock().unwrap().paths, [r"D:\work\a.txt"]);
    assert!(ui.toasts(cx).iter().any(|t| t == "1 項目をコピーしました"));
    ui.press(cx, "ctrl-v");
    // Into its own folder: a copy named as Explorer names it, picked once listed.
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt - コピー"]);
    let entry = ui.read(cx, |v| v.history[0].clone());
    assert_eq!(entry.items[0].path, r"D:\work\a.txt - コピー");
    assert_eq!(entry.items[0].from.as_deref(), Some(r"D:\work\a.txt"));
    // Paths as text, one a line.
    ui.press(cx, "ctrl-shift-c");
    assert_eq!(*fake.text.lock().unwrap(), r"D:\work\a.txt - コピー");
}

#[gpui_kit::test]
fn picks_what_the_shell_writes_out_by_its_name_once_it_is_listed(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    *fake.clipboard.lock().unwrap() = ClipboardFiles { contents: vec!["添付.txt".into()], sequence: 2, ..Default::default() };
    ui.press(cx, "ctrl-v");
    let entry = ui.read(cx, |v| v.history[0].clone());
    assert_eq!((entry.items[0].path.as_str(), entry.items[0].status), (r"D:\work\添付.txt", ItemStatus::Handed));
    assert!(!entry.has_problems());
    // Written out after the paste returned: picked once the listing shows it.
    fake.add(r"D:\work", file(r"D:\work\添付.txt"));
    ui.click(cx, "refresh");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\添付.txt"]);
}

#[gpui_kit::test]
fn the_buttons_done_on_the_spot_leave_the_keys_where_they_were(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    assert!(ui.columns_focused(cx));
    ui.click(cx, "copy");
    assert_eq!(fake.clipboard.lock().unwrap().paths, [r"D:\work\a.txt"]);
    assert!(ui.columns_focused(cx));
    ui.click(cx, "refresh");
    assert!(ui.columns_focused(cx));
    // ↓ still moves in the column the keys stayed in.
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\b.txt"]);
    // From the search field too.
    let search = ui.read(cx, |v| v.search.clone());
    ui.act(cx, |window, cx| search.update(cx, |input, cx| input.focus(window, cx)));
    ui.click(cx, "cut");
    assert!(ui.read(cx, |v| v.clipboard.cut && v.clipboard.paths == [r"D:\work\b.txt"]));
    assert!(ui.act(cx, |window, cx| search.read(cx).focus_handle(cx).is_focused(window)));
}

#[gpui_kit::test]
fn marks_the_rows_the_clipboard_holds(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-x");
    settle(cx);
    assert!(ui.read(cx, |v| v.clipboard.cut && v.clipboard.paths == [r"D:\work\a.txt"]));
    assert!(ui.act(cx, |window, _| window.try_find(SharedString::from(r"held:D:\work\a.txt")).is_some()));
}

#[gpui_kit::test]
fn marks_a_row_another_app_put_on_the_clipboard_in_another_case(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    *fake.clipboard.lock().unwrap() = ClipboardFiles { paths: vec![r"d:\WORK\A.TXT".into()], cut: false, sequence: 2, ..Default::default() };
    (fake.watcher.lock().unwrap().as_ref().unwrap())();
    settle(cx);
    assert!(ui.act(cx, |window, _| window.try_find(SharedString::from(r"held:D:\work\a.txt")).is_some()));
}

#[gpui_kit::test]
fn renames_from_f2_refusing_a_name_windows_would_reject(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match &v.dialog {
        Some(crate::places::Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("a:b", window, cx)));
    assert!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)).is_some());
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| v.dialog.is_some()));
    assert!(fake.operations.lock().unwrap().is_empty());
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("renamed.txt", window, cx)));
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert_eq!(fake.operations.lock().unwrap()[0].name, "renamed.txt");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\renamed.txt"]);
}

#[gpui_kit::test]
fn refuses_a_rename_to_a_name_its_folder_has_but_not_its_own_in_another_case(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match &v.dialog {
        Some(crate::places::Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("b.txt", window, cx)));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), None);
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| v.dialog.is_some()));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), Some("同じ名前の項目があります"));
    assert!(fake.operations.lock().unwrap().is_empty());
    // Typing again lifts it until the next try.
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("A.txt", window, cx)));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), None);
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert_eq!(fake.operations.lock().unwrap()[0].name, "A.txt");
}

#[gpui_kit::test]
fn asks_the_folder_about_a_new_name_off_the_ui_thread_and_lets_go_of_an_answer_overtaken(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match &v.dialog {
        Some(Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("e.txt", window, cx)));
    settle(cx);
    let checking = |v: &Colet| match &v.dialog {
        Some(Dialog::Name { checking, .. }) => checking.clone(),
        _ => None,
    };
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| v.submit_name(window, cx));
        // Pressed, the dialog waits for the folder to answer rather than the window for the disk.
        assert_eq!(checking(ui.view.read(cx)).as_deref(), Some("e.txt"));
        // Typed on before it answers, as the user types: setting the value says nothing of a change.
        window.input("d", cx);
    });
    settle(cx);
    // The answer, that "e.txt" is free, was about a name no longer there: nothing is sent.
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), None);
    assert!(ui.read(cx, |v| v.dialog.is_some() && checking(v).is_none()));
    assert!(fake.operations.lock().unwrap().is_empty());
    // Pressed again while the folder is asked, it is sent once, once the folder answers.
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| v.submit_name(window, cx));
        ui.view.update(cx, |v, cx| v.submit_name(window, cx));
    });
    settle(cx);
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    let names: Vec<String> = fake.operations.lock().unwrap().iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, ["e.txtd"]);
}

#[gpui_kit::test]
fn asks_whether_a_history_item_is_still_there_off_the_ui_thread_opening_only_the_last_pressed(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let highlight = |cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns.last().unwrap().highlight.clone());
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| {
            v.show_history(true, None, window, cx);
            v.reveal_from_history(r"D:\work\a.txt".into(), window, cx);
            v.reveal_from_history(r"D:\work\c.txt".into(), window, cx);
            // The history stays up while the disk is asked.
            assert!(v.history_open);
        });
    });
    settle(cx);
    assert!(ui.read(cx, |v| !v.history_open));
    assert_eq!(highlight(cx).as_deref(), Some(r"D:\work\c.txt"));
    // Closed before the answer came, the history has moved on from it.
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| {
            v.show_history(true, None, window, cx);
            v.reveal_from_history(r"D:\work\b.txt".into(), window, cx);
            v.show_history(false, None, window, cx);
        });
    });
    settle(cx);
    assert_eq!(highlight(cx).as_deref(), Some(r"D:\work\c.txt"));
    // Gone since, it says so and the history stays open.
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| {
            v.show_history(true, None, window, cx);
            v.reveal_from_history(r"D:\work\gone.txt".into(), window, cx);
        });
    });
    settle(cx);
    assert!(ui.read(cx, |v| v.history_open));
    assert!(ui.toasts(cx).iter().any(|t| t == "「gone.txt」はもうありません"), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn makes_a_folder_and_picks_it(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-shift-n");
    ui.click(cx, "submit-name");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\新しいフォルダー"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().path.clone()), r"D:\work\新しいフォルダー");
}

#[gpui_kit::test]
fn sends_to_the_bin_leaving_the_asking_to_the_shell_and_picks_the_next_row(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "down");
    ui.press(cx, "delete");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    let asked = fake.operations.lock().unwrap()[0].clone();
    assert_eq!((asked.kind, asked.paths), (Operation::Recycle, vec![r"D:\work\b.txt".to_owned()]));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\c.txt"]);
    let entry = ui.read(cx, |v| v.history[0].clone());
    assert!(entry.items[0].gone);
    assert_eq!(entry.items[0].status, crate::ops::ItemStatus::Done);
}

#[gpui_kit::test]
fn deletes_outright_on_shift_delete_and_says_so_of_what_the_bin_could_not_take(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-delete");
    assert_eq!(fake.operations.lock().unwrap()[0].kind, Operation::Delete);
    let entry = ui.read(cx, |v| v.history[0].clone());
    assert_eq!(entry.request.kind.title(), "完全に削除");
    assert_eq!(entry.items[0].status.label(), "完全に削除");
    // Sent to the bin, one it could not take is told apart from the rest.
    fake.unbinnable.lock().unwrap().push(r"D:\work\c.txt".into());
    ui.press(cx, "ctrl-a");
    ui.press(cx, "delete");
    let entry = ui.read(cx, |v| v.history[0].clone());
    assert_eq!(entry.request.kind, Operation::Recycle);
    let statuses: Vec<_> = entry.items.iter().map(|i| (folder_name(&i.path).to_owned(), i.status.label())).collect();
    assert_eq!(statuses, [("b.txt".to_owned(), "完了"), ("c.txt".to_owned(), "完全に削除")]);
    assert_eq!(entry.outcome(), "完了");
}

#[gpui_kit::test]
fn leaves_no_trace_of_a_deletion_turned_down_at_the_shells_question(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    fake.declined.store(true, Ordering::Relaxed);
    ui.press(cx, "delete");
    ui.press(cx, "shift-delete");
    assert_eq!(fake.operations.lock().unwrap().len(), 2);
    assert!(ui.read(cx, |v| v.history.is_empty() && v.unseen == 0));
    assert!(ui.toasts(cx).is_empty(), "{:?}", ui.toasts(cx));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
}

#[gpui_kit::test]
fn offers_the_explorer_menu_only_for_items_in_one_folder(cx: &mut TestAppContext) {
    let results = vec![file(r"D:\a\x.txt"), file(r"D:\a\y.txt"), file(r"D:\b\z.txt"), file(r"\\nas\share\w.txt")];
    let fake = Arc::new(Fake::default().with_results(results));
    let ui = open(cx, fake);
    ui.input(cx, "x");
    ui.press(cx, "enter");
    let reasons = |cx: &mut TestAppContext| ui.read(cx, |v| (v.reasons().shell, v.reasons().copy));
    assert_eq!(ui.selected(cx, 1), [r"D:\a\x.txt"]);
    assert_eq!(reasons(cx), (None, None));
    ui.press(cx, "shift-down");
    assert_eq!(ui.selected(cx, 1), [r"D:\a\x.txt", r"D:\a\y.txt"]);
    assert_eq!(reasons(cx), (None, None));
    ui.press(cx, "ctrl-a");
    assert_eq!(reasons(cx), (Some("同じフォルダーの項目を選んだときに使えます"), None));
}

#[gpui_kit::test]
fn never_sends_a_drive_at_home_to_the_bin(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake { roots: vec![Entry::folder(r"D:\", "D:")], ..Fake::default().with_folders(&[(r"D:\", vec![])]) });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "delete");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert!(ui.toasts(cx).iter().any(|t| t.starts_with("「ごみ箱へ移動」はホームでは使えません")), "{:?}", ui.toasts(cx));
    ui.press(cx, "ctrl-c");
    assert!(ui.toasts(cx).iter().any(|t| t == "「コピー」はドライブはコピーできません"), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn tells_of_a_failed_operation_and_opens_it_in_the_history(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    fake.stuck.lock().unwrap().push(r"D:\work\a.txt".into());
    ui.press(cx, "delete");
    assert!(ui.toasts(cx).iter().any(|t| t == "「ごみ箱へ移動」: 1 件失敗"), "{:?}", ui.toasts(cx));
    assert_eq!(ui.read(cx, |v| v.unseen), 1);
    let toast = ui.read(cx, |v| v.toasts.iter().find(|t| t.entry.is_some()).unwrap().id);
    ui.click(cx, ("toast-details", toast));
    assert!(ui.read(cx, |v| v.history_open && v.unseen == 0));
    let entry = ui.read(cx, |v| v.history[0].clone());
    assert!(ui.read(cx, |v| v.expanded.contains(&entry.id)));
    assert_eq!(entry.items[0].error.as_deref(), Some("アクセスが拒否されました。"));
    // Still there: its name opens it in the columns.
    ui.click(cx, ("history-item", entry.id << 16));
    assert!(ui.read(cx, |v| !v.history_open));
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().highlight.clone()).as_deref(), Some(r"D:\work\a.txt"));
}

#[gpui_kit::test]
fn acts_from_the_menu_on_the_row_right_clicked_leaving_the_selection(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.act(cx, |window, cx| window.right_click(row(r"D:\work\c.txt"), cx));
    settle(cx);
    assert!(ui.read(cx, |v| v.menu.is_some()));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    ui.click(cx, "menu-copy");
    assert_eq!(fake.clipboard.lock().unwrap().paths, [r"D:\work\c.txt"]);
    assert!(ui.read(cx, |v| v.menu.is_none()));
    // The Windows menu for the folder itself, from its empty space.
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.act(cx, |window, cx| window.right_click(("column", work), cx));
    settle(cx);
    ui.click(cx, "menu-windows");
    assert_eq!(*fake.menus.lock().unwrap(), [(vec![], r"D:\work".to_owned())]);
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn acts_from_the_menu_on_the_row_right_clicked_over_the_selection_or_none(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let dialog_paths = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read(cx, |v| match &v.dialog {
            Some(crate::places::Dialog::Name { request: Request { paths, .. }, .. }) => paths.clone(),
            _ => Vec::new(),
        })
    };
    let menu = |ui: &Ui, cx: &mut TestAppContext, path: &str, id: &str| {
        ui.act(cx, |window, cx| window.right_click(row(path), cx));
        settle(cx);
        ui.click(cx, id.to_owned());
    };
    // Another row than the selection: the rename is the one right-clicked's.
    menu(&ui, cx, r"D:\work\c.txt", "menu-rename");
    assert_eq!(dialog_paths(&ui, cx), [r"D:\work\c.txt"]);
    ui.press(cx, "escape");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    // Nothing selected in the column: the commands are on all the same.
    ui.view.update(cx, |v, cx| {
        v.state.columns[2].selected.clear();
        cx.notify();
    });
    settle(cx);
    assert!(ui.selected(cx, 2).is_empty());
    menu(&ui, cx, r"D:\work\b.txt", "menu-copy");
    assert_eq!(fake.clipboard.lock().unwrap().paths, [r"D:\work\b.txt"]);
    menu(&ui, cx, r"D:\work\b.txt", "menu-recycle");
    let asked = fake.operations.lock().unwrap().last().unwrap().clone();
    assert_eq!((asked.kind, asked.paths), (Operation::Recycle, vec![r"D:\work\b.txt".to_owned()]));
}

#[gpui_kit::test]
fn walks_the_file_menu_by_keys_and_closes_it_on_escape(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    assert!(ui.read(cx, |v| v.menu.is_some()));
    // From the keys, the first line is lit; ↓ passes over those that are off.
    assert_eq!(ui.read(cx, |v| v.menu_cursor), Some(0));
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(fake.clipboard.lock().unwrap().paths, [r"D:\work\a.txt"]);
    assert!(ui.read(cx, |v| v.menu.is_none()));
    assert!(ui.columns_focused(cx));
    ui.press(cx, "shift-f10");
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.menu.is_none()));
    assert!(ui.columns_focused(cx));
    // ↑ goes round to the last line that is on.
    ui.press(cx, "shift-f10");
    ui.press(cx, "up");
    let last = ui.read(cx, |v| v.menu_lines().iter().rposition(|l| l.off.is_none()));
    assert_eq!(ui.read(cx, |v| v.menu_cursor), last);
}

#[gpui_kit::test]
fn picks_the_sort_order_by_keys(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.click(cx, "sort");
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.settings.sort), Order { by: Sort::Size, descending: false });
    assert!(!ui.read(cx, |v| v.sort_open));
    assert!(ui.columns_focused(cx));
    // From the button itself, the sort in use is lit.
    let sort = ui.read(cx, |v| v.sort_focus.clone());
    ui.act(cx, |window, cx| sort.focus(window, cx));
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.menu_cursor), Some(1));
    ui.press(cx, "escape");
    assert!(!ui.read(cx, |v| v.sort_open));
}

#[gpui_kit::test]
fn marks_under_the_crumbs_what_is_on_screen_while_some_are_folded(cx: &mut TestAppContext) {
    let deep = file(r"D:\a\b\zeta.txt");
    let fake = Arc::new(
        Fake::default()
            .with_results(vec![deep.clone()])
            .with_folders(&[(r"D:\", vec![folder(r"D:\a")]), (r"D:\a", vec![folder(r"D:\a\b")]), (r"D:\a\b", vec![deep.clone()])]),
    );
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    assert!(ui.act(cx, |window, _| window.try_find("crumb-bar").is_none()));
    ui.click(cx, row(&deep.path));
    ui.act(cx, |_, _| ());
    assert!(ui.act(cx, |window, _| window.try_find("crumb-bar").is_some()));
    // Out for good, the bar stays only while some column is out of sight past an edge.
    ui.click(cx, "unfold");
    ui.act(cx, |_, _| ());
    let hidden = ui.read(cx, |v| v.on_screen().hidden_left() || v.on_screen().hidden_right());
    assert_eq!(ui.act(cx, |window, _| window.try_find("crumb-bar").is_some()), hidden);
}

#[gpui_kit::test]
fn runs_a_saved_search_from_enter_and_double_click_without_sending_it_to_the_shell(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches = vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None }];
    let ui = open(cx, fake.clone());
    ui.press(cx, "down");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "");
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf");
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(ui.home_focused(cx));
    assert!(fake.opened.lock().unwrap().is_empty());
    ui.press(cx, "ctrl-f");
    ui.press(cx, "escape");
    ui.act(cx, |window, cx| window.double_click(row(&search_key("ext:pdf", None)), cx));
    settle(cx);
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(fake.opened.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn brings_homes_focused_row_into_view_across_all_three_headings(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![folder(r"D:\")],
        ..Fake::default().with_folders(&[(r"D:\", vec![]), (r"D:\favorite", vec![])])
    });
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.favorites = vec![r"D:\favorite".into()];
        settings.saved_searches = (0..50).map(|i| SavedSearch { name: String::new(), query: format!("query{i}"), filter: None }).collect();
    }
    let ui = open(cx, fake);
    ui.press(cx, "down");
    ui.press(cx, "end");
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    assert!(ui.read(cx, |v| v.home_scroll.offset().y < px(0.)));
    assert!(ui.act(cx, |window, _| window.within("home-rows").find(row(r"D:\")).visible()));
    ui.press(cx, "home");
    assert_eq!(ui.selected(cx, 0), [r"D:\favorite"]);
    assert!(ui.act(cx, |window, _| window.within("home-rows").find(row(r"D:\favorite")).visible()));
    assert!(ui.home_focused(cx));
}

#[gpui_kit::test]
fn closes_the_menu_and_returns_the_keys_when_its_column_vanishes(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    fake.remove(r"D:\work");
    let changed = ui.read(cx, |v| v.changed.clone());
    *changed.lock().unwrap() = Changes::Paths(HashSet::from([r"D:\work".into()]));
    settle(cx);
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    assert!(ui.read(cx, |v| v.menu.is_none()));
    assert!(ui.columns_focused(cx));
    assert!(fake.menus.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn dismisses_a_removed_columns_menu_before_another_frame(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.view.update(cx, |v, cx| {
        v.state.select(1, vec![r"D:\other".into()]);
        v.changed(cx);
        assert!(v.menu.is_none());
        assert!(v.menu_cursor.is_none());
    });
    assert!(cx.update_window(ui.window, |_, window, cx| ui.view.read(cx).columns_focus.is_focused(window)).unwrap());
}

#[gpui_kit::test]
fn dismissing_a_removed_columns_menu_keeps_focus_given_to_a_field(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| {
        v.state.select(1, vec![r"D:\other".into()]);
        v.changed(cx);
        v.search.update(cx, |input, cx| input.focus(window, cx));
    }));
    assert!(cx.update_window(ui.window, |_, window, cx| {
        ui.view.read(cx).search.read(cx).focus_handle(cx).is_focused(window)
    }).unwrap());
    assert!(ui.read(cx, |v| v.menu.is_none()));
}

#[gpui_kit::test]
fn refuses_a_stale_menu_even_when_another_column_takes_its_index(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| {
        let column = v.menu.as_ref().unwrap().column;
        v.state.select(1, vec![r"D:\other".into()]);
        assert_ne!(v.state.columns[2].id, column);
        v.show_shell_menu(window, cx);
    }));
    assert!(fake.menus.lock().unwrap().is_empty());
    assert!(ui.read(cx, |v| v.menu.is_none() && v.menu_cursor.is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn showing_the_shell_menu_directly_returns_the_keys_to_its_column(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| v.show_shell_menu(window, cx)));
    settle(cx);
    assert_eq!(fake.menus.lock().unwrap().len(), 1);
    assert!(ui.read(cx, |v| v.menu.is_none() && v.menu_cursor.is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn the_final_settings_save_supersedes_debounced_saves(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    let before = fake.saves.load(Ordering::Relaxed);
    ui.view.update(cx, |v, cx| {
        v.update_settings(|s| s.favorites = vec!["older".into()], cx);
        v.update_settings(|s| s.favorites = vec!["latest".into()], cx);
        v.flush_settings().unwrap();
    });
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().favorites, ["latest"]);
    assert_eq!(fake.saves.load(Ordering::Relaxed), before + 1);
}

#[gpui_kit::test]
fn refreshes_every_loaded_search_page_after_a_change_overtaken_by_pagination(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results((0..511).map(|i| file(&format!(r"D:\{i:04}.txt"))).collect()));
    let ui = open(cx, fake.clone());
    ui.input(cx, "fixture");
    let id = ui.read(cx, |v| v.state.columns[1].id);
    ui.view.update(cx, |v, _| {
        let c = &mut v.state.columns[1];
        c.selected = vec![r"D:\0400.txt".into()];
        c.focused = c.selected.first().cloned();
    });
    fake.results.lock().unwrap().remove(0);
    fake.changed.store(true, Ordering::Relaxed);
    ui.view.update(cx, |v, cx| v.more(id, cx));
    for _ in 0..4 { settle(cx); }
    let paths = ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    assert_eq!(paths.len(), 510);
    assert!(!paths.contains(&r"D:\0000.txt".into()));
    assert!(paths.contains(&r"D:\0500.txt".into()));
    assert_eq!(ui.selected(cx, 1), [r"D:\0400.txt"]);
}

#[gpui_kit::test]
fn keeps_a_dialog_taller_than_the_window_within_it_its_buttons_in_reach(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default()));
    cx.simulate_window_resize(ui.window, size(px(1200.), px(500.)));
    ui.click(cx, "preferences");
    let (dialog, close) = ui.act(cx, |window, _| (window.find("dialog").bounds(), window.find("close-preferences").bounds()));
    assert!(dialog.top() >= px(0.) && dialog.bottom() <= px(500.), "{dialog:?}");
    assert!(close.bottom() <= dialog.bottom(), "{close:?} in {dialog:?}");
    // What does not fit scrolls between the heading and the buttons.
    assert!(ui.read(cx, |v| v.dialog_scroll.max_offset().y) > px(0.));
    ui.click(cx, "close-preferences");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
}

#[gpui_kit::test]
fn scrolls_a_column_s_last_row_clear_of_the_columns_scrollbar(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let deepest = chain.last().unwrap().clone();
    let files: Vec<Entry> = (0..80).map(|n| file(&format!(r"{deepest}\{n:02}.txt"))).collect();
    let last = files.last().unwrap().path.clone();
    for entry in files {
        fake.add(&deepest, entry);
    }
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..=chain.len() {
        ui.press(cx, "right");
    }
    ui.press(cx, "end");
    rest(&ui, cx);
    let active = ui.active(cx);
    assert_eq!(ui.selected(cx, active), [last.clone()]);
    let (view, bottom) = ui.act(cx, |window, _| (window.find("columns-scroll").bounds(), window.find(row(&last)).bounds().bottom()));
    // The columns scroll sideways, so their scrollbar lies over the bottom.
    assert!(ui.read(cx, |v| v.columns_scroll.max_offset().x) > px(0.));
    assert!(bottom <= view.bottom() - px(crate::view::SCROLLBAR_WIDTH), "{bottom:?} against {view:?}");
}

#[gpui_kit::test]
fn opens_the_folders_folded_off_the_trail_from_the_keys_and_goes_to_one(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    cx.simulate_window_resize(ui.window, size(px(800.), px(800.)));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    let fold = ui.read(cx, |v| v.stops["crumb-fold"].clone());
    ui.act(cx, |window, cx| fold.focus(window, cx));
    // Enter opens it with its first line lit; ↓ moves on, Esc hands the keys back to the button.
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| v.crumb_menu_open));
    assert_eq!(ui.read(cx, |v| v.crumb_cursor), Some(0));
    ui.press(cx, "down");
    assert_eq!(ui.read(cx, |v| v.crumb_cursor), Some(1));
    ui.press(cx, "escape");
    assert!(!ui.read(cx, |v| v.crumb_menu_open));
    assert!(ui.act(cx, |window, _| fold.is_focused(window)));
    // Space opens it too; the second line lit, Enter goes to its column, the keys with it.
    ui.press(cx, "space");
    ui.press(cx, "down");
    // The trail starts at the drive: its second line is the first folder.
    let target = ui.read(cx, |v| v.state.columns.iter().position(|c| c.path == chain[0]).unwrap());
    ui.press(cx, "enter");
    assert!(!ui.read(cx, |v| v.crumb_menu_open));
    assert_eq!(ui.active(cx), target);
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn closes_a_toast_from_the_keys_handing_them_back_to_where_they_were(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.click(cx, row(r"D:\"));
    ui.view.update(cx, |v, cx| v.notify("失敗しました", crate::app::Tone::Error, cx));
    ui.act(cx, |_, _| ());
    let close = ui.read(cx, |v| v.stops["close-toast:1"].clone());
    ui.act(cx, |window, cx| close.focus(window, cx));
    ui.press(cx, "enter");
    assert!(ui.toasts(cx).is_empty());
    assert!(ui.home_focused(cx));
    assert!(ui.read(cx, |v| !v.stops.contains_key("close-toast:1")));
}

#[gpui_kit::test]
fn narrows_a_folder_by_name_from_slash_and_lists_all_again_on_escape(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), folder(r"D:\gamma")]),
            (r"D:\alpha", vec![]),
        ])
    });
    let ui = open(cx, fake);
    // Home narrows nothing, and says so.
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "left");
    ui.press(cx, "/");
    assert!(ui.toasts(cx).iter().any(|t| t.contains("フォルダーのカラムでだけ")));
    ui.press(cx, "right");
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    let (home, id, alpha) = ui.read(cx, |v| (v.state.columns[0].id, v.state.columns[1].id, v.state.columns[2].id));
    // Each folder has its field from the start, an empty one too; home has none.
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    assert!(ui.act(cx, |window, _| window.try_find(("filter", alpha)).is_some()));
    assert!(ui.act(cx, |window, _| window.try_find(("filter", home)).is_none()));
    let typed = |ui: &Ui, cx: &mut TestAppContext| ui.view.read_with(cx, |v, cx| v.filters[&id].input.read(cx).value().to_string());
    ui.press(cx, "/");
    // What is typed narrows the rows rather than jumping through them; the folder it hides closes.
    ui.input(cx, "TA");
    let shown = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    assert_eq!(shown(&ui, cx), [r"D:\beta.txt"]);
    assert!(ui.selected(cx, 1).is_empty());
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    // ↓ goes into the rows, on the first.
    ui.press(cx, "down");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    // / goes back to the field, its text picked to type over.
    ui.press(cx, "/");
    ui.input(cx, "mm");
    assert_eq!(shown(&ui, cx), [r"D:\gamma"]);
    ui.press(cx, "enter");
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma"]);
    // Esc in the rows lists all again and empties the field, the pick kept.
    ui.press(cx, "escape");
    assert_eq!(shown(&ui, cx).len(), 3);
    assert_eq!(typed(&ui, cx), "");
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma"]);
    // In the field, Esc clears it, then goes back to the rows.
    ui.press(cx, "/");
    ui.input(cx, "zz");
    assert!(shown(&ui, cx).is_empty());
    ui.press(cx, "escape");
    assert_eq!(shown(&ui, cx).len(), 3);
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "escape");
    assert!(ui.columns_focused(cx));
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    // Its x clears it.
    ui.press(cx, "/");
    ui.input(cx, "be");
    assert_eq!(shown(&ui, cx), [r"D:\beta.txt"]);
    ui.click(cx, ("clear-filter", id));
    assert_eq!(shown(&ui, cx).len(), 3);
    assert_eq!(typed(&ui, cx), "");
    // Lifted for what was made or pasted, the field empties too.
    ui.press(cx, "/");
    ui.input(cx, "be");
    ui.view.update(cx, |v, cx| {
        v.state.set_filter(1, None);
        cx.notify();
    });
    settle(cx);
    ui.act(cx, |_, _| ());
    assert_eq!(typed(&ui, cx), "");
    assert_eq!(shown(&ui, cx).len(), 3);
}

#[gpui_kit::test]
fn keeps_the_arrows_and_enter_in_a_filter_field_to_itself(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\alpha"), file(r"D:\beta.txt")]),
            (r"D:\alpha", vec![]),
        ])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    let active = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.active);
    ui.press(cx, "/");
    ui.input(cx, "al");
    // Past either end of the text, ← and → stay in the field rather than going to the next column.
    ui.press(cx, "right");
    assert_eq!(active(&ui, cx), 1);
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "left");
    ui.press(cx, "left");
    ui.press(cx, "left");
    assert_eq!(active(&ui, cx), 1);
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "escape");
    ui.press(cx, "escape");
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    // Enter goes into the rows and opens nothing there.
    ui.press(cx, "/");
    ui.input(cx, "ta");
    ui.press(cx, "enter");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    assert!(fake.opened.lock().unwrap().is_empty());
}

fn with_filters(fake: Fake) -> Fake {
    fake.settings.lock().unwrap().filters = vec![
        SearchFilter { name: "画像".into(), query: "ext:jpg;png".into() },
        SearchFilter { name: "動画".into(), query: "ext:mp4".into() },
    ];
    fake
}

#[gpui_kit::test]
fn keeps_room_to_type_a_search_in_the_narrowest_window(cx: &mut TestAppContext) {
    let fake = drive_d();
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.filters = vec![SearchFilter { name: "去年の夏に撮ったとても大きな写真".into(), query: "ext:jpg".into() }];
        settings.filter = Some("去年の夏に撮ったとても大きな写真".into());
    }
    let ui = open(cx, Arc::new(fake));
    let (width, height) = crate::app::MIN_WINDOW;
    cx.simulate_window_resize(ui.window, size(px(width), px(height)));
    let (field, filter, last) = ui.act(cx, |window, _| {
        (window.find("search-box").bounds(), window.find("search-filter").bounds(), window.find("preferences").bounds())
    });
    assert!(field.size.width >= px(crate::app::SEARCH_FIELD_MIN - 6.), "{field:?}");
    // The filter's name gave way, and nothing is pushed past the window's edge.
    assert!(filter.size.width < px(200.), "{filter:?}");
    assert!(last.right() <= px(width), "{last:?}");
}

impl Ui {
    fn search_field_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).search.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Types into the fields of the filter being added or edited.
    fn type_filter(&self, cx: &mut TestAppContext, name: &str, query: &str) {
        let (name_field, query_field) = self.read(cx, |v| match &v.dialog {
            Some(Dialog::Filters { edit: Some(edit), .. }) => (edit.name.clone(), edit.query.clone()),
            _ => panic!("no filter being edited"),
        });
        self.act(cx, |window, cx| {
            name_field.update(cx, |input, cx| input.set_value(name.to_owned(), window, cx));
            query_field.update(cx, |input, cx| input.set_value(query.to_owned(), window, cx));
        });
        settle(cx);
    }
}

fn last_query(fake: &Fake) -> String {
    fake.queries().last().cloned().unwrap_or_default()
}

#[gpui_kit::test]
fn searches_within_the_filter_chosen_with_nothing_typed_too(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\a.jpg")])));
    let ui = open(cx, fake.clone());
    ui.click(cx, "search-filter");
    assert!(ui.read(cx, |v| v.filter_open));
    ui.click(cx, ("filter", 0usize));
    assert!(!ui.read(cx, |v| v.filter_open));
    assert_eq!(fake.settings.lock().unwrap().filter.as_deref(), Some("画像"));
    assert_eq!(last_query(&fake), "<ext:jpg;png>");
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
    // What is typed is searched within it.
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    assert_eq!(last_query(&fake), "<ext:jpg;png> <cat>");
    // Esc empties the field, the filter still showing all it lets through; again, it lets go of it.
    ui.press(cx, "escape");
    assert_eq!(ui.field(cx), "");
    assert_eq!(last_query(&fake), "<ext:jpg;png>");
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    ui.press(cx, "escape");
    assert_eq!(fake.settings.lock().unwrap().filter, None);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 1);
    // Without one, what is typed is searched as it stands.
    ui.input(cx, "cat");
    assert_eq!(last_query(&fake), "cat");
}

#[gpui_kit::test]
fn chooses_a_filter_from_the_keys_and_hands_them_back_to_the_field(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\a.jpg")])));
    let ui = open(cx, fake.clone());
    assert!(ui.search_field_focused(cx));
    ui.press(cx, "ctrl-shift-f");
    assert!(ui.read(cx, |v| v.filter_open));
    // The filter in use is lit: none yet.
    assert_eq!(ui.read(cx, |v| v.menu_cursor), Some(0));
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(fake.settings.lock().unwrap().filter.as_deref(), Some("動画"));
    assert!(ui.search_field_focused(cx));
    ui.press(cx, "ctrl-shift-f");
    assert_eq!(ui.read(cx, |v| v.menu_cursor), Some(2));
    ui.press(cx, "escape");
    assert!(!ui.read(cx, |v| v.filter_open));
    assert_eq!(fake.settings.lock().unwrap().filter.as_deref(), Some("動画"));
}

#[gpui_kit::test]
fn saves_a_search_with_its_filter_and_runs_it_within_it(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\cat.jpg")])));
    let ui = open(cx, fake.clone());
    ui.click(cx, "search-filter");
    ui.click(cx, ("filter", 0usize));
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    ui.press(cx, "ctrl-d");
    let saved = |filter: Option<&str>| SavedSearch { name: String::new(), query: "cat".into(), filter: filter.map(Into::into) };
    assert_eq!(fake.settings.lock().unwrap().saved_searches, [saved(Some("画像"))]);
    // The same query with no filter is another search, saved apart.
    ui.click(cx, "search-filter");
    ui.click(cx, "filter-none");
    assert_eq!(ui.read(cx, |v| (v.state.query.clone(), v.state.search_filter.clone())), ("cat".into(), None));
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-d");
    assert_eq!(fake.settings.lock().unwrap().saved_searches, [saved(Some("画像")), saved(None)]);
    // Run from home, it chooses its filter again.
    ui.click(cx, row(&search_key("cat", Some("画像"))));
    assert_eq!(fake.settings.lock().unwrap().filter.as_deref(), Some("画像"));
    assert_eq!(last_query(&fake), "<ext:jpg;png> <cat>");
    // Back, the filter of the search before comes back with it.
    ui.press(cx, "alt-left");
    assert_eq!(ui.read(cx, |v| v.state.search_filter.clone()), None);
    assert_eq!(fake.settings.lock().unwrap().filter, None);
    // Its query alone is copied.
    ui.click(cx, row(&search_key("cat", Some("画像"))));
    ui.press(cx, "ctrl-shift-c");
    assert_eq!(*fake.text.lock().unwrap(), "cat");
}

#[gpui_kit::test]
fn changes_the_filter_a_saved_search_runs_within_from_its_edit(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\cat.jpg")])));
    fake.settings.lock().unwrap().saved_searches = vec![SavedSearch { name: String::new(), query: "cat".into(), filter: Some("画像".into()) }];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(&search_key("cat", Some("画像"))));
    ui.press(cx, "f2");
    ui.click(cx, ("edit-search-filter", 2usize));
    ui.click(cx, "save-search");
    assert_eq!(fake.settings.lock().unwrap().saved_searches[0].filter.as_deref(), Some("動画"));
    // The search showing follows it.
    assert_eq!(last_query(&fake), "<ext:mp4> <cat>");
    assert_eq!(fake.settings.lock().unwrap().filter.as_deref(), Some("動画"));
}

#[gpui_kit::test]
fn adds_edits_and_deletes_filters_with_what_names_them_following(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\cat.jpg")]));
    let ui = open(cx, fake.clone());
    let listing = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| matches!(v.dialog, Some(Dialog::Filters { edit: None, deleting: None })));
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    assert!(listing(&ui, cx));
    ui.click(cx, "add-filter");
    ui.type_filter(cx, "画像", "ext:jpg;png");
    ui.click(cx, "save-filter");
    ui.click(cx, "add-filter");
    ui.type_filter(cx, "動画", "ext:mp4");
    ui.press(cx, "enter");
    let names = |fake: &Fake| fake.settings.lock().unwrap().filters.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&fake), ["画像", "動画"]);
    // A name taken cannot be had; Esc goes back to the list.
    ui.click(cx, ("filter-edit", 1usize));
    ui.type_filter(cx, "画像", "ext:mp4");
    ui.click(cx, "save-filter");
    assert!(!listing(&ui, cx));
    ui.press(cx, "escape");
    assert!(listing(&ui, cx));
    assert_eq!(names(&fake), ["画像", "動画"]);
    ui.click(cx, "close-filters");
    // Chosen, saved with a search, and shown.
    ui.click(cx, "search-filter");
    ui.click(cx, ("filter", 0usize));
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    ui.press(cx, "ctrl-d");
    // Renamed and asking another query, what names it follows, and the results ask again.
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    ui.click(cx, ("filter-edit", 0usize));
    ui.type_filter(cx, "写真", "ext:jpg");
    ui.click(cx, "save-filter");
    assert_eq!(names(&fake), ["写真", "動画"]);
    assert_eq!(fake.settings.lock().unwrap().filter.as_deref(), Some("写真"));
    assert_eq!(fake.settings.lock().unwrap().saved_searches[0].filter.as_deref(), Some("写真"));
    assert_eq!(ui.read(cx, |v| v.state.search_filter.clone()).as_deref(), Some("写真"));
    assert_eq!(last_query(&fake), "<ext:jpg> <cat>");
    // Put in another order.
    ui.click(cx, ("filter-down", 0usize));
    assert_eq!(names(&fake), ["動画", "写真"]);
    // Asked about first, with the saved searches that use it counted; gone, they go without it.
    ui.click(cx, ("filter-delete", 1usize));
    assert_eq!(ui.read(cx, |v| v.filter_users("写真")), 1);
    assert!(ui.read(cx, |v| matches!(&v.dialog, Some(Dialog::Filters { deleting: Some(name), .. }) if name == "写真")));
    ui.click(cx, "delete-filter");
    assert_eq!(names(&fake), ["動画"]);
    assert_eq!(fake.settings.lock().unwrap().filter, None);
    assert_eq!(fake.settings.lock().unwrap().saved_searches[0].filter, None);
    assert_eq!(last_query(&fake), "cat");
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
}
