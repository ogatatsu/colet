//! The search filters: Everything queries a search is narrowed by, one at a time, chosen from the menu
//! beside the search field and kept in a dialog of their own.
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{app::Colet, places::Dialog, settings::SearchFilter, state::HomeKey, theme::ActivePalette};

/// A filter being added, or edited, found by the name it had as the edit began.
pub struct FilterEdit {
    pub original: Option<String>,
    pub name: Entity<InputState>,
    pub query: Entity<InputState>,
}

impl Colet {
    /// The filter a tab opened afresh starts with.
    pub fn default_filter(&self) -> Option<String> {
        self.stored.settings().default_filter.clone()
    }

    /// Makes the filter named `name` the default, or, the default already, leaves none the default. The
    /// tabs open keep the filter each has.
    pub fn toggle_default_filter(&mut self, name: &str, cx: &mut Context<Self>) {
        let to = (self.stored.settings().default_filter.as_deref() != Some(name)).then(|| name.to_owned());
        self.update_settings(|s| s.default_filter = to, cx);
        cx.notify();
    }

    pub fn open_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.take_menu();
        self.back_to_filters(window, cx);
    }

    /// The list of filters, back from an edit or a deletion asked about.
    pub fn back_to_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        self.overlays.set_dialog(Dialog::Filters { edit: None, deleting: None });
        self.overlays.dialog_focus().focus(window, cx);
        cx.notify();
    }

    /// Esc: from an edit or a deletion back to the list, otherwise out of the dialog.
    pub fn escape_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.overlays.dialog() {
            Some(Dialog::Filters { edit, deleting }) if edit.is_some() || deleting.is_some() => self.back_to_filters(window, cx),
            _ => self.close_dialog(window, cx),
        }
    }

    /// Edits the filter named `original`, or adds one.
    pub fn edit_filter(&mut self, original: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let found = original
            .as_deref()
            .and_then(|name| self.stored.settings().filters.iter().find(|f| f.name == name))
            .cloned()
            .unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("画像").default_value(found.name));
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("ext:jpg;png;gif").default_value(found.query));
        self.begin_dialog(window, cx);
        for input in [&name, &query] {
            let subscription = cx.subscribe_in(input, window, |this: &mut Self, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_filter(window, cx),
                InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
            });
            self.overlays.listen_in_dialog(subscription);
            cx.palette().style_input(input, cx);
        }
        name.update(cx, |input, cx| input.focus(window, cx));
        self.overlays.set_dialog(Dialog::Filters { edit: Some(FilterEdit { original, name, query }), deleting: None });
        cx.notify();
    }

    /// Why the edited filter cannot be kept as it stands, if it cannot; empty when no word is needed.
    pub fn filter_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Filters { edit: Some(edit), .. }) = self.overlays.dialog() else { return None };
        let name = edit.name.read(cx).value().trim().to_owned();
        if name.is_empty() || edit.query.read(cx).value().trim().is_empty() {
            Some("")
        } else if edit.original.as_deref() != Some(name.as_str()) && self.stored.settings().filter_query(&name).is_some() {
            Some("この名前のフィルターは既にあります")
        } else {
            None
        }
    }

    /// Keeps the filter edited. Renamed, what names it follows: the saved searches, and in every tab the
    /// filter chosen and the way back and forth. The results within it ask again for its new query.
    pub fn submit_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter_problem(cx).is_some() {
            return;
        }
        let Some(Dialog::Filters { edit: Some(edit), .. }) = self.overlays.dialog() else { return };
        let original = edit.original.clone();
        let name = edit.name.read(cx).value().trim().to_owned();
        let query = edit.query.read(cx).value().trim().to_owned();
        let before = original.as_deref().and_then(|from| self.stored.settings().filter_query(from)).map(str::to_owned);
        let edited = SearchFilter { name: name.clone(), query: query.clone() };
        let to = name.clone();
        self.update_settings(
            |s| match &original {
                None => s.filters.push(edited),
                Some(from) => {
                    if let Some(filter) = s.filters.iter_mut().find(|f| &f.name == from) {
                        *filter = edited;
                    }
                    let follow = |named: &mut Option<String>| {
                        if named.as_ref() == Some(from) {
                            *named = Some(to.clone());
                        }
                    };
                    for saved in &mut s.saved_searches {
                        follow(&mut saved.filter);
                    }
                    follow(&mut s.default_filter);
                    // A pinned search keeps its pin under the name.
                    for pin in &mut s.pinned {
                        if let HomeKey::Search { query, filter: Some(filter), narrow, order } = HomeKey::of(pin)
                            && filter == from
                        {
                            *pin = HomeKey::search(query, Some(&to), narrow, order).key();
                        }
                    }
                }
            },
            cx,
        );
        if let Some(from) = &original {
            if *from != name {
                self.rename_filter_in_tabs(from, Some(&name), cx);
            }
            if before.as_deref() != Some(query.as_str()) && self.state.search_filter.as_deref() == Some(name.as_str()) {
                self.state.reset_search();
            }
            self.changed(cx);
        }
        self.back_to_filters(window, cx);
    }

    /// Asks before the filter named `name` goes.
    pub fn ask_delete_filter(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        self.overlays.set_dialog(Dialog::Filters { edit: None, deleting: Some(name) });
        self.overlays.dialog_focus().focus(window, cx);
        cx.notify();
    }

    /// How many saved searches run within the filter named `name`.
    pub fn filter_users(&self, name: &str) -> usize {
        self.stored.settings().saved_searches.iter().filter(|s| s.filter.as_deref() == Some(name)).count()
    }

    /// Deletes the filter asked about: what named it goes without it.
    pub fn delete_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Filters { deleting: Some(name), .. }) = self.overlays.dialog() else { return };
        let name = name.clone();
        self.update_settings(
            |s| {
                s.filters.retain(|f| f.name != name);
                if s.default_filter.as_ref() == Some(&name) {
                    s.default_filter = None;
                }
            },
            cx,
        );
        self.rename_filter_in_tabs(&name, None, cx);
        self.changed(cx);
        self.back_to_filters(window, cx);
    }

    /// Moves the filter named `name` up or down the menu by one.
    pub fn move_filter(&mut self, name: &str, by: isize, cx: &mut Context<Self>) {
        let Some(at) = self.stored.settings().filters.iter().position(|f| f.name == name) else { return };
        let to = at as isize + by;
        if to < 0 || to as usize >= self.stored.settings().filters.len() {
            return;
        }
        self.update_settings(|s| s.filters.swap(at, to as usize), cx);
    }
}
