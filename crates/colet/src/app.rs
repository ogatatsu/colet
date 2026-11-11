//! The window's one view: it keeps the navigation state, fetches what the columns list, watches the open
//! folders, and takes the keys. `view.rs` draws it.
use std::{
    borrow::Cow,
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ffi::OsString,
    ops::Range,
    path::{Path, PathBuf},
    rc::Rc,
    sync::{atomic::Ordering, mpsc},
    time::Duration,
};

use colet_core::{
    entry::{Entry, Order, Sort},
    everything::PAGE,
    operations::message,
    paths::{path_key, shown},
    quick_look, shell,
    watch::Changes,
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::component::{
    WindowExt,
    button::Button,
    notification::{Notification, NotificationType},
};
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::{
    address::Address,
    backend::{self, Search},
    crumbs::Trail,
    icons::Icons,
    loading::Loading,
    menus::Popup,
    ops::Operations,
    overlays::Overlays,
    places::{Dialog, HomeDragging, home_entries, home_groups},
    scrolls::Scrolls,
    settings::{self, File, LoadError, Saver, Saves, Settings, everything_query, narrowed_query},
    sorts::Sorts,
    state::{Column, ColumnId, Kind, Showing, State, narrow_words, room_for_a_word},
    stored::Stored,
    tabs::Tabs,
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
        CopyItems,
        CutItems,
        PasteItems,
        CopyPaths,
        ShowProperties,
        NewFolder,
        RecycleItems,
        RecycleItemsForGood,
        SelectAll,
        ExtendUp,
        ExtendDown,
        AddUp,
        AddDown,
        FocusPrevious,
        FocusNext,
        FocusPageUp,
        FocusPageDown,
        FocusFirst,
        FocusLast,
        TogglePicked,
        OpenMenu,
        OpenTextMenu,
        CloseMenu,
        OpenSort,
        TrimNarrowing,
        ClearNarrowing,
        OpenSearchFilters,
        Unfold,
        FocusHome,
        ToggleHidden,
        QuickLook,
        NewTab,
        CloseTab,
        NextTab,
        PreviousTab,
        ReopenTab,
        OpenInNewTab,
        FocusAddress,
        GoToAddressInNewTab,
    ]
);

/// Where an open menu takes the keys.
pub const MENU: &str = "Menu";
/// The sort order's button, which opens its menu from the keys.
pub const SORT: &str = "Sort";

/// Where a text field takes the keys: GPUI Base's own context for its fields.
pub const TEXT: &str = "Input";

/// The address bar's field, where Alt+Enter goes to the path typed in a new tab.
pub const ADDRESS: &str = "Address";

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
        KeyBinding::new("ctrl-l", FocusAddress, None),
        KeyBinding::new("alt-enter", GoToAddressInNewTab, Some(ADDRESS)),
        KeyBinding::new("ctrl-shift-f", OpenSearchFilters, None),
        KeyBinding::new("ctrl-u", Unfold, None),
        KeyBinding::new("ctrl-0", FocusHome, None),
        KeyBinding::new("ctrl-h", ToggleHidden, None),
        KeyBinding::new("f2", RenameItem, Some(COLUMNS)),
        KeyBinding::new("ctrl-c", CopyItems, Some(COLUMNS)),
        KeyBinding::new("ctrl-x", CutItems, Some(COLUMNS)),
        KeyBinding::new("ctrl-v", PasteItems, Some(COLUMNS)),
        KeyBinding::new("ctrl-shift-c", CopyPaths, Some(COLUMNS)),
        KeyBinding::new("alt-enter", ShowProperties, Some(COLUMNS)),
        KeyBinding::new("ctrl-shift-n", NewFolder, Some(COLUMNS)),
        KeyBinding::new("delete", RecycleItems, Some(COLUMNS)),
        KeyBinding::new("shift-delete", RecycleItemsForGood, Some(COLUMNS)),
        KeyBinding::new("ctrl-a", SelectAll, Some(COLUMNS)),
        KeyBinding::new("shift-up", ExtendUp, Some(COLUMNS)),
        KeyBinding::new("shift-down", ExtendDown, Some(COLUMNS)),
        KeyBinding::new("ctrl-shift-up", AddUp, Some(COLUMNS)),
        KeyBinding::new("ctrl-shift-down", AddDown, Some(COLUMNS)),
        // The keys move apart from the selection, Ctrl+Space picking the rows they pass on the way.
        KeyBinding::new("ctrl-up", FocusPrevious, Some(COLUMNS)),
        KeyBinding::new("ctrl-down", FocusNext, Some(COLUMNS)),
        KeyBinding::new("ctrl-pageup", FocusPageUp, Some(COLUMNS)),
        KeyBinding::new("ctrl-pagedown", FocusPageDown, Some(COLUMNS)),
        KeyBinding::new("ctrl-home", FocusFirst, Some(COLUMNS)),
        KeyBinding::new("ctrl-end", FocusLast, Some(COLUMNS)),
        KeyBinding::new("ctrl-space", TogglePicked, Some(COLUMNS)),
        // In the rows only: a field or a button there takes its own Space.
        KeyBinding::new("space", QuickLook, Some(COLUMNS)),
        KeyBinding::new("shift-f10", OpenMenu, Some(COLUMNS)),
        KeyBinding::new("menu", OpenMenu, Some(COLUMNS)),
        // In a text field, where the field's own keys are.
        KeyBinding::new("shift-f10", OpenTextMenu, Some(TEXT)),
        KeyBinding::new("menu", OpenTextMenu, Some(TEXT)),
        KeyBinding::new("f5", RefreshAll, None),
        KeyBinding::new("ctrl-d", SaveCurrent, None),
        KeyBinding::new("escape", CloseMenu, Some(MENU)),
        // The keys that open a menu close it again, as Windows' own menus do.
        KeyBinding::new("shift-f10", CloseMenu, Some(MENU)),
        KeyBinding::new("menu", CloseMenu, Some(MENU)),
        KeyBinding::new("enter", OpenSort, Some(SORT)),
        KeyBinding::new("space", OpenSort, Some(SORT)),
        KeyBinding::new("down", OpenSort, Some(SORT)),
        KeyBinding::new("ctrl-t", NewTab, None),
        KeyBinding::new("ctrl-w", CloseTab, None),
        KeyBinding::new("ctrl-tab", NextTab, None),
        KeyBinding::new("ctrl-shift-tab", PreviousTab, None),
        KeyBinding::new("ctrl-shift-t", ReopenTab, None),
        KeyBinding::new("ctrl-enter", OpenInNewTab, Some(COLUMNS)),
        KeyBinding::new("backspace", TrimNarrowing, Some(COLUMNS)),
        KeyBinding::new("escape", ClearNarrowing, Some(COLUMNS)),
    ]);
}

/// The widths the user drags: home's pane, the pinned search results, the folders, which share one, and
/// the search field.
pub struct Widths {
    pub sidebar: f32,
    pub search: f32,
    pub column: f32,
    pub field: f32,
    /// The drag on an edge under way.
    resizing: Option<Resize>,
}

impl Widths {
    pub fn new(sidebar: f32, search: f32, column: f32, field: f32) -> Self {
        Self { sidebar, search, column, field, resizing: None }
    }

    /// The width `of` names.
    pub fn width(&self, of: WidthOf) -> f32 {
        match of {
            WidthOf::Sidebar => self.sidebar,
            WidthOf::Search => self.search,
            WidthOf::Column => self.column,
            WidthOf::Field => self.field,
        }
    }

    /// A drag on the edge of what `of` names starts at `x`, from `start_width`: the width it shows,
    /// which a narrow window may have narrowed.
    pub fn start_resize(&mut self, of: WidthOf, x: Pixels, start_width: f32) {
        self.resizing = Some(Resize { of, start_x: x, start_width });
    }

    /// Which width a drag under way changes.
    pub fn resizing(&self) -> Option<WidthOf> {
        self.resizing.as_ref().map(|resize| resize.of)
    }

    /// The drag under way reached `x`: its width follows, within its bounds. False with none under way.
    pub fn drag_to(&mut self, x: Pixels) -> bool {
        let Some(resize) = &self.resizing else { return false };
        let (min, max) = match resize.of {
            WidthOf::Sidebar => SIDEBAR_WIDTHS,
            WidthOf::Field => SEARCH_FIELD_WIDTHS,
            WidthOf::Search | WidthOf::Column => COLUMN_WIDTHS,
        };
        // Display scaling makes the deltas fractional; widths stay whole pixels.
        let width = (resize.start_width + f32::from(x - resize.start_x)).clamp(min, max).round();
        match resize.of {
            WidthOf::Sidebar => self.sidebar = width,
            WidthOf::Search => self.search = width,
            WidthOf::Column => self.column = width,
            WidthOf::Field => self.field = width,
        }
        true
    }

    /// The drag under way is over: which width it changed.
    pub fn end_resize(&mut self) -> Option<WidthOf> {
        self.resizing.take().map(|resize| resize.of)
    }
}

/// At its least a column keeps some of a name beside a row's size and modified time.
pub const COLUMN_WIDTHS: (f32, f32) = (260., 640.);
pub const SIDEBAR_WIDTHS: (f32, f32) = (160., 480.);
/// The strip that brings out the folders folded away above a search result: room for two digits.
pub const FOLD_WIDTH: f32 = 28.;
/// What home's pane leaves beside it, narrowing in a narrow window to do so: the toolbar with the search
/// field at its least, the trail's buttons, and the pinned results with a fold strip.
pub const MAIN_MIN: f32 = 480.;
/// The smallest the window goes: home's pane at its usual width, and the rest.
pub const MIN_WINDOW: (f32, f32) = (240. + MAIN_MIN, 480.);
/// The search field's width at first, and the bounds a drag keeps it in. In a narrow window it narrows to
/// the least, the address beside it and the search filter's name giving way first.
pub const SEARCH_FIELD_WIDTH: f32 = 260.;
pub const SEARCH_FIELD_WIDTHS: (f32, f32) = (200., 640.);
/// The least the address keeps: the "…" of its folded crumbs, the last clipped, and room to press to type.
pub const ADDRESS_MIN: f32 = 120.;
/// What the toolbar takes from the window but the address and the search field, guessed before it is first
/// drawn.
pub const ADDRESS_GUESS: f32 = 240.;

/// Which width a drag on an edge changes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum WidthOf {
    Sidebar,
    Search,
    Column,
    Field,
}

struct Resize {
    of: WidthOf,
    start_x: Pixels,
    start_width: f32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    Error,
}

/// A file kept between starts as it was `read`, or the defaults with what to say of it, and its saves
/// through `write`: none while the file could not be read and is still there, so it is never written over.
fn kept<T: Default + Clone + Send + 'static>(
    read: Result<T, LoadError>,
    file: &File,
    write: impl Fn(&T) -> Result<(), String> + Send + Sync + 'static,
) -> (T, Saves<T>, Option<String>) {
    let (value, problem, still_there) = match read {
        Ok(value) => (value, None, false),
        Err(error) => (T::default(), Some(error.message), error.kept),
    };
    (value, Saves::new((!still_there).then(|| Saver::new(file, write))), problem)
}

/// What a listing came back with.
struct Listed {
    entries: Vec<Entry>,
    /// How many in all, loaded or not.
    total: usize,
    /// For a search, the Everything that answered it.
    connection: Option<String>,
    /// For a narrowed search's first page, how many it finds without the filter.
    whole: Option<usize>,
}

impl Listed {
    /// A listing loaded whole at once, as home's and a folder's are.
    fn all(entries: Vec<Entry>) -> Self {
        Listed { total: entries.len(), entries, connection: None, whole: None }
    }
}

type Listing = Result<Listed, String>;

/// The field narrowing a column, and what it last narrowed it to: the column lifting the filter on
/// its own, for what was made or pasted there, tells from that, not from text typed and yet to be told.
pub struct NarrowField {
    pub input: Entity<InputState>,
    applied: String,
    _changes: Subscription,
}

pub struct Colet {
    /// The tab in front's; the other tabs are parked in `tabs`, with the fields below kept for each.
    pub state: State,
    /// The filter the next search in the tab in front is narrowed by, by name.
    pub chosen_filter: Option<String>,
    pub tabs: Tabs,
    pub widths: Widths,
    pub palette: Palette,
    pub search: Entity<InputState>,
    pub address: Address,
    /// The field of each column, narrowing it to some names.
    pub narrow_fields: HashMap<ColumnId, NarrowField>,
    pub home_focus: FocusHandle,
    pub columns_focus: FocusHandle,
    pub icons: Icons,
    /// The window the notices go up in.
    window: AnyWindowHandle,
    pub(crate) next_toast: u64,
    /// Where the tab in front is scrolled.
    pub scrolls: Scrolls,
    pub(crate) owner: shell::Owner,
    pub(crate) backend: backend::Shared,
    /// The listings out, the search's reloads, the watch on the folders and the search engine.
    pub loading: Loading,
    /// The settings and the columns' own orders, as kept between starts.
    pub stored: Stored,
    pub home_drag: HomeDragging,
    /// The buttons Tab stops at, each with the focus it keeps across frames.
    pub(crate) stops: RefCell<HashMap<String, FocusHandle>>,
    /// The dialog and the menu open, and where the keys go back to as they close.
    pub overlays: Overlays,
    /// The sort order's button, a stop for Tab.
    pub sort_focus: FocusHandle,
    pub crumbs: Trail,
    /// Where the focused row of the active column was drawn, for the menu key.
    pub focused_row: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The file operations, the history and the clipboard.
    pub operations: Operations,
    /// What QuickLook was last told to show: a preview open follows the keys from it.
    quick_looked: Option<PathBuf>,
    _subscriptions: Vec<Subscription>,
}

impl Colet {
    /// `settings` and `sorts` as read at start; an error is said once the window is up, and the defaults
    /// used. A file still there unread is never written over.
    pub fn new(
        backend: backend::Shared,
        settings: Result<Settings, LoadError>,
        sorts: Result<Sorts, LoadError>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let (search, typed) = Self::search_field(window, cx);
        let (address, address_typed) = Self::address_field(window, cx);
        let subscriptions = vec![
            typed,
            address_typed,
            // Followed only while the theme follows Windows.
            cx.observe_window_appearance(window, |this, window, cx| this.apply_theme(window, cx)),
            cx.observe_window_bounds(window, |this, window, cx| this.remember_window(window, cx)),
        ];
        let view = cx.entity().downgrade();
        window.on_window_should_close(cx, move |_, cx| view.upgrade().is_none_or(|view| view.update(cx, |this, cx| this.may_close(cx))));
        let (settings, settings_saves, problem) = {
            let backend = backend.clone();
            kept(settings, &settings::SETTINGS, move |settings| backend.save_settings(settings))
        };
        let (sorts, sorts_saves, sorts_problem) = {
            let backend = backend.clone();
            kept(sorts, &crate::sorts::SORTS, move |sorts| backend.save_sorts(sorts))
        };
        let owner = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        };
        let mut this = Self {
            state: State::new(),
            chosen_filter: None,
            tabs: Tabs::default(),
            widths: Widths::new(settings.sidebar_width, settings.search_width, settings.column_width, settings.search_field_width),
            palette: Palette::for_window(window, settings.theme),
            search,
            address: Address::new(address),
            narrow_fields: HashMap::new(),
            home_focus: cx.focus_handle().tab_stop(true),
            columns_focus: cx.focus_handle().tab_stop(true),
            icons: Icons::default(),
            window: Window::window_handle(window),
            next_toast: 0,
            scrolls: Scrolls::default(),
            owner,
            backend,
            loading: Loading::default(),
            stored: Stored::new(settings, settings_saves, sorts, sorts_saves),
            home_drag: HomeDragging::default(),
            stops: RefCell::default(),
            sort_focus: cx.focus_handle().tab_stop(true),
            crumbs: Trail::default(),
            overlays: Overlays::new(cx),
            focused_row: Default::default(),
            operations: Operations::default(),
            quick_looked: None,
            _subscriptions: subscriptions,
        };
        for problem in [problem, sorts_problem].into_iter().flatten() {
            this.notify(problem, Tone::Error, cx);
        }
        this.palette.style_input(&this.search, cx);
        this.palette.style_input(&this.address.field, cx);
        this.restore_tabs(window, cx);
        // The window's dialog, the columns' fields and their scrolls follow `dialog` and the columns as each
        // change to Colet settles, before it is drawn.
        let colet = cx.entity();
        this._subscriptions.push(cx.observe_in(&colet, window, |this, _, window, cx| {
            this.sync_narrow_fields(window, cx);
            this.scroll_rows();
            this.sync_dialog(window, cx);
        }));
        // Tab, a click or a closing overlay bringing the keys to home or the columns makes that the column
        // worked in.
        let (home, columns) = (this.home_focus.clone(), this.columns_focus.clone());
        this._subscriptions.push(cx.on_focus(&home, window, |this, window, cx| this.follow_focus(window, cx)));
        this._subscriptions.push(cx.on_focus(&columns, window, |this, window, cx| this.follow_focus(window, cx)));
        // The keys leaving the open address field close it.
        let address = this.address.field.read(cx).focus_handle(cx);
        this._subscriptions.push(cx.on_focus_out(&address, window, |this, _, window, cx| {
            if this.follow_address(window, cx) {
                cx.notify();
            }
        }));
        this.sync_narrow_fields(window, cx);
        this.scroll_rows();
        crate::theme::apply(window, this.stored.settings().theme, cx);
        let changed = this.operations.clipboard().changed_flag();
        this.backend.watch_clipboard(Box::new(move || changed.store(true, Ordering::Relaxed)));
        this.load_send_targets(window, cx);
        this.search.update(cx, |input, cx| input.focus(window, cx));
        this.sync(cx);
        this.poll(cx);
        this
    }

    /// The search field, and what it tells: what is typed is searched within the filter chosen, as is an
    /// empty field while one is; Enter leaves it for the results.
    fn search_field(window: &mut Window, cx: &mut Context<Self>) -> (Entity<InputState>, Subscription) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Everythingで検索（Ctrl+F）"));
        let typed = cx.subscribe_in(&search, window, |this: &mut Self, input, event, window, cx| match event {
            InputEvent::Change => {
                let query = input.read(cx).value().to_string();
                this.state.set_search(&query, this.chosen_filter.clone());
                this.changed(cx);
            }
            InputEvent::PressEnter { .. } => this.leave_search(window, cx),
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
        });
        (search, typed)
    }

    /// Whether the window may close: not while a file operation or a send runs. The settings are written at once,
    /// rather than a moment after the last change; when that fails, the next close goes without them.
    pub(crate) fn may_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.operations.is_running() {
            self.notify("ファイル操作中です。完了を待つか、進捗ダイアログでキャンセルしてから閉じてください。", Tone::Info, cx);
            return false;
        }
        if self.operations.is_sending() {
            self.notify("「送る」の処理中です。完了を待ってから閉じてください。", Tone::Info, cx);
            return false;
        }
        if self.stored.is_unsaved_close() {
            return true;
        }
        match self.flush_settings() {
            Ok(()) => true,
            Err(error) => {
                self.stored.give_up_saving_on_close();
                self.notify(format!("{error}。もう一度閉じると、設定を保存せずに終了します。"), Tone::Error, cx);
                false
            }
        }
    }

    /// Takes the colors the theme now gives, in the fields too, the dialog's among them.
    pub fn apply_theme(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.palette = Palette::for_window(window, self.stored.settings().theme);
        crate::theme::apply(window, self.stored.settings().theme, cx);
        let mut inputs = vec![self.search.clone(), self.address.field.clone()];
        inputs.extend(self.narrow_fields.values().map(|field| field.input.clone()));
        inputs.extend(self.overlays.dialog().into_iter().flat_map(Dialog::fields));
        for input in &inputs {
            self.palette.style_input(input, cx);
        }
        cx.notify();
    }

    /// The focus a button keeps, a stop for Tab; Enter or Space presses it.
    pub fn stop(&self, id: impl Into<String>, cx: &App) -> FocusHandle {
        self.stops.borrow_mut().entry(id.into()).or_insert_with(|| cx.focus_handle().tab_stop(true)).clone()
    }

    /// Tab moves the keys between home's pane and the columns: the column worked in follows them. Into the
    /// columns they land where → would, past the folders folded above a favorite.
    /// What landing there opens, a folder picked as → would, is listed as → would list it.
    fn follow_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (home, columns) = (self.home_focus.is_focused(window), self.columns_focus.is_focused(window));
        if home && self.state.active != 0 {
            self.state.set_active(0);
        } else if columns && self.state.active == 0 && self.state.columns.len() > 1 {
            self.state.set_active(self.state.neighbour(0, true).unwrap_or(1));
        } else {
            return;
        }
        self.state.enter();
        self.changed(cx);
    }

    // ---- What happens after the state changes ----

    /// Starts what the columns now on screen need, drops what the ones gone needed, and redraws.
    pub fn changed(&mut self, cx: &mut Context<Self>) {
        self.sync(cx);
        self.follow_quick_look();
        self.scrolls.sideways.ask();
        self.keep_tabs(cx);
        cx.notify();
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let live: HashSet<ColumnId> = self.state.columns.iter().map(|c| c.id).collect();
        if self.menu().is_some_and(|menu| !live.contains(&menu.column)) {
            self.take_menu();
            let view = cx.weak_entity();
            let id = cx.entity_id();
            // Restore the keys after this update, before drawing, only if they are still in the
            // dismissed menu. A field or another menu opened meanwhile keeps its focus.
            cx.defer(move |cx| {
                cx.with_window(id, |window, cx| {
                    view.update(cx, |this, cx| {
                        if this.overlays.popup().is_none() && this.overlays.has_menu() {
                            let keys = this.menu_has_keys(window, cx);
                            this.close_menus(keys, window, cx);
                        }
                    })
                    .ok();
                });
            });
        }
        self.loading.keep(&live);
        self.scrolls.rows.keep(&live);
        let fresh: Vec<(ColumnId, Kind)> = self.state.columns.iter().filter(|c| self.loading.first(c.id)).map(|c| (c.id, c.kind)).collect();
        for (id, kind) in fresh {
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
        self.push_notice(text.into(), Tone::Error, Some(entry), cx);
    }

    pub fn notify(&mut self, text: impl Into<SharedString>, tone: Tone, cx: &mut Context<Self>) {
        self.push_notice(text.into(), tone, None, cx);
    }

    /// Puts a notice up in the window, the newest few at a time: errors stay until closed; the rest go on
    /// their own.
    fn push_notice(&mut self, text: SharedString, tone: Tone, entry: Option<u64>, cx: &mut Context<Self>) {
        self.next_toast += 1;
        let id = self.next_toast;
        let error = tone == Tone::Error;
        let colet = cx.weak_entity();
        let mut note = Notification::new()
            .id1::<Self>(("toast", id))
            .content(move |_, _, _| {
                div()
                    .id(("toast", id))
                    .test_support()
                    .role(if error { Role::Alert } else { Role::Status })
                    .aria_label(text.clone())
                    .text_sm()
                    .child(text.clone())
                    .into_any_element()
            })
            .autohide(!error);
        if error {
            note = note.with_type(NotificationType::Error);
        }
        if let Some(entry) = entry {
            note = note.action(move |_, _, _| {
                let colet = colet.clone();
                // The mouse leaves the keys where they are; from the keys, they go on into the history.
                crate::view::keep_focus(Button::new(("toast-details", id)).label("詳細")).on_click(move |event: &ClickEvent, window, cx| {
                    window.remove_notification1::<Colet>(("toast", id), cx);
                    colet
                        .update(cx, |this, cx| {
                            this.show_history(true, Some(entry), window, cx);
                            if event.is_keyboard() {
                                this.stop(format!("history-entry:{entry}"), cx).focus(window, cx);
                            }
                        })
                        .ok();
                })
            });
        }
        let window = self.window;
        cx.defer(move |cx| {
            window.update(cx, |_, window, cx| window.push_notification(note, cx)).ok();
        });
    }

    /// Runs `work` off the UI thread, then says `done` once it succeeds, or why it failed.
    pub(crate) fn in_background(
        &mut self,
        work: impl FnOnce() -> Result<(), String> + Send + 'static,
        done: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let task = cx.background_executor().spawn(async move { work() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match (result, done) {
                (Ok(()), Some(done)) => this.notify(done, Tone::Info, cx),
                (Ok(()), None) => {}
                (Err(error), _) => this.notify(error, Tone::Error, cx),
            })
            .ok();
        })
        .detach();
    }

    // ---- Listings ----

    /// Lists the column `id` from `offset` on; `quiet` keeps what it shows meanwhile.
    pub fn load(&mut self, id: ColumnId, offset: usize, quiet: bool, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        if offset > 0 && self.loading.is_loading(id) {
            return;
        }
        let request = self.loading.start(id);
        self.state.load(id, request, quiet);
        let c = &self.state.columns[index];
        let sort = self.order_of(c);
        let backend = self.backend.clone();
        let task: Task<Listing> = match c.kind {
            Kind::Home => {
                let settings = self.stored.settings().clone();
                cx.background_executor().spawn(async move { Ok(Listed::all(home_entries(&settings, backend.roots()))) })
            }
            Kind::Search => {
                let filter = self.state.search_filter.as_deref().and_then(|name| self.stored.settings().filter_query(name));
                let whole = everything_query(&self.state.query, filter);
                // Narrowed, the search is counted without it too, along with the first page.
                let (query, whole) = match &c.narrow {
                    Some(filter) => (narrowed_query(&whole, &filter.text), (offset == 0).then_some(whole)),
                    None => (whole, None),
                };
                // A refresh reloads all it has loaded, so later selections stay.
                let count = if offset > 0 { PAGE } else { PAGE.max(c.entries.len()) };
                self.loading.searching();
                let search = Search { query, sort, offset, count, refresh: offset == 0, request };
                cx.background_executor().spawn(async move {
                    let (page, connection) = backend.search(search)?;
                    let whole = whole.and_then(|query| backend.count(&query).ok());
                    Ok(Listed { entries: page.entries, total: page.total, connection: Some(connection), whole })
                })
            }
            Kind::Folder => {
                let path = c.path.clone();
                let (cancelled, owner) = (self.loading.cancel_before(id), self.owner);
                cx.background_executor().spawn(async move { Ok(Listed::all(backend.list(&path, sort, &cancelled, owner)?)) })
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
        let current = self.loading.answered(id, request);
        match result {
            Ok(Listed { entries, total, connection, whole }) => {
                if let Some(whole) = whole {
                    self.state.counted(id, request, whole);
                }
                self.state.page(id, request, entries, total, offset, self.operations.departed());
                // What the listing showed held in another case than the disk's, as a favorite kept from
                // before a rename elsewhere, is kept in the disk's from now on.
                for (from, to) in std::mem::take(&mut self.state.respelled) {
                    self.follow_move(&from, &to, cx);
                }
                self.loading.search_answered(connection, current);
            }
            Err(error) => {
                if kind == Kind::Search && current {
                    self.loading.search_failed(error.clone());
                }
                self.state.error(id, request, error);
            }
        }
        self.changed(cx);
    }

    /// The tab come to the front lists all its columns again: what it showed stays meanwhile, and what it
    /// never listed shows as loading. What the tab before it still had out is let go of.
    pub(crate) fn relist(&mut self, cx: &mut Context<Self>) {
        self.loading.let_go();
        let columns: Vec<(ColumnId, bool)> = self.state.columns.iter().map(|c| (c.id, c.request != 0)).collect();
        for (id, listed) in columns {
            self.loading.first(id);
            self.load(id, 0, listed, cx);
        }
    }

    /// Which hidden items the folders list, as the settings say.
    pub fn showing(&self) -> Showing {
        Showing { hidden: self.stored.settings().show_hidden, protected: self.stored.settings().show_protected }
    }

    /// The folders list their hidden items, or leave them out, in every tab and from now on.
    pub fn toggle_hidden(&mut self, cx: &mut Context<Self>) {
        let show = !self.stored.settings().show_hidden;
        self.update_settings(|s| s.show_hidden = show, cx);
        self.follow_showing(cx);
    }

    /// The protected operating system files show along with the hidden items, or stay out even then.
    pub fn toggle_protected(&mut self, cx: &mut Context<Self>) {
        let show = !self.stored.settings().show_protected;
        self.update_settings(|s| s.show_protected = show, cx);
        self.follow_showing(cx);
    }

    /// The tab in front lists the hidden items as the settings say now; the others follow once brought back.
    fn follow_showing(&mut self, cx: &mut Context<Self>) {
        self.state.set_showing(self.showing());
        // The rows move: the focused ones are brought in sight again.
        let folders: Vec<ColumnId> = self.state.columns.iter().filter(|c| c.kind == Kind::Folder).map(|c| c.id).collect();
        for id in folders {
            self.scrolls.rows.moved(id);
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

    /// The order the column `c` is listed in: its own, or the default.
    pub fn order_of(&self, c: &Column) -> Order {
        self.own_order(c).unwrap_or(self.stored.settings().sort)
    }

    /// The order the column `c` is given apart from the default: its folder's, or the search results'.
    pub fn own_order(&self, c: &Column) -> Option<Order> {
        match c.kind {
            Kind::Home => None,
            Kind::Search => self.stored.sorts().search,
            Kind::Folder => self.stored.sorts().folder(&c.path),
        }
    }

    /// The order of the column `id`, or the default when `None`.
    fn order_for(&self, target: Option<ColumnId>) -> Order {
        match target.and_then(|id| self.state.index_of(id)) {
            Some(index) => self.order_of(&self.state.columns[index]),
            None => self.stored.settings().sort,
        }
    }

    /// Sorts the column `target`, or by default, by `by`, keeping the way it runs.
    pub fn set_sort(&mut self, target: Option<ColumnId>, by: Sort, cx: &mut Context<Self>) {
        self.set_order(target, Order { by, ..self.order_for(target) }, cx);
    }

    pub fn set_direction(&mut self, target: Option<ColumnId>, descending: bool, cx: &mut Context<Self>) {
        self.set_order(target, Order { descending, ..self.order_for(target) }, cx);
    }

    /// Sorts the column `id` by the heading `by` pressed: the way it runs turned round when it already
    /// goes by it, or else as that heading starts, names from A and sizes and times from the largest.
    pub fn sort_by_heading(&mut self, id: ColumnId, by: Sort, cx: &mut Context<Self>) {
        let order = self.order_for(Some(id));
        let descending = if order.by == by { !order.descending } else { by != Sort::Name };
        self.set_order(Some(id), Order { by, descending }, cx);
    }

    /// Gives the column `target` the order, or makes it the default; a column given the default follows
    /// it from then on. The columns whose order this changes are listed again.
    pub fn set_order(&mut self, target: Option<ColumnId>, order: Order, cx: &mut Context<Self>) {
        if matches!(self.overlays.popup(), Some(Popup::Sort | Popup::ColumnSort(_))) {
            self.overlays.forget_popup();
        }
        let orders = |this: &Self| -> Vec<(ColumnId, Order)> {
            this.state.columns.iter().filter(|c| c.kind != Kind::Home).map(|c| (c.id, this.order_of(c))).collect()
        };
        let before = orders(self);
        match target.and_then(|id| self.state.index_of(id)) {
            None if target.is_none() && order != self.stored.settings().sort => self.update_settings(|s| s.sort = order, cx),
            Some(index) => {
                let c = &self.state.columns[index];
                let own = (order != self.stored.settings().sort).then_some(order);
                if own != self.own_order(c) {
                    let (kind, path) = (c.kind, c.path.clone());
                    self.update_sorts(
                        |sorts| match kind {
                            Kind::Search => sorts.search = own,
                            _ => sorts.set_folder(&path, own),
                        },
                        cx,
                    );
                }
            }
            None => {}
        }
        for ((id, was), (_, is)) in before.into_iter().zip(orders(self)) {
            if was != is {
                self.load(id, 0, true, cx);
            }
        }
        cx.notify();
    }

    // ---- Watching ----

    /// Watches the folders the columns could read, one level deep, as [`Watching::set`] does; a server's
    /// shares, which no change tells of, are read again with F5 only.
    pub(crate) fn watch(&mut self, cx: &mut Context<Self>) -> Option<mpsc::Receiver<()>> {
        let paths: Vec<PathBuf> = self
            .state
            .columns
            .iter()
            .filter(|c| c.kind == Kind::Folder && c.error.is_none() && !colet_core::paths::is_server(&c.path))
            .map(|c| c.path.clone())
            .collect();
        self.loading.watching_mut().set(paths).unwrap_or_else(|error| {
            self.notify(error, Tone::Error, cx);
            None
        })
    }

    /// Says which folders the watcher could list but not watch, each once.
    fn take_missed(&mut self, cx: &mut Context<Self>) {
        let fresh = self.loading.watching_mut().take_missed();
        if !fresh.is_empty() {
            let names: String = fresh.iter().map(|p| format!("「{}」", shown(colet_core::paths::folder_name(p)))).collect();
            self.notify(format!("{names}の変更を監視できません。F5で更新できます。"), Tone::Error, cx);
        }
    }

    /// Takes in what changed, every 200ms: folder changes reload the columns showing them, and what the
    /// watch on Everything heard of the results.
    fn poll(&mut self, cx: &mut Context<Self>) {
        let changed = self.loading.watching().changed.clone();
        let backend = self.backend.clone();
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(200)).await;
                if this.update(cx, |this, cx| this.take_changes(cx)).is_err() {
                    return;
                }
                let batch = changed.lock().unwrap().take();
                if batch != Changes::Nothing && this.update(cx, |this, cx| this.files_changed(batch, cx)).is_err() {
                    return;
                }
                let backend = backend.clone();
                let result = cx.background_executor().spawn(async move { backend.search_changed() }).await;
                let alive = this
                    .update(cx, |this, cx| match result {
                        Ok(true) => this.search_changed(cx),
                        Ok(false) => {}
                        Err(error) => {
                            this.loading.search_failed(error.clone());
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
        if self.operations.take_events() {
            cx.notify();
        }
        if self.operations.clipboard().take_changed() {
            let backend = self.backend.clone();
            let read = cx.background_executor().spawn(async move { backend.clipboard_files() });
            cx.spawn(async move |this, cx| {
                // None while another app holds it: try again next time.
                match read.await {
                    Some(files) => this.update(cx, |this, cx| {
                        this.operations.clipboard_mut().set_files(files);
                        cx.notify();
                    }),
                    None => this.update(cx, |this, _| this.operations.clipboard().read_again()),
                }
                .ok();
            })
            .detach();
        }
    }

    /// Reloads the folder columns whose listings changed.
    fn files_changed(&mut self, changes: Changes, cx: &mut Context<Self>) {
        let paths = match changes {
            Changes::Nothing => return,
            Changes::Paths(paths) => paths,
            Changes::All => {
                self.refresh(cx);
                return;
            }
        };
        let folders: HashSet<OsString> = paths.iter().map(|path| path_key(path)).collect();
        let ids: Vec<ColumnId> =
            self.state.columns.iter().filter(|c| c.kind == Kind::Folder && folders.contains(&path_key(&c.path))).map(|c| c.id).collect();
        for id in ids {
            self.load(id, 0, true, cx);
        }
    }

    /// A busy disk reports changes each second: the first reloads at once, then at most one every couple
    /// of seconds. One met by a reload in flight waits for it, as it may have read the results before.
    fn search_changed(&mut self, cx: &mut Context<Self>) {
        let Some(wait) = self.loading.refresh_wait() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(wait).await;
            loop {
                let busy = this
                    .update(cx, |this, cx| {
                        let Some(search) = this.state.columns.iter().find(|c| c.kind == Kind::Search).map(|c| c.id) else {
                            this.loading.refresh_cancel();
                            return false;
                        };
                        if this.loading.is_loading(search) {
                            return true;
                        }
                        this.loading.refresh_went();
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

    /// What takes the keys for the column at `index`: home's pane, or the columns beside it.
    pub fn column_focus(&self, index: usize) -> &FocusHandle {
        if index == 0 { &self.home_focus } else { &self.columns_focus }
    }

    /// The search field has the keys, or its menu has them for it: no column is marked as worked in.
    pub fn search_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.has_keys(&self.search, window, cx)
    }

    /// The keys are in home's pane or the columns: the column worked in rings its focused row.
    pub fn keys_in_column(&self, window: &Window) -> bool {
        self.home_focus.is_focused(window) || self.columns_focus.is_focused(window)
    }

    /// The keys are in the columns or home, or with a menu, dialog or the history that gives them back
    /// there: the column worked in is framed and its selection lit. Elsewhere, it steps back.
    pub fn columns_marked(&self, window: &Window) -> bool {
        self.keys_in_column(window) || self.keys_go_back_to_columns()
    }

    /// A menu, dialog or the history holds the keys, and closing gives them back to the columns or home,
    /// as [`Colet::restore_keys`] does.
    fn keys_go_back_to_columns(&self) -> bool {
        self.holds_keys() && self.overlays.keys_before().is_none_or(|before| before == self.home_focus || before == self.columns_focus)
    }

    /// A menu, dialog or the history holds the keys, and closing gives them back to `focus`.
    pub(crate) fn keys_go_back_to(&self, focus: &FocusHandle) -> bool {
        self.holds_keys() && self.overlays.keys_before().is_some_and(|before| before == *focus)
    }

    /// A menu, dialog or the history is open, holding the keys.
    fn holds_keys(&self) -> bool {
        self.overlays.is_open() || self.operations.history().open
    }

    /// A menu, dialog or the history opening: where the keys are now is where they go back to as it closes.
    /// One opened from another leaves it as the first found it.
    pub(crate) fn remember_keys(&mut self, window: &Window, cx: &App) {
        if !self.holds_keys() {
            self.overlays.remember_keys(window.focused(cx).as_ref());
        }
    }

    /// The last menu, dialog or history closing: the keys go back to where they were as the first opened.
    /// Gone since, or in a column, they go to the column worked in.
    pub(crate) fn restore_keys(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.overlays.take_keys_before() {
            Some(before) if before != self.home_focus && before != self.columns_focus => before.focus(window, cx),
            _ => self.column_focus(self.state.active).focus(window, cx),
        }
    }

    /// The keys go to the active column: home's pane or the columns beside it.
    pub fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.column_focus(self.state.active).focus(window, cx);
        self.state.enter();
        self.changed(cx);
    }

    /// Into the results, as from a search field in Explorer, or home with nothing searched, picking the
    /// row the keys land on there too unless it already is.
    pub fn leave_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pinned = self.state.pinned();
        self.state.set_active(if pinned { 1 } else { 0 });
        self.focus_active(window, cx);
        let home = &self.state.columns[0];
        if !pinned && let Some(path) = home.focused.clone().filter(|path| !home.selected.contains(path)) {
            self.state.select(0, vec![path]);
            // A saved search picked runs, which the field shows.
            self.sync_search_field(window, cx);
        }
    }

    /// Empties the field; the filter chosen, if any, still shows all it lets through.
    pub fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| input.set_value("", window, cx));
        self.state.set_search("", self.chosen_filter.clone());
        self.changed(cx);
    }

    /// Searches for `query` within `filter`, which becomes the one chosen.
    pub fn search_for(&mut self, query: &str, filter: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |input, cx| {
            input.set_value(query.to_owned(), window, cx);
            input.focus(window, cx);
        });
        self.state.set_search(query, filter);
        self.adopt_filter();
        self.changed(cx);
    }

    pub fn pick(&mut self, index: usize, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.state.select(index, vec![path]);
        self.state.set_active(index);
        self.column_focus(index).focus(window, cx);
        // A place picked in home leaves the search.
        self.sync_search_field(window, cx);
    }

    /// A folder opens beside it, as picking does; a file opens in its app.
    pub fn open(&mut self, index: usize, path: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let Some(entry) = self.state.columns.get(index).and_then(|c| c.entry(path)).cloned() else { return };
        if crate::state::HomeKey::of(&entry.path).is_search() {
            self.pick(index, entry.path, window, cx);
            return;
        }
        if entry.directory {
            self.state.select(index, vec![entry.path]);
            self.changed(cx);
            return;
        }
        let (backend, owner) = (self.backend.clone(), self.owner);
        self.in_background(move || backend.open(&entry.path, owner).map_err(|error| format!("開けません: {error}")), None, cx);
    }

    /// Space: QuickLook shows the row the keys are on, or closes on it. Nowhere for it to go is said, and
    /// only here: the keys moving on say nothing.
    pub fn quick_look(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.state.keys_on().map(Cow::into_owned) else { return };
        self.quick_looked = Some(path.clone());
        let done = self.backend.quick_look(quick_look::Message::Toggle, path);
        let task = cx.background_executor().spawn(async move { done.recv() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |this, cx| match result {
                Ok(Err(quick_look::Error::NotRunning)) => this.notify("QuickLook が起動していません", Tone::Info, cx),
                Ok(Err(quick_look::Error::Failed(error))) => {
                    this.notify(format!("QuickLook で表示できません: {}", message(error.code())), Tone::Error, cx)
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    /// A preview open goes on to the row the keys have gone to; there being none, or a saved search, it
    /// stays as it is. QuickLook takes it only while a preview is open.
    fn follow_quick_look(&mut self) {
        let Some(path) = self.state.keys_on() else { return };
        if self.quick_looked.as_deref() != Some(path.as_ref()) {
            self.quick_looked = Some(path.to_path_buf());
            drop(self.backend.quick_look(quick_look::Message::Switch, path.to_path_buf()));
        }
    }

    /// A crumb takes the keys to its column, bringing it on screen even when it is already active. It only
    /// goes there: nothing in it is picked, as a press in the column picks nothing.
    pub fn go_to(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_active(index);
        self.column_focus(index).focus(window, cx);
        self.changed(cx);
    }

    pub fn step(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        self.state.step(by);
        // Moving in home can run a saved search, which the field shows.
        self.sync_search_field(window, cx);
    }

    pub fn focus_step(&mut self, by: isize, cx: &mut Context<Self>) {
        self.state.focus_step(by);
        self.changed(cx);
    }

    /// Home's pane takes the keys, on the row they were on there, as ← back to it lands.
    pub fn focus_home(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.set_active(0);
        self.focus_active(window, cx);
    }

    pub fn move_column(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(next) = self.state.neighbour(self.state.active, right) {
            self.state.set_active(next);
            self.focus_active(window, cx);
        }
    }

    // ---- Narrowing ----

    /// Each column has its field, holding what it is narrowed to: a field is made for a column new here,
    /// goes with one gone, empties where a new folder or a paste lifted the filter, and takes the narrowing
    /// a favorite or saved search opened with.
    pub fn sync_narrow_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kept: HashSet<ColumnId> = self.state.columns.iter().map(|c| c.id).collect();
        self.narrow_fields.retain(|id, _| kept.contains(id));
        // So does its order's button's stop.
        self.stops
            .get_mut()
            .retain(|key, _| key.strip_prefix("column-sort:").is_none_or(|id| id.parse().is_ok_and(|id| kept.contains(&id))));
        for c in &self.state.columns {
            let id = c.id;
            match self.narrow_fields.get_mut(&id) {
                Some(field) => match &c.narrow {
                    None if !field.applied.is_empty() => {
                        field.applied.clear();
                        field.input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    Some(filter) if filter.text != field.applied => {
                        field.applied = filter.text.clone();
                        field.input.update(cx, |input, cx| input.set_value(filter.text.clone(), window, cx));
                    }
                    _ => {}
                },
                None => {
                    let text = c.narrow.as_ref().map(|f| f.text.clone()).unwrap_or_default();
                    let input = cx.new(|cx| InputState::new(window, cx).placeholder("絞り込む").default_value(text.clone()));
                    self.palette.style_input(&input, cx);
                    let subscription = cx.subscribe_in(&input, window, move |this: &mut Self, input, event, window, cx| match event {
                        InputEvent::Change => {
                            let text = input.read(cx).value().to_string();
                            this.narrow(id, &text, cx);
                        }
                        InputEvent::PressEnter { .. } => this.open_from_narrow_field(id, window, cx),
                        InputEvent::Focus | InputEvent::Blur => cx.notify(),
                    });
                    self.narrow_fields.insert(id, NarrowField { input, applied: text, _changes: subscription });
                }
            }
        }
    }

    /// What is typed in the rows goes on in their column's field, which takes the keys: after what it
    /// holds, as a word of its own. What an IME is composing, `marked` with its caret, is handed on as it
    /// is, so the field takes the rest of it. A space alone starts nothing.
    pub fn type_into_narrowing(&mut self, text: &str, marked: Option<Option<Range<usize>>>, window: &mut Window, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return;
        }
        let id = self.state.columns[self.state.active].id;
        self.sync_narrow_fields(window, cx);
        let Some(field) = self.narrow_fields.get(&id) else { return };
        field.input.clone().update(cx, |input, cx| {
            let held = room_for_a_word(&input.value());
            input.set_value(held, window, cx);
            input.focus(window, cx);
            match marked {
                Some(caret) => input.replace_and_mark_text_in_range(None, text, caret, window, cx),
                None => input.replace_text_in_range(None, text, window, cx),
            }
        });
        // The IME's panel goes on under the field's caret, now the field's to tell.
        window.invalidate_character_coordinates();
        self.changed(cx);
    }

    /// Backspace in the rows takes the last char off their column's field, which takes the keys; false
    /// while it holds none.
    pub fn trim_narrowing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let id = self.state.columns[self.state.active].id;
        let Some(field) = self.narrow_fields.get(&id) else { return false };
        let input = field.input.clone();
        let mut held = input.read(cx).value().to_string();
        if held.pop().is_none() {
            return false;
        }
        input.update(cx, |input, cx| {
            input.set_value(held.clone(), window, cx);
            input.focus(window, cx);
        });
        self.narrow(id, &held, cx);
        true
    }

    /// Narrows the column `id` to the names holding `text`, as it is typed.
    pub fn narrow(&mut self, id: ColumnId, text: &str, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        self.state.set_narrowing(index, Some(text));
        if let Some(field) = self.narrow_fields.get_mut(&id) {
            field.applied = text.into();
        }
        // Everything narrows the results, asked as the search field asks it: once typing pauses, for the
        // text still there.
        if self.state.columns[index].kind == Kind::Search {
            let text = text.to_owned();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                this.update(cx, |this, cx| {
                    let Some(index) = this.state.index_of(id) else { return };
                    // Spaces alone, no words, ask for all as an empty field does.
                    let applied = this.state.columns[index].narrow.as_ref().map_or("", |f| f.text.as_str());
                    if applied == text || applied.is_empty() && narrow_words(&text).is_empty() {
                        this.load(id, 0, true, cx);
                    }
                })
                .ok();
            })
            .detach();
        }
        // The rows move: the focused one is brought in sight again.
        self.scrolls.rows.moved(id);
        self.changed(cx);
    }

    /// From the field into the rows it lets through, landing on the first while none is picked.
    pub fn leave_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        self.state.set_active(index);
        self.focus_active(window, cx);
    }

    /// ↓ or Enter in a field picks the row the keys were on, or the first it lets through, as a click does,
    /// and the keys go there; a row picked already, among others too, is not picked again. With none let
    /// through, the field keeps the keys.
    pub fn open_from_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        let c = &self.state.columns[index];
        let shown = c.focused.clone().filter(|path| c.entry(path).is_some());
        let Some(path) = shown.or_else(|| c.entries.first().map(|e| e.path.clone())) else { return };
        if c.selected.contains(&path) {
            self.state.columns[index].focused = Some(path);
            self.leave_narrow_field(id, window, cx);
        } else {
            self.pick(index, path, window, cx);
        }
    }

    /// Lists all again and empties the field; the keys go back to the rows.
    pub fn clear_narrowing(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        self.state.set_narrowing(index, None);
        if self.state.columns[index].kind == Kind::Search {
            self.load(id, 0, true, cx);
        }
        if let Some(field) = self.narrow_fields.get_mut(&id) {
            field.applied.clear();
            field.input.clone().update(cx, |input, cx| input.set_value("", window, cx));
        }
        self.scrolls.rows.moved(id);
        self.leave_narrow_field(id, window, cx);
    }

    /// Esc in the field: clears what it holds, or, empty, goes back to the rows.
    pub fn escape_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.narrow_fields.get(&id) else { return };
        let input = field.input.clone();
        if input.read(cx).value().is_empty() {
            self.leave_narrow_field(id, window, cx);
        } else {
            input.update(cx, |input, cx| input.set_value("", window, cx));
            self.narrow(id, "", cx);
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

    /// Going back or forward can bring another query, and another filter; the field shows the one, and
    /// the other becomes the one chosen.
    pub(crate) fn sync_search_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.state.query.clone();
        if self.search.read(cx).value().as_ref() != query {
            self.search.update(cx, |input, cx| input.set_value(query, window, cx));
        }
        self.adopt_filter();
        self.changed(cx);
    }

    /// The results shown, a saved search's or those gone back to, choose their filter for the next search.
    fn adopt_filter(&mut self) {
        if self.state.pinned() && self.state.search_filter != self.chosen_filter {
            self.chosen_filter = self.state.search_filter.clone();
        }
    }

    // ---- Scrolling ----

    /// Keeps the focused row of each column in sight, once it is listed.
    pub fn scroll_rows(&mut self) {
        for c in &self.state.columns {
            let Some(path) = c.focused.as_ref().or(c.highlight.as_ref()) else { continue };
            if self.scrolls.rows.scrolled_to.get(&c.id) == Some(path) {
                continue;
            }
            let Some(index) = c.entries.iter().position(|e| &e.path == path) else { continue };
            if c.kind == Kind::Home {
                let mut row = 0;
                for (_, entries) in home_groups(self.stored.settings(), &c.entries) {
                    row += 1; // The heading is a child of the same scroll container.
                    if let Some(at) = entries.iter().position(|e| &e.path == path) {
                        self.scrolls.home.scroll_to_item(row + at);
                        break;
                    }
                    row += entries.len();
                }
            } else {
                let handle = self.scrolls.rows.handle(c.id);
                handle.scroll_to_item(index, ScrollStrategy::Nearest);
            }
            self.scrolls.rows.scrolled_to.insert(c.id, path.clone());
        }
    }

    pub fn list_handle(&mut self, id: ColumnId) -> UniformListScrollHandle {
        self.scrolls.rows.handle(id).clone()
    }
}

/// The rows take text as a field does, though they hold none: what is typed there, and what an IME
/// composes, goes on in their column's field. Taking text keeps the IME on while the keys are in them.
impl EntityInputHandler for Colet {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        None
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {}

    // A paste in the rows pastes files, from its keys.
    fn paste(&mut self, _: ClipboardItem, _: &mut Window, _: &mut Context<Self>) {}

    // Space with the IME on comes here as the space it types, not as the key: QuickLook all the same.
    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if !text.is_empty() && text.trim().is_empty() {
            self.quick_look(cx);
            return;
        }
        self.type_into_narrowing(text, None, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        caret: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.type_into_narrowing(text, Some(caret), window, cx);
    }

    // Where the IME shows what it composes: at the end of the field it goes on in, the column's own, not
    // the one it was last shown at.
    fn bounds_for_range(
        &mut self,
        _: Range<usize>,
        _: Bounds<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let id = self.state.columns[self.state.active].id;
        let input = self.narrow_fields.get(&id)?.input.clone();
        input.update(cx, |input, cx| {
            let end = input.value().encode_utf16().count();
            input.bounds_for_range(end..end, input.text_bounds()?, window, cx)
        })
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}
