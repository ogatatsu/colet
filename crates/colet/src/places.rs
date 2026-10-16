//! The places Colet keeps for the user: favorites and saved searches in home, the widths and the window,
//! which Everything to ask; and the dialogs that edit them.
use std::time::Duration;

use colet_core::{entry::Entry, paths::folder_name};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{
    app::{Colet, Engine, Tone, WidthOf},
    settings::{Placement, SavedSearch, Settings},
    state::{Kind, saved_query, search_key},
};

pub enum Dialog {
    /// A saved search's name and query, found by the query it had as the dialog opened.
    EditSearch { original: String, name: Entity<InputState>, query: Entity<InputState> },
    /// Which Everything to ask, and the keys.
    Preferences { instance: Entity<InputState> },
}

/// What a right-click on a favorite or saved search in home offers, and where.
pub struct HomeMenu {
    pub path: String,
    pub at: Point<Pixels>,
}

/// Home's rows: the favorites, the saved searches, then the drives, each place once.
pub fn home_entries(settings: &Settings, drives: Vec<Entry>) -> Vec<Entry> {
    let favorites = settings.favorites.iter().map(|path| Entry::folder(path, folder_name(path)));
    let searches = settings.saved_searches.iter().map(|s| Entry {
        directory: false,
        ..Entry::folder(&search_key(&s.query), s.label())
    });
    let mut seen = std::collections::HashSet::new();
    favorites.chain(searches).chain(drives).filter(|e| seen.insert(e.path.clone())).collect()
}

impl Colet {
    /// Changes the settings and writes them a moment later, once a burst of changes (a drag) is over.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let before = (self.settings.favorites.clone(), self.settings.saved_searches.clone());
        change(&mut self.settings);
        self.settings.normalize();
        if before != (self.settings.favorites.clone(), self.settings.saved_searches.clone()) {
            // Home lists them.
            let home = self.state.columns[0].id;
            self.load(home, 0, true, cx);
        }
        self.save_generation += 1;
        let generation = self.save_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let Some((backend, settings)) = this
                .update(cx, |this, _| (this.save_generation == generation).then(|| (this.backend.clone(), this.settings.clone())))
                .ok()
                .flatten()
            else {
                return;
            };
            let saved = cx.background_executor().spawn(async move { backend.save_settings(&settings) }).await;
            if let Err(error) = saved {
                this.update(cx, |this, cx| this.notify(error, Tone::Error, cx)).ok();
            }
        })
        .detach();
        cx.notify();
    }

    /// Writes the settings now, as the window closes.
    pub fn flush_settings(&mut self) -> Result<(), String> {
        self.save_generation += 1;
        self.backend.save_settings(&self.settings)
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

    /// A drag on a column's edge is over: keep the width it left.
    pub fn end_resize(&mut self, cx: &mut Context<Self>) {
        let Some(resize) = self.resizing.take() else { return };
        let widths = (self.widths.sidebar, self.widths.search, self.widths.column);
        self.update_settings(
            |s| match resize.of {
                WidthOf::Sidebar => s.sidebar_width = widths.0,
                WidthOf::Search => s.search_width = widths.1,
                WidthOf::Column => s.column_width = widths.2,
            },
            cx,
        );
    }

    pub fn toggle_favorite(&mut self, path: &str, cx: &mut Context<Self>) {
        let path = path.to_owned();
        self.update_settings(
            |s| {
                if s.is_favorite(&path) {
                    s.favorites.retain(|f| *f != path);
                } else {
                    s.favorites.push(path);
                }
            },
            cx,
        );
    }

    /// Saved with no name of its own, so home shows the query until one is given.
    pub fn toggle_saved(&mut self, query: &str, cx: &mut Context<Self>) {
        if query.trim().is_empty() {
            self.notify("検索語を入力すると保存できます", Tone::Info, cx);
            return;
        }
        let found = self.settings.saved(query).cloned();
        let query = query.to_owned();
        self.update_settings(
            |s| match &found {
                Some(_) => s.saved_searches.retain(|saved| saved.query != query),
                None => s.saved_searches.push(SavedSearch { name: String::new(), query }),
            },
            cx,
        );
        // The name given to it goes too; say so.
        if let Some(found) = found {
            self.notify(format!("「{}」を保存した検索から外しました", found.label()), Tone::Info, cx);
        }
    }

    /// Ctrl+D saves what the active column shows: the search (from its field too), or the folder.
    pub fn save_current(&mut self, cx: &mut Context<Self>) {
        let active = &self.state.columns[self.state.active];
        if self.search_focused || active.kind == Kind::Search {
            let query = self.state.query.clone();
            self.toggle_saved(&query, cx);
        } else if active.kind == Kind::Folder {
            let path = active.path.clone();
            self.toggle_favorite(&path, cx);
        } else {
            self.notify("「保存」は検索結果かフォルダーのカラムで使えます", Tone::Info, cx);
        }
    }

    /// F2 on a saved search in home edits its name and query.
    pub fn edit_item(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let c = &self.state.columns[self.state.active];
        if c.kind != Kind::Home {
            return;
        }
        if let Some(query) = c.focused.as_deref().and_then(saved_query).map(str::to_owned) {
            self.open_edit(&query, window, cx);
        }
    }

    pub fn open_edit(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self.settings.saved(query).cloned() else { return };
        self.home_menu = None;
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(found.query.clone()).default_value(found.name.clone()));
        let query = cx.new(|cx| InputState::new(window, cx).default_value(found.query.clone()));
        for input in [&name, &query] {
            let subscription = cx.subscribe_in(input, window, |this: &mut Self, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_edit(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            });
            self.dialog_subscriptions.push(subscription);
        }
        name.update(cx, |input, cx| input.focus(window, cx));
        self.dialog = Some(Dialog::EditSearch { original: found.query, name, query });
        cx.notify();
    }

    /// Why the edited search cannot be saved as it stands, if it cannot.
    pub fn edit_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::EditSearch { original, query, .. }) = &self.dialog else { return None };
        let query = query.read(cx).value();
        if query.trim().is_empty() {
            Some("")
        } else if query.as_ref() != original && self.settings.saved(&query).is_some() {
            Some("この検索は既に保存しています")
        } else {
            None
        }
    }

    pub fn submit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_problem(cx).is_some() {
            return;
        }
        let Some(Dialog::EditSearch { original, name, query }) = &self.dialog else { return };
        let original = original.clone();
        let name = name.read(cx).value().trim().to_owned();
        let query = query.read(cx).value().to_string();
        self.update_settings(
            |s| {
                if let Some(saved) = s.saved_searches.iter_mut().find(|saved| saved.query == original) {
                    *saved = SavedSearch { name, query: query.clone() };
                }
            },
            cx,
        );
        self.close_dialog(window, cx);
        // The search showing follows its new query.
        if self.state.query == original && query != original {
            self.search_for(&query, window, cx);
            self.focus_active(window, cx);
        }
    }

    pub fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let instance =
            cx.new(|cx| InputState::new(window, cx).placeholder("自動（既定 → 1.5a）").default_value(self.settings.instance.clone()));
        let subscription = cx.subscribe_in(&instance, window, |this: &mut Self, input, event, _, cx| {
            if let InputEvent::Change = event {
                let instance = input.read(cx).value().trim().to_owned();
                this.update_settings(|s| s.instance = instance, cx);
            }
        });
        self.dialog_subscriptions.push(subscription);
        instance.update(cx, |input, cx| input.focus(window, cx));
        self.dialog = Some(Dialog::Preferences { instance });
        cx.notify();
    }

    /// Lets go of Everything and makes the search afresh, so it connects to the instance now named.
    pub fn reconnect(&mut self, cx: &mut Context<Self>) {
        let backend = self.backend.clone();
        let task = cx.background_executor().spawn(async move { backend.reconnect() });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |this, cx| {
                this.engine = Engine::Unknown;
                this.state.reset_search();
                this.changed(cx);
                this.notify("接続設定を更新しました。検索を入力すると接続します。", Tone::Info, cx);
            })
            .ok();
        })
        .detach();
    }

    /// The keys go back to the column they came from.
    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.dialog_subscriptions.clear();
        let handle = if self.state.active == 0 { &self.home_focus } else { &self.columns_focus };
        handle.focus(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, SavedSearch, Settings, home_entries};

    #[test]
    fn lists_favorites_then_saved_searches_then_drives_each_once() {
        let settings = Settings {
            favorites: vec![r"D:\Work".into(), r"C:\".into()],
            saved_searches: vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into() }],
            ..Settings::default()
        };
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")]);
        let rows: Vec<(&str, &str)> = entries.iter().map(|e| (e.path.as_str(), e.name.as_str())).collect();
        assert_eq!(rows, [(r"D:\Work", "Work"), (r"C:\", "C:"), ("search:ext:pdf", "PDF"), (r"D:\", "D:")]);
        assert!(!entries[2].directory);
    }
}
