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
    entry::{Entry, Order, Sort},
    everything::PAGE,
    paths::path_key,
    shell,
    watch::FolderWatcher,
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use colet_core::shell::clipboard::ClipboardFiles;

use crate::{
    backend::{self, Search},
    icons::Icons,
    ops::{HistoryEntry, Menu, OperationEvents},
    places::{Dialog, home_entries, home_groups},
    settings::{LoadError, Settings, Writer},
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
        RenameItem,
        CloseDialog,
        ConfirmDialog,
        CopyItems,
        CutItems,
        PasteItems,
        CopyPaths,
        NewFolder,
        RecycleItems,
        RecycleItemsForGood,
        SelectAll,
        ExtendUp,
        ExtendDown,
        OpenMenu,
        MenuUp,
        MenuDown,
        MenuConfirm,
        CloseMenu,
        OpenSort,
    ]
);

/// Where an open menu takes the keys.
pub const MENU: &str = "Menu";
/// The sort order's button, which opens its menu from the keys.
pub const SORT: &str = "Sort";

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
        KeyBinding::new("f2", RenameItem, Some(COLUMNS)),
        KeyBinding::new("ctrl-c", CopyItems, Some(COLUMNS)),
        KeyBinding::new("ctrl-x", CutItems, Some(COLUMNS)),
        KeyBinding::new("ctrl-v", PasteItems, Some(COLUMNS)),
        KeyBinding::new("ctrl-shift-c", CopyPaths, Some(COLUMNS)),
        KeyBinding::new("ctrl-shift-n", NewFolder, Some(COLUMNS)),
        KeyBinding::new("delete", RecycleItems, Some(COLUMNS)),
        KeyBinding::new("shift-delete", RecycleItemsForGood, Some(COLUMNS)),
        KeyBinding::new("ctrl-a", SelectAll, Some(COLUMNS)),
        KeyBinding::new("shift-up", ExtendUp, Some(COLUMNS)),
        KeyBinding::new("shift-down", ExtendDown, Some(COLUMNS)),
        KeyBinding::new("shift-f10", OpenMenu, Some(COLUMNS)),
        KeyBinding::new("f5", RefreshAll, None),
        KeyBinding::new("ctrl-d", SaveCurrent, None),
        KeyBinding::new("escape", CloseDialog, Some(DIALOG)),
        KeyBinding::new("enter", ConfirmDialog, Some(DIALOG)),
        KeyBinding::new("up", MenuUp, Some(MENU)),
        KeyBinding::new("down", MenuDown, Some(MENU)),
        KeyBinding::new("enter", MenuConfirm, Some(MENU)),
        KeyBinding::new("space", MenuConfirm, Some(MENU)),
        KeyBinding::new("escape", CloseMenu, Some(MENU)),
        KeyBinding::new("enter", OpenSort, Some(SORT)),
        KeyBinding::new("space", OpenSort, Some(SORT)),
        KeyBinding::new("down", OpenSort, Some(SORT)),
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
/// The strip that brings out the folders folded away above a search result: room for two digits.
pub const FOLD_WIDTH: f32 = 28.;

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
    /// The operation in the history it tells of, which 詳細 opens.
    pub entry: Option<u64>,
}

/// The columns sliding to where they should rest; see [`Colet::reveal`].
pub struct Slide {
    /// Where the target was reckoned from, to reckon it again should the view change under it.
    pub origin: f32,
    pub position: f32,
    pub target: f32,
    pub velocity: f32,
    pub time: Option<Instant>,
}

/// What a listing came back with: the entries, how many in all, and for a search what answered it.
type Listing = Result<(Vec<Entry>, usize, Option<String>), String>;

pub struct Colet {
    pub state: State,
    pub sort: Order,
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
    /// beside them close and they slide back; once at rest, where they rest.
    pub hold: f32,
    /// The view's width and the columns' that the columns were last brought to rest, or set sliding, for.
    pub fitted: (f32, f32),
    pub(crate) owner: shell::Owner,
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
    /// Folders the watcher could list but not watch, until the UI says so.
    missed: Arc<Mutex<Vec<String>>>,
    searched_at: Option<Instant>,
    search_refresh: bool,
    /// What type-ahead has typed so far, when, and in which column.
    typed: (String, Instant, ColumnId),
    pub settings: Settings,
    /// The file read at start could not be read and is still there: never written over this session.
    pub(crate) keep_file: bool,
    pub(crate) settings_writer: Arc<Writer>,
    /// What the latest operation sent away, by path key: a listing that finds it gone takes it as
    /// Colet's own doing.
    pub(crate) departed: HashSet<String>,
    pub dialog: Option<Dialog>,
    pub(crate) dialog_subscriptions: Vec<Subscription>,
    /// Takes the keys in a dialog with no field, as the Recycle Bin's question.
    pub dialog_focus: FocusHandle,
    /// Home's list, for its scrollbar.
    pub home_scroll: ScrollHandle,
    /// The buttons Tab stops at, each with the focus it keeps across frames.
    pub(crate) stops: HashMap<String, FocusHandle>,
    pub history_focus: FocusHandle,
    /// The keys are in the column worked in: its focused row is ringed.
    pub keys_in_column: bool,
    pub menu: Option<Menu>,
    /// Takes the keys while a menu is open.
    pub menu_focus: FocusHandle,
    /// The sort order's button, a stop for Tab.
    pub sort_focus: FocusHandle,
    /// The line of the open menu the keys are on.
    pub menu_cursor: Option<usize>,
    /// Where each crumb was drawn, by its column, and the row they stand in: the bar under them follows
    /// the columns on screen.
    pub crumb_bounds: std::rc::Rc<std::cell::RefCell<HashMap<usize, Bounds<Pixels>>>>,
    pub crumbs_row: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// The width the crumbs were last folded for.
    pub crumbs_room: std::rc::Rc<std::cell::Cell<f32>>,
    /// Where the focused row of the active column was drawn, for the menu key.
    pub focused_row: std::rc::Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    /// A file operation is running; they go one at a time.
    pub running: bool,
    pub(crate) next_operation: u64,
    /// The file operations run since Colet opened, newest first.
    pub history: Vec<HistoryEntry>,
    /// Operations with problems since the history was last opened.
    pub unseen: usize,
    pub history_open: bool,
    /// The operations the history shows opened.
    pub expanded: HashSet<u64>,
    pub(crate) operation_events: OperationEvents,
    /// Where each running operation's last item ended, as a new folder's.
    pub(crate) made: HashMap<u64, String>,
    /// Where each running operation's copied or moved items went.
    pub(crate) placed: HashMap<u64, Vec<String>>,
    /// The files on the clipboard, marked in their rows until it changes.
    pub clipboard: ClipboardFiles,
    /// The clipboard's paths by path key: another app may write them in another case.
    pub held: HashSet<String>,
    clipboard_changed: Arc<AtomicBool>,
    _subscriptions: Vec<Subscription>,
}

impl Colet {
    /// `settings` as read at start; an error is said once the window is up, and the defaults used. A file
    /// still there unread is never written over.
    pub fn new(
        backend: backend::Shared,
        settings: Result<Settings, LoadError>,
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
            // Followed only while the theme follows Windows.
            cx.observe_window_appearance(window, |this, window, cx| this.apply_theme(window, cx)),
            cx.observe_window_bounds(window, |this, window, cx| this.remember_window(window, cx)),
        ];
        // Written at once as the window closes, rather than a moment after the last change.
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| {
            let Some(view) = view.upgrade() else { return true };
            view.update(cx, |this, cx| {
                if this.running {
                    this.notify(
                        "ファイル操作中です。完了を待つか、進捗ダイアログでキャンセルしてから閉じてください。",
                        Tone::Info,
                        cx,
                    );
                    return false;
                }
                let _ = this.flush_settings();
                true
            })
        });
        let (settings, problem, keep_file) = match settings {
            Ok(settings) => (settings, None, false),
            Err(error) => (Settings::default(), Some(error.message), error.kept),
        };
        let owner = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        };
        let mut this = Self {
            state: State::new(),
            sort: Order::default(),
            widths: Widths {
                sidebar: settings.sidebar_width,
                search: settings.search_width,
                column: settings.column_width,
            },
            palette: Palette::for_window(window, settings.theme),
            search,
            search_focused: false,
            home_focus: cx.focus_handle().tab_stop(true),
            columns_focus: cx.focus_handle().tab_stop(true),
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
            fitted: (0., 0.),
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
            missed: Arc::default(),
            searched_at: None,
            search_refresh: false,
            typed: (String::new(), Instant::now(), 0),
            settings,
            keep_file,
            settings_writer: Arc::default(),
            departed: HashSet::new(),
            dialog: None,
            dialog_subscriptions: Vec::new(),
            dialog_focus: cx.focus_handle(),
            home_scroll: ScrollHandle::new(),
            stops: HashMap::new(),
            history_focus: cx.focus_handle().tab_stop(true),
            keys_in_column: false,
            menu: None,
            menu_focus: cx.focus_handle(),
            sort_focus: cx.focus_handle().tab_stop(true),
            menu_cursor: None,
            focused_row: Default::default(),
            crumb_bounds: Default::default(),
            crumbs_row: Default::default(),
            crumbs_room: Default::default(),
            running: false,
            next_operation: 0,
            history: Vec::new(),
            unseen: 0,
            history_open: false,
            expanded: HashSet::new(),
            operation_events: Arc::default(),
            made: HashMap::new(),
            placed: HashMap::new(),
            clipboard: ClipboardFiles::default(),
            held: HashSet::new(),
            clipboard_changed: Arc::new(AtomicBool::new(true)),
            _subscriptions: subscriptions,
        };
        if let Some(problem) = problem {
            this.notify(problem, Tone::Error, cx);
        }
        this.palette.style_input(&this.search, cx);
        let changed = this.clipboard_changed.clone();
        this.backend.watch_clipboard(Box::new(move || changed.store(true, Ordering::Relaxed)));
        this.search.update(cx, |input, cx| input.focus(window, cx));
        this.sync(cx);
        this.poll(cx);
        this
    }

    /// Takes the colors the theme now gives, in the fields too, the dialog's among them.
    pub fn apply_theme(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.palette = Palette::for_window(window, self.settings.theme);
        let mut inputs = vec![self.search.clone()];
        match &self.dialog {
            Some(Dialog::Name { input, .. }) => inputs.push(input.clone()),
            Some(Dialog::EditSearch { name, query, .. }) => inputs.extend([name.clone(), query.clone()]),
            Some(Dialog::Preferences { instance }) => inputs.push(instance.clone()),
            Some(Dialog::Recycle { .. }) | None => {}
        }
        for input in &inputs {
            self.palette.style_input(input, cx);
        }
        cx.notify();
    }

    /// The focus a button keeps, a stop for Tab; Enter or Space presses it.
    pub fn stop(&mut self, id: impl Into<String>, cx: &mut Context<Self>) -> FocusHandle {
        self.stops.entry(id.into()).or_insert_with(|| cx.focus_handle().tab_stop(true)).clone()
    }

    /// Tab moves the keys between home's pane and the columns: the column worked in follows them. Into the
    /// columns they land where → would, past the folders folded above a favorite.
    pub fn follow_focus(&mut self, window: &Window) {
        let (home, columns) = (self.home_focus.is_focused(window), self.columns_focus.is_focused(window));
        if home && self.state.active != 0 {
            self.state.set_active(0);
            self.state.enter();
        } else if columns && self.state.active == 0 && self.state.columns.len() > 1 {
            self.state.set_active(self.state.neighbour(0, true).unwrap_or(1));
            self.state.enter();
        }
        self.keys_in_column = home || columns;
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
        if self.menu.as_ref().is_some_and(|menu| !live.contains(&menu.column)) {
            self.menu = None;
            self.menu_cursor = None;
            let view = cx.weak_entity();
            let id = cx.entity_id();
            // Restore the keys after this update, before drawing, only if they are still in the
            // dismissed menu. A field or another menu opened meanwhile keeps its focus.
            cx.defer(move |cx| {
                cx.with_window(id, |window, cx| {
                    view.update(cx, |this, cx| {
                        if this.menu.is_none() && !this.sort_open && this.menu_focus.is_focused(window) {
                            this.close_menus(true, window, cx);
                        }
                    }).ok();
                });
            });
        }
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

    /// An error that tells of an operation in the history, with a button to open it there.
    pub fn notify_entry(&mut self, text: impl Into<SharedString>, entry: u64, cx: &mut Context<Self>) {
        self.notify(text, Tone::Error, cx);
        if let Some(toast) = self.toasts.last_mut() {
            toast.entry = Some(entry);
        }
    }

    pub fn notify(&mut self, text: impl Into<SharedString>, tone: Tone, cx: &mut Context<Self>) {
        self.next_toast += 1;
        let id = self.next_toast;
        self.toasts.push(Toast { id, text: text.into(), tone, entry: None });
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
                self.state.page(id, request, entries, total, offset, &self.departed);
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

    /// Sorts by `by`, keeping the way it runs.
    pub fn set_sort(&mut self, by: Sort, cx: &mut Context<Self>) {
        self.set_order(Order { by, ..self.sort }, cx);
    }

    pub fn set_direction(&mut self, descending: bool, cx: &mut Context<Self>) {
        self.set_order(Order { descending, ..self.sort }, cx);
    }

    fn set_order(&mut self, order: Order, cx: &mut Context<Self>) {
        self.sort_open = false;
        if order != self.sort {
            self.sort = order;
            self.refresh(cx);
        }
        cx.notify();
    }

    // ---- Watching ----

    /// Watches the folders the columns could read, one level deep. The watcher opens them on its own
    /// thread, so a share slow to answer never holds up the window.
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
        if self.watcher.is_none() {
            let (changed, missed) = (self.changed.clone(), self.missed.clone());
            let watcher = FolderWatcher::new(
                move |paths| {
                    let mut queue = changed.lock().unwrap();
                    let batch = queue.get_or_insert_with(|| Some(HashSet::new()));
                    match (paths.is_empty(), batch.as_mut()) {
                        (true, _) => *batch = None,
                        (false, Some(batch)) => batch.extend(paths),
                        (false, None) => {}
                    }
                },
                move |paths| missed.lock().unwrap().extend(paths),
            );
            match watcher {
                Ok(watcher) => self.watcher = Some(watcher),
                Err(error) => {
                    self.notify(error, Tone::Error, cx);
                    return;
                }
            }
        }
        if let Some(watcher) = &self.watcher {
            watcher.set(paths);
        }
    }

    /// Says which folders the watcher could list but not watch, each once.
    fn take_missed(&mut self, cx: &mut Context<Self>) {
        let missed = std::mem::take(&mut *self.missed.lock().unwrap());
        let fresh: Vec<String> = missed.into_iter().filter(|p| self.unwatched.insert(path_key(p))).collect();
        if !fresh.is_empty() {
            let names: String = fresh.iter().map(|p| format!("「{}」", colet_core::paths::folder_name(p))).collect();
            self.notify(format!("{names}の変更を監視できません。F5で更新できます。"), Tone::Error, cx);
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
                if this.update(cx, |this, cx| this.take_changes(cx)).is_err() {
                    return;
                }
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

    /// The folders that went unwatched, how the running operation's items ended so far, and what the
    /// clipboard now holds.
    fn take_changes(&mut self, cx: &mut Context<Self>) {
        self.take_missed(cx);
        if self.take_operation_events() {
            cx.notify();
        }
        if self.clipboard_changed.swap(false, Ordering::Relaxed) {
            let backend = self.backend.clone();
            let read = cx.background_executor().spawn(async move { backend.clipboard_files() });
            cx.spawn(async move |this, cx| {
                // None while another app holds it: try again next time.
                match read.await {
                    Some(files) => this.update(cx, |this, cx| {
                        this.held = files.paths.iter().map(|p| path_key(p)).collect();
                        this.clipboard = files;
                        cx.notify();
                    }),
                    None => this.update(cx, |this, _| this.clipboard_changed.store(true, Ordering::Relaxed)),
                }
                .ok();
            })
            .detach();
        }
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
    pub fn open(&mut self, index: usize, path: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.state.columns.get(index).and_then(|c| c.entry(path)).cloned() else { return };
        if crate::state::saved_query(&entry.path).is_some() {
            self.pick(index, entry.path, window, cx);
            return;
        }
        if entry.directory {
            self.state.select(index, vec![entry.path]);
            self.changed(cx);
            return;
        }
        let (backend, owner) = (self.backend.clone(), self.owner);
        let task = cx.background_executor().spawn(async move { backend.open(&entry.path, owner) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                this.update(cx, |this, cx| this.notify(format!("開けません: {error}"), Tone::Error, cx)).ok();
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
            if c.kind == Kind::Home {
                let mut row = 0;
                for (_, entries) in home_groups(&self.settings, &c.entries) {
                    row += 1; // The heading is a child of the same scroll container.
                    if let Some(at) = entries.iter().position(|e| &e.path == path) {
                        self.home_scroll.scroll_to_item(row + at);
                        break;
                    }
                    row += entries.len();
                }
            } else {
                let handle = self.lists.entry(c.id).or_default();
                handle.scroll_to_item(index, ScrollStrategy::Nearest);
            }
            self.scrolled_to.insert(c.id, path.clone());
        }
    }

    pub fn list_handle(&mut self, id: ColumnId) -> UniformListScrollHandle {
        self.lists.entry(id).or_default().clone()
    }
}
