//! The places Colet keeps for the user: favorites and saved searches in home, the widths and the window,
//! which Everything to ask; and the dialogs that edit them.
use std::time::Duration;

use colet_core::{
    entry::Entry,
    paths::{folder_name, path_key},
};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{
    app::{Colet, Engine, Tone, WidthOf},
    search_filters::FilterEdit,
    settings::{Placement, SavedSearch, Settings, Theme, written},
    state::{Kind, saved_query, search_key},
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
    /// A saved search's name, query and filter, found by the query and filter it had as the dialog opened.
    EditSearch { original: (String, Option<String>), name: Entity<InputState>, query: Entity<InputState>, filter: Option<String> },
    /// The search filters, to add to, put in order, edit or delete: one at a time, in `edit`, or the one
    /// asked about before it goes.
    Filters { edit: Option<FilterEdit>, deleting: Option<String> },
    /// The theme, the connection to Everything, and the keys.
    Preferences,
}

/// The places home lists: the drives in letter order, then the other folders Everything's index starts
/// from, by path and named by it. An indexed volume, `C:`, is the drive `C:\`.
pub fn places(drives: Vec<Entry>, indexed: Vec<String>) -> Vec<Entry> {
    let mut seen: std::collections::HashSet<String> = drives.iter().map(|e| path_key(&e.path)).collect();
    let mut others: Vec<String> = indexed
        .into_iter()
        .map(|path| if path.len() == 2 && path.ends_with(':') { format!("{path}\\") } else { path })
        .filter(|path| seen.insert(path_key(path)))
        .collect();
    others.sort_by_key(|path| path.to_lowercase());
    drives.into_iter().chain(others.iter().map(|path| Entry::folder(path, path))).collect()
}

/// Home's rows: the favorites, the saved searches, then the places, each place once.
pub fn home_entries(settings: &Settings, drives: Vec<Entry>) -> Vec<Entry> {
    let favorites = settings.favorites.iter().map(|path| Entry::folder(path, folder_name(path)));
    let searches = settings.saved_searches.iter().map(|s| Entry {
        directory: false,
        ..Entry::folder(&search_key(&s.query, s.filter.as_deref()), s.label())
    });
    let mut seen = std::collections::HashSet::new();
    // A place is one whatever its case; a saved search is its query and filter exactly.
    let key = |e: &Entry| if saved_query(&e.path).is_some() { e.path.clone() } else { path_key(&e.path) };
    favorites.chain(searches).chain(drives).filter(|e| seen.insert(key(e))).collect()
}

/// Home's headings and their rows, shared by drawing and scrolling to the focused row.
pub fn home_groups<'a>(settings: &Settings, entries: &'a [Entry]) -> Vec<(&'static str, Vec<&'a Entry>)> {
    let favorite = |e: &&Entry| settings.is_favorite(&e.path);
    [
        ("お気に入り", entries.iter().filter(favorite).collect::<Vec<_>>()),
        ("保存した検索", entries.iter().filter(|e| saved_query(&e.path).is_some()).collect()),
        ("場所", entries.iter().filter(|e| saved_query(&e.path).is_none() && !favorite(e)).collect()),
    ]
    .into_iter()
    .filter(|(_, rows)| !rows.is_empty())
    .collect()
}

/// A favorite or saved search dragged in home, to a new place among its own kind.
#[derive(Clone)]
pub struct HomeDrag {
    pub path: String,
    pub search: bool,
}

/// Where a drag in home would put what it carries: before the row `before`, or after the last of its kind.
#[derive(Clone, Debug, PartialEq)]
pub struct Landing {
    pub before: Option<String>,
}

/// Where each of home's favorites and saved searches was drawn, by its path.
pub type HomeRows = std::rc::Rc<std::cell::RefCell<Vec<(String, Bounds<Pixels>)>>>;

/// Which of home's kinds that are put in order by dragging `path` is: a saved search, a favorite, or
/// neither, as a place is.
pub fn movable(settings: &Settings, path: &str) -> Option<bool> {
    if saved_query(path).is_some() {
        Some(true)
    } else {
        settings.is_favorite(path).then_some(false)
    }
}

/// Moves the item `is` finds by `path` to just before the one it finds by `before`, or to the end.
fn move_before<T>(items: &mut Vec<T>, is: impl Fn(&T, &str) -> bool, path: &str, before: Option<&str>) {
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
    let Some(&(id, display)) = held.or_else(|| displays.iter().find(|(id, _)| Some(*id) == primary)).or(displays.first())
    else {
        return (None, saved);
    };
    let (min_width, min_height) = crate::app::MIN_WINDOW;
    let size = size(
        saved.size.width.max(px(min_width)).min(display.size.width),
        saved.size.height.max(px(min_height)).min(display.size.height),
    );
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
        let before = (self.settings.favorites.clone(), self.settings.saved_searches.clone());
        change(&mut self.settings);
        self.settings.normalize();
        if before != (self.settings.favorites.clone(), self.settings.saved_searches.clone()) {
            // Home lists them.
            let home = self.state.columns[0].id;
            self.load(home, 0, true, cx);
        }
        self.settings_changes += 1;
        let change = self.settings_changes;
        cx.notify();
        if self.settings_saver.is_none() {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            // A later change has its own save coming.
            let queued = this
                .update(cx, |this, _| {
                    let saver = this.settings_saver.as_ref().filter(|_| this.settings_changes == change)?;
                    Some(saver.save(this.settings.clone()))
                })
                .ok()
                .flatten();
            let Some(queued) = queued else { return };
            let saved = cx.background_executor().spawn(async move { written(queued) }).await;
            if let Err(error) = saved {
                this.update(cx, |this, cx| this.notify(error, Tone::Error, cx)).ok();
            }
        })
        .detach();
    }

    /// Writes the settings now, as the window closes, once the saves before are written; the saves still
    /// waiting out a burst are dropped.
    pub fn flush_settings(&mut self) -> Result<(), String> {
        self.settings_changes += 1;
        match &self.settings_saver {
            Some(saver) => written(saver.save(self.settings.clone())),
            None => Ok(()),
        }
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
                    let key = path_key(&path);
                    s.favorites.retain(|f| path_key(f) != key);
                } else {
                    s.favorites.push(path);
                }
            },
            cx,
        );
    }

    /// A favorite or saved search is picked up in home: its row fades where it was.
    pub fn start_home_drag(&mut self, drag: &HomeDrag, cx: &mut Context<Self>) {
        self.home_drag = Some(drag.clone());
        self.landing = None;
        cx.notify();
    }

    /// The pointer moved in a drag in home, with home's list drawn at `list`.
    pub fn drag_home(&mut self, at: Point<Pixels>, list: Bounds<Pixels>, cx: &mut Context<Self>) {
        if self.home_drag.is_none() {
            return;
        }
        self.home_pointer = Some((at, list));
        self.aim_home_drag(cx);
        if drag_edge(at, list).is_some() {
            self.keep_home_scrolling(cx);
        }
    }

    /// Where the drag would land: between two rows of its own kind, by the half of the row it is over;
    /// nowhere outside them.
    fn aim_home_drag(&mut self, cx: &mut Context<Self>) {
        let (Some(drag), Some((at, list))) = (&self.home_drag, self.home_pointer) else { return };
        let landing = {
            let rows = self.home_rows.borrow();
            let own: Vec<_> = rows.iter().filter(|(path, _)| movable(&self.settings, path) == Some(drag.search)).collect();
            match (own.first(), own.last()) {
                (Some(first), Some(last)) if list.contains(&at) && at.y >= first.1.top() && at.y < last.1.bottom() => {
                    let over = own.iter().position(|(_, bounds)| at.y < bounds.bottom()).unwrap_or(own.len() - 1);
                    let (path, bounds) = own[over];
                    let before = if at.y < bounds.center().y { Some(path.clone()) } else { own.get(over + 1).map(|(path, _)| path.clone()) };
                    Some(Landing { before })
                }
                _ => None,
            }
        };
        if self.landing != landing {
            self.landing = landing;
            cx.notify();
        }
    }

    /// Scrolls home's list while a drag is held at its edge, a step a frame.
    fn keep_home_scrolling(&mut self, cx: &mut Context<Self>) {
        if self.home_scrolling {
            return;
        }
        self.home_scrolling = true;
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
        let way = self.home_drag.as_ref().and(self.home_pointer).and_then(|(at, list)| drag_edge(at, list));
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
                self.home_scrolling = false;
                false
            }
        }
    }

    /// The drag is let go of over home's list: what it carries goes where it would land, if anywhere.
    pub fn drop_home(&mut self, drag: &HomeDrag, cx: &mut Context<Self>) {
        let landing = self.landing.take();
        self.end_home_drag();
        cx.notify();
        let Some(Landing { before }) = landing else { return };
        let (path, before) = (drag.path.as_str(), before.as_deref());
        self.update_settings(
            |s| {
                if drag.search {
                    move_before(&mut s.saved_searches, |s, key| search_key(&s.query, s.filter.as_deref()) == key, path, before);
                } else {
                    move_before(&mut s.favorites, |f, key| path_key(f) == path_key(key), path, before);
                }
            },
            cx,
        );
    }

    /// The drag in home is over, dropped or not: nothing has moved.
    pub fn end_home_drag(&mut self) {
        self.home_drag = None;
        self.landing = None;
        self.home_pointer = None;
    }

    /// Saved with no name of its own, so home shows the query until one is given; with the filter it runs
    /// within, a search apart from the same query with another.
    pub fn toggle_saved(&mut self, query: &str, filter: Option<String>, cx: &mut Context<Self>) {
        if query.trim().is_empty() {
            self.notify("検索語を入力すると保存できます", Tone::Info, cx);
            return;
        }
        let found = self.settings.saved(query, filter.as_deref()).cloned();
        let query = query.to_owned();
        self.update_settings(
            |s| match &found {
                Some(_) => s.saved_searches.retain(|saved| saved.query != query || saved.filter != filter),
                None => s.saved_searches.push(SavedSearch { name: String::new(), query, filter }),
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
            let (query, filter) = (self.state.query.clone(), self.state.search_filter.clone());
            self.toggle_saved(&query, filter, cx);
        } else if active.kind == Kind::Folder {
            let path = active.path.clone();
            self.toggle_favorite(&path, cx);
        } else {
            self.notify("「保存」は検索結果かフォルダーのカラムで使えます", Tone::Info, cx);
        }
    }

    pub fn open_edit(&mut self, query: &str, filter: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(found) = self.settings.saved(query, filter).cloned() else { return };
        self.menu = None;
        let name = cx.new(|cx| InputState::new(window, cx).placeholder(found.query.clone()).default_value(found.name.clone()));
        let query = cx.new(|cx| InputState::new(window, cx).default_value(found.query.clone()));
        for input in [&name, &query] {
            let subscription = cx.subscribe_in(input, window, |this: &mut Self, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_edit(window, cx),
                InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
            });
            self.dialog_subscriptions.push(subscription);
            self.palette.style_input(input, cx);
        }
        name.update(cx, |input, cx| input.focus(window, cx));
        let filter = found.filter.clone();
        self.dialog = Some(Dialog::EditSearch { original: (found.query, found.filter), name, query, filter });
        cx.notify();
    }

    /// The filter the edited search is to run within.
    pub fn pick_edit_filter(&mut self, chosen: Option<String>, cx: &mut Context<Self>) {
        if let Some(Dialog::EditSearch { filter, .. }) = &mut self.dialog {
            *filter = chosen;
            cx.notify();
        }
    }

    /// Why the edited search cannot be saved as it stands, if it cannot.
    pub fn edit_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::EditSearch { original, query, filter, .. }) = &self.dialog else { return None };
        let query = query.read(cx).value();
        let same = (query.as_ref(), filter.as_deref()) == (original.0.as_str(), original.1.as_deref());
        if query.trim().is_empty() {
            Some("")
        } else if !same && self.settings.saved(&query, filter.as_deref()).is_some() {
            Some("この検索は既に保存しています")
        } else {
            None
        }
    }

    pub fn submit_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edit_problem(cx).is_some() {
            return;
        }
        let Some(Dialog::EditSearch { original, name, query, filter }) = &self.dialog else { return };
        let (original, filter) = (original.clone(), filter.clone());
        let name = name.read(cx).value().trim().to_owned();
        let query = query.read(cx).value().to_string();
        let edited = SavedSearch { name, query: query.clone(), filter: filter.clone() };
        self.update_settings(
            |s| {
                if let Some(saved) = s.saved_searches.iter_mut().find(|saved| (&saved.query, &saved.filter) == (&original.0, &original.1)) {
                    *saved = edited;
                }
            },
            cx,
        );
        self.close_dialog(window, cx);
        // The search showing follows its new query and filter.
        let showing = self.state.pinned() && (&self.state.query, &self.state.search_filter) == (&original.0, &original.1);
        if showing && (&query, &filter) != (&original.0, &original.1) {
            self.search_for(&query, filter, window, cx);
            self.focus_active(window, cx);
        }
    }

    pub fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    /// The keys go back to the column they came from.
    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dialog = None;
        self.dialog_subscriptions.clear();
        // The next dialog starts at its top.
        self.dialog_scroll = ScrollHandle::new();
        let handle = if self.state.active == 0 { &self.home_focus } else { &self.columns_focus };
        handle.focus(window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, Placement, SavedSearch, Settings, home_entries, move_before, places, restore_bounds};
    use gpui_kit::{Bounds, DisplayId, point, px, size};

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
                SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None },
                SavedSearch { name: "猫".into(), query: "cat".into(), filter: Some("画像".into()) },
            ],
            ..Settings::default()
        };
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")]);
        let rows: Vec<(&str, &str)> = entries.iter().map(|e| (e.path.as_str(), e.name.as_str())).collect();
        assert_eq!(rows, [(r"D:\Work", "Work"), (r"C:\", "C:"), ("search:ext:pdf", "PDF"), ("search:cat\0画像", "猫"), (r"D:\", "D:")]);
        assert!(!entries[2].directory);
    }

    #[test]
    fn lists_the_drives_then_the_other_index_roots_by_path_each_once() {
        let drives = vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "データ (D:)")];
        let indexed = vec!["C:".into(), r"\\nas\Share".into(), r"d:\Projects".into(), "E:".into(), r"D:\".into(), r"\\NAS\share".into()];
        let rows: Vec<(String, String)> = places(drives, indexed).into_iter().map(|e| (e.path, e.name)).collect();
        let at = |path: &str, name: &str| (path.to_string(), name.to_string());
        assert_eq!(
            rows,
            [at(r"C:\", "OS (C:)"), at(r"D:\", "データ (D:)"), at(r"\\nas\Share", r"\\nas\Share"), at(r"d:\Projects", r"d:\Projects"), at(r"E:\", r"E:\")]
        );
    }

    #[test]
    fn takes_a_place_in_another_case_as_the_same_but_not_a_query() {
        let search = |query: &str| SavedSearch { name: String::new(), query: query.into(), filter: None };
        let settings = Settings {
            favorites: vec![r"c:\".into()],
            saved_searches: vec![search("ext:PDF"), search("ext:pdf")],
            ..Settings::default()
        };
        assert!(settings.is_favorite(r"C:\") && settings.is_favorite(r"C:"));
        let entries = home_entries(&settings, vec![Entry::folder(r"C:\", "OS (C:)")]);
        let paths: Vec<&str> = entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, [r"c:\", "search:ext:PDF", "search:ext:pdf"]);
    }

    #[test]
    fn moves_an_item_before_another_or_to_the_end() {
        let moved = |path: &str, before: Option<&str>| {
            let mut items = vec!["a", "b", "c", "d"];
            move_before(&mut items, |item, key| *item == key, path, before);
            items
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
