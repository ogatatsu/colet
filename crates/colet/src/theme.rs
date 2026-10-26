//! Colet's colors by role, dark and light, as Windows is set or as the settings fix them. The dark ones
//! are the old app's; the light ones keep the same roles, hues and contrasts on a light ground.
use std::rc::Rc;

use gpui_kit::base::input::{InputEditorStyle, InputState};
use gpui_kit::component::{ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{App, Entity, Hsla, Rgba, Window, WindowAppearance, hsla};

use crate::settings::Theme;

fn hsl(h: f32, s: f32, l: f32) -> Hsla {
    hsla(h / 360., s / 100., l / 100., 1.)
}

#[derive(Clone, Copy)]
pub struct Palette {
    /// Surfaces, darkest to lightest in the dark setting.
    pub sidebar: Hsla,
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
    /// `danger` on the selection's fill.
    pub danger_on_fill: Hsla,
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
    /// Behind the edge columns are hidden past.
    pub shade: Hsla,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            sidebar: hsl(210., 15., 7.),
            bg: hsl(210., 14., 9.),
            surface: hsl(210., 13., 12.),
            raised: hsl(210., 12., 16.),
            hover: hsl(210., 12., 17.),
            raised_hover: hsl(210., 11., 22.),
            border: hsl(210., 10., 19.),
            border_strong: hsl(210., 9., 28.),
            text: hsl(210., 14., 90.),
            text_muted: hsl(210., 8., 62.),
            text_faint: hsl(210., 7., 56.),
            accent: hsl(178., 40., 62.),
            accent_fill: hsl(184., 42., 28.),
            selected_inactive: hsl(210., 10., 21.),
            highlight: hsl(184., 26., 22.),
            danger: hsl(0., 75., 70.),
            danger_on_fill: hsl(0., 75., 70.),
            danger_fill: hsl(0., 55., 40.),
            focus: hsl(178., 50., 68.),
            focus_on_fill: hsl(178., 50., 68.),
            scrollbar: hsla(210. / 360., 0.09, 0.40, 0.8),
            selection: hsl(184., 42., 28.),
            on_accent: hsl(0., 0., 100.),
            on_accent_muted: hsla(0., 0., 1., 0.85),
            shade: hsla(210. / 360., 0.3, 0.03, 0.55),
        }
    }

    pub fn light() -> Self {
        Self {
            sidebar: hsl(210., 18., 94.),
            bg: hsl(210., 20., 96.),
            surface: hsl(0., 0., 100.),
            raised: hsl(0., 0., 100.),
            hover: hsl(210., 20., 93.),
            raised_hover: hsl(210., 20., 93.),
            border: hsl(210., 14., 87.),
            border_strong: hsl(210., 12., 76.),
            text: hsl(210., 20., 13.),
            text_muted: hsl(210., 9., 38.),
            text_faint: hsl(210., 8., 45.),
            accent: hsl(184., 60., 30.),
            accent_fill: hsl(184., 50., 33.),
            selected_inactive: hsl(210., 14., 88.),
            highlight: hsl(184., 40., 89.),
            danger: hsl(0., 65., 45.),
            danger_on_fill: hsl(0., 100., 82.),
            danger_fill: hsl(0., 65., 42.),
            focus: hsl(184., 60., 35.),
            focus_on_fill: hsl(184., 70., 78.),
            scrollbar: hsla(210. / 360., 0.10, 0.55, 0.7),
            selection: hsl(184., 50., 80.),
            on_accent: hsl(0., 0., 100.),
            on_accent_muted: hsla(0., 0., 1., 0.85),
            shade: hsla(210. / 360., 0.3, 0.2, 0.18),
        }
    }

    /// Gives a field Colet's colors: its selected text on the selection fill, as the old app's.
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

    /// The colors `theme` gives: its own, or for following Windows, those of the window's appearance.
    pub fn for_window(window: &Window, theme: Theme) -> Self {
        if is_dark(window, theme) { Self::dark() } else { Self::light() }
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
            (&mut colors.accent, self.hover),
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
            (&mut colors.sidebar, self.sidebar),
            (&mut colors.sidebar_foreground, self.text),
            (&mut colors.sidebar_border, self.border),
            (&mut colors.sidebar_accent, self.hover),
            (&mut colors.sidebar_accent_foreground, self.text),
            (&mut colors.sidebar_primary, self.accent_fill),
            (&mut colors.sidebar_primary_foreground, self.on_accent),
            (&mut colors.scrollbar_thumb, self.scrollbar),
            (&mut colors.scrollbar_thumb_hover, self.text_muted),
            (&mut colors.overlay, self.shade),
        ] {
            *role = Some(hex(color).into());
        }
        ThemeConfig { name: name.into(), mode, colors, ..Default::default() }
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
        notices.margins.bottom = gpui_kit::px(36.);
        notices.margins.right = gpui_kit::px(12.);
        notices.max_items = 3;
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
                let tokens = SemanticThemeTokens { colors, ..Default::default() };
                let style = palette.input_style().resolved(&tokens);
                assert_eq!(style.foreground, palette.text);
                assert_eq!(style.muted_foreground, palette.text_muted);
                assert_eq!(style.caret, palette.text);
                assert_eq!(style.selection, palette.selection);
            }
        }
    }
}
