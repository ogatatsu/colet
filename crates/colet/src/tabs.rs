//! The tabs: each its own search, columns and way back and forth. The one in front lives in the window's
//! own fields; the others are parked as they were, listed and watched no more until they come to the
//! front again, when they are listed afresh.
use std::{collections::HashMap, path::Path};

use colet_core::paths::{folder_name, shown};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::{Colet, FilterField, RowScrolls},
    menus::Popup,
    reveal::Sideways,
    settings::KeptTab,
    state::{ColumnId, Kind, State},
    view::TipExt,
};

pub type TabId = u64;

/// How many closed tabs Ctrl+Shift+T can bring back.
const CLOSED: usize = 10;
/// A tab's width while the strip has room, and the least it narrows to before the strip scrolls.
const TAB_WIDTH: f32 = 220.;
const TAB_MIN: f32 = 100.;
/// What the strip leaves beside the tabs: its padding, the gaps, the button for another and room to
/// double-click.
const BESIDE_TABS: f32 = 16. + 8. + 26. + 48.;

/// A tab out of sight, kept as it was.
pub struct Parked {
    pub state: State,
    /// The filter its next search is narrowed by.
    pub filter: Option<String>,
    filters: HashMap<ColumnId, FilterField>,
    rows: RowScrolls,
    sideways: Sideways,
    home_scroll: ScrollHandle,
    /// The search field had the keys as it went, and has them again as it comes back.
    search_keys: bool,
}

impl Parked {
    fn new(state: State, filter: Option<String>, search_keys: bool) -> Self {
        Self {
            state,
            filter,
            filters: HashMap::new(),
            rows: RowScrolls::default(),
            sideways: Sideways::default(),
            home_scroll: ScrollHandle::new(),
            search_keys,
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

/// What a tab is called, from the column worked in, its icon, and what its tip says in full.
pub struct Title {
    pub glyph: &'static str,
    pub name: String,
    pub tip: String,
}

pub fn title(state: &State) -> Title {
    let c = &state.columns[state.active.min(state.columns.len() - 1)];
    match c.kind {
        Kind::Home => Title { glyph: "house", name: "ホーム".into(), tip: "ホーム".into() },
        Kind::Search => {
            let filter = state.search_filter.as_deref();
            let name = if state.query.is_empty() { filter.unwrap_or_default().to_owned() } else { state.query.clone() };
            let tip = match filter {
                Some(filter) if !state.query.is_empty() => format!("{filter} {}", state.query),
                _ => name.clone(),
            };
            Title { glyph: "search", name, tip }
        }
        Kind::Folder => Title { glyph: "folder", name: shown(folder_name(&c.path)), tip: shown(&c.path) },
    }
}

/// A tab carried along the strip.
#[derive(Clone)]
pub struct DraggedTab {
    id: TabId,
    name: String,
    glyph: &'static str,
    palette: crate::theme::Palette,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        div()
            .w(px(TAB_MIN + 60.))
            .h(px(30.))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(6.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .opacity(0.8)
            .font_family("Segoe UI")
            .text_size(px(13.))
            .text_color(p.text)
            .child(svg().path(format!("icons/{}.svg", self.glyph)).size(px(14.)).flex_none().text_color(p.text_muted))
            .child(div().min_w_0().truncate().child(self.name.clone()))
    }
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
                let tab = self.tabs.tab(Some(Parked::new(state, filter, false)));
                self.tabs.list.push(tab);
            }
        }
        if self.tabs.list.is_empty() {
            let tab = self.tabs.tab(None);
            self.tabs.list.push(tab);
        }
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

    /// Puts `parked` in front, handing back the tab that was.
    fn swap_in(&mut self, parked: Parked) -> Parked {
        let Parked { state, filter, filters, rows, sideways, home_scroll, search_keys: _ } = parked;
        Parked {
            state: std::mem::replace(&mut self.state, state),
            filter: std::mem::replace(&mut self.chosen_filter, filter),
            filters: std::mem::replace(&mut self.filters, filters),
            rows: std::mem::replace(&mut self.rows, rows),
            sideways: std::mem::replace(&mut self.sideways, sideways),
            home_scroll: std::mem::replace(&mut self.home_scroll, home_scroll),
            search_keys: self.search_focused,
        }
    }

    /// Brings the tab at `index` to the front, listed afresh and keeping what it picked and where it was
    /// scrolled; the keys go back to where they were in it.
    pub fn switch_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.tabs.current || self.tabs_held() {
            return;
        }
        let Some(incoming) = self.tabs.list.get_mut(index).and_then(|tab| tab.parked.take()) else { return };
        let search_keys = incoming.search_keys;
        let outgoing = self.swap_in(incoming);
        let current = self.tabs.current;
        self.tabs.list[current].parked = Some(outgoing);
        self.tabs.current = index;
        self.tabs.reveal = true;
        let query = self.state.query.clone();
        self.search.update(cx, |input, cx| input.set_value(query, window, cx));
        self.relist(cx);
        if search_keys {
            self.search.update(cx, |input, cx| input.focus(window, cx));
            self.changed(cx);
        } else {
            self.focus_active(window, cx);
        }
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
        let tab = self.tabs.tab(Some(Parked::new(State::new(), None, true)));
        self.tabs.list.push(tab);
        self.switch_tab(self.tabs.list.len() - 1, window, cx);
    }

    /// Opens the row `path` of the column at `index` in a tab of its own, right of this one and behind it:
    /// a row of home as picking it there opens it, anything else where it lives.
    pub fn open_in_tab(&mut self, index: usize, path: &Path, cx: &mut Context<Self>) {
        let Some(c) = self.state.columns.get(index) else { return };
        let Some(entry) = c.entry(path) else { return };
        let state = if c.kind == Kind::Home { State::opening_home(c, &entry.path) } else { State::opening(&entry.path, entry.directory) };
        let filter = if state.pinned() { state.search_filter.clone() } else { None };
        let tab = self.tabs.tab(Some(Parked::new(state, filter, false)));
        self.tabs.list.insert(self.tabs.current + 1, tab);
        self.keep_tabs(cx);
        cx.notify();
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
        let tab = self.tabs.tab(Some(Parked::new(state, filter, false)));
        self.tabs.list.insert(index + 1, tab);
        if index < self.tabs.current {
            self.tabs.current += 1;
        }
        self.switch_tab(index + 1, window, cx);
    }

    /// The tab `id` dropped where the tab at `to` stands.
    fn move_tab(&mut self, id: TabId, to: usize, cx: &mut Context<Self>) {
        let Some(from) = self.tabs.position(id) else { return };
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
        let room = f32::from(window.viewport_size().width) - BESIDE_TABS - 4. * (count - 1.);
        let width = (room / count).clamp(TAB_MIN, TAB_WIDTH);
        if std::mem::take(&mut self.tabs.reveal) {
            self.tabs.scroll.scroll_to_item(self.tabs.current);
        }
        let tabs: Vec<AnyElement> = (0..self.tabs.list.len()).map(|index| self.render_tab(index, width, cx)).collect();
        div()
            .id("tabbar")
            .h(px(36.))
            .flex_none()
            .w_full()
            .px_2()
            .pt(px(4.))
            .flex()
            .items_end()
            .gap_1()
            .bg(p.sidebar)
            .border_b_1()
            .border_color(p.border)
            .role(Role::TabList)
            .aria_label("タブ")
            .child(
                div()
                    .id("tab-strip")
                    .flex()
                    .min_w_0()
                    .h_full()
                    .items_end()
                    .gap_1()
                    .overflow_x_scroll()
                    .track_scroll(&self.tabs.scroll)
                    .children(tabs),
            )
            .child(
                div()
                    .id("new-tab")
                    .flex_none()
                    .mb(px(3.))
                    .size(px(26.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(|d| d.bg(p.hover))
                    .role(Role::Button)
                    .aria_label("新しいタブ")
                    .tip("新しいタブ（Ctrl+T）")
                    .child(svg().path("icons/plus.svg").size(px(14.)).text_color(p.text_muted))
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx)))
                    .test_support(),
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

    fn render_tab(&mut self, index: usize, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let tab = &self.tabs.list[index];
        let id = tab.id;
        let current = index == self.tabs.current;
        let Title { glyph, name, tip } = title(tab.parked.as_ref().map_or(&self.state, |parked| &parked.state));
        let dragged = DraggedTab { id, name: name.clone(), glyph, palette: p };
        div()
            .id(("tab", id as usize))
            .h(px(32.))
            .flex_none()
            .w(px(width))
            .pl(px(10.))
            .pr(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded_t(px(6.))
            .role(Role::Tab)
            .aria_label(name.clone())
            .aria_selected(current)
            .text_color(if current { p.text } else { p.text_muted })
            .when(current, |d| d.bg(p.bg).border_1().border_b_0().border_color(p.border))
            .when(!current, |d| d.hover(|d| d.bg(p.hover)))
            .tip(tip)
            .child(svg().path(format!("icons/{glyph}.svg")).size(px(14.)).flex_none().text_color(if current { p.accent } else { p.text_muted }))
            .child(div().flex_1().min_w_0().truncate().child(name))
            .child(
                div()
                    .id(("tab-close", id as usize))
                    .flex_none()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(|d| d.bg(p.raised_hover))
                    .role(Role::Button)
                    .aria_label("タブを閉じる")
                    .tip("タブを閉じる（Ctrl+W）")
                    .child(svg().path("icons/x.svg").size(px(12.)).text_color(p.text_muted))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if let Some(index) = this.tabs.position(id) {
                            this.close_tab(index, window, cx);
                        }
                    }))
                    .test_support(),
            )
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
            .on_drag(dragged, |dragged, _, _, cx| cx.new(|_| dragged.clone()))
            .drag_over::<DraggedTab>(move |style, _, _, _| style.bg(p.hover))
            .on_drop(cx.listener(move |this, dragged: &DraggedTab, _, cx| {
                if let Some(to) = this.tabs.position(id) {
                    this.move_tab(dragged.id, to, cx);
                }
            }))
            .test_support()
            .into_any_element()
    }
}
