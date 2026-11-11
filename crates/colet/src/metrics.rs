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
/// A control set among smaller ones: the default order's button, a column's filter field, a hint to search.
pub const SMALL_CONTROL_HEIGHT: f32 = 28.;
/// A square button holding only a glyph: a column's order, the folded crumbs.
pub const ICON_BUTTON: f32 = 24.;

/// The corners of a row, a crumb, a tab and a quiet button. A ring drawn round one takes the same, so the
/// two stay concentric.
pub const RADIUS: f32 = 4.;
/// The corners of a field and of a bordered control beside one; the frame that rings it takes the same.
pub const FIELD_RADIUS: f32 = 6.;

/// An item's own picture, as the shell draws it: in the rows, the menus and a row dragged in home.
pub const ICON: f32 = 16.;
/// Most of the glyphs drawn beside a label or alone in a button.
pub const GLYPH: f32 = 14.;

/// The window's text.
pub const FONT_UI: &str = "Segoe UI";
/// What is typed for Everything to read: a query, a filter's search.
pub const FONT_CODE: &str = "Consolas";
