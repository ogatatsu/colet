//! The window's one view: it keeps the navigation state and takes the keys. `view.rs` draws it; what the
//! columns list and the watch on the open folders are `loading.rs`'s, and the columns' fields `narrow_fields.rs`'s.
use std::{
    borrow::Cow,
    collections::HashSet,
    ops::Range,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use colet_core::{quick_look, shell};
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::words;
use crate::{
    address::AddressBar,
    backend,
    crumbs::Trail,
    dialogs::Dialog,
    history::HistoryPanel,
    home::{HomeDragging, HomePane, home_groups},
    icons::Icons,
    kept_file::{File, Loaded, Saves},
    loading::Loading,
    metrics::Scale,
    narrow_fields::NarrowFields,
    notices::{Details, Notices, Tone},
    ops::Operations,
    overlays::Overlays,
    scrolls::Scrolls,
    search_bar::SearchBar,
    settings::{self, Settings},
    sorts::Sorts,
    state::{ColumnId, Kind, Showing, State},
    stops::{Stop, Stops},
    stored::Stored,
    tabs::{TabStrip, Tabs},
    theme::ActivePalette,
    wake::Wake,
    watching::Watching,
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
        ToggleShortcuts,
    ]
);

/// Where an open menu takes the keys.
pub const MENU: &str = "Menu";
/// A button that opens its menu from the keys: a column's sort order's, the search filters'.
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
        KeyBinding::new("ctrl-/", ToggleShortcuts, None),
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

    /// A drag on the edge of what `of` names starts at `x`, from `start_width`: the width it shows,
    /// which a narrow window may have narrowed.
    pub fn start_resize(&mut self, of: WidthOf, x: Pixels, start_width: f32) {
        self.resizing = Some(Resize { of, start_x: x, start_width });
    }

    /// Which width a drag under way changes.
    pub fn resizing(&self) -> Option<WidthOf> {
        self.resizing.as_ref().map(|resize| resize.of)
    }

    /// The drag under way reached `x`, the window drawn at `scale`: its width follows, within its bounds.
    /// False with none under way.
    pub fn drag_to(&mut self, x: Pixels, scale: Scale) -> bool {
        let Some(resize) = &self.resizing else { return false };
        let (min, max) = match resize.of {
            WidthOf::Sidebar => SIDEBAR_WIDTHS,
            WidthOf::Field => SEARCH_FIELD_WIDTHS,
            WidthOf::Search | WidthOf::Column => COLUMN_WIDTHS,
        };
        // Display scaling and the zoom make the deltas fractional; widths stay whole pixels at 100%.
        let width = (resize.start_width + scale.down(f32::from(x - resize.start_x))).clamp(min, max).round();
        match resize.of {
            WidthOf::Sidebar => self.sidebar = width,
            WidthOf::Search => self.search = width,
            WidthOf::Column => self.column = width,
            WidthOf::Field => self.field = width,
        }
        true
    }

    /// The drag under way is over; false with none under way.
    pub fn end_resize(&mut self) -> bool {
        self.resizing.take().is_some()
    }
}

/// At its least a column keeps some of a name beside a row's size and modified time.
pub const COLUMN_WIDTHS: (f32, f32) = (260., 960.);
pub const SIDEBAR_WIDTHS: (f32, f32) = (160., 480.);
/// The strip that brings out the folders folded away above a search result: room for two digits.
pub const FOLD_WIDTH: f32 = 28.;
/// What home's pane leaves beside it, narrowing in a narrow window to do so: the toolbar with the search
/// field at its least, the trail's buttons, and the pinned results with a fold strip.
pub const MAIN_MIN: f32 = 480.;
/// The window's size the first time it opens, centered on the screen.
pub const FIRST_WINDOW: (f32, f32) = (1200., 780.);
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
pub const ADDRESS_GUESS: f32 = 274.;

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

/// A file kept between starts as it was `read`, what to say of what of it could not be, and its saves
/// through `write`: none while the file was not all read and is still there, so it is never written over.
fn kept<T: Default + Clone + Send + 'static>(
    read: Loaded<T>,
    file: &File,
    write: impl Fn(&T) -> Result<(), String> + Send + Sync + 'static,
) -> (T, Saves<T>, Option<String>) {
    let still_there = read.problem.as_ref().is_some_and(|problem| problem.kept);
    (read.value, Saves::new(file, (!still_there).then_some(write)), read.problem.map(|problem| problem.message))
}

pub struct Colet {
    /// The tab in front's; the other tabs are parked in `tabs`, with the fields below kept for each.
    pub state: State,
    pub tabs: Tabs,
    /// The strip the tabs are drawn on: its own entity, which keeps where it is scrolled and the tab carried
    /// along it, and which Colet hears from.
    pub tab_strip: Entity<TabStrip>,
    pub widths: Widths,
    /// The search field and the filter chosen for it, which the tab in front's next search is narrowed by:
    /// its own entity, which Colet fills and hears from.
    pub search: Entity<SearchBar>,
    /// The address bar's field and the asking whether what is typed there is: its own entity, which Colet
    /// opens and closes and hears from.
    pub address: Entity<AddressBar>,
    /// The field of each column, narrowing it to some names.
    pub narrow_fields: NarrowFields,
    pub home_focus: FocusHandle,
    pub columns_focus: FocusHandle,
    pub icons: Icons,
    /// The notices up in the window.
    pub(crate) notices: Notices,
    /// Where the tab in front is scrolled.
    pub scrolls: Scrolls,
    pub(crate) owner: shell::Owner,
    pub(crate) backend: backend::Shared,
    /// The listings out, the search's reloads and the search engine: its own entity, which Colet says what to
    /// list and hears the answers from.
    pub loading: Entity<Loading>,
    /// The watch on the folders the columns show.
    pub watching: Watching,
    /// The settings and the columns' own orders, as kept between starts.
    pub stored: Stored,
    /// The drag putting home's rows in order: its own entity, which keeps what is carried and where it would
    /// land, and which Colet observes.
    pub home_drag: Entity<HomeDragging>,
    /// How wide home's pane is drawn, out of its strip or folded into it.
    pub home_pane: HomePane,
    /// The row the left button is down on, until it is let go of or a drag out starts.
    pub(crate) press: Option<crate::drag_out::Press>,
    /// The buttons Tab stops at, each with the focus it keeps across frames.
    pub(crate) stops: Stops,
    /// The dialog and the menu open, and where the keys go back to as they close.
    pub overlays: Overlays,
    pub crumbs: Trail,
    /// Where the menu key's menu drops from, as the last frame drew it.
    pub key_menu: crate::view::KeyMenuAnchor,
    /// The file operations, the history and the clipboard: their own entity, which Colet starts and ends
    /// operations on and hears from.
    pub operations: Entity<Operations>,
    /// The history of the operations as the window shows it: its own entity, which Colet opens and closes
    /// and hears from.
    pub history: Entity<HistoryPanel>,
    /// What QuickLook was last told to show: a preview open follows the keys from it.
    quick_looked: Option<PathBuf>,
    /// Brings the window round to take in what changed off its thread.
    pub(crate) wake: Wake,
    _subscriptions: Vec<Subscription>,
}

impl Colet {
    /// `settings` and `sorts` as read at start; an error is said once the window is up, and the defaults
    /// used. A file still there unread is never written over.
    pub fn new(
        backend: backend::Shared,
        settings: Loaded<Settings>,
        sorts: Loaded<Sorts>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Spoken before anything says a word, the fields' placeholders among them.
        words::speak(settings.value.language.spoken(backend.windows_speaks()));
        let owner = match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
            Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
            _ => 0,
        };
        shell::refuse_drops(owner);
        let search = cx.new(|cx| SearchBar::new(window, cx));
        let address = cx.new(|cx| AddressBar::new(backend.clone(), owner, window, cx));
        let mut subscriptions = vec![
            // The ring around the field and the filter's button follow it.
            cx.observe(&search, |_, _, cx| cx.notify()),
            cx.subscribe_in(&search, window, |this, _, event, window, cx| this.on_search_event(event, window, cx)),
            // The field opening and closing redraws the trail in its place.
            cx.observe(&address, |_, _, cx| cx.notify()),
            cx.subscribe_in(&address, window, |this, _, event, window, cx| this.on_address_event(event, window, cx)),
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
        let (wake, woken) = crate::wake::wake();
        // Told from the watch's thread; asked of the search nothing holds yet, so it never waits.
        let told = wake.clone();
        backend.tell_search(Arc::new(move || told.wake()));
        let operations = cx.new(|cx| Operations::new(backend.clone(), owner, wake.clone(), cx));
        // The history's button, the rows' clipboard marks and the menu's Send To are drawn from it.
        subscriptions.push(cx.observe(&operations, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe_in(&operations, window, |this, _, event, window, cx| this.on_operations_event(event, window, cx)));
        // The status bar draws how the search engine answers from it.
        let loading = cx.new(|_| Loading::new(backend.clone(), owner));
        subscriptions.push(cx.observe(&loading, |_, _, cx| cx.notify()));
        subscriptions.push(cx.subscribe(&loading, |this, _, event, cx| this.on_loading_event(event, cx)));
        let tab_strip = cx.new(|_| TabStrip::default());
        subscriptions.push(cx.subscribe_in(&tab_strip, window, |this, _, event, window, cx| this.on_tab_strip_event(event, window, cx)));
        let history = cx.new(|cx| HistoryPanel::new(operations.clone(), backend.clone(), cx));
        subscriptions.push(cx.subscribe_in(&history, window, |this, _, event, window, cx| this.on_history_event(event, window, cx)));
        // Home's rows are drawn from it: the one carried fades, and a line shows where it would land.
        let home_drag = cx.new(|_| HomeDragging::default());
        subscriptions.push(cx.observe(&home_drag, |_, _, cx| cx.notify()));
        let mut this = Self {
            state: State::new(),
            tabs: Tabs::default(),
            tab_strip,
            widths: Widths::new(settings.sidebar_width, settings.search_width, settings.column_width, settings.search_field_width),
            search,
            address,
            narrow_fields: NarrowFields::default(),
            home_focus: cx.focus_handle().tab_stop(true),
            columns_focus: cx.focus_handle().tab_stop(true),
            icons: Icons::default(),
            notices: Notices::new(Window::window_handle(window)),
            scrolls: Scrolls::default(),
            owner,
            backend,
            loading,
            watching: Watching::default(),
            stored: Stored::new(settings, settings_saves, sorts, sorts_saves),
            home_drag,
            home_pane: HomePane::default(),
            press: None,
            stops: Stops::default(),
            crumbs: Trail::default(),
            overlays: Overlays::new(cx),
            key_menu: Default::default(),
            operations,
            history,
            quick_looked: None,
            wake,
            _subscriptions: subscriptions,
        };
        for problem in [problem, sorts_problem].into_iter().flatten() {
            this.notify(problem, Tone::Error, cx);
        }
        // The library draws in the theme kept before anything takes its colors from it.
        crate::theme::apply(window, this.stored.settings().theme, cx);
        cx.palette().style_input(&this.search_field(cx), cx);
        let address = this.address.read(cx).field().clone();
        cx.palette().style_input(&address, cx);
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
        let address = this.address.read(cx).field().read(cx).focus_handle(cx);
        this._subscriptions.push(cx.on_focus_out(&address, window, |this, _, window, cx| this.follow_address(window, cx)));
        // Nor is it told of the keys leaving it in the click that brings the window back to the front. That click
        // lands before the next frame, so the keys are looked at again once it is drawn.
        this._subscriptions.push(cx.observe_window_activation(window, |_, window, cx| {
            if window.is_window_active() {
                cx.on_next_frame(window, |this, window, cx| {
                    if window.is_window_active() {
                        this.follow_address(window, cx);
                    }
                });
            }
        }));
        this.sync_narrow_fields(window, cx);
        this.scroll_rows();
        crate::theme::set_zoom(this.stored.settings().zoom, cx);
        this.focus_search(window, cx);
        this.sync(cx);
        // Once at start, to read the clipboard.
        this.wake.wake();
        this.listen(woken, cx);
        this
    }

    /// Whether the window may close: not while a file operation or a send runs. The settings are written at once,
    /// rather than a moment after the last change; when that fails, the next close goes without them.
    pub(crate) fn may_close(&mut self, cx: &mut Context<Self>) -> bool {
        if self.operations.read(cx).is_running() {
            self.notify(words::window::busy_operating(), Tone::Info, cx);
            return false;
        }
        if self.operations.read(cx).is_sending() {
            self.notify(words::window::busy_sending(), Tone::Info, cx);
            return false;
        }
        if self.stored.is_unsaved_close() {
            return true;
        }
        match self.flush_settings() {
            Ok(()) => true,
            Err(error) => {
                self.stored.give_up_saving_on_close();
                self.notify(words::window::close_without_settings(error), Tone::Error, cx);
                false
            }
        }
    }

    /// Takes the colors the theme now gives, in the fields too, the dialog's among them.
    pub fn apply_theme(&mut self, window: &Window, cx: &mut Context<Self>) {
        crate::theme::apply(window, self.stored.settings().theme, cx);
        let mut inputs = vec![self.search_field(cx), self.address.read(cx).field().clone()];
        inputs.extend(self.narrow_fields.values().map(|field| field.input.clone()));
        inputs.extend(self.overlays.dialog().into_iter().flat_map(Dialog::fields));
        for input in &inputs {
            cx.palette().style_input(input, cx);
        }
        cx.notify();
    }

    /// The focus a button keeps, a stop for Tab; Enter or Space presses it.
    pub fn stop(&self, stop: impl Into<Stop>, cx: &App) -> FocusHandle {
        self.stops.get_or_make(stop.into(), cx)
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
                            let keys = this.overlays.menu_has_keys(window, cx);
                            this.close_menus(keys, window, cx);
                        }
                    })
                    .ok();
                });
            });
        }
        self.loading.update(cx, |loading, _| loading.keep(&live));
        self.stop_search_out_of_sight(cx);
        self.scrolls.rows.keep(&live);
        let fresh: Vec<(ColumnId, Kind)> = self
            .loading
            .update(cx, |loading, _| self.state.columns.iter().filter(|c| loading.first(c.id)).map(|c| (c.id, c.kind)).collect());
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
        let colet = cx.weak_entity();
        // From the keys, they go on into the history.
        let details: Details = Box::new(move |event, window, cx| {
            colet
                .update(cx, |this, cx| {
                    this.show_history(true, Some(entry), window, cx);
                    if event.is_keyboard() {
                        this.history.read(cx).entry_focus(entry, cx).focus(window, cx);
                    }
                })
                .ok();
        });
        self.notices.push(text.into(), Tone::Error, Some(details), cx);
    }

    /// Puts a notice up in the window, as [`Notices::push`] does.
    pub fn notify(&mut self, text: impl Into<SharedString>, tone: Tone, cx: &mut Context<Self>) {
        self.notices.push(text.into(), tone, None, cx);
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

    // ---- Hidden items ----

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

    // ---- Keys and picks ----

    /// What takes the keys for the column at `index`: home's pane, or the columns beside it.
    pub fn column_focus(&self, index: usize) -> &FocusHandle {
        if index == 0 { &self.home_focus } else { &self.columns_focus }
    }

    /// The keys are in home's pane or the columns: the column worked in rings its focused row.
    pub fn keys_in_column(&self, window: &Window) -> bool {
        self.home_focus.is_focused(window) || self.columns_focus.is_focused(window)
    }

    /// The keys are in the columns or home, or with a menu, dialog or the history that gives them back
    /// there: the column worked in is framed and its selection lit. Elsewhere, it steps back.
    pub fn columns_marked(&self, window: &Window) -> bool {
        // Closing gives the keys back to the columns or home, as `restore_keys` does; there too when where
        // they were is gone.
        self.keys_in_column(window) || self.overlays.keys_go_back_to_any(&[&self.home_focus, &self.columns_focus])
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

    /// Esc from a field, a tab back in front, or a button pressed with the keys on it: the keys go back to
    /// the active column, picking nothing there that was not; with no row they were on, they land on none.
    pub fn come_back_to_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.column_focus(self.state.active).focus(window, cx);
        self.state.come_back();
        self.changed(cx);
    }

    /// ↓ from the search or address field: the keys go to the active column. Home standing alone with
    /// nothing picked picks the row they were on, or its first, a saved search running; with anything open
    /// beside it, that is left be.
    pub fn down_into_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_active(window, cx);
        let home = &self.state.columns[0];
        let lands_on =
            home.focused.clone().filter(|path| home.entry(path).is_some()).or_else(|| home.entries.first().map(|e| e.path.clone()));
        if self.state.columns.len() == 1
            && home.selected.is_empty()
            && let Some(path) = lands_on
        {
            self.state.select(0, vec![path]);
            // A saved search picked runs, which the field shows.
            self.sync_search_field(window, cx);
        }
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
        self.in_background(
            move || backend.open(&entry.path, owner).map_err(|e| words::ops::cannot_open(words::windows_failure(e.code()))),
            None,
            cx,
        );
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
                Ok(Err(quick_look::Error::NotRunning)) => this.notify(words::window::quick_look_not_running(), Tone::Info, cx),
                Ok(Err(quick_look::Error::Unnamable)) => this.notify(words::window::quick_look_unnamable(), Tone::Info, cx),
                Ok(Err(quick_look::Error::Failed(error))) => {
                    this.notify(words::window::quick_look_failed(words::windows_failure(error.code())), Tone::Error, cx)
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

    /// Goes to the column `id`, wherever it stands now; nothing if it has closed since it was pointed at.
    pub fn go_to_column(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.state.index_of(id) {
            self.go_to(index, window, cx);
        }
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

    pub fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.back();
        self.sync_search_field(window, cx);
    }

    pub fn forward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.state.forward();
        self.sync_search_field(window, cx);
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
