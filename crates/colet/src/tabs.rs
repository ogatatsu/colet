//! The tabs: each its own search, columns and way back and forth. The one in front lives in the window's
//! own fields; the others are parked as they were, listed and watched no more until they come to the
//! front again, when they are listed afresh.
use std::{collections::HashMap, path::Path};

use colet_core::paths::{folder_name, path_key, shown};
use gpui_kit::component::{
    Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::{Colet, NarrowField, RowScrolls},
    menus::Popup,
    metrics,
    reveal::Sideways,
    settings::{KeptTab, Settings},
    state::{ColumnId, HomeKey, Kind, State},
    view::TipExt,
};

pub type TabId = u64;

/// How many closed tabs Ctrl+Shift+T can bring back.
const CLOSED: usize = 10;
/// A tab's width while the strip has room, and the least it narrows to before the strip scrolls.
const TAB_WIDTH: f32 = 220.;
const TAB_MIN: f32 = 100.;
const TAB_HEIGHT: f32 = 32.;
const TAB_TOP: f32 = 4.;
const TAB_CURVE: f32 = 8.;
/// What the strip leaves beside the tabs: its padding on the right, the gaps, the button for another
/// and room to double-click, and the curves at the strip's ends.
const BESIDE_TABS: f32 = 8. + 8. + 26. + 48. + TAB_CURVE * 2.;

/// A tab out of sight, kept as it was.
pub struct Parked {
    pub state: State,
    /// The filter its next search is narrowed by.
    pub filter: Option<String>,
    narrow_fields: HashMap<ColumnId, NarrowField>,
    rows: RowScrolls,
    sideways: Sideways,
    home_scroll: ScrollHandle,
    /// Where the keys were as it went, and go again as it comes back.
    keys: Keys,
}

/// Where a tab's keys are.
#[derive(Clone, PartialEq, Debug)]
pub enum Keys {
    Search,
    Column,
    /// The field narrowing the column.
    NarrowField(ColumnId),
    /// The address being typed, as it was left: what it holds and the text picked in it.
    Address {
        text: String,
        picked: std::ops::Range<usize>,
        reversed: bool,
    },
}

impl Parked {
    fn new(state: State, filter: Option<String>, keys: Keys) -> Self {
        Self {
            state,
            filter,
            narrow_fields: HashMap::new(),
            rows: RowScrolls::default(),
            sideways: Sideways::default(),
            home_scroll: ScrollHandle::new(),
            keys,
        }
    }
}

pub struct Tab {
    pub id: TabId,
    /// None for the tab in front.
    pub parked: Option<Parked>,
}

#[derive(Default)]
pub struct Tabs {
    pub list: Vec<Tab>,
    /// The tab in front.
    pub current: usize,
    next_id: TabId,
    /// The tabs closed, the latest last, each with where it stood.
    closed: Vec<(usize, Tab)>,
    pub scroll: ScrollHandle,
    /// The tab in front is to be brought in sight on the strip.
    reveal: bool,
    pub drag: TabDrag,
}

/// A tab carried along the strip, and the gap between tabs it would land in.
#[derive(Default)]
pub struct TabDrag {
    pub carried: Option<TabId>,
    /// 0 before the first tab, up to the count after the last; none while the pointer is off the strip.
    pub gap: Option<usize>,
}

impl Tabs {
    fn tab(&mut self, parked: Option<Parked>) -> Tab {
        self.next_id += 1;
        Tab { id: self.next_id, parked }
    }

    fn position(&self, id: TabId) -> Option<usize> {
        self.list.iter().position(|tab| tab.id == id)
    }
}

/// What a tab is called, its icon, and what its tip says in full.
pub struct Title {
    pub glyph: &'static str,
    pub name: String,
    pub tip: String,
}

/// A tab is called for what it started from in home, as long as that leads it: the saved search it runs,
/// else the search it made, else the favorite it opened, however deep it has gone since. Else, for the
/// column worked in, or, while home is, for the place picked in it. The tip tells the column worked in, or
/// what home opened while home is worked in.
pub fn title(state: &State, settings: &Settings) -> Title {
    let active = state.active.min(state.columns.len() - 1);
    let (name, tip) = column_title(state, active);
    if state.pinned() {
        let tip = if active == 0 { column_title(state, 1).1 } else { tip };
        let saved = settings.saved(&state.query, state.search_filter.as_deref(), state.search_narrowing().as_deref());
        let name = saved.map_or_else(|| column_title(state, 1).0, |s| s.label().to_owned());
        return Title { glyph: "search", name, tip };
    }
    // The column the row picked in home opened, while it is open still; the folders above it may be folded
    // before it.
    let opened = match state.columns[0].selected.as_slice() {
        [key] => {
            let path = HomeKey::on_disk(key);
            state.columns.iter().rposition(|c| c.kind == Kind::Folder && path_key(&c.path) == path_key(&path))
        }
        _ => None,
    };
    let Some(at) = opened else { return Title { glyph: if active == 0 { "house" } else { "folder" }, name, tip } };
    let opened = &state.columns[at];
    let (place, place_tip) = column_title(state, at);
    let tip = if active == 0 { place_tip } else { tip };
    match settings.favorite(&opened.path, opened.narrowing().as_deref()) {
        Some(favorite) => Title { glyph: "star", name: favorite.label(), tip },
        None if active == 0 => Title { glyph: "folder", name: place, tip },
        None => Title { glyph: "folder", name, tip },
    }
}

/// The name and tip of the column at `index`, alone.
fn column_title(state: &State, index: usize) -> (String, String) {
    let c = &state.columns[index];
    match c.kind {
        Kind::Home => ("ホーム".into(), "ホーム".into()),
        Kind::Search => {
            let filter = state.search_filter.as_deref();
            let name = if state.query.is_empty() { filter.unwrap_or_default().to_owned() } else { state.query.clone() };
            let tip = match filter {
                Some(filter) if !state.query.is_empty() => format!("{filter} {}", state.query),
                _ => name.clone(),
            };
            (name, tip)
        }
        Kind::Folder => (shown(folder_name(&c.path)), shown(&c.path)),
    }
}

/// A tab carried along the strip.
#[derive(Clone)]
pub struct DraggedTab {
    id: TabId,
    name: String,
    glyph: &'static str,
    /// The tab's width on the strip, curves aside.
    width: f32,
    palette: crate::theme::Palette,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        // As wide as the tab and held where it was picked up; the curves reach out beside it. Faint
        // enough that the landing line it passes over still shows through.
        div()
            .relative()
            .w(px(self.width))
            .h(px(TAB_HEIGHT))
            .pl(px(10.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(tab_shape(p.bg, Some(p.border)))
            .child(svg().path(format!("icons/{}.svg", self.glyph)).size(px(14.)).flex_none().text_color(p.text))
            .child(div().min_w_0().truncate().child(self.name.clone()))
            .opacity(0.5)
            .font_family("Segoe UI")
            .text_size(px(metrics::TEXT_LARGE))
            .text_color(p.text)
    }
}

/// A small button with a glyph on the tab bar, which the mouse alone presses: Tab passes it by.
fn tab_button(p: crate::theme::Palette, id: impl Into<ElementId>, glyph: &str, label: &'static str, tip: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .rounded(px(4.))
        .icon(crate::view::glyph(glyph))
        .accessibility_label(label)
        .tooltip(tip)
        .tab_stop(false)
        .flex_none()
        .text_color(p.text_muted)
}

/// Decoration only: the tab's hitbox stays rectangular and the curves have no mouse listeners.
fn tab_shape(background: Hsla, border: Option<Hsla>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let mut fill = PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::tolerance(0.02)));
            trace_tab(&mut fill, bounds);
            fill.close();
            if let Ok(path) = fill.build() {
                window.paint_path(path, background);
            }
            if let Some(border) = border {
                let mut outline = PathBuilder::stroke(px(1.));
                trace_tab(&mut outline, bounds);
                // Leave the bottom open to the toolbar.
                if let Ok(path) = outline.build() {
                    window.paint_path(path, border);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .bottom_0()
    .left(px(-TAB_CURVE))
    .right(px(-TAB_CURVE))
}

fn trace_tab(path: &mut PathBuilder, bounds: Bounds<Pixels>) {
    let curve = px(TAB_CURVE);
    // Keep the one-pixel outline inside the canvas, including at the scroll viewport's ends.
    let left = bounds.left() + px(0.5);
    let right = bounds.right() - px(0.5);
    let top = bounds.top() + px(0.5);
    let bottom = bounds.bottom();
    let tab_left = left + curve;
    let tab_right = right - curve;
    path.move_to(point(left, bottom));
    tab_corner(path, point(left, bottom), point(tab_left, bottom), point(tab_left, bottom - curve));
    path.line_to(point(tab_left, top + curve));
    tab_corner(path, point(tab_left, top + curve), point(tab_left, top), point(tab_left + curve, top));
    path.line_to(point(tab_right - curve, top));
    tab_corner(path, point(tab_right - curve, top), point(tab_right, top), point(tab_right, top + curve));
    path.line_to(point(tab_right, bottom - curve));
    tab_corner(path, point(tab_right, bottom - curve), point(tab_right, bottom), point(right, bottom));
}

fn tab_corner(path: &mut PathBuilder, from: Point<Pixels>, corner: Point<Pixels>, to: Point<Pixels>) {
    const K: f32 = 0.552_284_8;
    path.cubic_bezier_to(to, from + (corner - from) * K, to + (corner - to) * K);
}

impl Colet {
    pub fn current_tab(&self) -> TabId {
        self.tabs.list[self.tabs.current].id
    }

    /// A menu or a dialog open holds the tabs where they are.
    fn tabs_held(&self) -> bool {
        self.dialog.is_some() || self.popup.is_some()
    }

    /// The tabs as they were kept, the one in front among them; with none, home alone.
    pub(crate) fn restore_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let kept = std::mem::take(&mut self.settings.tabs);
        let front = self.settings.tab.min(kept.len().saturating_sub(1));
        for (i, KeptTab { place, filter }) in kept.into_iter().enumerate() {
            let state = State::from_kept(place);
            if i == front {
                self.state = state;
                self.chosen_filter = filter;
                let tab = self.tabs.tab(None);
                self.tabs.list.push(tab);
            } else {
                let tab = self.tabs.tab(Some(Parked::new(state, filter, Keys::Column)));
                self.tabs.list.push(tab);
            }
        }
        if self.tabs.list.is_empty() {
            let tab = self.tabs.tab(None);
            self.tabs.list.push(tab);
        }
        self.state.set_showing(self.showing());
        self.tabs.current = front;
        self.tabs.reveal = true;
        let query = self.state.query.clone();
        self.search.update(cx, |input, cx| input.set_value(query, window, cx));
        self.keep_tabs(cx);
    }

    /// Writes the tabs into the settings when they are not as kept there.
    pub(crate) fn keep_tabs(&mut self, cx: &mut Context<Self>) {
        if self.tabs.list.is_empty() {
            return;
        }
        let tabs: Vec<KeptTab> = self
            .tabs
            .list
            .iter()
            .map(|tab| match &tab.parked {
                Some(parked) => KeptTab { place: parked.state.kept(), filter: parked.filter.clone() },
                None => KeptTab { place: self.state.kept(), filter: self.chosen_filter.clone() },
            })
            .collect();
        let current = self.tabs.current;
        if tabs != self.settings.tabs || current != self.settings.tab {
            self.update_settings(
                move |s| {
                    s.tabs = tabs;
                    s.tab = current;
                },
                cx,
            );
        }
    }

    /// The state of the tab `id`, in front or parked; none once it is closed.
    pub fn state_in(&mut self, id: TabId) -> Option<&mut State> {
        let at = self.tabs.position(id)?;
        match &mut self.tabs.list[at].parked {
            None => Some(&mut self.state),
            Some(parked) => Some(&mut parked.state),
        }
    }

    /// Where the keys are in the tab in front.
    fn keys(&self, window: &Window, cx: &App) -> Keys {
        if self.search_focused {
            return Keys::Search;
        }
        // Open, the field has the keys, or had them until the press that switches tabs.
        if self.address.is_open() {
            let field = self.address.field.read(cx);
            let text = field.value().to_string();
            return Keys::Address { text, picked: field.selected_range(), reversed: field.is_selection_reversed() };
        }
        match self.narrow_fields.iter().find(|(_, field)| self.has_keys(&field.input, window, cx)) {
            Some((id, _)) => Keys::NarrowField(*id),
            None => Keys::Column,
        }
    }

    /// Puts `parked` in front, handing back the tab that was with the keys where they are in it.
    fn swap_in(&mut self, parked: Parked, keys: Keys) -> Parked {
        let Parked { mut state, filter, narrow_fields: filters, rows, sideways, home_scroll, keys: _ } = parked;
        // Shown or not while it was parked, the hidden items follow the setting now.
        state.set_showing(self.showing());
        Parked {
            state: std::mem::replace(&mut self.state, state),
            filter: std::mem::replace(&mut self.chosen_filter, filter),
            narrow_fields: std::mem::replace(&mut self.narrow_fields, filters),
            rows: std::mem::replace(&mut self.rows, rows),
            sideways: std::mem::replace(&mut self.sideways, sideways),
            home_scroll: std::mem::replace(&mut self.home_scroll, home_scroll),
            keys,
        }
    }

    /// Brings the tab at `index` to the front, listed afresh and keeping what it picked and where it was
    /// scrolled; the keys go back to where they were in it.
    pub fn switch_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.tabs.current || self.tabs_held() {
            return;
        }
        let Some(incoming) = self.tabs.list.get_mut(index).and_then(|tab| tab.parked.take()) else { return };
        let keys = incoming.keys.clone();
        let outgoing = self.swap_in(incoming, self.keys(window, cx));
        let current = self.tabs.current;
        self.tabs.list[current].parked = Some(outgoing);
        self.tabs.current = index;
        self.tabs.reveal = true;
        let query = self.state.query.clone();
        self.search.update(cx, |input, cx| input.set_value(query, window, cx));
        self.relist(cx);
        match keys {
            Keys::Search => {
                self.search.update(cx, |input, cx| input.focus(window, cx));
                self.changed(cx);
            }
            Keys::Column => self.focus_active(window, cx),
            Keys::NarrowField(id) => self.focus_narrow_field(id, window, cx),
            Keys::Address { text, picked, reversed } => {
                self.reopen_address(text, picked, reversed, window, cx);
                self.changed(cx);
            }
        }
    }

    /// The keys go to the field narrowing the column `id`, which becomes the active one; a field made just
    /// now, for a tab never in front before, has the caret after what it holds. With the column gone, they
    /// go to the active one.
    fn focus_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return self.focus_active(window, cx) };
        let fresh = !self.narrow_fields.contains_key(&id);
        self.sync_narrow_fields(window, cx);
        let Some(field) = self.narrow_fields.get(&id) else { return self.focus_active(window, cx) };
        let input = field.input.clone();
        self.state.set_active(index);
        input.update(cx, |input, cx| {
            if fresh {
                input.set_cursor_position(gpui_kit::base::input::Position::new(0, u32::MAX), window, cx);
            } else {
                input.focus(window, cx);
            }
        });
        self.changed(cx);
    }

    /// Ctrl+Tab and Ctrl+Shift+Tab: the next tab along the strip, or the one before, round from the end.
    pub fn step_tab(&mut self, by: isize, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.tabs.list.len() as isize;
        let index = (self.tabs.current as isize + by).rem_euclid(count) as usize;
        self.switch_tab(index, window, cx);
    }

    /// A tab of home alone at the end, in front with the keys in its search field.
    pub fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() {
            return;
        }
        let tab = self.tabs.tab(Some(Parked::new(State::new(), None, Keys::Search)));
        self.tabs.list.push(tab);
        self.switch_tab(self.tabs.list.len() - 1, window, cx);
    }

    /// Opens the row `path` of the column at `index` in a tab of its own, right of this one and behind it:
    /// a row of home as picking it there opens it, anything else where it lives. A folder or a search opened
    /// has the keys in the field narrowing its column once the tab comes to the front, and is worked in, and
    /// so named, from the start; a file, on its row. A folder column's own folder opens narrowed as it is.
    pub fn open_in_tab(&mut self, index: usize, path: &Path, cx: &mut Context<Self>) {
        let Some(c) = self.state.columns.get(index) else { return };
        let (mut state, directory) = if c.kind == Kind::Folder && c.path == path {
            let mut state = State::opening(path, true);
            let last = state.columns.len() - 1;
            state.set_narrowing(last, c.narrow.as_ref().map(|f| f.text.as_str()));
            (state, true)
        } else {
            let Some(entry) = c.entry(path) else { return };
            let state =
                if c.kind == Kind::Home { State::opening_home(c, &entry.path) } else { State::opening(&entry.path, entry.directory) };
            (state, entry.directory)
        };
        let filter = if state.pinned() { state.search_filter.clone() } else { None };
        let opened = state.columns.last().filter(|last| last.kind != Kind::Home && (directory || state.pinned())).map(|last| last.id);
        if opened.is_some() {
            state.set_active(state.columns.len() - 1);
        }
        let keys = opened.map_or(Keys::Column, Keys::NarrowField);
        let tab = self.tabs.tab(Some(Parked::new(state, filter, keys)));
        self.tabs.list.insert(self.tabs.current + 1, tab);
        self.keep_tabs(cx);
        cx.notify();
    }

    /// Opens `path` in a tab of its own right of this one, in front: a folder with its contents worked in,
    /// a file where it lives, its row with the keys. The folders above fold away, as a search result opens.
    pub fn open_path_in_tab(&mut self, path: &Path, directory: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() {
            return;
        }
        let mut state = State::opening(path, directory);
        if directory {
            state.set_active(state.columns.len() - 1);
        }
        let tab = self.tabs.tab(Some(Parked::new(state, None, Keys::Column)));
        self.tabs.list.insert(self.tabs.current + 1, tab);
        self.switch_tab(self.tabs.current + 1, window, cx);
    }

    /// Closes the tab at `index`; the one right of it comes to the front in its place, or the one left of
    /// it at the end. The last one closes the window.
    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() || index >= self.tabs.list.len() {
            return;
        }
        if self.tabs.list.len() == 1 {
            if self.may_close(cx) {
                window.remove_window();
            }
            return;
        }
        if index == self.tabs.current {
            let next = if index + 1 < self.tabs.list.len() { index + 1 } else { index - 1 };
            self.switch_tab(next, window, cx);
        }
        self.drop_tab(index);
        self.keep_tabs(cx);
        cx.notify();
    }

    /// Takes the tab at `index`, out of sight, off the strip, to be brought back.
    fn drop_tab(&mut self, index: usize) {
        let tab = self.tabs.list.remove(index);
        if index < self.tabs.current {
            self.tabs.current -= 1;
        }
        self.tabs.closed.push((index, tab));
        if self.tabs.closed.len() > CLOSED {
            self.tabs.closed.remove(0);
        }
    }

    /// Closes every tab but the one at `index`, or those right of it, bringing it to the front if one of
    /// them was.
    pub fn close_tabs_but(&mut self, index: usize, only_right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() || index >= self.tabs.list.len() {
            return;
        }
        if self.tabs.current != index && (!only_right || self.tabs.current > index) {
            self.switch_tab(index, window, cx);
        }
        for at in (0..self.tabs.list.len()).rev() {
            if at > index || (!only_right && at < index) {
                self.drop_tab(at);
            }
        }
        self.keep_tabs(cx);
        cx.notify();
    }

    pub fn can_reopen_tab(&self) -> bool {
        !self.tabs.closed.is_empty()
    }

    /// Ctrl+Shift+T: the tab closed last, where it stood and as it was, in front.
    pub fn reopen_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() {
            return;
        }
        let Some((at, tab)) = self.tabs.closed.pop() else { return };
        let at = at.min(self.tabs.list.len());
        self.tabs.list.insert(at, tab);
        if at <= self.tabs.current {
            self.tabs.current += 1;
        }
        self.switch_tab(at, window, cx);
    }

    /// A copy of the tab at `index` beside it, in front.
    pub fn duplicate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.list.get(index) else { return };
        let (state, filter) = match &tab.parked {
            Some(parked) => (parked.state.clone(), parked.filter.clone()),
            None => (self.state.clone(), self.chosen_filter.clone()),
        };
        let tab = self.tabs.tab(Some(Parked::new(state, filter, Keys::Column)));
        self.tabs.list.insert(index + 1, tab);
        if index < self.tabs.current {
            self.tabs.current += 1;
        }
        self.switch_tab(index + 1, window, cx);
    }

    /// The tab `id` dropped in the gap `gap`: 0 before the first tab, up to the count after the last.
    fn move_tab(&mut self, id: TabId, gap: usize, cx: &mut Context<Self>) {
        let Some(from) = self.tabs.position(id) else { return };
        // Taken out first, the tabs past it close up by one.
        let to = if gap > from { gap - 1 } else { gap };
        if from == to {
            return;
        }
        let front = self.current_tab();
        let tab = self.tabs.list.remove(from);
        self.tabs.list.insert(to.min(self.tabs.list.len()), tab);
        self.tabs.current = self.tabs.position(front).unwrap_or_default();
        self.keep_tabs(cx);
        cx.notify();
    }

    /// A tab is picked up: it fades where it was.
    fn start_tab_drag(&mut self, id: TabId, cx: &mut Context<Self>) {
        self.tabs.drag = TabDrag { carried: Some(id), gap: None };
        cx.notify();
    }

    /// The pointer moved in a tab's drag, with the tab bar drawn at `bar` and its tabs `width` wide.
    fn aim_tab_drag(&mut self, at: Point<Pixels>, bar: Bounds<Pixels>, width: f32, cx: &mut Context<Self>) {
        if self.tabs.drag.carried.is_none() {
            return;
        }
        let gap = bar.contains(&at).then(|| {
            let strip = self.tabs.scroll.bounds();
            let along = f32::from(at.x - strip.left() - self.tabs.scroll.offset().x) - TAB_CURVE;
            // Over a tab's left half it goes before it, over its right half after; past the last, last.
            ((along / width).round().max(0.) as usize).min(self.tabs.list.len())
        });
        if self.tabs.drag.gap != gap {
            self.tabs.drag.gap = gap;
            cx.notify();
        }
    }

    fn land_tab(&mut self, dragged: &DraggedTab, cx: &mut Context<Self>) {
        let gap = self.tabs.drag.gap;
        self.end_tab_drag();
        if let Some(gap) = gap {
            self.move_tab(dragged.id, gap, cx);
        }
        cx.notify();
    }

    pub fn end_tab_drag(&mut self) {
        self.tabs.drag = TabDrag::default();
    }

    /// The search filter `from` is named `to` now, or is gone: every tab follows it, closed ones too.
    pub fn rename_filter_in_tabs(&mut self, from: &str, to: Option<&str>) {
        let follow = |named: &mut Option<String>| {
            if named.as_deref() == Some(from) {
                *named = to.map(Into::into);
            }
        };
        self.state.rename_search_filter(from, to);
        follow(&mut self.chosen_filter);
        let closed = self.tabs.closed.iter_mut().map(|(_, tab)| tab);
        for parked in self.tabs.list.iter_mut().chain(closed).filter_map(|tab| tab.parked.as_mut()) {
            parked.state.rename_search_filter(from, to);
            follow(&mut parked.filter);
        }
    }

    pub fn open_tab_menu(&mut self, id: TabId, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            return;
        }
        self.popup = Some(Popup::Tab { tab: id, at });
        self.show_menu(window, cx);
    }

    // ---- Drawing ----

    /// The strip of tabs over the window, and room right of them to open another.
    pub fn render_tabbar(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        // The tabs share the room there is, down to their least; past that the strip scrolls.
        let count = self.tabs.list.len() as f32;
        let room = f32::from(window.viewport_size().width) - BESIDE_TABS;
        let width = (room / count).clamp(TAB_MIN, TAB_WIDTH);
        if self.tabs.reveal {
            // Before the first prepaint the handle does not yet know that it scrolls horizontally;
            // scroll_to_item would consume the request without moving it. Try on the next frame.
            if self.tabs.scroll.bounds().size.width > px(0.) {
                self.tabs.reveal = false;
                self.tabs.scroll.scroll_to_item(self.tabs.current);
            } else {
                window.request_animation_frame();
            }
        }
        let current = self.tabs.current;
        let count = self.tabs.list.len();
        let dragging = self.tabs.drag.carried.is_some();
        let mut tabs = Vec::with_capacity(count * 2);
        for index in 0..count {
            // Keep one cell per tab, in order, so scroll_to_item includes both curves. The selected
            // tab is painted last, over its neighbors' hover fills, within this same scroll clip.
            let cell = div()
                .flex_none()
                .w(px(width + TAB_CURVE * 2.))
                .h_full()
                .pt(px(TAB_TOP))
                .px(px(TAB_CURVE))
                .when(index + 1 < count, |d| d.mr(px(-TAB_CURVE * 2.)))
                .when(index != current, |d| d.child(self.render_tab(index, width, cx).test_support()));
            tabs.push(cell.into_any_element());
        }
        for index in 0..count.saturating_sub(1) {
            if index == current || index + 1 == current {
                continue;
            }
            // This passive hover region spans the two neighboring tab bodies. Its opacity hides
            // their shared separator without changing hitboxes or storing pointer state.
            tabs.push(
                div()
                    .id(("tab-separator", self.tabs.list[index].id as usize))
                    .absolute()
                    .left(px(index as f32 * width + TAB_CURVE))
                    .top(px(TAB_TOP))
                    .w(px(width * 2.))
                    .h(px(TAB_HEIGHT))
                    // While a tab is carried, only the line tells where it goes.
                    .when(!dragging, |d| d.hover(|s| s.opacity(0.)))
                    .child(div().absolute().left(px(width - 1.)).top(px(8.)).w(px(1.)).h(px(16.)).bg(p.border))
                    .into_any_element(),
            );
        }
        tabs.push(
            self.render_tab(current, width, cx)
                .absolute()
                .left(px(current as f32 * width + TAB_CURVE))
                .top(px(TAB_TOP))
                .test_support()
                .into_any_element(),
        );
        if let Some(gap) = self.tabs.drag.gap.filter(|_| dragging) {
            // Over every tab, at the edge between the bodies the carried one would land between.
            tabs.push(
                div()
                    .id("tab-landing")
                    .absolute()
                    .left(px(TAB_CURVE + gap as f32 * width - 1.))
                    .top(px(TAB_TOP))
                    .w(px(2.))
                    .h(px(TAB_HEIGHT))
                    .rounded(px(1.))
                    .bg(p.accent)
                    .test_support()
                    .into_any_element(),
            );
        }
        div()
            .id("tabbar")
            .on_drag_move(cx.listener(move |this, event: &DragMoveEvent<DraggedTab>, _, cx| {
                this.aim_tab_drag(event.event.position, event.bounds, width, cx)
            }))
            .on_drop(cx.listener(|this, dragged: &DraggedTab, _, cx| this.land_tab(dragged, cx)))
            .h(px(TAB_HEIGHT + TAB_TOP))
            .flex_none()
            .w_full()
            .relative()
            .pr_2()
            .flex()
            .items_center()
            .gap_1()
            .bg(p.sidebar)
            .role(Role::TabList)
            .aria_label("タブ")
            // The selected tab paints over this line to join the toolbar below.
            .child(div().absolute().left_0().right_0().bottom_0().h(px(1.)).bg(p.border))
            .child(
                div()
                    .id("tab-strip")
                    .relative()
                    .flex()
                    .min_w_0()
                    .h_full()
                    .items_end()
                    .overflow_x_scroll()
                    .track_scroll(&self.tabs.scroll)
                    .children(tabs),
            )
            .child(
                tab_button(p, "new-tab", "plus", "新しいタブ", "新しいタブ（Ctrl+T）")
                    .small()
                    .size(px(26.))
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            )
            // A double-click in the room left opens another, as in a browser.
            .child(
                div()
                    .id("tab-room")
                    .flex_1()
                    .h_full()
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            this.new_tab(window, cx);
                        }
                    }))
                    .test_support(),
            )
            .into_any_element()
    }

    fn render_tab(&mut self, index: usize, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.palette;
        let tab = &self.tabs.list[index];
        let id = tab.id;
        let current = index == self.tabs.current;
        let group = SharedString::from(format!("tab-{id}"));
        let Title { glyph, name, tip } = title(tab.parked.as_ref().map_or(&self.state, |parked| &parked.state), &self.settings);
        let dragged = DraggedTab { id, name: name.clone(), glyph, width, palette: p };
        let dragging = self.tabs.drag.carried.is_some();
        let carried = self.tabs.drag.carried == Some(id);
        let view = cx.entity().downgrade();
        div()
            .id(("tab", id as usize))
            .group(group.clone())
            .h(px(TAB_HEIGHT))
            .flex_none()
            .w(px(width))
            .relative()
            .pl(px(10.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .role(Role::Tab)
            .aria_label(name.clone())
            .aria_selected(current)
            .aria_position_in_set(index + 1)
            .aria_size_of_set(self.tabs.list.len())
            .text_color(if current { p.text } else { p.text_muted })
            .when(current, |d| d.child(tab_shape(p.bg, Some(p.border))))
            .when(!current && !dragging, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom(px(2.))
                        .left_0()
                        .right_0()
                        .invisible()
                        .group_hover(group, |s| s.visible())
                        .child(tab_shape(p.hover, None)),
                )
            })
            .when(carried, |d| d.opacity(0.4))
            .tip(tip)
            .child(svg().path(format!("icons/{glyph}.svg")).size(px(14.)).flex_none().text_color(if current {
                p.text
            } else {
                p.text_muted
            }))
            .child(div().flex_1().min_w_0().truncate().child(name))
            .child(tab_button(p, ("tab-close", id as usize), "x", "タブを閉じる", "タブを閉じる（Ctrl+W）").xsmall().on_click(cx.listener(
                move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) = this.tabs.position(id) {
                        this.close_tab(index, window, cx);
                    }
                },
            )))
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(index) = this.tabs.position(id) {
                    this.switch_tab(index, window, cx);
                }
            }))
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) = this.tabs.position(id) {
                        this.close_tab(index, window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_tab_menu(id, event.position, window, cx);
                }),
            )
            .on_drag(dragged, move |dragged, _, _, cx| {
                view.update(cx, |this, cx| this.start_tab_drag(dragged.id, cx)).ok();
                cx.new(|_| dragged.clone())
            })
    }
}
