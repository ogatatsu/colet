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
    paths::ancestors,
};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AnyWindowHandle, App, AppContext as _, Bounds, ElementId, Entity, ScrollStrategy, TestAppContext, Window,
    WindowBounds, WindowOptions, point, px, size,
};

use crate::{
    app::{Colet, Engine},
    backend::{Backend, Search},
    settings::{SavedSearch, Settings},
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
    reconnects: AtomicUsize,
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

    fn load_settings(&self) -> Result<Settings, String> {
        Ok(self.settings.lock().unwrap().clone())
    }

    fn save_settings(&self, settings: &Settings) -> Result<(), String> {
        self.saves.fetch_add(1, Ordering::Relaxed);
        *self.settings.lock().unwrap() = settings.clone();
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
    // The strip beside the results brings them out for good, and goes.
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
    ui.click(cx, "unfavorite");
    assert!(fake.settings.lock().unwrap().favorites.is_empty());
    assert!(ui.read(cx, |v| v.home_menu.is_none()));
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
