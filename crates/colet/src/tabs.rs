//! The tabs: each its own search, columns and way back and forth. The one in front lives in the window's
//! own fields; the others are parked as they were, listed and watched no more until they come to the
//! front again, when they are listed afresh. The strip they are drawn on is [`strip`]'s.
use std::path::Path;

use colet_core::paths::{folder_name, path_key, shown};
use gpui_kit::*;

use crate::{
    app::Colet,
    menus::{Line, Popup, line, plain},
    settings::{KeptTab, Settings},
    state::{ColumnId, HomeKey, Kind, State},
};

/// What a line of a tab's menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum TabCommand {
    Close(TabId),
    /// Every tab but this one.
    CloseOthers(TabId),
    /// The tabs right of this one.
    CloseRight(TabId),
    Duplicate(TabId),
    Reopen,
}

mod list;
mod strip;

pub use list::{Front, Keys, Parked, TabId, Tabs};
pub use strip::{TabBar, TabLook, TabStrip, TabStripEvent};

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
        let saved = settings.saved(&state.query, state.search_filter.as_deref(), state.search_narrowing().as_deref(), state.search_order);
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
        Some(favorite) => Title { glyph: "star-fill", name: favorite.label(), tip },
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
            let name = state.query.clone();
            let tip = match state.search_filter.as_deref() {
                Some(filter) => format!("{filter} {name}"),
                None => name.clone(),
            };
            (name, tip)
        }
        Kind::Folder => (shown(folder_name(&c.path)), shown(&c.path)),
    }
}

impl Colet {
    pub fn current_tab(&self) -> TabId {
        self.tabs.current_id()
    }

    /// A menu or a dialog open holds the tabs where they are.
    fn tabs_held(&self) -> bool {
        self.overlays.has_dialog() || self.overlays.popup().is_some()
    }

    /// The tabs as they were kept, the one in front among them; with none, home alone.
    pub(crate) fn restore_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (kept, front) = self.stored.take_kept_tabs();
        let front = front.min(kept.len().saturating_sub(1));
        let mut chosen = None;
        for (i, KeptTab { place, filter }) in kept.into_iter().enumerate() {
            let state = State::from_kept(place);
            if i == front {
                self.state = state;
                chosen = filter;
                self.tabs.push(None);
            } else {
                self.tabs.push(Some(Parked::new(state, filter, Keys::Column)));
            }
        }
        if self.tabs.is_empty() {
            chosen = self.default_filter();
            self.tabs.push(None);
        }
        self.state.set_showing(self.showing());
        self.tabs.set_front(front);
        let query = self.state.query.clone();
        self.search.update(cx, |search, cx| {
            search.set_filter(chosen, cx);
            search.show(&query, window, cx);
        });
        self.keep_tabs(cx);
    }

    /// Writes the tabs into the settings when they are not as kept there.
    pub(crate) fn keep_tabs(&mut self, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let chosen = self.chosen_filter(cx);
        let tabs: Vec<KeptTab> = self
            .tabs
            .iter()
            .map(|tab| match tab.parked() {
                Some(Parked { front, .. }) => KeptTab { place: front.state.kept(), filter: front.filter.clone() },
                None => KeptTab { place: self.state.kept(), filter: chosen.clone() },
            })
            .collect();
        let current = self.tabs.current();
        if tabs != self.stored.settings().tabs || current != self.stored.settings().tab {
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
        if self.tabs.is_front(id) {
            return Some(&mut self.state);
        }
        self.tabs.parked_mut(id).map(|parked| &mut parked.front.state)
    }

    /// Where the keys are in the tab in front.
    fn keys(&self, window: &Window, cx: &App) -> Keys {
        if self.search_has_keys(window, cx) {
            return Keys::Search;
        }
        // Open, the field has the keys, or had them until the press that switches tabs.
        let address = self.address.read(cx);
        if address.is_open() {
            let field = address.field().read(cx);
            let text = field.value().to_string();
            return Keys::Address { text, picked: field.selected_range(), reversed: field.is_selection_reversed() };
        }
        match self.narrow_fields.iter().find(|(_, field)| self.has_keys(&field.input, window, cx)) {
            Some((id, _)) => Keys::NarrowField(*id),
            None => Keys::Column,
        }
    }

    /// Puts `incoming` in front, handing back what was there.
    fn swap_in(&mut self, mut incoming: Front, cx: &mut Context<Self>) -> Front {
        // Shown or not while it was parked, the hidden items follow the setting now.
        incoming.state.set_showing(self.showing());
        Front {
            state: std::mem::replace(&mut self.state, incoming.state),
            filter: self.search.update(cx, |search, cx| search.set_filter(incoming.filter, cx)),
            narrow_fields: std::mem::replace(&mut self.narrow_fields, incoming.narrow_fields),
            scrolls: std::mem::replace(&mut self.scrolls, incoming.scrolls),
        }
    }

    /// Brings the tab at `index` to the front, listed afresh and keeping what it picked and where it was
    /// scrolled; the keys go back to where they were in it.
    pub fn switch_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index == self.tabs.current() || self.tabs_held() {
            return;
        }
        let Some(Parked { front: incoming, keys }) = self.tabs.take_parked(index) else { return };
        let left = self.keys(window, cx);
        let outgoing = self.swap_in(incoming, cx);
        self.tabs.bring_to_front(index, Parked { front: outgoing, keys: left });
        let query = self.state.query.clone();
        self.search.update(cx, |search, cx| search.show(&query, window, cx));
        self.relist(cx);
        match keys {
            Keys::Search => {
                self.focus_search(window, cx);
                self.changed(cx);
            }
            Keys::Column => self.come_back_to_active(window, cx),
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
        let Some(index) = self.state.index_of(id) else { return self.come_back_to_active(window, cx) };
        let fresh = !self.narrow_fields.contains_key(&id);
        self.sync_narrow_fields(window, cx);
        let Some(field) = self.narrow_fields.get(&id) else { return self.come_back_to_active(window, cx) };
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
        let count = self.tabs.len() as isize;
        let index = (self.tabs.current() as isize + by).rem_euclid(count) as usize;
        self.switch_tab(index, window, cx);
    }

    /// A tab of home alone at the end, in front with the keys in its search field.
    pub fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() {
            return;
        }
        let at = self.tabs.insert(self.tabs.len(), Parked::new(State::new(), self.default_filter(), Keys::Search));
        self.switch_tab(at, window, cx);
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
            state.set_narrowing(last, c.narrow.as_ref().map(|f| f.text()));
            (state, true)
        } else {
            let Some(entry) = c.entry(path) else { return };
            let state =
                if c.kind == Kind::Home { State::opening_home(c, &entry.path) } else { State::opening(&entry.path, entry.directory) };
            (state, entry.directory)
        };
        // A saved search keeps the filter it was saved with, none too; anything else starts with the default.
        let filter = if state.pinned() { state.search_filter.clone() } else { self.default_filter() };
        let opened = state.columns.last().filter(|last| last.kind != Kind::Home && (directory || state.pinned())).map(|last| last.id);
        if opened.is_some() {
            state.set_active(state.columns.len() - 1);
        }
        let keys = opened.map_or(Keys::Column, Keys::NarrowField);
        self.tabs.insert(self.tabs.current() + 1, Parked::new(state, filter, keys));
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
        let at = self.tabs.insert(self.tabs.current() + 1, Parked::new(state, self.default_filter(), Keys::Column));
        self.switch_tab(at, window, cx);
    }

    /// Closes the tab at `index`; the one right of it comes to the front in its place, or the one left of
    /// it at the end. The last one closes the window.
    pub fn close_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() || index >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            if self.may_close(cx) {
                window.remove_window();
            }
            return;
        }
        if index == self.tabs.current() {
            let next = if index + 1 < self.tabs.len() { index + 1 } else { index - 1 };
            self.switch_tab(next, window, cx);
        }
        self.tabs.close(index);
        self.keep_tabs(cx);
        cx.notify();
    }

    /// Closes every tab but the one at `index`, or those right of it, bringing it to the front if one of
    /// them was.
    pub fn close_tabs_but(&mut self, index: usize, only_right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() || index >= self.tabs.len() {
            return;
        }
        let current = self.tabs.current();
        if current != index && (!only_right || current > index) {
            self.switch_tab(index, window, cx);
        }
        for at in (0..self.tabs.len()).rev() {
            if at > index || (!only_right && at < index) {
                self.tabs.close(at);
            }
        }
        self.keep_tabs(cx);
        cx.notify();
    }

    pub fn can_reopen_tab(&self) -> bool {
        self.tabs.can_reopen()
    }

    /// Ctrl+Shift+T: the tab closed last, where it stood and as it was, in front.
    pub fn reopen_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs_held() {
            return;
        }
        let Some(at) = self.tabs.reopen() else { return };
        self.switch_tab(at, window, cx);
    }

    /// A copy of the tab at `index` beside it, in front.
    pub fn duplicate_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(index) else { return };
        let (state, filter) = match tab.parked() {
            Some(Parked { front, .. }) => (front.state.clone(), front.filter.clone()),
            None => (self.state.clone(), self.chosen_filter(cx)),
        };
        let at = self.tabs.insert(index + 1, Parked::new(state, filter, Keys::Column));
        self.switch_tab(at, window, cx);
    }

    /// The tab carried is put down where it does not land: nothing has moved.
    pub fn end_tab_drag(&mut self, cx: &mut Context<Self>) {
        self.tab_strip.update(cx, |strip, cx| {
            strip.end_drag();
            cx.notify();
        });
    }

    /// The search filter `from` is named `to` now, or is gone: every tab follows it, closed ones too.
    pub fn rename_filter_in_tabs(&mut self, from: &str, to: Option<&str>, cx: &mut Context<Self>) {
        self.state.rename_search_filter(from, to);
        self.search.update(cx, |search, cx| search.rename_filter(from, to, cx));
        self.tabs.rename_filter(from, to);
    }

    pub fn open_tab_menu(&mut self, id: TabId, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlays.has_dialog() {
            return;
        }
        self.overlays.set_popup(Popup::Tab { tab: id, at });
        self.show_menu(window, cx);
    }

    /// The lines of the tab `tab`'s menu.
    pub(crate) fn tab_lines(&self, tab: TabId) -> Vec<Line> {
        let at = self.tabs.position(tab).unwrap_or_default();
        let count = self.tabs.len();
        let reopen = self.can_reopen_tab();
        vec![
            line("tab-close", "x", "閉じる", Some("Ctrl+W"), None, TabCommand::Close(tab)),
            plain("tab-close-others", "他のタブを閉じる", "", count > 1, TabCommand::CloseOthers(tab)),
            plain("tab-close-right", "右のタブを閉じる", "", at + 1 < count, TabCommand::CloseRight(tab)),
            line("tab-duplicate", "copy", "複製", None, None, TabCommand::Duplicate(tab)).grouped(),
            Line {
                off: (!reopen).then_some(""),
                ..line("tab-reopen", "rotate-ccw", "閉じたタブを開き直す", Some("Ctrl+Shift+T"), None, TabCommand::Reopen)
            },
        ]
    }

    /// Does what the line picked in a tab's menu says. The menu goes first, as it holds the tabs where
    /// they are.
    pub(crate) fn run_tab_command(&mut self, command: TabCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(true, window, cx);
        let index = |this: &Self, tab: TabId| this.tabs.position(tab);
        match command {
            TabCommand::Close(tab) => {
                if let Some(index) = index(self, tab) {
                    self.close_tab(index, window, cx);
                }
            }
            TabCommand::CloseOthers(tab) => {
                if let Some(index) = index(self, tab) {
                    self.close_tabs_but(index, false, window, cx);
                }
            }
            TabCommand::CloseRight(tab) => {
                if let Some(index) = index(self, tab) {
                    self.close_tabs_but(index, true, window, cx);
                }
            }
            TabCommand::Duplicate(tab) => {
                if let Some(index) = index(self, tab) {
                    self.duplicate_tab(index, window, cx);
                }
            }
            TabCommand::Reopen => self.reopen_tab(window, cx),
        }
    }

    // ---- The strip ----

    /// The strip of tabs, drawn from each tab's title.
    pub fn tab_bar(&self) -> TabBar {
        let settings = self.stored.settings();
        let tabs = self
            .tabs
            .iter()
            .map(|tab| TabLook { id: tab.id(), title: title(tab.parked().map_or(&self.state, |parked| &parked.front.state), settings) })
            .collect();
        TabBar::new(&self.tab_strip, tabs, self.tabs.current())
    }

    /// What the strip asks for.
    pub(crate) fn on_tab_strip_event(&mut self, event: &TabStripEvent, window: &mut Window, cx: &mut Context<Self>) {
        match *event {
            TabStripEvent::Select(id) => {
                if let Some(index) = self.tabs.position(id) {
                    self.switch_tab(index, window, cx);
                }
            }
            TabStripEvent::Close(id) => {
                if let Some(index) = self.tabs.position(id) {
                    self.close_tab(index, window, cx);
                }
            }
            TabStripEvent::Menu(id, at) => self.open_tab_menu(id, at, window, cx),
            TabStripEvent::New => self.new_tab(window, cx),
            TabStripEvent::Move { tab, gap } => {
                if self.tabs.move_tab(tab, gap) {
                    self.keep_tabs(cx);
                }
                cx.notify();
            }
        }
    }
}
