//! The window driven as the user would: keys, clicks and drags, against listings the test makes up.
use std::{
    collections::{HashMap, HashSet},
    ffi::OsStr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc,
    },
    time::Duration,
};

use colet_core::{
    entry::{Entry, Hiding, Marks, Order, Sort},
    everything::Page,
    operations::{CANCELLED, Event, Kind as Operation, Request},
    paths::{ancestors, child, folder_name, id_text, path_key},
    quick_look::{Error as QuickLookError, Message as QuickLookMessage},
    shell::{Dropped, Owner, SendTarget, clipboard::ClipboardFiles},
    watch::Changes,
};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ElementId, Entity, EntityInputHandler as _, Focusable, Modifiers, ScrollDelta,
    ScrollStrategy, SharedString, TestAppContext, Window, WindowBounds, WindowOptions, point, px, size,
};

use crate::{
    app::Colet,
    backend::{Backend, PathKind, Search},
    loading::Engine,
    ops::ItemStatus,
    places::Dialog,
    settings::{LoadError, SavedSearch, SearchFilter, Settings, Theme},
    sorts::Sorts,
    state::{ColumnId, HomeKey, Kind},
    stops::Stop,
    theme::Palette,
};

mod address;
mod column_filter;
mod file_ops;
mod focus;
mod hidden;
mod home;
mod menus;
mod navigation;
mod properties;
mod quick_look;
mod scrolling;
mod search;
mod settings;
mod sorting;
mod tabs;
mod watch;
mod window;

#[derive(Default)]
struct Fake {
    roots: Vec<Entry>,
    folders: Mutex<HashMap<String, Vec<Entry>>>,
    results: Mutex<Vec<Entry>>,
    /// Each search sent: its query and where its page starts.
    searches: Mutex<Vec<(String, usize)>>,
    /// Each folder listed.
    lists: Mutex<Vec<String>>,
    /// The order each listing and search was asked in.
    orders: Mutex<Vec<Order>>,
    /// Each folder listed, with the order it was asked in.
    listed: Mutex<Vec<(String, Order)>>,
    failing: Mutex<Option<String>>,
    changed: AtomicBool,
    /// The settings as last written, starting from those it is made with.
    settings: Mutex<Settings>,
    saves: AtomicUsize,
    /// What reading the settings fails with, in place of them.
    unreadable: Mutex<Option<LoadError>>,
    /// The columns' own orders as last written, starting from those it is made with, and what reading
    /// them fails with.
    sorts: Mutex<Sorts>,
    sort_saves: AtomicUsize,
    unreadable_sorts: Mutex<Option<LoadError>>,
    clipboard: Mutex<ClipboardFiles>,
    text: Mutex<String>,
    /// What each operation was asked, and the paths that refuse to go.
    operations: Mutex<Vec<Request>>,
    stuck: Mutex<Vec<String>>,
    /// The name the next rename gets in place of the one asked, as the shell may give.
    renamed_as: Mutex<Option<String>>,
    /// Paths the Recycle Bin cannot take, deleted outright.
    unbinnable: Mutex<Vec<String>>,
    /// The shell's question on deleting is turned down.
    declined: AtomicBool,
    opened: Mutex<Vec<String>>,
    /// Paths that Windows has no app to open with.
    unopenable: Mutex<Vec<String>>,
    /// What each Explorer window was asked to show selected.
    revealed: Mutex<Vec<Vec<String>>>,
    /// What each properties dialog was asked to show.
    properties: Mutex<Vec<Vec<String>>>,
    menus: Mutex<Vec<(Vec<String>, String)>>,
    /// What Send To offers, and each send: where to, what, and with Shift held or not.
    send_targets: Mutex<Vec<SendTarget>>,
    sent: Mutex<Vec<(String, Vec<String>, bool)>>,
    /// What QuickLook was told, in order; it is not running while `no_quick_look` is set.
    quick_looks: Mutex<Vec<(QuickLookMessage, String)>>,
    no_quick_look: AtomicBool,
    /// Told of every clipboard change, as the real watcher is.
    watcher: Mutex<Option<Box<dyn Fn() + Send>>>,
}

impl Fake {
    fn remove(&self, path: &Path) -> Option<Entry> {
        let parent = ancestors(path).into_iter().rev().nth(1)?;
        let mut folders = self.folders.lock().unwrap();
        let list = folders.get_mut(parent.to_str()?)?;
        let at = list.iter().position(|e| e.path == path)?;
        Some(list.remove(at))
    }

    fn add(&self, parent: &Path, entry: Entry) {
        self.folders.lock().unwrap().entry(text(parent)).or_default().push(entry);
    }
}

impl Backend for Fake {
    fn roots(&self) -> Vec<Entry> {
        self.roots.clone()
    }

    fn list(&self, path: &Path, sort: Order, _: &AtomicBool, _: colet_core::shell::Owner) -> Result<Vec<Entry>, String> {
        self.lists.lock().unwrap().push(text(path));
        self.orders.lock().unwrap().push(sort);
        self.listed.lock().unwrap().push((text(path), sort));
        // As the disk lists it: found whatever its case, each item under the folder as asked for.
        let folders = self.folders.lock().unwrap();
        let (folder, entries) = folders
            .get_key_value(&text(path))
            .or_else(|| folders.iter().find(|(p, _)| path_key(Path::new(p)) == path_key(path)))
            .ok_or_else(|| "フォルダーを開けません".to_string())?;
        let under = |e: &Entry| Entry {
            path: colet_core::paths::moved(&e.path, Path::new(folder), path).unwrap_or_else(|| e.path.clone()),
            ..e.clone()
        };
        Ok(entries.iter().map(under).collect())
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
        // Narrowed, as Everything is, to the names holding the text.
        let narrow = search.query.rsplit_once(" nopath:nowildcards:\"").map(|(_, text)| text.trim_end_matches('"').to_lowercase());
        let results: Vec<Entry> = self
            .results
            .lock()
            .unwrap()
            .iter()
            .filter(|e| narrow.as_ref().is_none_or(|text| e.name.to_lowercase().contains(text.as_str())))
            .cloned()
            .collect();
        let end = (search.offset + search.count).min(results.len());
        let entries = results[search.offset.min(end)..end].to_vec();
        Ok((Page { entries, total: results.len(), offset: search.offset }, "Everything 1.5".into()))
    }

    fn count(&self, _: &str) -> Result<usize, String> {
        Ok(self.results.lock().unwrap().len())
    }

    fn search_changed(&self) -> Result<bool, String> {
        Ok(self.changed.load(Ordering::Relaxed))
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

    fn load_sorts(&self) -> Result<Sorts, LoadError> {
        match self.unreadable_sorts.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => Ok(self.sorts.lock().unwrap().clone()),
        }
    }

    fn save_sorts(&self, sorts: &Sorts) -> Result<(), String> {
        self.sort_saves.fetch_add(1, Ordering::Relaxed);
        *self.sorts.lock().unwrap() = sorts.clone();
        Ok(())
    }

    /// Acts on the listings as the shell would on the disk.
    fn operate(
        &self,
        request: Request,
        read: Option<ClipboardFiles>,
        _: Owner,
        mut report: Box<dyn FnMut(Event) + Send>,
    ) -> Result<(), String> {
        self.operations.lock().unwrap().push(request.clone());
        let stuck = self.stuck.lock().unwrap().clone();
        let mut failed = false;
        match request.kind {
            Operation::Recycle | Operation::Delete if self.declined.load(Ordering::Relaxed) => return Err(CANCELLED.into()),
            Operation::Recycle | Operation::Delete => {
                let unbinnable = self.unbinnable.lock().unwrap().clone();
                for path in &request.paths {
                    if stuck.contains(&text(path)) {
                        failed = true;
                        report(Event::Failed { path: path.clone(), error: "アクセスが拒否されました。".into() });
                    } else {
                        self.remove(path);
                        report(Event::Done { path: path.clone() });
                        if request.kind == Operation::Delete || unbinnable.contains(&text(path)) {
                            report(Event::Destroyed { path: path.clone() });
                        }
                        report(Event::Left { path: path.clone() });
                    }
                }
            }
            Operation::Rename => {
                let from = &request.paths[0];
                let mut entry = self.remove(from).unwrap();
                // As `remove` finds it: a drive's root keeps its backslash.
                let parent = ancestors(from).into_iter().rev().nth(1).unwrap();
                let name = self.renamed_as.lock().unwrap().take().unwrap_or_else(|| request.name.clone());
                entry.path = child(&parent, &name);
                entry.name = name;
                let to = entry.path.clone();
                self.add(&parent, entry);
                // A folder's listing goes with it.
                let mut folders = self.folders.lock().unwrap();
                if let Some(list) = folders.remove(&text(from)) {
                    folders.insert(text(&to), list);
                }
                drop(folders);
                report(Event::Done { path: from.clone() });
                report(Event::Left { path: from.clone() });
                report(Event::Placed { path: to });
            }
            Operation::NewFolder => {
                let path = child(&request.destination, &request.name);
                self.add(&request.destination, Entry { parent: colet_core::paths::shown(&request.destination), ..folder(&text(&path)) });
                self.folders.lock().unwrap().insert(text(&path), vec![]);
                report(Event::Done { path });
            }
            Operation::Paste => {
                let files = read.unwrap_or_else(|| self.clipboard.lock().unwrap().clone());
                for path in files.paths {
                    let name = colet_core::paths::shown(folder_name(&path));
                    let parent = ancestors(&path).into_iter().rev().nth(1).unwrap();
                    let name = if parent == request.destination { format!("{name} - コピー") } else { name };
                    let made = child(&request.destination, &name);
                    if files.cut {
                        self.remove(&path);
                    }
                    self.add(&request.destination, file(&text(&made)));
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

    fn copy_files(&self, paths: &[PathBuf], cut: bool, _: Owner) -> Result<(), String> {
        *self.clipboard.lock().unwrap() = ClipboardFiles { paths: paths.to_vec(), cut, sequence: 1, ..Default::default() };
        if let Some(changed) = &*self.watcher.lock().unwrap() {
            changed();
        }
        Ok(())
    }

    fn copy_text(&self, copied: &OsStr, _: Owner) -> Result<(), String> {
        *self.text.lock().unwrap() = copied.to_str().unwrap().into();
        Ok(())
    }

    fn watch_clipboard(&self, changed: Box<dyn Fn() + Send>) {
        *self.watcher.lock().unwrap() = Some(changed);
    }

    fn open(&self, path: &Path, _: Owner) -> Result<(), String> {
        if self.unopenable.lock().unwrap().iter().any(|p| Path::new(p) == path) {
            return Err("関連付けられたアプリがありません".into());
        }
        self.opened.lock().unwrap().push(text(path));
        Ok(())
    }

    fn reveal(&self, paths: &[PathBuf]) -> Result<(), String> {
        self.revealed.lock().unwrap().push(paths.iter().map(|p| text(p)).collect());
        Ok(())
    }

    fn properties(&self, paths: &[PathBuf], _: Owner) -> Result<(), String> {
        self.properties.lock().unwrap().push(paths.iter().map(|p| text(p)).collect());
        Ok(())
    }

    fn path_kind(&self, path: &Path) -> PathKind {
        let folders = self.folders.lock().unwrap();
        match folders.values().flatten().find(|e| e.path == path) {
            Some(e) if e.directory => PathKind::Folder,
            Some(_) => PathKind::File,
            None => PathKind::Gone,
        }
    }

    /// Without case, as the disk finds it: a drive or a folder listed, or an item in one.
    fn locate(&self, path: &Path) -> Option<(PathBuf, bool)> {
        let key = path_key(path);
        let folders = self.folders.lock().unwrap();
        if let Some(folder) = folders.keys().find(|p| path_key(Path::new(p)) == key) {
            return Some((PathBuf::from(folder), true));
        }
        folders.values().flatten().find(|e| path_key(&e.path) == key).map(|e| (e.path.clone(), e.directory))
    }

    fn shell_menu(&self, paths: Vec<PathBuf>, background: PathBuf, _: (i32, i32), _: Owner) -> Result<(), String> {
        self.menus.lock().unwrap().push((paths.iter().map(|p| text(p)).collect(), text(&background)));
        Ok(())
    }

    fn send_targets(&self) -> Vec<SendTarget> {
        self.send_targets.lock().unwrap().clone()
    }

    fn send_to(&self, target: &Path, paths: &[PathBuf], shift: bool, _: Owner) -> Result<Dropped, String> {
        self.sent.lock().unwrap().push((text(target), paths.iter().map(|p| text(p)).collect(), shift));
        Ok(if shift { Dropped::Moved } else { Dropped::Copied })
    }

    fn quick_look(&self, message: QuickLookMessage, path: PathBuf) -> mpsc::Receiver<Result<(), QuickLookError>> {
        let (done, result) = mpsc::channel();
        if self.no_quick_look.load(Ordering::Relaxed) {
            done.send(Err(QuickLookError::NotRunning)).unwrap();
        } else {
            self.quick_looks.lock().unwrap().push((message, text(&path)));
            done.send(Ok(())).unwrap();
        }
        result
    }
}

/// The favorites' folders, as the fake keeps them.
fn favorite_texts(favorites: &[crate::settings::Favorite]) -> Vec<String> {
    favorites.iter().map(|f| text(&f.path)).collect()
}

/// Paths the tests made, as the fake keeps them.
fn texts(paths: &[PathBuf]) -> Vec<String> {
    paths.iter().map(|path| text(path)).collect()
}

/// A path as the fake keeps it: what is shown of it, the paths the tests make being text.
fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn entry(path: &str, directory: bool) -> Entry {
    let chain = ancestors(Path::new(path));
    Entry {
        path: path.into(),
        name: colet_core::paths::shown(folder_name(Path::new(path))),
        parent: chain.len().checked_sub(2).map(|i| colet_core::paths::shown(&chain[i])).unwrap_or_default(),
        directory,
        size: (!directory).then_some(1),
        modified: None,
        hidden: Hiding::Shown,
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
        crate::theme::init(cx);
        // What opens shows at once, where a press finds it, as with Windows' animations off.
        cx.set_reduce_motion(true);
        crate::app::bind_keys(cx);
    });
    let (window, view) = cx
        .update(|cx| {
            gpui_kit::open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(Default::default(), size(px(1200.), px(800.))))),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let (settings, sorts) = (fake.load_settings(), fake.load_sorts());
                    cx.new(|cx| Colet::new(fake, settings, sorts, window, cx))
                },
            )
        })
        .expect("open the window");
    // In front, as a window just opened is: the focus listeners only hear of the keys in an active window.
    cx.update_window(window, |_, window, _| window.activate_window()).unwrap();
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

    /// Clicks `id` as a hand does: the press and the release each an event of its own, with what the
    /// press set off run before the release comes.
    fn press_and_release(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) {
        use gpui_kit::{MouseButton, test::TestEventExt};
        let id = id.into();
        let at = self.act(cx, |window, cx| {
            let at = window.find(id).bounds().center();
            window.pointer_move(at, None, cx);
            window.pointer_down(at, MouseButton::Left, cx);
            at
        });
        cx.run_until_parked();
        self.act(cx, |window, cx| window.pointer_up(at, MouseButton::Left, cx));
        settle(cx);
    }

    fn click_with(&self, cx: &mut TestAppContext, id: impl Into<ElementId>, modifiers: Modifiers) {
        let id = id.into();
        self.act(cx, |window, cx| window.click_with_modifiers(id, modifiers, cx));
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
        self.read(cx, |v| texts(&v.state.columns[index].selected))
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

    /// What the notices up in the window say, oldest first.
    fn toasts(&self, cx: &mut TestAppContext) -> Vec<String> {
        let last = self.read(cx, |v| v.next_toast);
        self.act(cx, |window, _| {
            (1..=last).filter_map(|id| window.try_find(("toast", id as usize))).filter_map(|t| t.label().map(str::to_owned)).collect()
        })
    }

    /// The search field has the keys, or its menu has them for it.
    fn search_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).search_has_keys(window, cx))
    }

    /// Whether the column worked in is marked, and whether its focused row is ringed.
    fn column_marks(&self, cx: &mut TestAppContext) -> (bool, bool) {
        let view = self.view.clone();
        self.act(cx, |window, cx| {
            let view = view.read(cx);
            (view.columns_marked(window), view.keys_in_column(window))
        })
    }

    fn columns_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).columns_focus.is_focused(window))
    }

    /// Whether `id`, a button that keeps its own focus, has the keys.
    fn has_keys(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
        let id = id.into();
        self.act(cx, |window, _| window.try_find(id).and_then(|e| e.focused()) == Some(true))
    }

    /// Presses Tab until `id` has the keys, as a user would reach it.
    fn tab_to(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) {
        let id = id.into();
        for _ in 0..60 {
            if self.has_keys(cx, id.clone()) {
                return;
            }
            self.press(cx, "tab");
        }
        panic!("Tab never reached {id:?}");
    }
}

fn row(path: impl AsRef<Path>) -> ElementId {
    ElementId::Name(id_text(path.as_ref()).into())
}

fn drive_d() -> Fake {
    Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![])])
    }
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
        if ui.read(cx, |v| v.scrolls.sideways.slide.is_none()) {
            break;
        }
    }
}

/// How far the columns' view reaches past the last column, as the frame drawn now shows it: none when
/// it is flush with the right edge.
fn room_past_last(ui: &Ui, cx: &mut TestAppContext) -> f32 {
    let last = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    ui.act(cx, |window, _| f32::from(window.find("columns-scroll").bounds().right() - window.find(("column", last)).bounds().right()))
}

/// A file four folders down, found and picked: its folder opens with those above it folded away.
fn folded_deep(cx: &mut TestAppContext) -> Ui {
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
    ui
}

/// Where column `index` starts and ends on screen.
fn column_edges(ui: &Ui, cx: &mut TestAppContext, index: usize) -> (f32, f32) {
    let id = ui.read(cx, |v| v.state.columns[index].id);
    ui.act(cx, |window, _| {
        let bounds = window.find(("column", id)).bounds();
        (f32::from(bounds.left()), f32::from(bounds.right()))
    })
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
    assert_eq!(ui.read(cx, |v| v.state.columns[2].path.to_str().unwrap().to_owned()), r"D:\work");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    (fake, ui)
}

/// The order the folder `path` was last listed in.
fn listed_in(fake: &Fake, path: &str) -> Option<Order> {
    fake.listed.lock().unwrap().iter().rev().find(|(p, _)| p == path).map(|(_, order)| *order)
}

fn listings_of(fake: &Fake, path: &str) -> usize {
    fake.listed.lock().unwrap().iter().filter(|(p, _)| p == path).count()
}

/// Home with three favorites and two saved searches.
fn ordered_home(cx: &mut TestAppContext) -> (Arc<Fake>, Ui) {
    let fake = Arc::new(Fake {
        roots: vec![folder(r"D:\")],
        ..Fake::default().with_folders(&[(r"D:\", vec![]), (r"D:\a", vec![]), (r"D:\b", vec![]), (r"D:\c", vec![])])
    });
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.favorites = vec![r"D:\a".into(), r"D:\b".into(), r"D:\c".into()];
        settings.saved_searches =
            ["one", "two"].map(|q| SavedSearch { name: String::new(), query: q.into(), filter: None, narrow: None }).to_vec();
    }
    let ui = open(cx, fake.clone());
    (fake, ui)
}

impl Ui {
    fn home_row(&self, cx: &mut TestAppContext, path: &str) -> Bounds<gpui_kit::Pixels> {
        self.act(cx, |window, _| window.within("home-rows").find(row(path)).bounds())
    }

    /// Picks up what is at `from` and holds it at `to`, without letting go.
    fn hold_drag(&self, cx: &mut TestAppContext, from: gpui_kit::Point<gpui_kit::Pixels>, to: gpui_kit::Point<gpui_kit::Pixels>) {
        use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent};
        self.act(cx, |window, cx| {
            let modifiers = window.modifiers();
            window.dispatch_event(MouseMoveEvent { position: from, pressed_button: None, modifiers }.to_platform_input(), cx);
            window.render_frame(cx);
            let down = MouseDownEvent { button: MouseButton::Left, position: from, modifiers, click_count: 1, first_mouse: false };
            window.dispatch_event(down.to_platform_input(), cx);
            window.render_frame(cx);
            for step in 1..=4 {
                let position = point(from.x + (to.x - from.x) * (step as f32 / 4.), from.y + (to.y - from.y) * (step as f32 / 4.));
                window.dispatch_event(
                    MouseMoveEvent { position, pressed_button: Some(MouseButton::Left), modifiers }.to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
            }
        });
    }

    fn let_go(&self, cx: &mut TestAppContext, at: gpui_kit::Point<gpui_kit::Pixels>) {
        use gpui_kit::{InputEvent as _, MouseButton, MouseUpEvent};
        self.act(cx, |window, cx| {
            let modifiers = window.modifiers();
            window.dispatch_event(
                MouseUpEvent { button: MouseButton::Left, position: at, modifiers, click_count: 1 }.to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        });
        settle(cx);
    }
}

/// A row's upper or lower half.
fn upper(b: Bounds<gpui_kit::Pixels>) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(b.center().x, b.top() + b.size.height / 4.)
}

fn lower(b: Bounds<gpui_kit::Pixels>) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(b.center().x, b.bottom() - b.size.height / 4.)
}

fn with_filters(fake: Fake) -> Fake {
    fake.settings.lock().unwrap().filters = vec![
        SearchFilter { name: "画像".into(), query: "ext:jpg;png".into() },
        SearchFilter { name: "動画".into(), query: "ext:mp4".into() },
    ];
    fake
}

impl Ui {
    fn search_field_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).search.read(cx).focus_handle(cx).is_focused(window))
    }

    /// Types into the fields of the filter being added or edited.
    fn type_filter(&self, cx: &mut TestAppContext, name: &str, query: &str) {
        let (name_field, query_field) = self.read(cx, |v| match v.overlays.dialog() {
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

impl Ui {
    /// Right-clicks `input` near its right end, past its text and so past what is picked in it.
    fn right_click_field(&self, cx: &mut TestAppContext, input: &Entity<gpui_kit::base::input::InputState>) {
        use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseUpEvent};
        let input = input.clone();
        self.act(cx, |window, cx| {
            let bounds = input.read(cx).input_bounds();
            let position = point(bounds.right() - px(4.), bounds.center().y);
            let modifiers = window.modifiers();
            let down = MouseDownEvent { button: MouseButton::Right, position, modifiers, click_count: 1, first_mouse: false };
            window.dispatch_event(down.to_platform_input(), cx);
            window.dispatch_event(MouseUpEvent { button: MouseButton::Right, position, modifiers, click_count: 1 }.to_platform_input(), cx);
            window.render_frame(cx);
        });
        settle(cx);
    }

    /// The lines of the open menu that are on.
    fn lines_on(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |v| v.menu_lines().into_iter().filter(|l| l.off.is_none()).map(|l| l.text.to_string()).collect())
    }

    fn has_focus(&self, cx: &mut TestAppContext, input: &Entity<gpui_kit::base::input::InputState>) -> bool {
        let input = input.clone();
        self.act(cx, |window, cx| input.read(cx).focus_handle(cx).is_focused(window))
    }
}

fn clipboard_text(cx: &mut TestAppContext) -> Option<String> {
    cx.read_from_clipboard().and_then(|item| item.text())
}

impl Fake {
    /// What QuickLook was told since it was last asked, each with its path.
    fn quick_looked(&self) -> Vec<(QuickLookMessage, String)> {
        std::mem::take(&mut *self.quick_looks.lock().unwrap())
    }
}
