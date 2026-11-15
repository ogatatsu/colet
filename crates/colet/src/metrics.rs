//! Colet's sizes by role, as [`crate::theme::Palette`] gives its colors by role. Only the sizes that recur,
//! or that several parts must share to line up, are named here.
//!
//! Every size in Colet, these and the widths the user drags, is given in pixels as drawn at 100%: the
//! theme's base font at [`BASE`]. Drawn, it goes through [`scaled`], a length in rems, so it follows the
//! zoom the settings choose along with the text; reckoned against what layout measured, it goes through
//! [`Scale`], which knows how many pixels it is drawn as.
use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Pixels, Rems, Window, px, rems};

/// The theme's base font at 100%: the rem the sizes here are given against.
pub const BASE: f32 = 16.;

/// A size given in pixels at 100%, as a length that follows the zoom.
pub fn scaled(size: f32) -> Rems {
    rems(size / BASE)
}

/// How many pixels a pixel at 100% is drawn as: the zoom, as the window's rem has it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scale(f32);

impl Default for Scale {
    fn default() -> Self {
        Self(1.)
    }
}

impl Scale {
    pub fn of(window: &Window) -> Self {
        Self(f32::from(window.rem_size()) / BASE)
    }

    /// `size`, given at 100%, in pixels as drawn.
    pub fn px(self, size: f32) -> Pixels {
        px(size * self.0)
    }

    /// `size`, given at 100%, as the number of pixels it is drawn as.
    pub fn up(self, size: f32) -> f32 {
        size * self.0
    }

    /// A length as drawn, as given at 100%.
    pub fn down(self, drawn: f32) -> f32 {
        drawn / self.0
    }
}

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
/// The status bar along the bottom; the notices stand clear of it.
pub const STATUS_BAR_HEIGHT: f32 = 26.;

/// The theme's corners by tier, given at 100% as [`scaled`], `ring` and `frame` take sizes. Drawn, they
/// come back to the theme's own pixels at any zoom, so Colet's parts round as the library's do. A ring
/// drawn round a part takes the part's tier, so the two stay concentric.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    /// A row, a crumb, a tab, a badge and a quiet button.
    pub sm: f32,
    /// A field and a bordered control beside one, as the library's own controls.
    pub md: f32,
    /// A raised surface: the history.
    pub lg: f32,
}

impl Radii {
    pub fn of(window: &Window, cx: &App) -> Self {
        Self::at(Scale::of(window), cx)
    }

    /// The tiers for a window drawn at `scale`.
    pub fn at(scale: Scale, cx: &App) -> Self {
        let tiers = cx.theme().radius_tokens();
        let down = |radius: Pixels| scale.down(f32::from(radius));
        Self { sm: down(tiers.sm), md: down(tiers.md), lg: down(tiers.lg) }
    }
}

/// An item's own picture, as the shell draws it: in the rows, the menus and a row dragged in home.
pub const ICON: f32 = 16.;
/// Most of the glyphs drawn beside a label or alone in a button.
pub const GLYPH: f32 = 14.;

/// The window's text.
pub const FONT_UI: &str = "Segoe UI";
/// What is typed for Everything to read: a query, a filter's search.
pub const FONT_CODE: &str = "Consolas";
