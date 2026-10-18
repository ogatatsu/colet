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
    entry::{Entry, Sort},
    everything::Page,
    operations::{Event, Kind as Operation, Request},
    paths::{ancestors, child, folder_name},
    shell::{Owner, clipboard::ClipboardFiles},
};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ElementId, SharedString, Entity, ScrollDelta, ScrollStrategy, TestAppContext, Window,
    WindowBounds, WindowOptions, point, px, size,
};

use crate::{
    app::{Colet, Engine},
    backend::{Backend, Search},
    settings::{LoadError, SavedSearch, Settings},
    state::{Kind, search_key},
};

#[derive(Default)]
struct Fake {
    roots: Vec<Entry>,
    folders: Mutex<HashMap<String, Vec<Entry>>>,
    results: Mutex<Vec<Entry>>,
    /// Each search sent: its query and where its page starts.
    searches: Mutex<Vec<(String, usize)>>,
    /// Each folder listed.
    lists: Mutex<Vec<String>>,
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

    fn list(&self, path: &str, _: Sort, _: &AtomicBool) -> Result<Vec<Entry>, String> {
        self.lists.lock().unwrap().push(path.into());
        self.folders.lock().unwrap().get(path).cloned().ok_or_else(|| "フォルダーを開けません".into())
    }

    fn search(&self, search: Search) -> Result<(Page, String), String> {
        self.searches.lock().unwrap().push((search.query.clone(), search.offset));
        if let Some(error) = self.failing.lock().unwrap().clone() {
            return Err(error);
        }
        let results = self.results.lock().unwrap();
        let end = (search.offset + search.count).min(results.len());
        let entries = results[search.offset.min(end)..end].to_vec();
        Ok((Page { entries, total: results.len(), offset: search.offset }, "Everything 1.5".into()))
    }

    fn search_changed(&self) -> Result<bool, String> {
        Ok(self.changed.swap(false, Ordering::Relaxed))
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
            Operation::Recycle => {
                for path in &request.paths {
                    if stuck.contains(path) {
                        failed = true;
                        report(Event::Failed { path: path.clone(), error: "アクセスが拒否されました。".into() });
                    } else {
                        self.remove(path);
                        report(Event::Done { path: path.clone() });
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
            }
        }
        if failed { Err("一部の項目を操作できませんでした。結果を確認してください。".into()) } else { Ok(()) }
    }

    fn clipboard_files(&self) -> Option<ClipboardFiles> {
        Some(self.clipboard.lock().unwrap().clone())
    }

    fn copy_files(&self, paths: &[String], cut: bool, _: Owner) -> Result<(), String> {
        *self.clipboard.lock().unwrap() = ClipboardFiles { paths: paths.to_vec(), cut, sequence: 1 };
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
    *changed.lock().unwrap() = Some(Some(HashSet::from([r"D:\a\new.txt".to_owned()])));
    settle(cx);
    assert_eq!(*fake.lists.lock().unwrap(), [r"D:\a"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].entries.len()), 2);
    // Any change, unnamed, reloads them all.
    *changed.lock().unwrap() = Some(None);
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
    ui.click(cx, "大きい順");
    assert_eq!(ui.read(cx, |v| v.sort), Sort::Size);
    assert_eq!(fake.queries().len(), before + 1);
    assert!(!ui.read(cx, |v| v.sort_open));
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

#[gpui_kit::test]
fn saves_a_search_and_runs_it_from_home_keeping_the_keys_there(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake { roots: vec![Entry::folder(r"D:\", "D:")], ..Fake::default() }.with_results(vec![file(r"D:\a.pdf")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("star", results));
    assert_eq!(fake.settings.lock().unwrap().saved_searches, [SavedSearch { name: String::new(), query: "ext:pdf".into() }]);
    ui.press(cx, "ctrl-f");
    ui.press(cx, "escape");
    assert_eq!(ui.field(cx), "");
    ui.click(cx, row(&search_key("ext:pdf")));
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
    fake.settings.lock().unwrap().saved_searches = vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into() }];
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
        SavedSearch { name: String::new(), query: "ext:pdf".into() },
        SavedSearch { name: String::new(), query: "dm:today".into() },
    ];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(&search_key("ext:pdf")));
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
        SavedSearch { name: "PDF".into(), query: "ext:pdf dm:thisweek".into() }
    );
    // The search showing follows its new query.
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf dm:thisweek");
    assert_eq!(ui.field(cx), "ext:pdf dm:thisweek");
    // Escape closes the dialog without a change.
    ui.click(cx, row(&search_key("ext:pdf dm:thisweek")));
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
    let instance = ui.read(cx, |v| match &v.dialog {
        Some(crate::places::Dialog::Preferences { instance }) => instance.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| instance.update(cx, |input, cx| input.set_value("1.5a", window, cx)));
    ui.input(cx, " ");
    settle(cx);
    ui.click(cx, "reconnect");
    assert_eq!(fake.reconnects.load(Ordering::Relaxed), 1);
    assert_ne!(ui.read(cx, |v| v.state.columns[1].id), before);
    assert_eq!(fake.queries().len(), searches + 1);
    assert!(matches!(ui.read(cx, |v| v.engine.clone()), Engine::Connected(_)));
    ui.click(cx, "close-preferences");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
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
    *fake.clipboard.lock().unwrap() = ClipboardFiles { paths: vec![r"d:\WORK\A.TXT".into()], cut: false, sequence: 2 };
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
fn makes_a_folder_and_picks_it(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-shift-n");
    ui.click(cx, "submit-name");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\新しいフォルダー"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().path.clone()), r"D:\work\新しいフォルダー");
}

#[gpui_kit::test]
fn sends_to_the_bin_after_asking_and_picks_the_next_row(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "down");
    ui.press(cx, "delete");
    assert!(ui.read(cx, |v| matches!(v.dialog, Some(crate::places::Dialog::Recycle { .. }))));
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert_eq!(fake.operations.lock().unwrap()[0].paths, [r"D:\work\b.txt"]);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\c.txt"]);
    assert!(ui.read(cx, |v| v.history[0].items[0].gone));
    // Escape leaves it be.
    ui.press(cx, "delete");
    ui.press(cx, "escape");
    assert_eq!(fake.operations.lock().unwrap().len(), 1);
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
    ui.click(cx, "confirm-recycle");
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
}

#[gpui_kit::test]
fn acts_from_the_menu_on_the_row_right_clicked_over_the_selection_or_none(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let dialog_paths = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read(cx, |v| match &v.dialog {
            Some(crate::places::Dialog::Name { paths, .. } | crate::places::Dialog::Recycle { paths, .. }) => paths.clone(),
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
    assert_eq!(dialog_paths(&ui, cx), [r"D:\work\b.txt"]);
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
    assert_eq!(ui.read(cx, |v| v.sort), Sort::Size);
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
