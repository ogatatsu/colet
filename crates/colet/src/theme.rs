//! Colet's colors by role, dark and light, as Windows is set or as the settings fix them. The light ones
//! keep the dark ones' roles, hues and contrasts on a light ground.
use std::{rc::Rc, sync::LazyLock};

use gpui_kit::base::input::{InputEditorStyle, InputState};
use gpui_kit::component::{ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{App, Entity, Hsla, Rgba, Window, WindowAppearance, hsla};

use crate::settings::Theme;

fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360., s / 100., l / 100., 1.)
}

/// Colet's colors as the component library's theme draws now. The library's theme is the one source: its
/// mode, set by [`apply`], picks these, and its own colors are made from them (see [`init`]), so Colet's
/// parts and the library's never draw from two settings.
pub trait ActivePalette {
    fn palette(&self) -> Palette;
}

impl ActivePalette for App {
    fn palette(&self) -> Palette {
        static LIGHT: LazyLock<Palette> = LazyLock::new(Palette::light);
        static DARK: LazyLock<Palette> = LazyLock::new(Palette::dark);
        if gpui_kit::component::Theme::global(self).is_dark() { *DARK } else { *LIGHT }
    }
}

#[derive(Clone, Copy)]
pub struct Palette {
    // Surfaces, darkest to lightest in the dark setting.
    /// Behind the tabs: darker than `bg`, which the current tab takes, so that tab stands off the rest.
    pub tab_strip: Hsla,
    /// The frame around the columns: the toolbar, the file actions' row, home, the fold strip and the
    /// status bar, all in one.
    pub bg: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub hover: Hsla,
    /// What the pointer is on over a raised surface: a notice, a dialog or the history.
    pub raised_hover: Hsla,
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub accent_fill: Hsla,
    pub selected_inactive: Hsla,
    /// Kept well below `accent_fill`, so a pointed-at file never reads as the selection, and off
    /// `hover`, so it never reads as where the pointer is.
    pub highlight: Hsla,
    pub danger: Hsla,
    /// Behind the count of problems on the history's button.
    pub danger_fill: Hsla,
    /// The ring around what has the keys.
    pub focus: Hsla,
    /// The ring around a selected row with the keys, set off from the selection's fill.
    pub focus_on_fill: Hsla,
    /// The thumb of a scrollbar.
    pub scrollbar: Hsla,
    /// Behind the selected text of a field.
    pub selection: Hsla,
    pub on_accent: Hsla,
    /// Secondary text on the accent fill.
    pub on_accent_muted: Hsla,
    /// What a search matched, on the accent fill: set off from `on_accent` around it.
    pub on_accent_match: Hsla,
    /// Behind the edge columns are hidden past.
    pub shade: Hsla,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            tab_strip: hsl(220., 4., 4.),
            bg: hsl(220., 4., 9.),
            surface: hsl(220., 4., 12.),
            raised: hsl(220., 3., 16.),
            hover: hsl(220., 3., 17.),
            raised_hover: hsl(220., 3., 22.),
            border: hsl(220., 3., 19.),
            border_strong: hsl(220., 3., 28.),
            text: hsl(220., 4., 90.),
            text_muted: hsl(220., 3., 62.),
            text_faint: hsl(220., 3., 56.),
            accent: hsl(212., 85., 72.),
            accent_fill: hsl(214., 62., 36.),
            selected_inactive: hsl(220., 3., 21.),
            highlight: hsl(214., 30., 22.),
            danger: hsl(0., 72., 70.),
            danger_fill: hsl(0., 58., 42.),
            focus: hsl(212., 90., 72.),
            focus_on_fill: hsl(212., 90., 80.),
            scrollbar: hsla(220. / 360., 0.03, 0.40, 0.8),
            selection: hsl(214., 55., 32.),
            on_accent: hsl(0., 0., 100.),
            on_accent_muted: hsla(0., 0., 1., 0.85),
            on_accent_match: hsl(45., 95., 75.),
            shade: hsla(220. / 360., 0.1, 0.03, 0.55),
        }
    }

    pub fn light() -> Self {
        Self {
            tab_strip: hsl(220., 4., 89.),
            bg: hsl(220., 4., 96.),
            surface: hsl(0., 0., 100.),
            raised: hsl(0., 0., 100.),
            hover: hsl(220., 4., 93.),
            raised_hover: hsl(220., 4., 93.),
            border: hsl(220., 4., 87.),
            border_strong: hsl(220., 3., 76.),
            text: hsl(220., 4., 13.),
            text_muted: hsl(220., 3., 38.),
            text_faint: hsl(220., 3., 42.),
            accent: hsl(214., 80., 38.),
            accent_fill: hsl(214., 72., 40.),
            selected_inactive: hsl(220., 4., 88.),
            highlight: hsl(214., 80., 89.),
            danger: hsl(0., 65., 45.),
            danger_fill: hsl(0., 65., 42.),
            focus: hsl(214., 80., 45.),
            focus_on_fill: hsl(214., 90., 82.),
            scrollbar: hsla(220. / 360., 0.03, 0.55, 0.7),
            selection: hsl(214., 80., 85.),
            on_accent: hsl(0., 0., 100.),
            on_accent_muted: hsla(0., 0., 1., 0.85),
            on_accent_match: hsl(48., 100., 80.),
            shade: hsla(220. / 360., 0.1, 0.2, 0.18),
        }
    }

    /// Gives a field Colet's colors: its selected text on the selection fill.
    pub fn style_input(self, input: &Entity<InputState>, cx: &mut App) {
        input.update(cx, |input, _| input.set_editor_style(self.input_style()));
    }

    fn input_style(self) -> InputEditorStyle {
        InputEditorStyle {
            foreground: self.text,
            muted_foreground: self.text_muted,
            background: self.surface,
            border: self.border_strong,
            selection: self.selection,
            caret: self.text,
            ..Default::default()
        }
    }

    /// These roles as a theme for the component library, its other colors, hovers and presses among them,
    /// following from these as the library's own do.
    fn config(self, name: &'static str, mode: ThemeMode) -> ThemeConfig {
        let mut colors = ThemeConfigColors::default();
        for (role, color) in [
            (&mut colors.background, self.bg),
            (&mut colors.foreground, self.text),
            (&mut colors.border, self.border),
            (&mut colors.input, self.border_strong),
            (&mut colors.muted, self.surface),
            (&mut colors.muted_foreground, self.text_muted),
            // The menus light their lines with it, over the popover's raised surface.
            (&mut colors.accent, self.raised_hover),
            (&mut colors.accent_foreground, self.text),
            (&mut colors.primary, self.accent_fill),
            (&mut colors.primary_foreground, self.on_accent),
            (&mut colors.secondary, self.raised),
            (&mut colors.secondary_hover, self.raised_hover),
            (&mut colors.secondary_foreground, self.text),
            (&mut colors.danger, self.danger_fill),
            (&mut colors.danger_foreground, self.on_accent),
            (&mut colors.ring, self.focus),
            (&mut colors.selection, self.selection),
            (&mut colors.caret, self.text),
            (&mut colors.link, self.accent),
            (&mut colors.popover, self.raised),
            (&mut colors.popover_foreground, self.text),
            (&mut colors.list, self.surface),
            (&mut colors.list_hover, self.hover),
            (&mut colors.sidebar, self.bg),
            (&mut colors.sidebar_foreground, self.text),
            (&mut colors.sidebar_border, self.border),
            (&mut colors.sidebar_accent, self.hover),
            (&mut colors.sidebar_accent_foreground, self.text),
            (&mut colors.sidebar_primary, self.accent_fill),
            (&mut colors.sidebar_primary_foreground, self.on_accent),
            (&mut colors.scrollbar_thumb, self.scrollbar),
            (&mut colors.scrollbar_thumb_hover, self.text_muted),
            (&mut colors.overlay, self.shade),
            (&mut colors.status_bar, self.bg),
            (&mut colors.status_bar_border, self.border),
        ] {
            *role = Some(hex(color).into());
        }
        let mut config = ThemeConfig::default();
        config.name = name.into();
        config.mode = mode;
        config.colors = colors;
        config
    }
}

/// `#RRGGBBAA`, each channel rounded to the nearest of its 256 steps, as a theme file gives a color.
fn hex(color: Hsla) -> String {
    let Rgba { r, g, b, a } = color.into();
    let [r, g, b, a] = [r, g, b, a].map(|channel| (channel * 255.).round() as u8);
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}

/// Whether `theme` is dark: as it is set, or for following Windows, as the window's appearance is.
fn is_dark(window: &Window, theme: Theme) -> bool {
    match (theme, window.appearance()) {
        (Theme::Light, _) => false,
        (Theme::Dark, _) => true,
        (Theme::System, WindowAppearance::Dark | WindowAppearance::VibrantDark) => true,
        (Theme::System, _) => false,
    }
}

/// Gives the component library Colet's colors, light and dark, so its components draw in them.
pub fn init(cx: &mut App) {
    let light = Rc::new(Palette::light().config("Colet Light", ThemeMode::Light));
    let dark = Rc::new(Palette::dark().config("Colet Dark", ThemeMode::Dark));
    gpui_kit::component::Theme::update(cx, |kit| {
        kit.light_theme = light;
        kit.dark_theme = dark;
        // The notices stand over the status bar's right end, by the history they tell of; the newest
        // three at most.
        let notices = &mut kit.notification;
        notices.placement = gpui_kit::Anchor::BottomRight;
        notices.max_items = 3;
    });
    set_zoom(100, cx);
}

/// Draws everything at `zoom` percent: the theme's base font is the window's rem, which every size Colet
/// and the component library give in rems follows (see [`crate::metrics`]). The notices' margins, which
/// the library takes in pixels, follow with it.
pub fn set_zoom(zoom: u16, cx: &mut App) {
    let scale = f32::from(zoom) / 100.;
    gpui_kit::component::Theme::update(cx, |kit| {
        kit.font_size = gpui_kit::px(crate::metrics::BASE * scale);
        // Clear of the status bar by 10, by the history the notices tell of.
        kit.notification.margins.bottom = gpui_kit::px((crate::metrics::STATUS_BAR_HEIGHT + 10.) * scale);
        kit.notification.margins.right = gpui_kit::px(12. * scale);
    });
}

/// Has the component library draw in the colors `theme` gives the window.
pub fn apply(window: &Window, theme: Theme, cx: &mut App) {
    let mode = if is_dark(window, theme) { ThemeMode::Dark } else { ThemeMode::Light };
    gpui_kit::component::Theme::change(mode, None, cx);
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::base::{ColorTokens, SemanticThemeTokens};

    #[test]
    fn the_pointer_and_the_pointed_at_stand_off_what_they_lie_on() {
        for (palette, dark) in [(Palette::light(), false), (Palette::dark(), true)] {
            // Some 4% of lightness, short of what rounding takes off.
            let apart = |a: Hsla, b: Hsla| (a.l - b.l).abs() >= 0.039;
            assert!(apart(palette.raised_hover, palette.raised), "hover over a raised surface, dark: {dark}");
            assert!(apart(palette.highlight, palette.hover), "pointed-at against hovered, dark: {dark}");
            // Short of the selection's fill, toward the ground.
            assert!(if dark { palette.highlight.l < palette.accent_fill.l } else { palette.highlight.l > palette.accent_fill.l });
        }
    }

    /// WCAG's contrast ratio of two opaque colors.
    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let luminance = |c: Hsla| {
            let c = gpui_kit::Rgba::from(c);
            let lin = |v: f32| if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) };
            0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
        };
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn small_text_on_a_fill_reads_at_4_5_to_1_or_more() {
        for (palette, dark) in [(Palette::light(), false), (Palette::dark(), true)] {
            // The count of problems on the history's button.
            let badge = contrast(palette.on_accent, palette.danger_fill);
            assert!(badge >= 4.5, "badge {badge}, dark: {dark}");
            // Muted text on the window's own color.
            let paths = contrast(palette.text_muted, palette.bg);
            assert!(paths >= 4.5, "paths {paths}, dark: {dark}");
            assert!(palette.bg != palette.raised, "the paths' ground stands off the dialog, dark: {dark}");
        }
    }

    #[test]
    fn text_on_each_ground_it_is_drawn_on_reads_at_4_5_to_1_or_more() {
        for (palette, dark) in [(Palette::light(), false), (Palette::dark(), true)] {
            for (what, ink, ground) in [
                ("a row on the selection", palette.on_accent, palette.accent_fill),
                ("a match in a row on the selection", palette.on_accent_match, palette.accent_fill),
                ("a match in a row", palette.accent, palette.surface),
                ("a match in a row selected aside", palette.accent, palette.selected_inactive),
                ("a path in a row selected aside", palette.text_muted, palette.selected_inactive),
                ("a column's count", palette.text_faint, palette.surface),
                ("faint text in home and the status bar", palette.text_faint, palette.bg),
                ("a tab not current", palette.text_muted, palette.tab_strip),
            ] {
                let ratio = contrast(ink, ground);
                assert!(ratio >= 4.5, "{what}: {ratio}, dark: {dark}");
            }
        }
    }

    #[test]
    fn the_component_theme_takes_each_role_as_it_is() {
        for (palette, mode) in [(Palette::light(), ThemeMode::Light), (Palette::dark(), ThemeMode::Dark)] {
            let config = palette.config("test", mode);
            let colors = &config.colors;
            for (given, role) in [
                (&colors.background, palette.bg),
                (&colors.foreground, palette.text),
                (&colors.primary, palette.accent_fill),
                (&colors.ring, palette.focus),
                (&colors.scrollbar_thumb, palette.scrollbar),
                (&colors.overlay, palette.shade),
            ] {
                let given = gpui_kit::component::try_parse_color(given.as_deref().unwrap()).unwrap();
                assert_eq!(hex(given), hex(role), "{mode:?}");
            }
        }
    }

    #[test]
    fn input_ink_and_caret_keep_the_chosen_theme_against_either_base_theme() {
        for palette in [Palette::light(), Palette::dark()] {
            for colors in [ColorTokens::light(), ColorTokens::dark()] {
                let mut tokens = SemanticThemeTokens::default();
                tokens.colors = colors;
                let style = palette.input_style().resolved(&tokens);
                assert_eq!(style.foreground, palette.text);
                assert_eq!(style.muted_foreground, palette.text_muted);
                assert_eq!(style.caret, palette.text);
                assert_eq!(style.selection, palette.selection);
            }
        }
    }
}
