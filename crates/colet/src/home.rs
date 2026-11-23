//! Home: the pinned rows, the favorites, the saved searches and the places, the dialogs that edit a
//! favorite or a saved search, and the drag that puts the pinned rows, favorites and saved searches in
//! order, each among its own kind.
//!
//! [`HomeDragging`] owns the drag: what is carried, where it would land, where home's rows were drawn for
//! it to find its place among, and the scrolling while it is held at an edge of home's list. Colet owns
//! the rest: what home lists, kept in the settings, the column home is in the tab's state, and where the
//! keys are.
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    rc::Rc,
    time::{Duration, Instant},
};

use colet_core::{
    entry::{Entry, Order, Sort},
    paths::{folder_name, shown},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::{Colet, FOLD_WIDTH, WidthOf},
    dialogs::{Choice, Dialog, DialogParts, Pick, SearchEdit, field, label, push},
    file_menu::FileCommand,
    menus::{Line, line},
    metrics::{self, Scale},
    notices::Tone,
    ops::Reasons,
    reveal::Slide,
    search_filters::filter_tool_id,
    settings::{Favorite, SavedSearch, Settings},
    state::{HomeKey, Kind, narrowing},
    stops::{OrderChoice, Stop},
    theme::ActivePalette,
};

mod view;

pub use view::HomeGhost;

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
pub fn home_groups<'a>(settings: &Settings, entries: &'a [std::sync::Arc<Entry>]) -> Vec<(&'static str, Vec<&'a Entry>)> {
    let pinned = |e: &&Entry| settings.is_pinned(&e.path);
    let favorite = |e: &&Entry| !pinned(e) && settings.is_favorite(&e.path);
    let search = |e: &&Entry| !pinned(e) && HomeKey::of(&e.path).is_search();
    let entries = || entries.iter().map(|e| &**e);
    [
        (words::home::pinned(), entries().filter(pinned).collect::<Vec<_>>()),
        (words::home::favorites(), entries().filter(favorite).collect()),
        (words::home::saved_searches(), entries().filter(search).collect()),
        (words::home::places(), entries().filter(|e| !pinned(e) && !favorite(e) && !search(e)).collect()),
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

/// Where each of home's rows that a drag puts in order was drawn, by its path, with its kind.
pub type HomeRows = Rc<RefCell<Vec<(PathBuf, Shelf, Bounds<Pixels>)>>>;

/// Where each of home's headings was drawn, top to bottom.
pub type HomeHeadings = Rc<RefCell<Vec<Bounds<Pixels>>>>;

/// A drag in home, putting a favorite, saved search or pinned row in order among its kind.
#[derive(Default)]
pub struct HomeDragging {
    /// What is being dragged; none while no drag is under way.
    carried: Option<HomeDrag>,
    /// Where it would land; none while outside its own kind.
    landing: Option<Landing>,
    /// Where the pointer last was, and home's list then: held at an edge, it scrolls on.
    pointer: Option<(Point<Pixels>, Bounds<Pixels>)>,
    /// Home's list as the pointer last moved over it, and the zoom it is drawn at.
    list: Option<(ScrollHandle, Scale)>,
    /// Home's list is scrolling toward the edge the drag is held at.
    scrolling: bool,
    /// Where each of the rows a drag puts in order was drawn, for a drag to find its place among them.
    rows: HomeRows,
    /// Where each of home's headings was drawn: a kind reaches from its own to the next.
    headings: HomeHeadings,
}

impl HomeDragging {
    /// What is being dragged, if a drag is under way.
    pub fn carried(&self) -> Option<&HomeDrag> {
        self.carried.as_ref()
    }

    /// Where what is carried would land; none while outside its own kind.
    pub fn landing(&self) -> Option<&Landing> {
        self.landing.as_ref().filter(|_| self.carried.is_some())
    }

    /// Where the rows a drag puts in order are drawn, to be filled in as home is drawn.
    pub fn rows(&self) -> &HomeRows {
        &self.rows
    }

    /// Where home's headings are drawn, to be filled in as home is drawn.
    pub fn headings(&self) -> &HomeHeadings {
        &self.headings
    }

    /// `drag` is picked up: its row fades where it was.
    pub fn start(&mut self, drag: HomeDrag, cx: &mut Context<Self>) {
        self.carried = Some(drag);
        self.landing = None;
        cx.notify();
    }

    /// The pointer moved to `at` in the drag, with home's list drawn at `bounds`, scrolled by `scroll` and
    /// drawn at the zoom `scale` gives.
    pub fn drag_to(&mut self, at: Point<Pixels>, bounds: Bounds<Pixels>, scroll: &ScrollHandle, scale: Scale, cx: &mut Context<Self>) {
        if self.carried.is_none() {
            return;
        }
        self.pointer = Some((at, bounds));
        self.list = Some((scroll.clone(), scale));
        if self.aim() {
            cx.notify();
        }
        if drag_edge(at, bounds, scale).is_some() {
            self.keep_scrolling(cx);
        }
    }

    /// The drag is let go of over home's list: where it would land, if anywhere. The drag is over.
    pub fn drop(&mut self, cx: &mut Context<Self>) -> Option<Landing> {
        let landing = self.landing.take();
        self.end(cx);
        landing
    }

    /// The drag is over, dropped or not: nothing has moved.
    pub fn end(&mut self, cx: &mut Context<Self>) {
        self.carried = None;
        self.landing = None;
        self.pointer = None;
        self.list = None;
        cx.notify();
    }

    /// Where the drag would land: between two rows of its own kind, by the half of the row it is over.
    /// Its kind reaches from its heading to the next kind's, or the list's end: over its heading it goes
    /// first, below its last row last. Nowhere outside that. True when that moved.
    fn aim(&mut self) -> bool {
        let (Some(drag), Some((at, list))) = (&self.carried, self.pointer) else { return false };
        let landing = {
            let rows = self.rows.borrow();
            let headings = self.headings.borrow();
            let own: Vec<_> = rows.iter().filter(|(_, shelf, _)| *shelf == drag.shelf).map(|(path, _, bounds)| (path, bounds)).collect();
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
                        if at.y < bounds.center().y { Some(path.clone()) } else { own.get(over + 1).map(|(path, _)| (*path).clone()) };
                    Some(Landing { before })
                }
                _ => None,
            }
        };
        let moved = self.landing != landing;
        self.landing = landing;
        moved
    }

    /// Scrolls home's list while the drag is held at its edge, a step a frame.
    fn keep_scrolling(&mut self, cx: &mut Context<Self>) {
        if self.scrolling {
            return;
        }
        self.scrolling = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_millis(16)).await;
                let more = this.update(cx, |this, cx| this.scroll_step(cx)).unwrap_or(false);
                if !more {
                    break;
                }
            }
        })
        .detach();
    }

    /// One step toward the edge the drag is held at; false once it is let go of, moved off the edge, or
    /// the list is as far as it goes.
    fn scroll_step(&mut self, cx: &mut Context<Self>) -> bool {
        let step = self.carried.as_ref().and(self.pointer).zip(self.list.clone()).and_then(|((at, bounds), (scroll, scale))| {
            let way = drag_edge(at, bounds, scale)?;
            let offset = scroll.offset();
            let max = f32::from(scroll.max_offset().y);
            let to = (f32::from(offset.y) - way * scale.up(8.)).clamp(-max, 0.);
            (to != f32::from(offset.y)).then(|| (scroll, point(offset.x, px(to))))
        });
        match step {
            Some((scroll, to)) => {
                scroll.set_offset(to);
                self.aim();
                cx.notify();
                true
            }
            None => {
                self.scrolling = false;
                false
            }
        }
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

/// Which way home's list scrolls with a drag held at `at`: up at its top edge, down at its bottom, the
/// edges as deep as `scale` draws them.
fn drag_edge(at: Point<Pixels>, list: Bounds<Pixels>, scale: Scale) -> Option<f32> {
    if at.x < list.left() || at.x >= list.right() {
        None
    } else if at.y < list.top() + scale.px(DRAG_EDGE) {
        Some(-1.)
    } else if at.y >= list.bottom() - scale.px(DRAG_EDGE) {
        Some(1.)
    } else {
        None
    }
}

/// How wide home's pane is drawn, and the spring that takes it out of its strip and back in when it folds
/// there (see [`Colet::home_open`]).
#[derive(Default)]
pub struct HomePane {
    /// At 100%; none before the first frame.
    width: Option<f32>,
    /// Whether it was out the last frame.
    open: Option<bool>,
    slide: Option<Slide>,
    /// The left button went down with it out, and is not yet let go of: it stays out, so nothing moves
    /// under the press, however the keys left it.
    held: bool,
}

impl HomePane {
    /// Out of its strip the last frame.
    fn out(&self) -> bool {
        self.open == Some(true)
    }

    /// How wide it is drawn this frame, at 100%.
    pub fn width(&self) -> f32 {
        self.width.unwrap_or_default()
    }

    /// Coming out of its strip or going back into it.
    pub fn moving(&self) -> bool {
        self.slide.is_some()
    }

    /// How wide it will stand once out of its strip or back in it, at 100%.
    pub fn target(&self) -> f32 {
        self.slide.as_ref().map_or(self.width(), |slide| slide.target)
    }

    /// A frame toward `target`, out of its strip when `open`: when it comes out or goes in, by `spring`,
    /// sliding as the columns do; otherwise, as a width dragged or a window narrowed changes it, at once.
    /// True while it is still on its way.
    fn step(&mut self, target: f32, open: bool, spring: bool, now: Instant) -> bool {
        let turned = self.open.replace(open).is_some_and(|was| was != open);
        match self.width {
            Some(width) if spring && (turned || self.slide.is_some()) => {
                let slide = self.slide.get_or_insert_with(|| Slide::new(width, width, target, None));
                slide.target = target;
                let (position, done) = slide.step(now);
                self.width = Some(position);
                if done {
                    self.slide = None;
                }
                !done
            }
            _ => {
                self.width = Some(target);
                self.slide = None;
                false
            }
        }
    }
}

impl Colet {
    /// Whether home's pane is out of its strip: while the keys are in it, or a menu or dialog opened from it
    /// gives them back there, and while the tab has nothing but home. Out as the left button goes down, it
    /// stays out until the button is let go of.
    pub fn home_open(&self, window: &Window, cx: &App) -> bool {
        if self.state.columns.len() == 1 || self.home_pane.held {
            return true;
        }
        let here = |focus: &FocusHandle| focus.is_focused(window) || self.overlays.keys_go_back_to(focus);
        let field = self.narrow_fields.get(&self.state.columns[0].id).map(|field| field.input.read(cx).focus_handle(cx));
        here(&self.home_focus) || field.is_some_and(|field| here(&field))
    }

    /// Home's pane a frame toward the width it is to be drawn at: out as wide as dragged, or folded into
    /// its strip. Stepped before the columns slide, so they reckon with the room it leaves them this frame.
    pub(crate) fn step_home_pane(&mut self, window: &mut Window, cx: &App) {
        let open = self.home_open(window, cx);
        // Folding back, it brings the column worked in on screen, however the keys went there: a press let
        // go of off it, a menu opened on it.
        if self.home_pane.out() && !open {
            self.scrolls.sideways.ask();
        }
        let target = if open { self.sidebar_width(window) } else { FOLD_WIDTH };
        // With motion reduced, as with Windows' animations off, it stands as wide as it is to be at once.
        let spring = self.widths.resizing() != Some(WidthOf::Sidebar) && !cx.reduce_motion();
        if self.home_pane.step(target, open, spring, Instant::now()) {
            window.request_animation_frame();
        }
    }

    /// The left button went down anywhere: with home's pane out of its strip, it stays out until the
    /// button is let go of, so the keys leaving it fold it only then, with nothing moved under the press.
    pub(crate) fn hold_home(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        if event.button == MouseButton::Left && self.home_pane.out() && !self.home_pane.held {
            self.home_pane.held = true;
            // For the window-wide listener to be drawn.
            cx.notify();
        }
    }

    /// The button that held home's pane out is let go of: it folds now if the keys have left it.
    pub(crate) fn let_go_of_home(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.home_pane.held) {
            cx.notify();
        }
    }

    /// While a press holds home's pane out, hears the pointer anywhere, in or out of the window, until the
    /// button is let go of: seen let go of, or moved with it up, as after a drag out of the window, which
    /// Windows runs and which keeps the release from the window.
    pub(crate) fn home_hold_listener(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.home_pane.held {
            return None;
        }
        let view = cx.entity().downgrade();
        let listen = move |_: Bounds<Pixels>, _: (), window: &mut Window, _: &mut App| {
            let moved = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && event.pressed_button != Some(MouseButton::Left) {
                    moved.update(cx, |this, cx| this.let_go_of_home(cx)).ok();
                }
            });
            let up = view.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && event.button == MouseButton::Left {
                    up.update(cx, |this, cx| this.let_go_of_home(cx)).ok();
                }
            });
        };
        Some(canvas(|_, _, _| {}, listen).absolute().size_0().into_any_element())
    }

    /// Keeps or lets go of the favorite `key` names: a folder's path, or a narrowed favorite's key.
    pub fn toggle_favorite(&mut self, key: &Path, cx: &mut Context<Self>) {
        let HomeKey::Folder { path, narrow } = HomeKey::of(key) else { return };
        let (path, narrow) = (path.into_owned(), narrow.map(std::borrow::Cow::into_owned));
        let found = self.stored.settings().favorite(&path, narrow.as_deref()).cloned();
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
            self.notify(words::home::unfavorited(found.label()), Tone::Info, cx);
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

    // ---- The drag in home ----

    /// The drag in home, while one is under way: what it carries.
    pub fn home_carried(&self, cx: &App) -> Option<HomeDrag> {
        self.home_drag.read(cx).carried().cloned()
    }

    /// A favorite or saved search is picked up in home: its row fades where it was.
    pub fn start_home_drag(&mut self, drag: &HomeDrag, cx: &mut Context<Self>) {
        self.home_drag.update(cx, |dragging, cx| dragging.start(drag.clone(), cx));
    }

    /// The pointer moved in a drag in home, with home's list drawn at `list`.
    pub fn drag_home(&mut self, at: Point<Pixels>, list: Bounds<Pixels>, cx: &mut Context<Self>) {
        let (scroll, scale) = (self.scrolls.home.clone(), self.scale());
        self.home_drag.update(cx, |dragging, cx| dragging.drag_to(at, list, &scroll, scale, cx));
    }

    /// The drag is let go of over home's list: what it carries goes where it would land, if anywhere.
    pub fn drop_home(&mut self, drag: &HomeDrag, cx: &mut Context<Self>) {
        let landing = self.home_drag.update(cx, |dragging, cx| dragging.drop(cx));
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
    pub fn end_home_drag(&mut self, cx: &mut Context<Self>) {
        self.home_drag.update(cx, |dragging, cx| dragging.end(cx));
    }

    // ---- Saved searches ----

    /// Saved with no name of its own, so home shows the query until one is given; with the filter it runs
    /// within, what its results are narrowed to and their order, a search apart from the same query with
    /// others.
    pub fn toggle_saved(&mut self, query: &str, filter: Option<String>, narrow: Option<String>, order: Order, cx: &mut Context<Self>) {
        if query.trim().is_empty() {
            self.notify(words::home::query_needed(), Tone::Info, cx);
            return;
        }
        let found = self.stored.settings().saved(query, filter.as_deref(), narrow.as_deref(), order).cloned();
        let query = query.to_owned();
        self.update_settings(
            |s| match &found {
                Some(found) => s.saved_searches.retain(|saved| saved != found),
                None => s.saved_searches.push(SavedSearch { name: String::new(), query, filter, narrow, order }),
            },
            cx,
        );
        // The name given to it goes too; say so.
        if let Some(found) = found {
            self.notify(words::home::unsaved(found.label()), Tone::Info, cx);
        }
    }

    /// The search showing, as the results' star and Ctrl+D keep it: its query, its filter, what its
    /// results are narrowed to now, and their order.
    pub fn toggle_saved_current(&mut self, cx: &mut Context<Self>) {
        let (query, filter, narrow) = (self.state.query.clone(), self.state.search_filter.clone(), self.state.search_narrowing());
        self.toggle_saved(&query, filter, narrow, self.state.search_order, cx);
    }

    /// Ctrl+D saves what the active column shows: the search (from its field too), or the folder, each
    /// narrowed as it is.
    pub fn save_current(&mut self, window: &Window, cx: &mut Context<Self>) {
        let active = &self.state.columns[self.state.active];
        if self.search_has_keys(window, cx) || active.kind == Kind::Search {
            self.toggle_saved_current(cx);
        } else if active.kind == Kind::Folder {
            let key = self.favorite_key_of(self.state.active);
            self.toggle_favorite(&key, cx);
        } else {
            self.notify(words::home::save_where(), Tone::Info, cx);
        }
    }

    // ---- The dialogs that edit a favorite or a saved search ----

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
        self.overlays.listen_in_dialog(subscription);
        cx.palette().style_input(&input, cx);
        input
    }

    pub fn open_edit(&mut self, key: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let HomeKey::Search { query, filter, narrow, order } = HomeKey::of(key) else { return };
        let Some(found) = self.stored.settings().saved(query, filter, narrow, order).cloned() else { return };
        self.take_menu();
        self.begin_dialog(window, cx);
        let name = self.edit_field(found.name.clone(), found.query.clone(), Self::submit_edit, window, cx);
        let query = self.edit_field(found.query.clone(), String::new(), Self::submit_edit, window, cx);
        let narrow = self.edit_field(found.narrow.clone().unwrap_or_default(), words::none().into(), Self::submit_edit, window, cx);
        name.update(cx, |input, cx| input.focus(window, cx));
        let (filter, order) = (found.filter.clone(), found.order);
        self.overlays.set_dialog(Dialog::EditSearch(SearchEdit { original: found, name, query, filter, narrow, order }));
        cx.notify();
    }

    /// The filter the edited search is to run within.
    pub fn pick_edit_filter(&mut self, chosen: Option<String>, cx: &mut Context<Self>) {
        if let Some(Dialog::EditSearch(SearchEdit { filter, .. })) = self.overlays.dialog_mut() {
            *filter = chosen;
            cx.notify();
        }
    }

    /// The order the edited search's results are to be listed in, as `change` makes it.
    pub fn change_edit_order(&mut self, change: impl FnOnce(&mut Order), cx: &mut Context<Self>) {
        if let Some(Dialog::EditSearch(SearchEdit { order, .. })) = self.overlays.dialog_mut() {
            change(order);
            cx.notify();
        }
    }

    /// The saved search as the dialog now has it, its name and narrowing as they are kept.
    fn edited_search(&self, cx: &App) -> Option<SavedSearch> {
        let Some(Dialog::EditSearch(SearchEdit { name, query, filter, narrow, order, .. })) = self.overlays.dialog() else { return None };
        Some(SavedSearch {
            name: name.read(cx).value().trim().to_owned(),
            query: query.read(cx).value().to_string(),
            filter: filter.clone(),
            narrow: narrowing(&narrow.read(cx).value()),
            order: *order,
        })
    }

    /// Why the edited search cannot be saved as it stands, if it cannot.
    pub fn edit_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::EditSearch(SearchEdit { original, .. })) = self.overlays.dialog() else { return None };
        let edited = self.edited_search(cx)?;
        if edited.query.trim().is_empty() {
            Some("")
        } else if edited.key() != original.key()
            && self.stored.settings().saved(&edited.query, edited.filter.as_deref(), edited.narrow.as_deref(), edited.order).is_some()
        {
            Some(words::home::search_saved_already())
        } else {
            None
        }
    }

    pub fn submit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_problem(cx).is_some() {
            return;
        }
        let (Some(Dialog::EditSearch(SearchEdit { original, .. })), Some(edited)) = (self.overlays.dialog(), self.edited_search(cx)) else {
            return;
        };
        let original = original.clone();
        let (was, is) = (original.key(), edited.key());
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
    }

    /// The favorite `key` names, its name and narrowing to edit.
    pub fn open_favorite_edit(&mut self, key: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let HomeKey::Folder { path, narrow } = HomeKey::of(key) else { return };
        let Some(found) = self.stored.settings().favorite(&path, narrow.as_deref()).cloned() else { return };
        self.take_menu();
        self.begin_dialog(window, cx);
        let name = self.edit_field(found.name.clone(), shown(folder_name(&found.path)), Self::submit_favorite_edit, window, cx);
        let narrow =
            self.edit_field(found.narrow.clone().unwrap_or_default(), words::none().into(), Self::submit_favorite_edit, window, cx);
        name.update(cx, |input, cx| input.focus(window, cx));
        self.overlays.set_dialog(Dialog::EditFavorite { original: found, name, narrow });
        cx.notify();
    }

    /// The favorite as the dialog now has it, its name and narrowing as they are kept.
    fn edited_favorite(&self, cx: &App) -> Option<Favorite> {
        let Some(Dialog::EditFavorite { original, name, narrow }) = self.overlays.dialog() else { return None };
        Some(Favorite {
            path: original.path.clone(),
            name: name.read(cx).value().trim().to_owned(),
            narrow: narrowing(&narrow.read(cx).value()),
        })
    }

    /// Why the edited favorite cannot be kept as it stands, if it cannot.
    pub fn favorite_edit_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::EditFavorite { original, .. }) = self.overlays.dialog() else { return None };
        let edited = self.edited_favorite(cx)?;
        let other = edited.narrow != original.narrow && self.stored.settings().favorite(&edited.path, edited.narrow.as_deref()).is_some();
        other.then_some(words::home::favorite_exists())
    }

    pub fn submit_favorite_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.favorite_edit_problem(cx).is_some() {
            return;
        }
        let (Some(Dialog::EditFavorite { original, .. }), Some(edited)) = (self.overlays.dialog(), self.edited_favorite(cx)) else {
            return;
        };
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
}

impl Colet {
    /// The field for what a favorite or saved search opens narrowed to, and why it cannot be kept, if it
    /// cannot.
    fn narrow_field(&self, input: &Entity<InputState>, problem: Option<&'static str>, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label(p, words::narrowing()))
            .child(field(p, input, self.has_keys(input, window, cx), radii.md, Self::right_press_menu(input, cx)))
            .child(match problem {
                Some(problem) => div().text_xs().text_color(p.danger).child(problem),
                None => div().text_xs().text_color(p.text_faint).child(words::home::narrow_hint()),
            })
    }

    /// The order a saved search lists its results in: what it sorts by, the way it runs, and whether folders
    /// come first, each a row of choices as the sort order's menu has them.
    fn order_field(&self, order: Order, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = cx.palette();
        let mut row = |id: &'static str, label: &'static str, choices: Vec<(OrderChoice, &'static str)>| {
            let choices = choices
                .into_iter()
                .map(|(choice, text)| {
                    let pick: Pick = Rc::new(move |this, _, cx| this.change_edit_order(|order| choice.apply(order), cx));
                    let item = ElementId::NamedChild(std::sync::Arc::new(id.into()), SharedString::from(text));
                    Choice::new(item, text, Stop::EditSearchOrder(choice), choice.is_in(order), pick)
                })
                .collect();
            self.radio_group(id, label, choices, window, cx)
        };
        let by = Sort::ALL.into_iter().map(|by| (OrderChoice::By(by), words::sort_name(by))).collect();
        let direction = [false, true].into_iter().map(|d| (OrderChoice::Descending(d), words::direction(d))).collect();
        let folders =
            vec![(OrderChoice::FoldersFirst(true), words::home::folders_first()), (OrderChoice::FoldersFirst(false), words::home::mixed())];
        let (by, direction, folders) = (
            row("edit-search-sort", words::home::sort_by(), by),
            row("edit-search-direction", words::home::direction(), direction),
            row("edit-search-folders", words::home::folders(), folders),
        );
        div().flex().flex_col().gap_1().child(label(p, words::sort_order())).child(by).child(direction).child(folders)
    }

    /// A favorite's name and narrowing.
    pub(crate) fn favorite_dialog(
        &self,
        original: &Favorite,
        name: &Entity<InputState>,
        narrow: &Entity<InputState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> DialogParts {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let problem = self.favorite_edit_problem(cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_xs().text_color(p.text_faint).child(words::home::folder_of(shown(&original.path))))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, words::name()))
                    .child(field(p, name, self.has_keys(name, window, cx), radii.md, Self::right_press_menu(name, cx)))
                    .child(div().text_xs().text_color(p.text_faint).child(words::home::favorite_name_hint())),
            )
            .child(self.narrow_field(narrow, problem, window, cx));
        let buttons = vec![
            (
                "cancel",
                push("cancel", words::cancel(), false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            (
                "save-favorite",
                push("save-favorite", words::save(), true, problem.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.submit_favorite_edit(window, cx))),
            ),
        ];
        (words::home::edit_favorite(), content, buttons)
    }

    /// A saved search's name, query, filter, narrowing and order.
    pub(crate) fn search_dialog(&self, edit: &SearchEdit, window: &Window, cx: &mut Context<Self>) -> DialogParts {
        let SearchEdit { name, query, filter, narrow, order, .. } = edit;
        let order = *order;
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        // The saved search's choice of filter, none first.
        let choices: Vec<Option<String>> =
            std::iter::once(None).chain(self.stored.settings().filters.iter().map(|f| Some(f.name.clone()))).collect();
        let problem = self.edit_problem(cx);
        let chosen = filter.clone();
        let filters = choices
            .into_iter()
            .map(|choice| {
                let text = choice.clone().unwrap_or_else(|| words::none().into());
                let id = match &choice {
                    Some(name) => filter_tool_id("edit-search-filter", name),
                    None => "edit-search-filter-none".into(),
                };
                let picked = choice.clone();
                let pick: Pick = Rc::new(move |this, _, cx| this.pick_edit_filter(picked.clone(), cx));
                let is_chosen = choice == chosen;
                Choice::new(id, text, Stop::EditSearchFilter(choice), is_chosen, pick)
            })
            .collect();
        let filters = self.radio_group("edit-search-filters", words::search_filter(), filters, window, cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, words::name()))
                    .child(field(p, name, self.has_keys(name, window, cx), radii.md, Self::right_press_menu(name, cx)))
                    .child(div().text_xs().text_color(p.text_faint).child(words::home::search_name_hint())),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, words::query()))
                    .child(field(p, query, self.has_keys(query, window, cx), radii.md, Self::right_press_menu(query, cx)))
                    .when_some(problem.filter(|p| !p.is_empty()), |d, problem| {
                        d.child(div().text_xs().text_color(p.danger).child(problem))
                    }),
            )
            .child(div().flex().flex_col().gap_1().child(label(p, words::search_filter())).child(filters))
            .child(self.narrow_field(narrow, None, window, cx))
            .child(self.order_field(order, window, cx));
        let buttons = vec![
            (
                "cancel",
                push("cancel", words::cancel(), false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            (
                "save-search",
                push("save-search", words::save(), true, problem.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.submit_edit(window, cx))),
            ),
        ];
        (words::home::edit_search(), content, buttons)
    }
}

/// What a line of home's menu does that the file menu does not.
#[derive(Clone, Debug, PartialEq)]
pub enum HomeCommand {
    /// A row of home, pinned or put back.
    Pin(PathBuf),
    /// A saved search, by its row's key.
    EditSearch(PathBuf),
    Unsave(PathBuf),
}

impl Colet {
    /// The menu of rows of home: opening them, their paths, and keeping them there. What home never does,
    /// as copying, cutting, pasting or deleting, is not offered. Home picks one row at a time.
    pub(crate) fn home_lines(&self, picked: &[PathBuf], reasons: &Reasons) -> Vec<Line> {
        let [path] = picked else { return Vec::new() };
        let path = path.clone();
        // A row of home is pinned above the rest, or put back.
        let pin = if self.stored.settings().is_pinned(&path) {
            line("menu-unpin", "pin-off", words::unpin(), None, None, HomeCommand::Pin(path.clone()))
        } else {
            line("menu-pin", "pin", words::pin(), None, None, HomeCommand::Pin(path.clone()))
        };
        let in_tab =
            line("menu-open-tab", "app-window", words::open_in_new_tab(), Some("Ctrl+Enter"), None, FileCommand::OpenInTab(path.clone()));
        if HomeKey::of(&path).is_search() {
            return vec![
                line("menu-open", "external-link", words::open(), Some("Enter"), None, FileCommand::Open),
                in_tab,
                line("menu-copy-query", "link", words::home::copy_query(), Some("Ctrl+Shift+C"), None, FileCommand::CopyPath).grouped(),
                line("menu-edit-search", "pencil-line", words::edit(), Some("F2"), None, HomeCommand::EditSearch(path.clone())).grouped(),
                pin,
                line("menu-unsave", "star", words::unsave_search(), None, None, HomeCommand::Unsave(path)),
            ];
        }
        let mut lines = vec![
            line("menu-open", "external-link", words::open(), Some("Enter"), None, FileCommand::Open),
            in_tab,
            line("menu-explorer", "folder-open", words::show_in_explorer(), None, reasons.explorer, FileCommand::Explorer),
            line("menu-copy-path", "link", words::copy_path(), Some("Ctrl+Shift+C"), reasons.copy_path, FileCommand::CopyPath).grouped(),
        ];
        // F2 on a favorite edits it: a place has nothing to edit, and its folder is not renamed from here.
        let favorite = self.stored.settings().is_favorite(&path);
        if favorite {
            lines.push(line("menu-edit-favorite", "pencil-line", words::edit(), Some("F2"), None, FileCommand::Rename));
        }
        lines.push(pin);
        lines.push(line(
            "menu-favorite",
            if favorite { "star-fill" } else { "star" },
            if favorite { words::remove_favorite() } else { words::add_favorite() },
            Some("Ctrl+D"),
            None,
            FileCommand::Favorite(path),
        ));
        lines[4].group = true;
        lines.push(
            line("menu-properties", "info", words::properties(), Some("Alt+Enter"), reasons.properties, FileCommand::Properties).grouped(),
        );
        lines.push(line("menu-windows", "ellipsis", words::more_options(), None, reasons.shell, FileCommand::Windows));
        lines
    }

    /// Does what the line picked in home's menu says.
    pub(crate) fn run_home_command(&mut self, command: HomeCommand, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            HomeCommand::Pin(path) => self.toggle_pin(&path, cx),
            HomeCommand::EditSearch(key) => self.open_edit(&key, window, cx),
            HomeCommand::Unsave(key) => {
                if let HomeKey::Search { query, filter, narrow, order } = HomeKey::of(&key) {
                    self.toggle_saved(query, filter.map(str::to_owned), narrow.map(str::to_owned), order, cx);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Order, SavedSearch, Settings, Shelf, home_entries, home_groups, movable, move_before};
    use std::path::Path;

    #[test]
    fn lists_favorites_then_saved_searches_then_drives_each_once() {
        let settings = Settings {
            favorites: vec![r"D:\Work".into(), r"C:\".into()],
            saved_searches: vec![
                SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None, narrow: None, order: Order::default() },
                SavedSearch {
                    name: "猫".into(), query: "cat".into(), filter: Some("画像".into()), narrow: None, order: Order::default()
                },
            ],
            ..Settings::default()
        };
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")]);
        let rows: Vec<(&str, &str)> = entries.iter().map(|e| (e.path.to_str().unwrap(), e.name.as_str())).collect();
        assert_eq!(rows, [(r"D:\Work", "Work"), (r"C:\", "C:"), ("search:ext:pdf", "PDF"), ("search:cat\0画像", "猫"), (r"D:\", "D:")]);
        assert!(!entries[2].directory);
    }

    #[test]
    fn takes_a_place_in_another_case_as_the_same_but_not_a_query() {
        let search =
            |query: &str| SavedSearch { name: String::new(), query: query.into(), filter: None, narrow: None, order: Order::default() };
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
            saved_searches: vec![SavedSearch {
                name: "PDF".into(),
                query: "ext:pdf".into(),
                filter: None,
                narrow: None,
                order: Order::default(),
            }],
            // A drive unplugged waits unseen; a place is pinned whatever its case.
            pinned: vec!["search:ext:pdf".into(), r"E:\".into(), r"c:\".into(), r"D:\Music".into()],
            ..Settings::default()
        };
        let entries: Vec<std::sync::Arc<Entry>> =
            home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")])
                .into_iter()
                .map(std::sync::Arc::new)
                .collect();
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
