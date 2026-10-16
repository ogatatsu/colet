//! How many crumbs fold into the "…" menu, as the old app's: when the trail outgrows the bar, the crumbs
//! furthest from the deepest folder fold first so the nearest stay readable. Names are never shortened,
//! but the last crumb's, which always stays, clipped to half the bar at most.

/// The gap between crumbs.
pub const GAP: f32 = 2.;
/// The chevron before a crumb, with the gap between them.
pub const SEPARATOR: f32 = 12. + GAP;
/// The "…" button.
pub const MORE: f32 = 24.;

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

#[cfg(test)]
mod tests {
    use super::*;

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
