//! The window's one view: it keeps the navigation state, fetches what the columns list, watches the open
//! folders, and takes the keys. `view.rs` draws it.
use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use colet_core::{
    entry::{Entry, Sort},
    everything::PAGE,
    paths::path_key,
    shell,
    watch::FolderWatcher,
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::{
    backend::{self, Search},
    icons::Icons,
    places::{Dialog, HomeMenu, home_entries},
    settings::Settings,
    state::{ColumnId, Kind, State},
    theme::Palette,
};

actions!(
    colet,
    [
        SelectPrevious,
        SelectNext,
        SelectPageUp,
        SelectPageDown,
        SelectFirst,
        SelectLast,
        ColumnLeft,
        ColumnRight,
        OpenItem,
        GoBack,
        GoForward,
        FocusSearch,
        RefreshAll,
        SaveCurrent,
        EditItem,
        CloseDialog,
    ]
);

/// Where the dialogs take the keys.
pub const DIALOG: &str = "Dialog";

/// Where the column keys apply: home and the columns beside it.
pub const COLUMNS: &str = "Columns";

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, Some(COLUMNS)),
        KeyBinding::new("down", SelectNext, Some(COLUMNS)),
        KeyBinding::new("pageup", SelectPageUp, Some(COLUMNS)),
        KeyBinding::new("pagedown", SelectPageDown, Some(COLUMNS)),
        KeyBinding::new("home", SelectFirst, Some(COLUMNS)),
        KeyBinding::new("end", SelectLast, Some(COLUMNS)),
        KeyBinding::new("left", ColumnLeft, Some(COLUMNS)),
        KeyBinding::new("right", ColumnRight, Some(COLUMNS)),
        KeyBinding::new("enter", OpenItem, Some(COLUMNS)),
        KeyBinding::new("alt-left", GoBack, Some(COLUMNS)),
        KeyBinding::new("alt-right", GoForward, Some(COLUMNS)),
        KeyBinding::new("ctrl-f", FocusSearch, None),
        KeyBinding::new("ctrl-l", FocusSearch, None),
        KeyBinding::new("f2", EditItem, Some(COLUMNS)),
        KeyBinding::new("f5", RefreshAll, None),
        KeyBinding::new("ctrl-d", SaveCurrent, None),
        KeyBinding::new("escape", CloseDialog, Some(DIALOG)),
    ]);
}

/// The widths the user drags: home's pane, the pinned search results and the folders, which share one.
pub struct Widths {
    pub sidebar: f32,
    pub search: f32,
    pub column: f32,
}

pub const COLUMN_WIDTHS: (f32, f32) = (192., 640.);
pub const SIDEBAR_WIDTHS: (f32, f32) = (160., 480.);
/// The strip that brings out the folders folded away above a search result.
pub const FOLD_WIDTH: f32 = 20.;

/// Which width a drag on a column's edge changes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WidthOf {
    Sidebar,
    Search,
    Column,
}

pub struct Resize {
    pub of: WidthOf,
    pub start_x: Pixels,
    pub start_width: f32,
}

/// Whether a search has reached Everything; unknown until one has, since home needs none.
#[derive(Clone)]
pub enum Engine {
    Unknown,
    Connected(String),
    Failed(String),
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Error,
}

pub struct Toast {
    pub id: u64,
    pub text: SharedString,
    pub tone: Tone,
}

/// The columns sliding to where they should rest; see [`Colet::reveal`].
pub struct Slide {
    pub position: f32,
    pub target: f32,
    pub velocity: f32,
    pub time: Option<Instant>,
}

/// What a listing came back with: the entries, how many in all, and for a search what answered it.
type Listing = Result<(Vec<Entry>, usize, Option<String>), String>;

pub struct Colet {
    pub state: State,
    pub sort: Sort,
    pub widths: Widths,
    pub palette: Palette,
    pub search: Entity<InputState>,
    /// The search field has the keys: no column is marked as worked in.
    pub search_focused: bool,
    pub home_focus: FocusHandle,
    pub columns_focus: FocusHandle,
    pub icons: Icons,
    pub engine: Engine,
    pub toasts: Vec<Toast>,
    next_toast: u64,
    pub sort_open: bool,
    pub crumb_menu_open: bool,
    pub resizing: Option<Resize>,
    pub lists: HashMap<ColumnId, UniformListScrollHandle>,
    scrolled_to: HashMap<ColumnId, String>,
    pub columns_scroll: ScrollHandle,
    pub slide: Option<Slide>,
    /// Asks the next frame to bring the active column on screen, sliding there when true.
    pub reveal_pending: Option<bool>,
    /// The scroll offset the columns were last brought to, which holds their scroll width while columns
    /// beside them close.
    pub hold: f32,
    owner: shell::Owner,
    pub(crate) backend: backend::Shared,
    request: u64,
    requested: HashSet<ColumnId>,
    pending: HashMap<ColumnId, u64>,
    cancels: HashMap<ColumnId, Arc<AtomicBool>>,
    watcher: Option<FolderWatcher>,
    watched: Vec<String>,
    /// Folders already said to go unwatched, each said once.
    unwatched: HashSet<String>,
    /// What the watcher saw change, collected until the UI takes it: `None` inside for any change.
    pub(crate) changed: Arc<Mutex<Option<Option<HashSet<String>>>>>,
    searched_at: Option<Instant>,
    search_refresh: bool,
    /// What type-ahead has typed so far, when, and in which column.
    typed: (String, Instant, ColumnId),
    pub settings: Settings,
    pub(crate) save_generation: u64,
    pub dialog: Option<Dialog>,
    pub(crate) dialog_subscriptions: Vec<Subscription>,
    pub home_menu: Option<HomeMenu>,
    _subscriptions: Vec<Subscription>,
}

impl Colet {
    /// `settings` as read at start; an error is said once the window is up, and the defaults used.
    pub fn new(
        backend: backend::Shared,
        settings: Result<Settings, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Everythingで検索（Ctrl+F）"));
        let subscriptions = vec![
            cx.subscribe_in(&search, window, |this: &mut Self, input, event, window, cx| match event {
                InputEvent::Change => {
                    let query = input.read(cx).value().to_string();
                    this.state.set_query(&query);
                    this.changed(cx);
                }
                InputEvent::PressEnter { .. } => this.leave_search(window, cx),
                InputEvent::Focus => {
                    this.search_focused = true;
                    cx.notify();
                }
                InputEvent::Blur => {
                    this.search_focused = false;
                    cx.notify();
                }
            }),
            cx.observe_window_appearance(window, |this, window, cx| {
                this.palette = Palette::for_window(window);
                cx.notify();
            }),
            cx.observe_window_bounds(window, |this, window, cx| this.remember_window(window, cx)),
        ];
        // Written at once as the window closes, rather than a moment after the last change.
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            if let Some(view) = view.upgrade() {
                let _ = view.update(cx, |this, _| this.flush_settings());
            }
            true
        });
        let (settings, problem) = match settings {
            Ok(settings) => (settings, None),
            Err(error) => (Settings::default(), Some(error)),
        };
        let owner = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        };
        let mut this = Self {
            state: State::new(),
            sort: Sort::Name,
            widths: Widths {
                sidebar: settings.sidebar_width,
                search: settings.search_width,
                column: settings.column_width,
            },
            palette: Palette::for_window(window),
            search,
            search_focused: false,
            home_focus: cx.focus_handle(),
            columns_focus: cx.focus_handle(),
            icons: Icons::default(),
            engine: Engine::Unknown,
            toasts: Vec::new(),
            next_toast: 0,
            sort_open: false,
            crumb_menu_open: false,
            resizing: None,
            lists: HashMap::new(),
            scrolled_to: HashMap::new(),
            columns_scroll: ScrollHandle::new(),
            slide: None,
            reveal_pending: None,
            hold: 0.,
            owner,
            backend,
            request: 0,
            requested: HashSet::new(),
            pending: HashMap::new(),
            cancels: HashMap::new(),
            watcher: None,
            watched: Vec::new(),
            unwatched: HashSet::new(),
            changed: Arc::default(),
            searched_at: None,
            search_refresh: false,
            typed: (String::new(), Instant::now(), 0),
            settings,
            save_generation: 0,
            dialog: None,
            dialog_subscriptions: Vec::new(),
            home_menu: None,
            _subscriptions: subscriptions,
        };
        if let Some(problem) = problem {
            this.notify(problem, Tone::Error, cx);
        }
        this.search.update(cx, |input, cx| input.focus(window, cx));
        this.sync(cx);
        this.poll(cx);
        this
    }

    // ---- What happens after the state changes ----

    /// Starts what the columns now on screen need, drops what the ones gone needed, and redraws.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        self.sync(cx);
        if self.reveal_pending.is_none() {
            self.reveal_pending = Some(true);
        }
        cx.notify();
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let live: HashSet<ColumnId> = self.state.columns.iter().map(|c| c.id).collect();
        // A column gone stops its listing; a late answer finds no column and is dropped.
        self.cancels.retain(|id, cancelled| {
            let keep = live.contains(id);
            if !keep {
                cancelled.store(true, Ordering::Relaxed);
            }
            keep
        });
        self.requested.retain(|id| live.contains(id));
        self.pending.retain(|id, _| live.contains(id));
        self.lists.retain(|id, _| live.contains(id));
        self.scrolled_to.retain(|id, _| live.contains(id));
        let fresh: Vec<(ColumnId, Kind)> =
            self.state.columns.iter().filter(|c| !self.requested.contains(&c.id)).map(|c| (c.id, c.kind)).collect();
        for (id, kind) in fresh {
            self.requested.insert(id);
            if kind == Kind::Search {
                // Typing makes a new search per key; only the one still there after a pause runs.
                cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(Duration::from_millis(150)).await;
                    this.update(cx, |this, cx| this.load(id, 0, false, cx)).ok();
                })
                .detach();
            } else {
                self.load(id, 0, false, cx);
            }
        }
        self.watch(cx);
        if let Some(notice) = self.state.notice.take() {
            self.notify(notice, Tone::Info, cx);
        }
    }

    pub fn notify(&mut self, text: impl Into<SharedString>, tone: Tone, cx: &mut Context<Self>) {
        self.next_toast += 1;
        let id = self.next_toast;
        self.toasts.push(Toast { id, text: text.into(), tone });
        if self.toasts.len() > 3 {
            self.toasts.remove(0);
        }
        // Errors stay until dismissed; the rest fade on their own.
        if tone == Tone::Info {
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(7)).await;
                this.update(cx, |this, cx| this.dismiss(id, cx)).ok();
            })
            .detach();
        }
        cx.notify();
    }

    pub fn dismiss(&mut self, id: u64, cx: &mut Context<Self>) {
        self.toasts.retain(|t| t.id != id);
        cx.notify();
    }

    // ---- Listings ----

    /// Lists the column `id` from `offset` on; `quiet` keeps what it shows meanwhile.
    pub fn load(&mut self, id: ColumnId, offset: usize, quiet: bool, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        if offset > 0 && self.pending.contains_key(&id) {
            return;
        }
        self.request += 1;
        let request = self.request;
        self.pending.insert(id, request);
        self.state.load(id, request, quiet);
        let c = &self.state.columns[index];
        let sort = self.sort;
        let backend = self.backend.clone();
        let task: Task<Listing> = match c.kind {
            Kind::Home => {
                let settings = self.settings.clone();
                cx.background_executor().spawn(async move {
                    let entries = home_entries(&settings, backend.roots());
                    let total = entries.len();
                    Ok((entries, total, None))
                })
            }
            Kind::Search => {
                let query = self.state.query.clone();
                // A refresh reloads all it has loaded, so later selections stay.
                let count = if offset > 0 { PAGE } else { PAGE.max(c.entries.len()) };
                let instance = self.settings.instance.clone();
                let search = Search { query, sort, offset, count, refresh: offset == 0, instance, request };
                cx.background_executor().spawn(async move {
                    let (page, connection) = backend.search(search)?;
                    Ok((page.entries, page.total, Some(connection)))
                })
            }
            Kind::Folder => {
                let path = c.path.clone();
                let cancelled = Arc::new(AtomicBool::new(false));
                if let Some(previous) = self.cancels.insert(id, cancelled.clone()) {
                    previous.store(true, Ordering::Relaxed);
                }
                cx.background_executor().spawn(async move {
                    let entries = backend.list(&path, sort, &cancelled)?;
                    let total = entries.len();
                    Ok((entries, total, None))
                })
            }
        };
        let kind = c.kind;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| this.listed(id, kind, request, offset, result, cx)).ok();
        })
        .detach();
    }

    fn listed(&mut self, id: ColumnId, kind: Kind, request: u64, offset: usize, result: Listing, cx: &mut Context<Self>) {
        let current = self.pending.get(&id) == Some(&request);
        if current {
            self.pending.remove(&id);
        }
        match result {
            Ok((entries, total, connection)) => {
                self.state.page(id, request, entries, total, offset);
                if let Some(name) = connection.filter(|_| current) {
                    self.engine = Engine::Connected(name);
                }
            }
            Err(error) => {
                if kind == Kind::Search && current {
                    self.engine = Engine::Failed(error.clone());
                }
                self.state.error(id, request, error);
            }
        }
        self.changed(cx);
    }

    /// Reloads every column, keeping what each shows meanwhile.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let ids: Vec<ColumnId> = self.state.columns.iter().map(|c| c.id).collect();
        for id in ids {
            self.load(id, 0, true, cx);
        }
        cx.notify();
    }

    /// The search results load their next page once the list nears its end.
    pub fn more(&mut self, id: ColumnId, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        let c = &self.state.columns[index];
        if c.kind == Kind::Search && c.error.is_none() && !c.loading && c.entries.len() < c.total {
            let offset = c.entries.len();
            self.load(id, offset, true, cx);
        }
    }

    pub fn set_sort(&mut self, sort: Sort, cx: &mut Context<Self>) {
        self.sort_open = false;
        if sort != self.sort {
            self.sort = sort;
            self.refresh(cx);
        }
        cx.notify();
    }

    // ---- Watching ----

    /// Watches the folders the columns could read, one level deep.
    fn watch(&mut self, cx: &mut Context<Self>) {
        let paths: Vec<String> = self
            .state
            .columns
            .iter()
            .filter(|c| c.kind == Kind::Folder && c.error.is_none())
            .map(|c| c.path.clone())
            .collect();
        if paths == self.watched {
            return;
        }
        self.watched = paths.clone();
        let changed = self.changed.clone();
        match FolderWatcher::new(&paths, move |paths| {
            let mut queue = changed.lock().unwrap();
            let batch = queue.get_or_insert_with(|| Some(HashSet::new()));
            match (paths.is_empty(), batch.as_mut()) {
                (true, _) => *batch = None,
                (false, Some(batch)) => batch.extend(paths),
                (false, None) => {}
            }
        }) {
            Ok((watcher, unwatched)) => {
                self.watcher = Some(watcher);
                let fresh: Vec<String> =
                    unwatched.into_iter().filter(|p| self.unwatched.insert(path_key(p))).collect();
                if !fresh.is_empty() {
                    let names: String = fresh
                        .iter()
                        .map(|p| format!("「{}」", colet_core::paths::folder_name(p)))
                        .collect();
                    self.notify(format!("{names}の変更を監視できません。F5で更新できます。"), Tone::Error, cx);
                }
            }
            Err(error) => self.notify(error, Tone::Error, cx),
        }
    }

    /// Takes in what changed, every 200ms: folder changes reload the columns showing them, and every
    /// second Everything is asked whether the results changed.
    fn poll(&mut self, cx: &mut Context<Self>) {
        let changed = self.changed.clone();
        let backend = self.backend.clone();
        cx.spawn(async move |this, cx| {
            let mut tick = 0u32;
            loop {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                tick += 1;
                let batch = changed.lock().unwrap().take();
                if let Some(batch) = batch
                    && this.update(cx, |this, cx| this.files_changed(batch, cx)).is_err()
                {
                    return;
                }
                if !tick.is_multiple_of(5) {
                    continue;
                }
                let backend = backend.clone();
                let result = cx.background_executor().spawn(async move { backend.search_changed() }).await;
                let alive = this
                    .update(cx, |this, cx| match result {
                        Ok(true) => this.search_changed(cx),
                        Ok(false) => {}
                        Err(error) => {
                            this.engine = Engine::Failed(error.clone());
                            this.notify(error, Tone::Error, cx);
                        }
                    })
                    .is_ok();
                if !alive {
                    return;
                }
            }
        })
        .detach();
    }

    /// The folder columns a changed item shows in: the folder holding it, and its own when it is one.
    fn files_changed(&mut self, paths: Option<HashSet<String>>, cx: &mut Context<Self>) {
        let Some(paths) = paths else {
            self.refresh(cx);
            return;
        };
        let folders: HashSet<String> = paths
            .iter()
            .flat_map(|path| {
                let key = path_key(path);
                let parent = key.rfind('\\').map(|i| key[..i].to_owned()).unwrap_or_default();
                [key, parent]
            })
            .collect();
        let ids: Vec<ColumnId> = self
            .state
            .columns
            .iter()
            .filter(|c| c.kind == Kind::Folder && folders.contains(&path_key(&c.path)))
            .map(|c| c.id)
            .collect();
        for id in ids {
            self.load(id, 0, true, cx);
        }
    }

    /// A busy disk reports changes each second: the first reloads at once, then at most one every couple
    /// of seconds. One met by a reload in flight waits for it, as it may have read the results before.
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        if self.search_refresh {
            return;
        }
        self.search_refresh = true;
        let wait = self.searched_at.map_or(Duration::ZERO, |at| Duration::from_secs(2).saturating_sub(at.elapsed()));
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            loop {
                let busy = this
                    .update(cx, |this, cx| {
                        let Some(search) = this.state.columns.iter().find(|c| c.kind == Kind::Search).map(|c| c.id) else {
                            this.search_refresh = false;
                            return false;
                        };
                        if this.pending.contains_key(&search) {
                            return true;
                        }
                        this.search_refresh = false;
                        this.searched_at = Some(Instant::now());
                        this.load(search, 0, true, cx);
                        false
                    })
                    .unwrap_or(false);
                if !busy {
                    return;
                }
                cx.background_executor().timer(Duration::from_millis(200)).await;
            }
        })
        .detach();
    }

    // ---- Keys and picks ----

    /// The keys go to the active column: home's pane or the columns beside it.
    pub fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let handle = if self.state.active == 0 { &self.home_focus } else { &self.columns_focus };
        handle.focus(window, cx);
        self.state.enter();
        self.changed(cx);
    }

    /// Into the results, as from a search field in Explorer, or home with nothing searched.
    pub fn leave_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_active(if self.state.query.is_empty() { 0 } else { 1 });
        self.focus_active(window, cx);
    }

    pub fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| input.set_value("", window, cx));
        self.state.set_query("");
        self.changed(cx);
    }

    pub fn search_for(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| {
            input.set_value(query.to_owned(), window, cx);
            input.focus(window, cx);
        });
        self.state.set_query(query);
        self.changed(cx);
    }

    pub fn pick(&mut self, index: usize, path: String, window: &mut Window, cx: &mut Context<Self>) {
        self.state.select(index, vec![path]);
        self.state.set_active(index);
        let handle = if index == 0 { &self.home_focus } else { &self.columns_focus };
        handle.focus(window, cx);
        // A place picked in home leaves the search.
        self.sync_search_field(window, cx);
    }

    /// A folder opens beside it, as picking does; a file opens in its app.
    pub fn open(&mut self, index: usize, path: &str, cx: &mut Context<Self>) {
        let Some(entry) = self.state.columns.get(index).and_then(|c| c.entry(path)).cloned() else { return };
        if entry.directory {
            self.state.select(index, vec![entry.path]);
            self.changed(cx);
            return;
        }
        let owner = self.owner;
        let task = cx.background_executor().spawn(async move { shell::open(&entry.path, owner) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                this.update(cx, |this, cx| {
                    this.notify(format!("開けません: {}", error.message()), Tone::Error, cx)
                })
                .ok();
            }
        })
        .detach();
    }

    /// A crumb takes the keys to its column, bringing it on screen even when it is already active.
    pub fn go_to(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_active(index);
        self.focus_active(window, cx);
    }

    pub fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.step(by);
        // Moving in home can run a saved search, which the field shows.
        self.sync_search_field(window, cx);
    }

    pub fn move_column(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.state.neighbour(self.state.active, right) {
            self.state.set_active(next);
            self.focus_active(window, cx);
        }
    }

    /// Typing a name's start picks the first row from the focused one on that starts with it; the typed
    /// text runs on while keys come within a second of each other in one column.
    pub fn type_ahead(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.state.active;
        let column = self.state.columns[index].id;
        if self.typed.1.elapsed() > Duration::from_secs(1) || self.typed.2 != column {
            self.typed.0.clear();
        }
        self.typed.0.push_str(&text.to_lowercase());
        self.typed.1 = Instant::now();
        self.typed.2 = column;
        let c = &self.state.columns[index];
        let start = c.focused.as_ref().and_then(|f| c.entries.iter().position(|e| &e.path == f)).unwrap_or(0);
        let typed = &self.typed.0;
        // A run of one letter steps through the names starting with it.
        let repeat = typed.chars().count() > 1 && typed.chars().all(|ch| typed.starts_with(ch));
        let (needle, skip) = if repeat { (&typed[..typed.chars().next().unwrap().len_utf8()], 1) } else { (typed.as_str(), 0) };
        let count = c.entries.len();
        let found = (0..count)
            .map(|k| (start + skip + k) % count.max(1))
            .find(|&i| c.entries[i].name.to_lowercase().starts_with(needle));
        if let Some(i) = found {
            let path = c.entries[i].path.clone();
            if index == 0 {
                self.state.columns[0].focused = Some(path.clone());
            }
            self.state.select(index, vec![path]);
            self.sync_search_field(window, cx);
        }
    }

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.back();
        self.sync_search_field(window, cx);
    }

    pub fn forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.forward();
        self.sync_search_field(window, cx);
    }

    /// Going back or forward can bring another query; the field shows it.
    pub(crate) fn sync_search_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.state.query.clone();
        if self.search.read(cx).value().as_ref() != query {
            self.search.update(cx, |input, cx| input.set_value(query, window, cx));
        }
        self.changed(cx);
    }

    // ---- Scrolling ----

    /// Keeps the focused row of each column in sight, once it is listed.
    pub fn scroll_rows(&mut self) {
        for c in &self.state.columns {
            let Some(path) = c.focused.as_ref().or(c.highlight.as_ref()) else { continue };
            if self.scrolled_to.get(&c.id) == Some(path) {
                continue;
            }
            let Some(index) = c.entries.iter().position(|e| &e.path == path) else { continue };
            let handle = self.lists.entry(c.id).or_default();
            handle.scroll_to_item(index, ScrollStrategy::Nearest);
            self.scrolled_to.insert(c.id, path.clone());
        }
    }

    pub fn list_handle(&mut self, id: ColumnId) -> UniformListScrollHandle {
        self.lists.entry(id).or_default().clone()
    }
}
