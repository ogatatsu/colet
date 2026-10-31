//! The search filters: Everything queries a search is narrowed by, one at a time, chosen from the menu
//! beside the search field and kept in a dialog of their own.
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{app::Colet, places::Dialog, settings::SearchFilter};

/// A filter being added, or edited, found by the name it had as the edit began.
pub struct FilterEdit {
    pub original: Option<String>,
    pub name: Entity<InputState>,
    pub query: Entity<InputState>,
}

impl Colet {
    /// Searches within `name`, or none, from now on: what is typed, or with nothing typed all the filter
    /// lets through; with neither, home shows alone.
    pub fn choose_filter(&mut self, name: Option<String>, cx: &mut Context<Self>) {
        if self.settings.filter != name {
            let chosen = name.clone();
            self.update_settings(|s| s.filter = chosen, cx);
        }
        let query = self.state.query.clone();
        self.state.set_search(&query, name);
        self.changed(cx);
    }

    pub fn open_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.take_menu();
        self.back_to_filters(window, cx);
    }

    /// The list of filters, back from an edit or a deletion asked about.
    pub fn back_to_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog_subscriptions.clear();
        self.dialog = Some(Dialog::Filters { edit: None, deleting: None });
        self.dialog_focus.focus(window, cx);
        cx.notify();
    }

    /// Esc: from an edit or a deletion back to the list, otherwise out of the dialog.
    pub fn escape_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.dialog {
            Some(Dialog::Filters { edit, deleting }) if edit.is_some() || deleting.is_some() => self.back_to_filters(window, cx),
            _ => self.close_dialog(window, cx),
        }
    }

    /// Edits the filter named `original`, or adds one.
    pub fn edit_filter(&mut self, original: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let found = original.as_deref().and_then(|name| self.settings.filters.iter().find(|f| f.name == name)).cloned().unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("画像").default_value(found.name));
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("ext:jpg;png;gif").default_value(found.query));
        self.dialog_subscriptions.clear();
        for input in [&name, &query] {
            let subscription = cx.subscribe_in(input, window, |this: &mut Self, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_filter(window, cx),
                InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
            });
            self.dialog_subscriptions.push(subscription);
            self.palette.style_input(input, cx);
        }
        name.update(cx, |input, cx| input.focus(window, cx));
        self.dialog = Some(Dialog::Filters { edit: Some(FilterEdit { original, name, query }), deleting: None });
        cx.notify();
    }

    /// Why the edited filter cannot be kept as it stands, if it cannot; empty when no word is needed.
    pub fn filter_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Filters { edit: Some(edit), .. }) = &self.dialog else { return None };
        let name = edit.name.read(cx).value().trim().to_owned();
        if name.is_empty() || edit.query.read(cx).value().trim().is_empty() {
            Some("")
        } else if edit.original.as_deref() != Some(name.as_str()) && self.settings.filter_query(&name).is_some() {
            Some("この名前のフィルターは既にあります")
        } else {
            None
        }
    }

    /// Keeps the filter edited. Renamed, what names it follows: the filter chosen, the saved searches and
    /// the way back and forth. The results within it ask again for its new query.
    pub fn submit_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter_problem(cx).is_some() {
            return;
        }
        let Some(Dialog::Filters { edit: Some(edit), .. }) = &self.dialog else { return };
        let original = edit.original.clone();
        let name = edit.name.read(cx).value().trim().to_owned();
        let query = edit.query.read(cx).value().trim().to_owned();
        let before = original.as_deref().and_then(|from| self.settings.filter_query(from)).map(str::to_owned);
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
                    follow(&mut s.filter);
                    for saved in &mut s.saved_searches {
                        follow(&mut saved.filter);
                    }
                    // A pinned search keeps its pin under the name.
                    for pin in &mut s.pinned {
                        if let Some((query, Some(filter), narrow)) = crate::state::saved_query(pin)
                            && filter == from
                        {
                            *pin = crate::state::search_key(query, Some(&to), narrow);
                        }
                    }
                }
            },
            cx,
        );
        if let Some(from) = &original {
            if *from != name {
                self.state.rename_search_filter(from, Some(&name));
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
        self.dialog_subscriptions.clear();
        self.dialog = Some(Dialog::Filters { edit: None, deleting: Some(name) });
        self.dialog_focus.focus(window, cx);
        cx.notify();
    }

    /// How many saved searches run within the filter named `name`.
    pub fn filter_users(&self, name: &str) -> usize {
        self.settings.saved_searches.iter().filter(|s| s.filter.as_deref() == Some(name)).count()
    }

    /// Deletes the filter asked about: what named it goes without it.
    pub fn delete_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Filters { deleting: Some(name), .. }) = &self.dialog else { return };
        let name = name.clone();
        self.update_settings(|s| s.filters.retain(|f| f.name != name), cx);
        self.state.rename_search_filter(&name, None);
        // Its buttons' stops go with it.
        self.stops.retain(|key, _| !(key.starts_with("filter-") && key.ends_with(&format!(":{name}"))));
        self.changed(cx);
        self.back_to_filters(window, cx);
    }

    /// Moves the filter named `name` up or down the menu by one.
    pub fn move_filter(&mut self, name: &str, by: isize, cx: &mut Context<Self>) {
        let Some(at) = self.settings.filters.iter().position(|f| f.name == name) else { return };
        let to = at as isize + by;
        if to < 0 || to as usize >= self.settings.filters.len() {
            return;
        }
        self.update_settings(|s| s.filters.swap(at, to as usize), cx);
    }
}
