//! Where the scrolling columns should rest so the one being worked in, and what it opened, are on screen.
//! When they must move, the last column ends flush with the right edge, so no room is left empty past it,
//! unless that would hide the column worked in, or, worked in outside them, cut the last one on the left.
//! Picking along a column, which opens and closes the one
//! beside it, leaves them where they are.
use std::time::Instant;

use gpui_kit::ScrollHandle;

use crate::{metrics::Scale, state::ColumnId};

/// Where the scrolling columns sit sideways, and how they get where they should rest. Every length here is
/// in pixels at 100% (see [`crate::metrics`]); the scroll handle's, read and set through
/// [`Sideways::offset`] and the rest, are as drawn.
#[derive(Default)]
pub struct Sideways {
    pub scroll: ScrollHandle,
    /// The zoom the window was last drawn at, which turns the scroll handle's lengths into these.
    pub scale: Scale,
    pub slide: Option<Slide>,
    /// Asks the next frame to bring the active column on screen, sliding there when true.
    pub pending: Option<bool>,
    /// The scroll offset the columns were last brought to, which holds their scroll width while columns
    /// beside them close and they slide back; once at rest, where they rest.
    pub hold: f32,
    /// The view's width and the columns' that the columns were last brought to rest, or set sliding, for.
    pub fitted: (f32, f32),
    /// The anchor of the fold last drawn, and how many folders it held away, to tell when they come out.
    pub drawn_fold: Option<(ColumnId, usize)>,
    /// The scrolling columns the columns were last brought on screen with, to tell home or the pinned
    /// results taken up again beside the same columns from a pick there opening others.
    pub revealed: Vec<ColumnId>,
}

impl Sideways {
    /// Brings the active column on screen next frame, sliding there, unless that is already asked.
    pub fn ask(&mut self) {
        self.pending.get_or_insert(true);
    }

    /// The scroll offset, as a distance from the left.
    pub fn offset(&self) -> f32 {
        self.scale.down(-f32::from(self.scroll.offset().x))
    }

    pub fn set_offset(&self, at: f32) {
        self.scroll.set_offset(gpui_kit::point(-self.scale.px(at), gpui_kit::px(0.)));
    }

    /// The window is drawn at `scale` now: the columns stay scrolled as far at 100%, so the same of them
    /// is on screen as the zoom changes, rather than as many pixels.
    pub fn rescale(&mut self, scale: Scale) {
        if scale != self.scale {
            let at = self.offset();
            self.scale = scale;
            self.set_offset(at);
        }
    }

    /// How far the scroll goes at most.
    pub fn max_offset(&self) -> f32 {
        self.scale.down(f32::from(self.scroll.max_offset().x))
    }

    /// How wide the columns' view was laid out.
    pub fn view_width(&self) -> f32 {
        self.scale.down(f32::from(self.scroll.bounds().size.width))
    }
}

/// The columns sliding to where they should rest; see `Colet::reveal`.
pub struct Slide {
    /// Where the target was reckoned from, to reckon it again should the view change under it.
    pub origin: f32,
    pub position: f32,
    pub target: f32,
    pub velocity: f32,
    pub time: Option<Instant>,
    /// Folders brought out by their strip: the column held at the right edge as they slide in.
    pub keep: Option<usize>,
}

impl Slide {
    pub fn new(origin: f32, position: f32, target: f32, keep: Option<usize>) -> Self {
        Self { origin, position, target, velocity: 0., time: None, keep }
    }

    /// Moves all of it by `by`, as the columns under it are.
    pub fn shift(&mut self, by: f32) {
        self.origin += by;
        self.position += by;
        self.target += by;
    }

    /// One frame's step at `now`: where the columns stand, and whether they are at rest there.
    ///
    /// Critically damped: settles in about 350ms without overshooting. A frame held up counts as at most
    /// two, so the slide slows there rather than leaping most of the way.
    pub fn step(&mut self, now: Instant) -> (f32, bool) {
        let stiffness = 27.;
        let t = self.time.map_or(1. / 60., |then| (now - then).as_secs_f32().clamp(0., 1. / 30.));
        self.time = Some(now);
        let x = self.position - self.target;
        let k = self.velocity + stiffness * x;
        let decay = (-stiffness * t).exp();
        self.position = self.target + (x + k * t) * decay;
        self.velocity = (self.velocity - stiffness * k * t) * decay;
        let done = (self.position - self.target).abs() < 0.5 && self.velocity.abs() < 20.;
        (if done { self.target } else { self.position }, done)
    }
}

/// A scrolling column's place: where it starts and how wide it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub start: f32,
    pub width: f32,
}

/// The scroll offset to rest at, from `from`, with a view `view` wide. `active` is the active column
/// among `cells`, or `None` when it is one of those outside them (home, the pinned search results).
pub fn reveal_target(from: f32, view: f32, cells: &[Cell], active: Option<usize>) -> f32 {
    let Some(last) = cells.len().checked_sub(1) else { return 0. };
    let first = active.unwrap_or(0).min(last);
    let start = cells[first].start;
    let end = cells[last].start + cells[last].width;
    // The last column flush with the right edge.
    let flush = (end - view).max(0.);
    // A pixel either way, for widths rounded under display scaling.
    if start >= from - 1. && end <= from + view + 1. {
        return from;
    }
    if end - start > view + 1. {
        // Worked in outside them, the last is shown whole, or from its left edge when it is wider than the
        // view.
        return if active.is_none() { flush.min(cells[last].start) } else { start };
    }
    // From the active one to the last fit: brought in from either side, the last ends at the right edge.
    flush
}

/// The scroll offset to rest at once the folders folded above a column come out by their strip: as far
/// left as keeps column `keep` whole at the right edge, so as many of them as fit show beside it.
pub fn uncover_target(view: f32, cells: &[Cell], keep: usize) -> f32 {
    let Some(cell) = cells.get(keep) else { return 0. };
    (cell.start + cell.width - view).clamp(0., cell.start)
}

/// Where columns resting at `from` go once the view or their width changes under them, as the window is
/// resized or a width dragged: `was` and `was_end` are the view and the columns' end they rested over.
/// The last column in sight before stays flush with the right edge, unless that would push the active
/// one off on the left, or, worked in outside them, the last one; brought there by hand short of it, the left edge stays, with no room left past
/// the last column.
pub fn refit(from: f32, was: f32, was_end: f32, view: f32, cells: &[Cell], active: Option<usize>) -> f32 {
    let Some(last) = cells.len().checked_sub(1) else { return 0. };
    let end = cells[last].start + cells[last].width;
    let flush = (end - view).max(0.);
    // A pixel either way, for widths rounded under display scaling.
    if from + was >= was_end - 1. { flush.min(cells[active.unwrap_or(last).min(last)].start) } else { from.min(flush) }
}

/// What is on screen, in columns: 1.5 is halfway across the second. Above 0, some are hidden on the
/// left; below the count, on the right. `None` when there are none.
pub fn on_screen(from: f32, view: f32, cells: &[Cell]) -> Option<(f32, f32)> {
    if cells.is_empty() {
        return None;
    }
    let at = |x: f32| {
        let Some(i) = cells.iter().position(|c| x < c.start + c.width) else { return cells.len() as f32 };
        let Cell { start, width } = cells[i];
        let into = (x - start).max(0.);
        // A pixel either way, for widths rounded under display scaling.
        if into <= 1. {
            i as f32
        } else if width - into <= 1. {
            (i + 1) as f32
        } else {
            i as f32 + into / width
        }
    };
    Some((at(from), at(from + view)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Columns 240px wide, two to a 480px view.
    fn cells(n: usize) -> Vec<Cell> {
        (0..n).map(|i| Cell { start: i as f32 * 240., width: 240. }).collect()
    }

    #[test]
    fn stays_put_while_the_column_and_what_it_opened_are_on_screen() {
        assert_eq!(reveal_target(240., 480., &cells(3), Some(1)), 240.);
        assert_eq!(reveal_target(240., 480., &cells(2), Some(1)), 240.);
    }

    #[test]
    fn stays_put_when_a_pick_closes_the_column_beside_it() {
        assert_eq!(reveal_target(480., 480., &cells(3), Some(2)), 480.);
    }

    #[test]
    fn moves_right_only_as_far_as_the_column_opened_takes() {
        assert_eq!(reveal_target(0., 480., &cells(3), Some(1)), 240.);
    }

    #[test]
    fn brings_the_last_column_flush_with_the_right_edge_leaving_no_room_past_it() {
        assert_eq!(reveal_target(0., 600., &cells(3), Some(1)), 120.);
    }

    #[test]
    fn brings_a_column_clipped_on_the_left_back_whole() {
        assert_eq!(reveal_target(360., 480., &cells(3), Some(1)), 240.);
    }

    #[test]
    fn brings_a_column_clipped_on_the_left_back_without_room_past_the_last() {
        assert_eq!(reveal_target(600., 600., &cells(4), Some(2)), 360.);
    }

    #[test]
    fn keeps_the_active_column_when_it_and_the_rest_cannot_all_fit() {
        assert_eq!(reveal_target(720., 480., &cells(4), Some(0)), 0.);
    }

    #[test]
    fn shows_the_last_opened_when_the_active_column_stands_outside() {
        assert_eq!(reveal_target(0., 480., &cells(4), None), 480.);
        assert_eq!(reveal_target(0., 480., &cells(2), None), 0.);
    }

    #[test]
    fn shows_the_last_opened_from_its_left_edge_when_it_is_wider_than_the_view() {
        assert_eq!(reveal_target(0., 200., &cells(3), None), 480.);
        assert_eq!(reveal_target(0., 200., &cells(1), None), 0.);
    }

    #[test]
    fn goes_back_to_the_start_once_nothing_scrolls() {
        assert_eq!(reveal_target(480., 480., &[], None), 0.);
    }

    #[test]
    fn uncovers_as_many_folders_as_fit_beside_the_column_kept_at_the_right_edge() {
        assert_eq!(uncover_target(480., &cells(4), 3), 480.);
        assert_eq!(uncover_target(600., &cells(4), 2), 120.);
        // Columns past it go off on the right.
        assert_eq!(uncover_target(480., &cells(4), 1), 0.);
    }

    #[test]
    fn uncovers_them_all_when_they_fit() {
        assert_eq!(uncover_target(1200., &cells(4), 3), 0.);
    }

    #[test]
    fn keeps_the_column_kept_whole_from_the_left_when_it_is_wider_than_the_view() {
        assert_eq!(uncover_target(200., &cells(4), 3), 720.);
    }

    #[test]
    fn keeps_the_last_column_at_the_right_edge_as_the_view_widens_or_narrows() {
        // Four columns end at 960; resting flush in a 480 view, at 480.
        assert_eq!(refit(480., 480., 960., 600., &cells(4), Some(3)), 360.);
        assert_eq!(refit(480., 480., 960., 400., &cells(4), Some(3)), 560.);
        // Wide enough for all of them, they start at the left.
        assert_eq!(refit(480., 480., 960., 1200., &cells(4), Some(3)), 0.);
    }

    #[test]
    fn keeps_the_last_column_flush_once_all_that_fit_no_longer_do() {
        assert_eq!(refit(0., 960., 960., 480., &cells(4), Some(2)), 480.);
    }

    #[test]
    fn stops_at_the_active_column_rather_than_push_it_off_the_left() {
        assert_eq!(refit(480., 480., 960., 300., &cells(4), Some(2)), 480.);
        assert_eq!(refit(480., 480., 960., 200., &cells(4), Some(2)), 480.);
    }

    #[test]
    fn keeps_the_last_column_from_its_left_edge_once_it_is_wider_than_the_view_worked_in_outside() {
        assert_eq!(refit(480., 480., 960., 300., &cells(4), None), 660.);
        assert_eq!(refit(480., 480., 960., 200., &cells(4), None), 720.);
    }

    #[test]
    fn keeps_where_it_was_brought_by_hand_closing_only_the_room_past_the_last() {
        assert_eq!(refit(100., 480., 960., 400., &cells(4), Some(3)), 100.);
        assert_eq!(refit(300., 480., 960., 800., &cells(4), Some(3)), 160.);
    }

    #[test]
    fn follows_a_width_dragged_as_it_does_a_view() {
        // Flush at 240 over three 240 columns in 480; widened to 300 each, they end at 900.
        let wider: Vec<Cell> = (0..3).map(|i| Cell { start: i as f32 * 300., width: 300. }).collect();
        assert_eq!(refit(240., 480., 720., 480., &wider, Some(2)), 420.);
    }

    #[test]
    fn spans_whole_columns_when_the_edges_fall_between_them() {
        assert_eq!(on_screen(0., 480., &cells(4)), Some((0., 2.)));
        assert_eq!(on_screen(480., 480., &cells(4)), Some((2., 4.)));
    }

    #[test]
    fn cuts_the_columns_the_edges_cut() {
        assert_eq!(on_screen(120., 480., &cells(4)), Some((0.5, 2.5)));
        assert_eq!(on_screen(100., 100., &cells(2)), Some((100. / 240., 200. / 240.)));
    }

    #[test]
    fn allows_a_pixel_for_widths_rounded_under_display_scaling() {
        assert_eq!(on_screen(239., 481., &cells(4)), Some((1., 3.)));
        assert_eq!(on_screen(1., 478., &cells(4)), Some((0., 2.)));
    }

    #[test]
    fn ends_at_the_last_column_when_the_view_reaches_past_it() {
        assert_eq!(on_screen(240., 480., &cells(2)), Some((1., 2.)));
    }

    #[test]
    fn spans_them_all_when_they_fit_a_pixel_over_or_not() {
        assert_eq!(on_screen(0., 480., &cells(2)), Some((0., 2.)));
        assert_eq!(on_screen(1., 479., &cells(2)), Some((0., 2.)));
    }

    #[test]
    fn finds_none_without_columns() {
        assert_eq!(on_screen(0., 480., &[]), None);
    }
}
