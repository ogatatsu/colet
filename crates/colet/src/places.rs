//! The places Colet keeps for the user: favorites and saved searches in home, the widths and the window,
//! which Everything to ask; and the dialogs that edit them.
use std::{
    cell::RefCell,
    ffi::OsString,
    path::{Path, PathBuf},
    rc::Rc,
    sync::mpsc::Receiver,
    time::Duration,
};

use colet_core::{
    entry::Entry,
    paths::{child, folder_name, path_key, shown},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::component::WindowExt;
use gpui_kit::*;

use crate::{
    app::{Colet, Engine, Tone, WidthOf},
    search_filters::FilterEdit,
    settings::{Favorite, Placement, SavedSearch, Settings, Theme, written},
    sorts::Sorts,
    state::{HomeKey, Kind, narrowing},
};

pub enum Dialog {
    /// A rename's new name, or a new folder's: the request it sends, its name taken from `input`. `taken`: the
    /// name tried last that the rename's folder already has. `checking`: the name tried whose folder is being
    /// asked, off the UI thread, whether it has it already; typing lets go of it.
    Name {
        request: colet_core::operations::Request,
        input: Entity<InputState>,
        tried: bool,
        taken: Option<String>,
        checking: Option<String>,
    },
    /// A saved search's name, query, filter and narrowing, found by those it had as the dialog opened.
    EditSearch {
        original: SavedSearch,
        name: Entity<InputState>,
        query: Entity<InputState>,
        filter: Option<String>,
        narrow: Entity<InputState>,
    },
    /// A favorite's name and narrowing, found by those it had as the dialog opened; its folder stays.
    EditFavorite { original: Favorite, name: Entity<InputState>, narrow: Entity<InputState> },
    /// The search filters, to add to, put in order, edit or delete: one at a time, in `edit`, or the one
    /// asked about before it goes.
    Filters { edit: Option<FilterEdit>, deleting: Option<String> },
    /// The theme, the connection to Everything, and the keys.
    Preferences,
    /// The files to open together, asked about first when they are many.
    Open { paths: Vec<PathBuf> },
}

impl Dialog {
    /// Its text fields, in their order.
    pub fn fields(&self) -> Vec<Entity<InputState>> {
        match self {
            Dialog::Name { input, .. } => vec![input.clone()],
            Dialog::EditSearch { name, query, narrow, .. } => vec![name.clone(), query.clone(), narrow.clone()],
            Dialog::EditFavorite { name, narrow, .. } => vec![name.clone(), narrow.clone()],
            Dialog::Filters { edit: Some(edit), .. } => vec![edit.name.clone(), edit.query.clone()],
            Dialog::Filters { .. } | Dialog::Preferences | Dialog::Open { .. } => Vec::new(),
        }
    }
}

/// The places home lists: the drives in letter order, then the other folders Everything's index starts
/// from, by path and named by it. An indexed volume, `C:`, is the drive `C:\`.
pub fn places(drives: Vec<Entry>, indexed: Vec<PathBuf>) -> Vec<Entry> {
    let mut seen: std::collections::HashSet<OsString> = drives.iter().map(|e| path_key(&e.path)).collect();
    let mut others: Vec<PathBuf> = indexed
        .into_iter()
        .map(|path| {
            let volume = path.as_os_str().len() == 2 && path.as_os_str().as_encoded_bytes().ends_with(b":");
            if volume { child(&path, "") } else { path }
        })
        .filter(|path| seen.insert(path_key(path)))
        .collect();
    // In lower case, which puts a server (`\\`) before a drive's letters.
    others.sort_by_cached_key(|path| {
        let mut key = path_key(path);
        key.make_ascii_lowercase();
        key
    });
    drives.into_iter().chain(others.into_iter().map(|path| Entry::folder(&path, &shown(&path)))).collect()
}

/// Home's rows: the pinned in their order, then the favorites, the saved searches and the places, each
/// place once. A pin whose row is not there, a drive unplugged, waits for it unseen.
pub fn home_entries(settings: &Settings, drives: Vec<Entry>) -> Vec<Entry> {
    let favorites = settings.favorites.iter().map(|f| Entry::folder(f.key(), &f.label()));
    let searches = settings.saved_searches.iter().map(|s| Entry { directory: false, ..Entry::folder(s.key(), s.label()) });
    let mut seen = std::collections::HashSet::new();
    let mut rest: Vec<Entry> = favorites.chain(searches).chain(drives).filter(|e| seen.insert(HomeKey::of(&e.path).pin())).collect();
    let mut pinned = Vec::new();
    for pin in settings.pinned.iter().map(|pin| HomeKey::of(pin).pin()) {
        if let Some(at) = rest.iter().position(|e| HomeKey::of(&e.path).pin() == pin) {
            pinned.push(rest.remove(at));
        }
    }
    pinned.extend(rest);
    pinned
}

/// Home's headings and their rows, shared by drawing and scrolling to the focused row.
pub fn home_groups<'a>(settings: &Settings, entries: &'a [Entry]) -> Vec<(&'static str, Vec<&'a Entry>)> {
    let pinned = |e: &&Entry| settings.is_pinned(&e.path);
    let favorite = |e: &&Entry| !pinned(e) && settings.is_favorite(&e.path);
    let search = |e: &&Entry| !pinned(e) && HomeKey::of(&e.path).is_search();
    [
        ("ピン留め", entries.iter().filter(pinned).collect::<Vec<_>>()),
        ("お気に入り", entries.iter().filter(favorite).collect()),
        ("保存した検索", entries.iter().filter(search).collect()),
        ("場所", entries.iter().filter(|e| !pinned(e) && !favorite(e) && !search(e)).collect()),
    ]
    .into_iter()
    .filter(|(_, rows)| !rows.is_empty())
    .collect()
}

/// The kinds of home's rows that are put in order by dragging, each among its own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Shelf {
    Pinned,
    Favorite,
    Search,
}

/// A pinned row, favorite or saved search dragged in home, to a new place among its own kind.
#[derive(Clone)]
pub struct HomeDrag {
    pub path: PathBuf,
    pub shelf: Shelf,
}

/// Where a drag in home would put what it carries: before the row `before`, or after the last of its kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Landing {
    pub before: Option<PathBuf>,
}

/// Where each of home's favorites and saved searches was drawn, by its path.
pub type HomeRows = Rc<RefCell<Vec<(PathBuf, Bounds<Pixels>)>>>;

/// A drag in home, putting a favorite, saved search or pinned row in order among its kind.
#[derive(Default)]
pub struct HomeDragging {
    /// What is being dragged; none while no drag is under way.
    pub carried: Option<HomeDrag>,
    /// Where it would land; none while outside its own kind.
    pub landing: Option<Landing>,
    /// Where the pointer last was, and home's list then: held at an edge, it scrolls on.
    pointer: Option<(Point<Pixels>, Bounds<Pixels>)>,
    /// Home's list is scrolling toward the edge the drag is held at.
    scrolling: bool,
    /// Where each of home's favorites and saved searches was drawn, for a drag to find its place among them.
    pub rows: HomeRows,
    /// Where each of home's headings was drawn, top to bottom: a kind reaches from its own to the next.
    pub headings: Rc<RefCell<Vec<Bounds<Pixels>>>>,
}

impl HomeDragging {
    /// Where the drag would land: between two rows of its own kind, by the half of the row it is over.
    /// Its kind reaches from its heading to the next kind's, or the list's end: over its heading it goes
    /// first, below its last row last. Nowhere outside that. True when that moved.
    fn aim(&mut self, settings: &Settings) -> bool {
        let (Some(drag), Some((at, list))) = (&self.carried, self.pointer) else { return false };
        let landing = {
            let rows = self.rows.borrow();
            let headings = self.headings.borrow();
            let own: Vec<_> = rows.iter().filter(|(path, _)| movable(settings, path) == Some(drag.shelf)).collect();
            let reach = own.first().zip(own.last()).map(|(first, last)| {
                let top = headings.iter().rev().map(|h| h.top()).find(|&top| top <= first.1.top()).unwrap_or(first.1.top());
                let bottom = headings.iter().map(|h| h.top()).find(|&top| top >= last.1.bottom()).unwrap_or(list.bottom());
                (top, bottom)
            });
            match reach {
                // Above its first row it lands before it; below its last, after it.
                Some((top, bottom)) if list.contains(&at) && at.y >= top && at.y < bottom => {
                    let over = own.iter().position(|(_, bounds)| at.y < bounds.bottom()).unwrap_or(own.len() - 1);
                    let (path, bounds) = own[over];
                    let before =
                        if at.y < bounds.center().y { Some(path.clone()) } else { own.get(over + 1).map(|(path, _)| path.clone()) };
                    Some(Landing { before })
                }
                _ => None,
            }
        };
        let moved = self.landing != landing;
        self.landing = landing;
        moved
    }

    /// The way home's list scrolls while the drag is held at its edge.
    fn edge(&self) -> Option<f32> {
        self.carried.as_ref().and(self.pointer).and_then(|(at, list)| drag_edge(at, list))
    }

    /// Over, dropped or not: nothing has moved.
    fn end(&mut self) {
        self.carried = None;
        self.landing = None;
        self.pointer = None;
    }
}

/// Which of home's kinds that are put in order by dragging `path` is: pinned, a saved search, a favorite,
/// or none, as a place is.
pub fn movable(settings: &Settings, path: &Path) -> Option<Shelf> {
    if settings.is_pinned(path) {
        Some(Shelf::Pinned)
    } else if HomeKey::of(path).is_search() {
        Some(Shelf::Search)
    } else {
        settings.is_favorite(path).then_some(Shelf::Favorite)
    }
}

/// Moves the item `is` finds by `path` to just before the one it finds by `before`, or to the end.
fn move_before<T>(items: &mut Vec<T>, is: impl Fn(&T, &Path) -> bool, path: &Path, before: Option<&Path>) {
    let Some(from) = items.iter().position(|item| is(item, path)) else { return };
    if before.is_some_and(|before| is(&items[from], before)) {
        return;
    }
    let item = items.remove(from);
    let to = before.and_then(|before| items.iter().position(|item| is(item, before))).unwrap_or(items.len());
    items.insert(to, item);
}

/// How near the top or bottom of home's list a drag scrolls it.
const DRAG_EDGE: f32 = 24.;

/// Which way home's list scrolls with a drag held at `at`: up at its top edge, down at its bottom.
fn drag_edge(at: Point<Pixels>, list: Bounds<Pixels>) -> Option<f32> {
    if at.x < list.left() || at.x >= list.right() {
        None
    } else if at.y < list.top() + px(DRAG_EDGE) {
        Some(-1.)
    } else if at.y >= list.bottom() - px(DRAG_EDGE) {
        Some(1.)
    } else {
        None
    }
}

/// Where the window opens again: on the display holding the middle of where it was left, which the window
/// must name or it opens on the primary one; otherwise centered on `primary`, as after that display was
/// unplugged. Either way it is kept within the display it opens on, and no smaller than the window goes.
pub fn restore_bounds(
    placement: Placement,
    displays: &[(DisplayId, Bounds<Pixels>)],
    primary: Option<DisplayId>,
) -> (Option<DisplayId>, Bounds<Pixels>) {
    let saved = Bounds::new(point(px(placement.x), px(placement.y)), size(px(placement.width), px(placement.height)));
    let held = displays.iter().find(|(_, d)| d.contains(&saved.center()));
    let Some(&(id, display)) = held.or_else(|| displays.iter().find(|(id, _)| Some(*id) == primary)).or(displays.first()) else {
        return (None, saved);
    };
    let (min_width, min_height) = crate::app::MIN_WINDOW;
    let size =
        size(saved.size.width.max(px(min_width)).min(display.size.width), saved.size.height.max(px(min_height)).min(display.size.height));
    let origin = if held.is_some() {
        point(
            saved.origin.x.clamp(display.left(), display.right() - size.width),
            saved.origin.y.clamp(display.top(), display.bottom() - size.height),
        )
    } else {
        display.center() - point(size.width / 2., size.height / 2.)
    };
    (Some(id), Bounds::new(origin, size))
}

impl Colet {
    /// Changes the settings and writes them a moment later, once a burst of changes (a drag) is over;
    /// never over a file that could not be read.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let listed = |s: &Settings| (s.favorites.clone(), s.saved_searches.clone(), s.pinned.clone());
        let before = listed(&self.settings);
        change(&mut self.settings);
        self.settings.normalize();
        if before != listed(&self.settings) {
            // Home lists them.
            let home = self.state.columns[0].id;
            self.load(home, 0, true, cx);
        }
        cx.notify();
        if let Some(change) = self.settings_saves.changed() {
            Self::save_later(cx, move |this| this.settings_saves.save_if_last(change, &this.settings));
        }
    }

    /// Changes the orders the columns are given apart from the default, and writes them as the settings
    /// are written.
    pub fn update_sorts(&mut self, change: impl FnOnce(&mut Sorts), cx: &mut Context<Self>) {
        change(&mut self.sorts);
        cx.notify();
        if let Some(change) = self.sorts_saves.changed() {
            Self::save_later(cx, move |this| this.sorts_saves.save_if_last(change, &this.sorts));
        }
    }

    /// Sends the save `queue` makes a moment later, once a burst of changes (a drag) is over, and says
    /// if it fails; `queue` sends none when a later change has its own coming.
    fn save_later(cx: &mut Context<Self>, queue: impl FnOnce(&Self) -> Option<Receiver<Result<(), String>>> + 'static) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let Some(queued) = this.update(cx, |this, _| queue(this)).ok().flatten() else { return };
            let saved = cx.background_executor().spawn(async move { written(queued) }).await;
            if let Err(error) = saved {
                this.update(cx, |this, cx| this.notify(error, Tone::Error, cx)).ok();
            }
        })
        .detach();
    }

    /// Writes the settings and the columns' orders now, as the window closes, once the saves before are
    /// written; the saves still waiting out a burst are dropped.
    pub fn flush_settings(&mut self) -> Result<(), String> {
        let settings = self.settings_saves.save_now(&self.settings);
        let sorts = self.sorts_saves.save_now(&self.sorts);
        let settings = settings.map_or(Ok(()), written);
        let sorts = sorts.map_or(Ok(()), written);
        settings.and(sorts)
    }

    pub fn remember_window(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (bounds, maximized) = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => (bounds, false),
            WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
        };
        let placement = Placement {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            maximized,
        };
        if self.settings.window != Some(placement) {
            self.update_settings(|s| s.window = Some(placement), cx);
        }
    }

    /// A drag on an edge is over: keep the width it left.
    pub fn end_resize(&mut self, cx: &mut Context<Self>) {
        let Some(resize) = self.resizing.take() else { return };
        let widths = (self.widths.sidebar, self.widths.search, self.widths.column, self.widths.field);
        self.update_settings(
            |s| match resize.of {
                WidthOf::Sidebar => s.sidebar_width = widths.0,
                WidthOf::Search => s.search_width = widths.1,
                WidthOf::Column => s.column_width = widths.2,
                WidthOf::Field => s.search_field_width = widths.3,
            },
            cx,
        );
    }

    /// Keeps or lets go of the favorite `key` names: a folder's path, or a narrowed favorite's key.
    pub fn toggle_favorite(&mut self, key: &Path, cx: &mut Context<Self>) {
        let HomeKey::Folder { path, narrow } = HomeKey::of(key) else { return };
        let (path, narrow) = (path.into_owned(), narrow.map(std::borrow::Cow::into_owned));
        let found = self.settings.favorite(&path, narrow.as_deref()).cloned();
        self.update_settings(
            |s| match &found {
                Some(found) => {
                    // Its pin goes with it.
                    let key = found.home_key().pin();
                    s.favorites.retain(|f| !f.is(&path, narrow.as_deref()));
                    s.pinned.retain(|pin| HomeKey::of(pin).pin() != key);
                }
                None => s.favorites.push(Favorite { path: path.clone(), name: String::new(), narrow: narrow.clone() }),
            },
            cx,
        );
        // The name given to it, or the narrowing, goes too; say so.
        if let Some(found) = found.filter(|f| !f.name.is_empty() || f.narrow.is_some()) {
            self.notify(format!("「{}」をお気に入りから外しました", found.label()), Tone::Info, cx);
        }
    }

    /// The favorite a folder column's star and Ctrl+D keep: its folder, narrowed as it is now.
    pub fn favorite_key_of(&self, index: usize) -> PathBuf {
        let c = &self.state.columns[index];
        HomeKey::favorite(&c.path, c.narrowing().as_deref()).key()
    }

    /// Pins a row of home at the end of the pinned ones, or puts it back among its own kind.
    pub fn toggle_pin(&mut self, path: &Path, cx: &mut Context<Self>) {
        let path = path.to_owned();
        self.update_settings(
            |s| {
                if s.is_pinned(&path) {
                    let key = HomeKey::of(&path).pin();
                    s.pinned.retain(|pin| HomeKey::of(pin).pin() != key);
                } else {
                    s.pinned.push(path);
                }
            },
            cx,
        );
    }

    /// A favorite or saved search is picked up in home: its row fades where it was.
    pub fn start_home_drag(&mut self, drag: &HomeDrag, cx: &mut Context<Self>) {
        self.home_drag.carried = Some(drag.clone());
        self.home_drag.landing = None;
        cx.notify();
    }

    /// The pointer moved in a drag in home, with home's list drawn at `list`.
    pub fn drag_home(&mut self, at: Point<Pixels>, list: Bounds<Pixels>, cx: &mut Context<Self>) {
        if self.home_drag.carried.is_none() {
            return;
        }
        self.home_drag.pointer = Some((at, list));
        self.aim_home_drag(cx);
        if drag_edge(at, list).is_some() {
            self.keep_home_scrolling(cx);
        }
    }

    fn aim_home_drag(&mut self, cx: &mut Context<Self>) {
        if self.home_drag.aim(&self.settings) {
            cx.notify();
        }
    }

    /// Scrolls home's list while a drag is held at its edge, a step a frame.
    fn keep_home_scrolling(&mut self, cx: &mut Context<Self>) {
        if self.home_drag.scrolling {
            return;
        }
        self.home_drag.scrolling = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let more = this.update(cx, |this, cx| this.scroll_home_drag(cx)).unwrap_or(false);
                if !more {
                    break;
                }
            }
        })
        .detach();
    }

    /// One step toward the edge the drag is held at; false once it is let go of, moved off the edge, or
    /// the list is as far as it goes.
    fn scroll_home_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let way = self.home_drag.edge();
        let offset = self.home_scroll.offset();
        let max = f32::from(self.home_scroll.max_offset().y);
        let to = way.map(|way| (f32::from(offset.y) - way * 8.).clamp(-max, 0.));
        match to {
            Some(y) if y != f32::from(offset.y) => {
                self.home_scroll.set_offset(point(offset.x, px(y)));
                self.aim_home_drag(cx);
                cx.notify();
                true
            }
            _ => {
                self.home_drag.scrolling = false;
                false
            }
        }
    }

    /// The drag is let go of over home's list: what it carries goes where it would land, if anywhere.
    pub fn drop_home(&mut self, drag: &HomeDrag, cx: &mut Context<Self>) {
        let landing = self.home_drag.landing.take();
        self.end_home_drag();
        cx.notify();
        let Some(Landing { before }) = landing else { return };
        let (path, before) = (drag.path.as_path(), before.as_deref());
        self.update_settings(
            |s| match drag.shelf {
                Shelf::Pinned => move_before(&mut s.pinned, |pin, key| HomeKey::of(pin).pin() == HomeKey::of(key).pin(), path, before),
                Shelf::Search => move_before(&mut s.saved_searches, |s, key| s.key() == key, path, before),
                Shelf::Favorite => move_before(&mut s.favorites, |f, key| f.home_key().pin() == HomeKey::of(key).pin(), path, before),
            },
            cx,
        );
    }

    /// The drag in home is over, dropped or not: nothing has moved.
    pub fn end_home_drag(&mut self) {
        self.home_drag.end();
    }

    /// Saved with no name of its own, so home shows the query until one is given; with the filter it runs
    /// within and what its results are narrowed to, a search apart from the same query with others.
    pub fn toggle_saved(&mut self, query: &str, filter: Option<String>, narrow: Option<String>, cx: &mut Context<Self>) {
        if query.trim().is_empty() {
            self.notify("検索語を入力すると保存できます", Tone::Info, cx);
            return;
        }
        let found = self.settings.saved(query, filter.as_deref(), narrow.as_deref()).cloned();
        let query = query.to_owned();
        self.update_settings(
            |s| match &found {
                Some(found) => s.saved_searches.retain(|saved| saved != found),
                None => s.saved_searches.push(SavedSearch { name: String::new(), query, filter, narrow }),
            },
            cx,
        );
        // The name given to it goes too; say so.
        if let Some(found) = found {
            self.notify(format!("「{}」を保存した検索から外しました", found.label()), Tone::Info, cx);
        }
    }

    /// The search showing, as the results' star and Ctrl+D keep it: its query, its filter, and what its
    /// results are narrowed to now.
    pub fn toggle_saved_current(&mut self, cx: &mut Context<Self>) {
        let (query, filter, narrow) = (self.state.query.clone(), self.state.search_filter.clone(), self.state.search_narrowing());
        self.toggle_saved(&query, filter, narrow, cx);
    }

    /// Ctrl+D saves what the active column shows: the search (from its field too), or the folder, each
    /// narrowed as it is.
    pub fn save_current(&mut self, cx: &mut Context<Self>) {
        let active = &self.state.columns[self.state.active];
        if self.search_focused || active.kind == Kind::Search {
            self.toggle_saved_current(cx);
        } else if active.kind == Kind::Folder {
            let key = self.favorite_key_of(self.state.active);
            self.toggle_favorite(&key, cx);
        } else {
            self.notify("「保存」は検索結果かフォルダーのカラムで使えます", Tone::Info, cx);
        }
    }

    /// A dialog's field, its Enter running `submit`.
    fn edit_field(
        &mut self,
        value: String,
        placeholder: String,
        submit: fn(&mut Self, &mut Window, &mut Context<Self>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder).default_value(value));
        let subscription = cx.subscribe_in(&input, window, move |this: &mut Self, _, event, window, cx| match event {
            InputEvent::PressEnter { .. } => submit(this, window, cx),
            InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
        });
        self.dialog_subscriptions.push(subscription);
        self.palette.style_input(&input, cx);
        input
    }

    pub fn open_edit(&mut self, key: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let HomeKey::Search { query, filter, narrow } = HomeKey::of(key) else { return };
        let Some(found) = self.settings.saved(query, filter, narrow).cloned() else { return };
        self.take_menu();
        self.begin_dialog(window, cx);
        let name = self.edit_field(found.name.clone(), found.query.clone(), Self::submit_edit, window, cx);
        let query = self.edit_field(found.query.clone(), String::new(), Self::submit_edit, window, cx);
        let narrow = self.edit_field(found.narrow.clone().unwrap_or_default(), "なし".into(), Self::submit_edit, window, cx);
        name.update(cx, |input, cx| input.focus(window, cx));
        let filter = found.filter.clone();
        self.dialog = Some(Dialog::EditSearch { original: found, name, query, filter, narrow });
        cx.notify();
    }

    /// The filter the edited search is to run within.
    pub fn pick_edit_filter(&mut self, chosen: Option<String>, cx: &mut Context<Self>) {
        if let Some(Dialog::EditSearch { filter, .. }) = &mut self.dialog {
            *filter = chosen;
            cx.notify();
        }
    }

    /// The saved search as the dialog now has it, its name and narrowing as they are kept.
    fn edited_search(&self, cx: &App) -> Option<SavedSearch> {
        let Some(Dialog::EditSearch { name, query, filter, narrow, .. }) = &self.dialog else { return None };
        Some(SavedSearch {
            name: name.read(cx).value().trim().to_owned(),
            query: query.read(cx).value().to_string(),
            filter: filter.clone(),
            narrow: narrowing(&narrow.read(cx).value()),
        })
    }

    /// Why the edited search cannot be saved as it stands, if it cannot.
    pub fn edit_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::EditSearch { original, .. }) = &self.dialog else { return None };
        let edited = self.edited_search(cx)?;
        if edited.query.trim().is_empty() {
            Some("")
        } else if edited.key() != original.key()
            && self.settings.saved(&edited.query, edited.filter.as_deref(), edited.narrow.as_deref()).is_some()
        {
            Some("この検索は既に保存しています")
        } else {
            None
        }
    }

    pub fn submit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_problem(cx).is_some() {
            return;
        }
        let (Some(Dialog::EditSearch { original, .. }), Some(edited)) = (&self.dialog, self.edited_search(cx)) else { return };
        let original = original.clone();
        let (was, is) = (original.key(), edited.key());
        let query_filter = (edited.query.clone(), edited.filter.clone());
        self.update_settings(
            |s| {
                if let Some(saved) = s.saved_searches.iter_mut().find(|saved| saved.key() == was) {
                    *saved = edited;
                    // Its pin follows it, where it was.
                    for pin in s.pinned.iter_mut().filter(|pin| **pin == was) {
                        *pin = is.clone();
                    }
                }
            },
            cx,
        );
        self.close_dialog(window, cx);
        // The search showing follows its new query and filter.
        let (query, filter) = query_filter;
        let showing = self.state.pinned() && (&self.state.query, &self.state.search_filter) == (&original.query, &original.filter);
        if showing && (&query, &filter) != (&original.query, &original.filter) {
            self.search_for(&query, filter, window, cx);
            self.focus_active(window, cx);
        }
    }

    /// The favorite `key` names, its name and narrowing to edit.
    pub fn open_favorite_edit(&mut self, key: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let HomeKey::Folder { path, narrow } = HomeKey::of(key) else { return };
        let Some(found) = self.settings.favorite(&path, narrow.as_deref()).cloned() else { return };
        self.take_menu();
        self.begin_dialog(window, cx);
        let name = self.edit_field(found.name.clone(), shown(folder_name(&found.path)), Self::submit_favorite_edit, window, cx);
        let narrow = self.edit_field(found.narrow.clone().unwrap_or_default(), "なし".into(), Self::submit_favorite_edit, window, cx);
        name.update(cx, |input, cx| input.focus(window, cx));
        self.dialog = Some(Dialog::EditFavorite { original: found, name, narrow });
        cx.notify();
    }

    /// The favorite as the dialog now has it, its name and narrowing as they are kept.
    fn edited_favorite(&self, cx: &App) -> Option<Favorite> {
        let Some(Dialog::EditFavorite { original, name, narrow }) = &self.dialog else { return None };
        Some(Favorite {
            path: original.path.clone(),
            name: name.read(cx).value().trim().to_owned(),
            narrow: narrowing(&narrow.read(cx).value()),
        })
    }

    /// Why the edited favorite cannot be kept as it stands, if it cannot.
    pub fn favorite_edit_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::EditFavorite { original, .. }) = &self.dialog else { return None };
        let edited = self.edited_favorite(cx)?;
        let other = edited.narrow != original.narrow && self.settings.favorite(&edited.path, edited.narrow.as_deref()).is_some();
        other.then_some("この絞り込みのお気に入りは既にあります")
    }

    pub fn submit_favorite_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.favorite_edit_problem(cx).is_some() {
            return;
        }
        let (Some(Dialog::EditFavorite { original, .. }), Some(edited)) = (&self.dialog, self.edited_favorite(cx)) else { return };
        let (was, is) = (original.home_key().pin(), edited.key());
        let original = original.clone();
        self.update_settings(
            |s| {
                if let Some(favorite) = s.favorites.iter_mut().find(|f| f.is(&original.path, original.narrow.as_deref())) {
                    *favorite = edited;
                    // Its pin follows it, where it was.
                    for pin in s.pinned.iter_mut().filter(|pin| HomeKey::of(pin).pin() == was) {
                        *pin = is.clone();
                    }
                }
            },
            cx,
        );
        self.close_dialog(window, cx);
    }

    pub fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        // The theme now chosen takes the keys, as a radio group's checked one does.
        self.stop(format!("theme:{:?}", self.settings.theme), cx).focus(window, cx);
        self.dialog = Some(Dialog::Preferences);
        cx.notify();
    }

    /// Takes `theme`'s colors at once, and keeps it.
    pub fn set_theme(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        self.update_settings(|s| s.theme = theme, cx);
        self.apply_theme(window, cx);
    }

    /// Lets go of Everything and makes the search afresh, so it connects anew.
    pub fn reconnect(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_executor().spawn(async move { backend.reconnect() });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |this, cx| {
                this.engine = Engine::Unknown;
                this.state.reset_search();
                this.changed(cx);
                this.notify("Everythingとの接続を切りました。次の検索で接続し直します。", Tone::Info, cx);
            })
            .ok();
        })
        .detach();
    }

    /// Lets go of what the dialog before listened to. Called first by whatever opens one, before it makes
    /// the fields the new dialog listens to, and before the keys go into it: where they are is where they go
    /// back to as it closes.
    pub(crate) fn begin_dialog(&mut self, window: &Window, cx: &App) {
        self.remember_keys(window, cx);
        self.dialog_subscriptions.clear();
    }

    /// The keys go back to where they were as the dialog opened, or the menu that opened it.
    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.dialog_subscriptions.clear();
        self.restore_keys(window, cx);
        cx.notify();
    }

    /// Opens the window's dialog as [`Colet::dialog`] comes, and closes it as that goes; the keys stay where
    /// Colet put them, in a field of the dialog or back in the columns.
    pub fn sync_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.dialog.is_some();
        if open == self.dialog_shown {
            return;
        }
        self.dialog_shown = open;
        let focused = window.focused(cx);
        if open {
            let colet = cx.weak_entity();
            window.open_dialog(cx, move |dialog, window, cx| match colet.upgrade() {
                Some(colet) => colet.update(cx, |this, cx| this.dialog_frame(dialog, window, cx)),
                None => dialog,
            });
        } else {
            window.close_dialog(cx);
        }
        if let Some(focused) = focused {
            focused.focus(window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Entry, Placement, SavedSearch, Settings, Shelf, home_entries, home_groups, movable, move_before, places, restore_bounds, shown,
    };
    use gpui_kit::{Bounds, DisplayId, point, px, size};
    use std::path::Path;

    #[test]
    fn reopens_on_the_display_it_was_left_on_or_centered_on_the_primary_one() {
        let primary = (DisplayId::new(1), Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))));
        let second = (DisplayId::new(2), Bounds::new(point(px(1920.), px(0.)), size(px(1280.), px(720.))));
        let at = |x: f32, y: f32, width: f32, height: f32| Placement { x, y, width, height, maximized: false };
        let bounds = |x: f32, y: f32, w: f32, h: f32| Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
        // Left on the second display, it opens there as it was.
        assert_eq!(
            restore_bounds(at(2000., 50., 800., 600.), &[primary, second], Some(primary.0)),
            (Some(second.0), bounds(2000., 50., 800., 600.))
        );
        // Larger than that display now is, it is kept within it.
        assert_eq!(
            restore_bounds(at(2000., 0., 1600., 900.), &[primary, second], Some(primary.0)),
            (Some(second.0), bounds(1920., 0., 1280., 720.))
        );
        // That display unplugged, it opens centered on the primary one with the size it had.
        assert_eq!(
            restore_bounds(at(2000., 50., 800., 600.), &[primary], Some(primary.0)),
            (Some(primary.0), bounds(560., 240., 800., 600.))
        );
        // Left smaller than the window now goes, it opens at the least it goes, where it was.
        assert_eq!(
            restore_bounds(at(100., 100., 400., 300.), &[primary], Some(primary.0)),
            (Some(primary.0), bounds(100., 100., 720., 480.))
        );
    }

    #[test]
    fn lists_favorites_then_saved_searches_then_drives_each_once() {
        let settings = Settings {
            favorites: vec![r"D:\Work".into(), r"C:\".into()],
            saved_searches: vec![
                SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None, narrow: None },
                SavedSearch { name: "猫".into(), query: "cat".into(), filter: Some("画像".into()), narrow: None },
            ],
            ..Settings::default()
        };
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")]);
        let rows: Vec<(&str, &str)> = entries.iter().map(|e| (e.path.to_str().unwrap(), e.name.as_str())).collect();
        assert_eq!(rows, [(r"D:\Work", "Work"), (r"C:\", "C:"), ("search:ext:pdf", "PDF"), ("search:cat\0画像", "猫"), (r"D:\", "D:")]);
        assert!(!entries[2].directory);
    }

    #[test]
    fn lists_the_drives_then_the_other_index_roots_by_path_each_once() {
        let drives = vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "データ (D:)")];
        let indexed = vec!["C:".into(), r"\\nas\Share".into(), r"d:\Projects".into(), "E:".into(), r"D:\".into(), r"\\NAS\share".into()];
        let rows: Vec<(String, String)> = places(drives, indexed).into_iter().map(|e| (shown(&e.path), e.name)).collect();
        let at = |path: &str, name: &str| (path.to_string(), name.to_string());
        assert_eq!(
            rows,
            [
                at(r"C:\", "OS (C:)"),
                at(r"D:\", "データ (D:)"),
                at(r"\\nas\Share", r"\\nas\Share"),
                at(r"d:\Projects", r"d:\Projects"),
                at(r"E:\", r"E:\")
            ]
        );
    }

    #[test]
    fn takes_a_place_in_another_case_as_the_same_but_not_a_query() {
        let search = |query: &str| SavedSearch { name: String::new(), query: query.into(), filter: None, narrow: None };
        let settings =
            Settings { favorites: vec![r"c:\".into()], saved_searches: vec![search("ext:PDF"), search("ext:pdf")], ..Settings::default() };
        assert!(settings.is_favorite(Path::new(r"C:\")) && settings.is_favorite(Path::new("C:")));
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)")]);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.to_str().unwrap()).collect();
        assert_eq!(paths, [r"c:\", "search:ext:PDF", "search:ext:pdf"]);
    }

    #[test]
    fn lists_the_pinned_first_in_their_order_each_out_of_its_kind() {
        let settings = Settings {
            favorites: vec![r"D:\Work".into(), r"D:\Music".into()],
            saved_searches: vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None, narrow: None }],
            // A drive unplugged waits unseen; a place is pinned whatever its case.
            pinned: vec!["search:ext:pdf".into(), r"E:\".into(), r"c:\".into(), r"D:\Music".into()],
            ..Settings::default()
        };
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")]);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.to_str().unwrap()).collect();
        assert_eq!(paths, ["search:ext:pdf", r"C:\", r"D:\Music", r"D:\Work", r"D:\"]);
        let groups: Vec<(&str, Vec<&str>)> = home_groups(&settings, &entries)
            .into_iter()
            .map(|(title, rows)| (title, rows.into_iter().map(|e| e.path.to_str().unwrap()).collect()))
            .collect();
        assert_eq!(
            groups,
            [("ピン留め", vec!["search:ext:pdf", r"C:\", r"D:\Music"]), ("お気に入り", vec![r"D:\Work"]), ("場所", vec![r"D:\"])]
        );
        assert_eq!(movable(&settings, Path::new(r"C:\")), Some(Shelf::Pinned));
        assert_eq!(movable(&settings, Path::new(r"D:\Work")), Some(Shelf::Favorite));
        assert_eq!(movable(&settings, Path::new(r"D:\")), None);
    }

    #[test]
    fn moves_an_item_before_another_or_to_the_end() {
        let moved = |path: &str, before: Option<&str>| {
            let mut items = vec![Path::new("a"), Path::new("b"), Path::new("c"), Path::new("d")];
            move_before(&mut items, |item, key| *item == key, Path::new(path), before.map(Path::new));
            items.into_iter().map(|item| item.to_str().unwrap()).collect::<Vec<_>>()
        };
        assert_eq!(moved("c", Some("a")), ["c", "a", "b", "d"]);
        assert_eq!(moved("a", Some("c")), ["b", "a", "c", "d"]);
        assert_eq!(moved("b", None), ["a", "c", "d", "b"]);
        // Before itself, or before the one after it: where it was.
        assert_eq!(moved("b", Some("b")), ["a", "b", "c", "d"]);
        assert_eq!(moved("b", Some("c")), ["a", "b", "c", "d"]);
        // Not there to move.
        assert_eq!(moved("x", Some("a")), ["a", "b", "c", "d"]);
    }
}
