//! The columns beside home's pane, as the window draws them: the search results pinned at the left when
//! they are, the strip the folders folded above them fold into, and the folder columns scrolled sideways.
//! Each column has its heading, its narrowing field, the headings of its rows and its rows
//! ([`column`], [`rows`]); [`scrolling`] keeps what of them is on screen and brings one in sight.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{app::*, theme::ActivePalette};

mod column;
mod rows;
mod scrolling;

pub(crate) use rows::{MODIFIED_WIDTH, PAGE_AHEAD, SIZE_TRAIL, SIZE_WIDTH, row_height, rows_in_sight};
#[cfg(test)]
pub(crate) use rows::{row_id, row_part_id};

impl Colet {
    /// The columns' side of the window, which takes the keys for every column but home.
    pub(crate) fn render_columns(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let pinned = self.state.pinned();
        let away = self.state.folded_away();
        let first = if pinned { 2 } else { 1 } + away;
        let only_home = self.state.columns.len() == 1;
        let pinned_column = pinned.then(|| self.render_column(1, window, cx));
        let strip = (away > 0).then(|| self.unfold_strip(away, window, cx));
        let columns: Vec<AnyElement> = (first..self.state.columns.len()).map(|i| self.render_column(i, window, cx)).collect();
        let scrolling = self.scrolling_columns(columns, window, cx);
        let main = div()
            .id("main")
            .track_focus(&self.columns_focus)
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .bg(p.bg)
            .child(self.takes_text(&self.columns_focus, cx))
            .children(pinned_column)
            .children(strip)
            .child(scrolling)
            // Nothing searched or opened leaves this side dark; show where to start.
            .when(only_home, |d| d.child(self.home_hint(window, cx)));
        self.column_keys(main, cx).into_any_element()
    }
}
