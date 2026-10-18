//! Where the scrolling columns should rest so the one being worked in, and what it opened, are on screen.
//! When they must move, the last column ends flush with the right edge, so no room is left empty past it,
//! unless that would hide the column worked in. Picking along a column, which opens and closes the one
//! beside it, leaves them where they are.

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
        return if active.is_none() { flush } else { start };
    }
    // From the active one to the last fit: brought in from either side, the last ends at the right edge.
    flush
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
    fn goes_back_to_the_start_once_nothing_scrolls() {
        assert_eq!(reveal_target(480., 480., &[], None), 0.);
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
