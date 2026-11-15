//! The search field and the search filter chosen for it. What is typed is searched within the filter
//! chosen; an empty field searches for nothing. Enter or ↓ leaves the field for the results, Esc empties
//! it and leaves the filter as it is.
//!
//! [`SearchBar`] owns the field and the filter chosen, the one the tab in front's next search is narrowed
//! by; it tells what is typed through [`SearchEvent`]. Colet owns the rest: the search itself, kept in the
//! tab's state with the way back and forth, the filters there are, and handing the filter chosen to and
//! from the tabs parked.
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::app::Colet;

mod view;

/// What the empty search field says.
const PLACEHOLDER: &str = "Everythingで検索（Ctrl+F）";

/// The search field and the filter chosen for the next search in the tab in front.
pub struct SearchBar {
    field: Entity<InputState>,
    filter: Option<String>,
    _typed: Subscription,
}

/// What is done in the field. Neither is told of what Colet puts in the field itself.
pub enum SearchEvent {
    /// The field holds `query` now, typed or pasted, to be searched for.
    Typed(String),
    /// Enter: the keys go on to the results.
    Leave,
}

impl EventEmitter<SearchEvent> for SearchBar {}

impl SearchBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let typed = cx.subscribe_in(&field, window, |_: &mut Self, input, event, _, cx| match event {
            InputEvent::Change => cx.emit(SearchEvent::Typed(input.read(cx).value().to_string())),
            InputEvent::PressEnter { .. } => cx.emit(SearchEvent::Leave),
            // The ring around the field follows the keys.
            InputEvent::Focus | InputEvent::Blur => cx.notify(),
        });
        Self { field, filter: None, _typed: typed }
    }

    pub fn field(&self) -> &Entity<InputState> {
        &self.field
    }

    /// The filter chosen, by name; none searches unnarrowed.
    pub fn filter(&self) -> Option<&str> {
        self.filter.as_deref()
    }

    /// Chooses `filter` for the next search, handing back the one chosen before.
    pub fn set_filter(&mut self, filter: Option<String>, cx: &mut Context<Self>) -> Option<String> {
        cx.notify();
        std::mem::replace(&mut self.filter, filter)
    }

    /// The filter `from` is named `to` now, or is gone.
    pub fn rename_filter(&mut self, from: &str, to: Option<&str>, cx: &mut Context<Self>) {
        if self.filter.as_deref() == Some(from) {
            self.filter = to.map(Into::into);
            cx.notify();
        }
    }

    /// The query the field holds.
    pub fn query(&self, cx: &App) -> SharedString {
        self.field.read(cx).value()
    }

    /// Puts `query` in the field, the caret at its start. Nothing is told: the query is the search
    /// already.
    pub fn show(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.field.update(cx, |input, cx| input.set_value(query.to_owned(), window, cx));
    }

    /// The keys go to the field.
    pub fn focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.field.update(cx, |input, cx| input.focus(window, cx));
    }

    /// The keys go to the field, all it holds picked to be typed over.
    pub fn focus_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.field.update(cx, |input, cx| {
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }
}

impl Colet {
    /// The search field's text field.
    pub fn search_field(&self, cx: &App) -> Entity<InputState> {
        self.search.read(cx).field().clone()
    }

    /// The keys go to the search field.
    pub fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| search.focus(window, cx));
    }

    /// The filter chosen for the next search in the tab in front.
    pub fn chosen_filter(&self, cx: &App) -> Option<String> {
        self.search.read(cx).filter().map(str::to_owned)
    }

    /// What the field tells: what is typed is searched for within the filter chosen; Enter leaves it for
    /// the results.
    pub(crate) fn on_search_event(&mut self, event: &SearchEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            SearchEvent::Typed(query) => {
                let filter = self.chosen_filter(cx);
                self.state.set_search(query, filter);
                self.changed(cx);
            }
            SearchEvent::Leave => self.leave_search(window, cx),
        }
    }

    /// The search field has the keys, or its menu has them for it: no column is marked as worked in.
    pub fn search_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.has_keys(self.search.read(cx).field(), window, cx)
    }

    /// Searches within `name`, or none, from now on: what is typed is searched again within it; with
    /// nothing typed home stays as it is, the filter waiting for the first key.
    pub fn choose_filter(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| search.set_filter(name.clone(), cx));
        let query = self.state.query.clone();
        self.state.set_search(&query, name);
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

    /// Empties the field, and home shows alone; the filter chosen stays for the next search.
    pub fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| search.show("", window, cx));
        let filter = self.chosen_filter(cx);
        self.state.set_search("", filter);
        self.changed(cx);
    }

    /// Searches for `query` within `filter`, which becomes the one chosen.
    pub fn search_for(&mut self, query: &str, filter: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |search, cx| {
            search.show(query, window, cx);
            search.focus(window, cx);
        });
        self.state.set_search(query, filter);
        self.adopt_filter(cx);
        self.changed(cx);
    }

    /// Going back or forward can bring another query, and another filter; the field shows the one, and
    /// the other becomes the one chosen.
    pub(crate) fn sync_search_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let query = self.state.query.clone();
        // Left alone when it holds the query already, with what is picked there.
        if self.search.read(cx).query(cx).as_ref() != query {
            self.search.update(cx, |search, cx| search.show(&query, window, cx));
        }
        self.adopt_filter(cx);
        self.changed(cx);
    }

    /// The results shown, a saved search's or those gone back to, choose their filter for the next search.
    fn adopt_filter(&mut self, cx: &mut Context<Self>) {
        if self.state.pinned() && self.state.search_filter.as_deref() != self.search.read(cx).filter() {
            let filter = self.state.search_filter.clone();
            self.search.update(cx, |search, cx| search.set_filter(filter, cx));
        }
    }
}
