//! The buttons Tab stops at, each with the focus it keeps across frames. Colet keeps them, not the buttons,
//! for the keys are sent to some of them from outside drawing: the dialog's default button as it opens,
//! a theme as the preferences open, a step of the history from its notice.
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
};

use colet_core::entry::{Order, Sort};
use gpui_kit::{App, FocusHandle, Window};

use crate::{
    settings::{Language, Theme},
    state::ColumnId,
};

/// Which button a stop is for.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Stop {
    /// A button of which the window has only one: the toolbar's, a dialog's, the history's.
    Named(&'static str),
    /// A column's sort order's button.
    ColumnSort(ColumnId),
    /// A column's button to list it again after it could not be.
    Retry(ColumnId),
    /// A column's star, making it a favorite.
    Star(ColumnId),
    /// A column's button lifting its narrowing.
    ClearFilter(ColumnId),
    /// The crumb of the trail for a column.
    Crumb(ColumnId),
    /// A language among the preferences.
    Language(Language),
    /// A theme among the preferences.
    Theme(Theme),
    /// A size to draw everything at among the preferences, in percent.
    Zoom(u16),
    /// A tool on a search filter in the filters' dialog, by the filter's name.
    Filter(FilterTool, String),
    /// A choice of filter for a saved search being edited, by the filter's name; none for no filter.
    EditSearchFilter(Option<String>),
    /// A choice of the order a saved search being edited lists its results in.
    EditSearchOrder(OrderChoice),
    /// A hint to search for a query.
    Hint(String),
    /// A step in the history.
    HistoryEntry(u64),
    /// One of the files of a step in the history, by its place in the step: a step only ever adds its
    /// items at the end, so the place stays the item's.
    HistoryItem(u64, usize),
}

impl From<&'static str> for Stop {
    fn from(name: &'static str) -> Self {
        Stop::Named(name)
    }
}

/// A part of an order to choose: what it sorts by, the way it runs, or whether folders come first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum OrderChoice {
    By(Sort),
    Descending(bool),
    FoldersFirst(bool),
}

impl OrderChoice {
    /// Whether `order` has it.
    pub fn is_in(self, order: Order) -> bool {
        match self {
            OrderChoice::By(by) => order.by == by,
            OrderChoice::Descending(descending) => order.descending == descending,
            OrderChoice::FoldersFirst(first) => order.folders_first == first,
        }
    }

    /// `order` with it.
    pub fn apply(self, order: &mut Order) {
        match self {
            OrderChoice::By(by) => order.by = by,
            OrderChoice::Descending(descending) => order.descending = descending,
            OrderChoice::FoldersFirst(first) => order.folders_first = first,
        }
    }
}

/// What a tool on a search filter does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FilterTool {
    Default,
    Up,
    Down,
    Edit,
    Delete,
}

/// The stops, each made as it is first asked for. One not asked for between two frames, its button no
/// longer drawn or sent the keys, goes as the next frame is drawn: no one has to say which buttons went.
/// One with the keys stays, though: the keys can be sent to a button drawn only a frame or two later, as
/// the window's dialog opens.
#[derive(Default)]
pub struct Stops {
    handles: RefCell<HashMap<Stop, FocusHandle>>,
    /// Asked for since the last frame began.
    asked: RefCell<HashSet<Stop>>,
}

impl Stops {
    /// The stop for `stop`, made if it has none yet.
    pub fn get_or_make(&self, stop: Stop, cx: &App) -> FocusHandle {
        let handle = self.handles.borrow_mut().entry(stop.clone()).or_insert_with(|| cx.focus_handle().tab_stop(true)).clone();
        self.asked.borrow_mut().insert(stop);
        handle
    }

    /// The stop for `stop`, if it has one; it is not made for asking, nor kept.
    pub fn get(&self, stop: &Stop) -> Option<FocusHandle> {
        self.handles.borrow().get(stop).cloned()
    }

    /// The keys are on one of the stops.
    pub fn has_keys(&self, window: &Window) -> bool {
        self.handles.borrow().values().any(|handle| handle.is_focused(window))
    }

    /// A frame begins: only the stops asked for since the last one began, and the one with the keys, are
    /// kept.
    pub fn begin_frame(&mut self, window: &Window) {
        let asked = std::mem::take(self.asked.get_mut());
        self.handles.get_mut().retain(|stop, handle| asked.contains(stop) || handle.is_focused(window));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui_kit::test]
    fn keeps_only_the_stops_asked_for_since_the_last_frame(cx: &mut gpui_kit::TestAppContext) {
        cx.add_empty_window().update(|window, cx| {
            let mut stops = Stops::default();
            let back = stops.get_or_make("back".into(), cx);
            stops.get_or_make(Stop::Star(1), cx);
            stops.begin_frame(window);
            // Both asked for before the frame: kept, the same focus.
            assert_eq!(stops.get(&"back".into()), Some(back.clone()));
            assert!(stops.get(&Stop::Star(1)).is_some());
            stops.get_or_make("back".into(), cx);
            stops.begin_frame(window);
            // The star not asked for again: gone, and asking only to look does not keep one.
            assert_eq!(stops.get(&"back".into()), Some(back));
            assert_eq!(stops.get(&Stop::Star(1)), None);
            stops.begin_frame(window);
            assert_eq!(stops.get(&"back".into()), None);
        });
    }

    #[gpui_kit::test]
    fn keeps_the_stop_with_the_keys_though_not_asked_for(cx: &mut gpui_kit::TestAppContext) {
        cx.add_empty_window().update(|window, cx| {
            let mut stops = Stops::default();
            let open = stops.get_or_make("confirm-open".into(), cx);
            open.focus(window, cx);
            stops.begin_frame(window);
            stops.begin_frame(window);
            assert_eq!(stops.get(&"confirm-open".into()), Some(open));
        });
    }
}
