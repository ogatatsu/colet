//! Colet's sizes by role, in pixels, as [`crate::theme::Palette`] gives its colors by role. Only the sizes
//! that recur, or that several parts must share to line up, are named here.

/// A dialog's title.
pub const TEXT_TITLE: f32 = 15.;
/// The window's own size, which text takes unless it sets one: a dialog's message, a tab's title, a crumb.
pub const TEXT_LARGE: f32 = 13.;
/// Set on most of the window's text: rows, headings, fields and buttons.
pub const TEXT_BODY: f32 = 12.;
/// The second line of a row and other secondary text.
pub const TEXT_SMALL: f32 = 11.;
/// The count on the history's button.
pub const TEXT_TINY: f32 = 10.;

/// The heading of each column, of home's pane and of the folded columns' strip, which stand level with
/// one another.
pub const HEADING_HEIGHT: f32 = 34.;
/// A text field, as the search field, the address and a dialog's fields, and a button standing beside one.
pub const CONTROL_HEIGHT: f32 = 32.;
