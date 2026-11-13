//! How many crumbs fold into the "…" menu: when the trail outgrows the bar, the crumbs
//! furthest from the deepest folder fold first so the nearest stay readable. Names are never shortened,
//! but the last crumb's, which always stays, clipped to half the bar at most.
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
};

use gpui_kit::{Bounds, Pixels};

use crate::state::ColumnId;

/// A crumb on the trail: the column it goes to, its name, and the path its tip shows. It is known by the
/// column's id, not its place: folding and unfolding move the columns, and its stop and its id go with it.
#[derive(Clone)]
pub struct Crumb {
    pub column: ColumnId,
    pub name: String,
    pub path: String,
}

/// The trail as it was last drawn, shared with what measures it as it is painted.
#[derive(Default)]
pub struct Trail {
    /// The crumbs folded into the "…" menu.
    pub folded: Vec<Crumb>,
    /// Where each crumb was drawn, by its column, and the row they stand in: the bar under them follows
    /// the columns on screen.
    pub bounds: Rc<RefCell<HashMap<ColumnId, Bounds<Pixels>>>>,
    pub row: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// Where the bar under the crumbs was last painted, `(start, end)` across the window; `None` while
    /// none was.
    pub bar: Rc<Cell<Option<(f32, f32)>>>,
    /// The width the crumbs were last folded for.
    pub room: Rc<Cell<f32>>,
}

/// The gap between crumbs.
pub const GAP: f32 = 2.;
/// The chevron before a crumb, with the gap between them.
pub const SEPARATOR: f32 = 13. + GAP;
/// The "…" button.
pub const MORE: f32 = 24.;
/// The room the crumbs always leave past them, to press to type a path.
pub const TYPING_ROOM: f32 = 32.;

/// The leading crumbs to fold, given each crumb's whole width (its separator and gap counted) and the
/// bar's `room`.
pub fn folded(widths: &[f32], room: f32) -> usize {
    if widths.iter().sum::<f32>() <= room || widths.is_empty() {
        return 0;
    }
    let last = widths.len() - 1;
    let mut left = room - MORE - GAP - widths[last].min(room / 2.);
    let mut next = last;
    while next > 0 && widths[next - 1] <= left {
        next -= 1;
        left -= widths[next];
    }
    next
}

/// Where the bar under the crumbs starts and ends, given the columns on screen as [`crate::reveal::on_screen`]
/// measures them and where each column's crumb was drawn, `(left, right)`, by the column's place on
/// screen; `None` for one folded into the menu. Resting on a column's edge, the bar ends on its crumb's
/// edge; scrolled part way, its start runs over the crumb and the gap and chevron after it, its end over
/// the gap and chevron before the crumb and the crumb, so it never jumps across them.
pub fn bar(span: (f32, f32), drawn: impl Fn(usize) -> Option<(f32, f32)>) -> Option<(f32, f32)> {
    let (s0, s1) = span;
    let head = s0.floor() as usize;
    let tail = (s1.ceil() as usize).saturating_sub(1);
    // Crumbs folded into the menu have no place to mark: start at the first one shown.
    let from = (head..=tail).find(|&i| drawn(i).is_some())?;
    let start = if from == head {
        let (left, right) = drawn(head)?;
        let next = drawn(head + 1).map_or(right, |n| n.0);
        left + (next - left) * (s0 - s0.floor())
    } else {
        drawn(from)?.0
    };
    let (left, right) = drawn(tail)?;
    let before = if tail > from { drawn(tail - 1).map_or(left, |p| p.1) } else { left };
    let end = before + (right - before) * (s1 - tail as f32).clamp(0., 1.);
    Some((start, end))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crumbs 80px wide with 20px between them, from 0: column `i`'s spans `100 i` to `100 i + 80`.
    fn spaced(i: usize) -> Option<(f32, f32)> {
        (i < 4).then(|| (i as f32 * 100., i as f32 * 100. + 80.))
    }

    #[test]
    fn the_bar_rests_on_the_crumbs_edges() {
        assert_eq!(bar((1., 3.), spaced), Some((100., 280.)));
        assert_eq!(bar((0., 4.), spaced), Some((0., 380.)));
    }

    #[test]
    fn the_bar_runs_over_the_gaps_without_a_jump() {
        // Halfway into a column, the start is halfway from its crumb to the next.
        assert_eq!(bar((1.5, 3.5), spaced), Some((150., 330.)));
        // Either side of a column's edge, both ends land where they would on it.
        let (a, b) = bar((0.999, 2.999), spaced).unwrap();
        let (c, d) = bar((1.001, 3.001), spaced).unwrap();
        assert!((a - 100.).abs() < 0.2 && (c - 100.).abs() < 0.2, "{a} {c}");
        assert!((b - 280.).abs() < 0.2 && (d - 280.).abs() < 0.2, "{b} {d}");
    }

    #[test]
    fn the_bar_keeps_to_the_first_and_last_crumbs() {
        // Past the last crumb there is no gap to run over: the start goes along the crumb alone.
        assert_eq!(bar((3.5, 4.), spaced), Some((340., 380.)));
        // Folded into the menu, the first two have no place: the bar starts at the first shown.
        let shown = |i: usize| if i < 2 { None } else { spaced(i) };
        assert_eq!(bar((0.5, 3.5), shown), Some((200., 330.)));
        // With no crumb shown before the last on screen, its end goes along it alone.
        assert_eq!(bar((1.5, 2.5), shown), Some((200., 240.)));
    }

    #[test]
    fn folds_nothing_while_the_trail_fits() {
        assert_eq!(folded(&[100., 100., 100.], 300.), 0);
        assert_eq!(folded(&[], 10.), 0);
    }

    #[test]
    fn folds_the_furthest_first_keeping_the_nearest_whole() {
        // 400 wanted in 300: the last keeps 100, "…" takes 26, so 174 is left for whole crumbs.
        assert_eq!(folded(&[100., 100., 100., 100.], 300.), 2);
        assert_eq!(folded(&[50., 50., 100., 100., 100.], 330.), 2);
    }

    #[test]
    fn keeps_only_the_last_clipped_to_half_the_bar_when_nothing_else_fits() {
        // The first fits beside the last clipped to half the bar: nothing folds.
        assert_eq!(folded(&[100., 500.], 300.), 0);
        assert_eq!(folded(&[140., 500.], 300.), 1);
        assert_eq!(folded(&[200., 200., 200.], 250.), 2);
    }
}
