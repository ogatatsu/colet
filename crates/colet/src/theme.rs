//! Colet's colors by role, dark and light, as Windows is set or as the settings fix them. The light ones
//! keep the dark ones' roles, hues and contrasts on a light ground.
use std::{rc::Rc, sync::LazyLock, time::Duration};

use gpui_kit::base::input::{CursorBlink, InputEditorStyle, InputState};
use gpui_kit::component::{ThemeConfig, ThemeConfigColors, ThemeMode};
use gpui_kit::{App, Entity, Hsla, Rgba, Window, WindowAppearance, hsla, rgb, white};

use crate::settings::Theme;

/// Behind the button that closes the window, under the pointer and pressed, as Windows 11 draws it.
const CLOSE_HOVER: u32 = 0xc42b1c;
const CLOSE_PRESSED: u32 = 0xc83c31;

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
    /// A tab not current under the pointer: between `tab_strip` and `bg`, so it never outshines the
    /// current tab.
    pub tab_hover: Hsla,
    /// The frame around the columns: the toolbar, home, the fold strip and the
    /// status bar, all in one.
    pub bg: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub hover: Hsla,
    /// What the pointer is on over a raised surface: a notice, a dialog or the history.
    pub raised_hover: Hsla,
    /// Behind a button whose menu, or the history, is open, for as long as it is: a step past `raised_hover`,
    /// which the library's quiet buttons take under the pointer, and past `hover`, so it never reads as where
    /// the pointer is.
    pub pressed: Hsla,
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
    /// Behind the chosen one of a radio group's choices, ringed in `accent`: off `raised`, which the others
    /// take, and off `raised_hover`, so it never reads as where the pointer is.
    pub chosen: Hsla,
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
    /// Behind the button that closes the window, under the pointer and pressed, and its glyph there: the
    /// red Windows 11 draws in both settings.
    pub close_hover: Hsla,
    pub close_pressed: Hsla,
    pub on_close: Hsla,
}

impl Palette {
    pub fn dark() -> Self {
        Self {
            tab_strip: hsl(220., 4., 4.),
            tab_hover: hsl(220., 4., 9.),
            bg: hsl(220., 4., 11.),
            surface: hsl(220., 4., 14.),
            raised: hsl(220., 3., 18.),
            hover: hsl(220., 3., 19.),
            raised_hover: hsl(220., 3., 24.),
            pressed: hsl(220., 3., 28.),
            border: hsl(220., 3., 21.),
            border_strong: hsl(220., 3., 30.),
            text: hsl(220., 4., 90.),
            text_muted: hsl(220., 3., 65.),
            text_faint: hsl(220., 3., 58.),
            accent: hsl(213., 65., 72.),
            accent_fill: hsl(214., 48., 38.),
            selected_inactive: hsl(220., 3., 23.),
            highlight: hsl(214., 24., 25.),
            chosen: hsl(214., 32., 30.),
            danger: hsl(0., 72., 70.),
            danger_fill: hsl(0., 58., 42.),
            focus: hsl(213., 70., 72.),
            focus_on_fill: hsl(213., 70., 82.),
            scrollbar: hsla(220. / 360., 0.03, 0.40, 0.8),
            selection: hsl(214., 42., 33.),
            on_accent: hsl(0., 0., 100.),
            on_accent_muted: hsla(0., 0., 1., 0.85),
            on_accent_match: hsl(45., 95., 75.),
            shade: hsla(220. / 360., 0.1, 0.03, 0.55),
            close_hover: rgb(CLOSE_HOVER).into(),
            close_pressed: rgb(CLOSE_PRESSED).into(),
            on_close: white(),
        }
    }

    pub fn light() -> Self {
        Self {
            tab_strip: hsl(220., 4., 86.),
            tab_hover: hsl(220., 4., 93.),
            bg: hsl(220., 4., 96.),
            surface: hsl(0., 0., 100.),
            raised: hsl(0., 0., 100.),
            hover: hsl(220., 4., 93.),
            raised_hover: hsl(220., 4., 93.),
            pressed: hsl(220., 3., 87.),
            border: hsl(220., 4., 87.),
            border_strong: hsl(220., 3., 76.),
            text: hsl(220., 4., 13.),
            text_muted: hsl(220., 3., 36.),
            text_faint: hsl(220., 3., 42.),
            accent: hsl(214., 65., 38.),
            accent_fill: hsl(214., 58., 40.),
            selected_inactive: hsl(220., 4., 88.),
            highlight: hsl(214., 60., 89.),
            chosen: hsl(214., 75., 88.),
            danger: hsl(0., 65., 45.),
            danger_fill: hsl(0., 65., 42.),
            focus: hsl(214., 65., 45.),
            focus_on_fill: hsl(214., 75., 82.),
            scrollbar: hsla(220. / 360., 0.03, 0.55, 0.7),
            selection: hsl(214., 65., 85.),
            on_accent: hsl(0., 0., 100.),
            on_accent_muted: hsla(0., 0., 1., 0.85),
            on_accent_match: hsl(48., 100., 80.),
            shade: hsla(220. / 360., 0.1, 0.2, 0.18),
            close_hover: rgb(CLOSE_HOVER).into(),
            close_pressed: rgb(CLOSE_PRESSED).into(),
            on_close: white(),
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
            // A quiet button of the library's fills with it while its menu is open.
            (&mut colors.secondary_active, self.pressed),
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
        // Clear of the status bar, by the history the notices tell of.
        let margin = crate::metrics::NOTICE_MARGIN;
        kit.notification.margins.bottom = gpui_kit::px((crate::metrics::STATUS_BAR_HEIGHT + margin) * scale);
        kit.notification.margins.right = gpui_kit::px(margin * scale);
    });
}

/// Has the component library draw in the colors `theme` gives the window.
pub fn apply(window: &Window, theme: Theme, cx: &mut App) {
    let mode = if is_dark(window, theme) { ThemeMode::Dark } else { ThemeMode::Light };
    gpui_kit::component::Theme::change(mode, None, cx);
    // Read again with the theme, so a change made in Windows meanwhile is taken up.
    let blink = caret_blink();
    if gpui_kit::component::Theme::global(cx).cursor_blink != blink {
        gpui_kit::component::Theme::update(cx, |theme| theme.cursor_blink = blink);
    }
}

/// The caret blinks as Windows has it blink in its own fields: at its rate, and only for so long after
/// the last key or move, after which it stays shown. Each blink draws the window afresh, so a field left
/// with the keys in it costs nothing once the blinking stops.
fn caret_blink() -> CursorBlink {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCaretBlinkTime, SPI_GETCARETTIMEOUT, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    let interval = unsafe { GetCaretBlinkTime() };
    let mut timeout = 0u32;
    let read =
        unsafe { SystemParametersInfoW(SPI_GETCARETTIMEOUT, 0, Some((&raw mut timeout).cast()), SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0)) };
    caret_blink_of(interval, read.ok().map(|()| timeout))
}

/// The blinking for Windows's blink time and caret timeout, both in milliseconds: a blink time of
/// `INFINITE` is not to blink at all, as accessibility may set; 0, it could not be read, and the default
/// blinks. A timeout not read, 0 or `INFINITE` lets it blink on.
fn caret_blink_of(interval: u32, timeout: Option<u32>) -> CursorBlink {
    const INFINITE: u32 = u32::MAX;
    let blink = match interval {
        INFINITE => return CursorBlink::steady(),
        0 => CursorBlink::default(),
        ms => CursorBlink::blinking(Duration::from_millis(ms.into())),
    };
    match timeout {
        Some(ms) if ms != 0 && ms != INFINITE => blink.with_idle_timeout(Duration::from_millis(ms.into())),
        _ => blink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::base::{ColorTokens, SemanticThemeTokens};

    #[test]
    fn blinks_the_caret_as_windows_has_it_and_stops_when_it_says() {
        let ms = |ms: u64| Duration::from_millis(ms);
        // As Windows 11 is set out of the box: blinking, and still after 5 seconds of nothing.
        let blink = caret_blink_of(530, Some(5000));
        assert_eq!(blink, CursorBlink::blinking(ms(530)).with_idle_timeout(ms(5000)));
        assert_eq!((blink.interval(), blink.idle_timeout()), (Some(ms(530)), Some(ms(5000))));
        // Set not to blink, for accessibility.
        assert!(caret_blink_of(u32::MAX, Some(5000)).is_steady());
        // A timeout not read, or none, lets it blink on.
        for timeout in [None, Some(0), Some(u32::MAX)] {
            assert_eq!(caret_blink_of(530, timeout).idle_timeout(), None, "{timeout:?}");
        }
        // A blink time not read blinks as the library does.
        assert_eq!(caret_blink_of(0, None), CursorBlink::default());
    }

    #[test]
    fn the_pointer_and_the_pointed_at_stand_off_what_they_lie_on() {
        for (palette, dark) in [(Palette::light(), false), (Palette::dark(), true)] {
            // Some 4% of lightness, short of what rounding takes off.
            let apart = |a: Hsla, b: Hsla| (a.l - b.l).abs() >= 0.039;
            assert!(apart(palette.raised_hover, palette.raised), "hover over a raised surface, dark: {dark}");
            assert!(apart(palette.highlight, palette.hover), "pointed-at against hovered, dark: {dark}");
            // The chosen one of a radio group's choices against the others and the one under the pointer.
            assert!(apart(palette.chosen, palette.raised), "chosen against a choice, dark: {dark}");
            assert!(apart(palette.chosen, palette.raised_hover), "chosen against a hovered choice, dark: {dark}");
            // Its border, a boundary that tells it apart, at 3:1 or more on both sides.
            for (side, ground) in [("inside", palette.chosen), ("outside", palette.raised)] {
                let ratio = contrast(palette.accent, ground);
                assert!(ratio >= 3., "chosen's border {side}: {ratio}, dark: {dark}");
            }
            // A button with its menu open, against the ground it stands on and a button under the pointer.
            for (what, other) in [
                ("the window's own color", palette.bg),
                ("a column's or field's", palette.surface),
                ("a button of Colet's under the pointer", palette.hover),
                ("a button of the library's under the pointer", palette.raised_hover),
            ] {
                assert!(apart(palette.pressed, other), "open against {what}, dark: {dark}");
            }
            // Short of the selection's fill, toward the ground.
            assert!(if dark { palette.highlight.l < palette.accent_fill.l } else { palette.highlight.l > palette.accent_fill.l });
        }
    }

    #[test]
    fn the_current_tab_outshines_the_strip_and_a_hovered_tab() {
        for (palette, dark) in [(Palette::light(), false), (Palette::dark(), true)] {
            // The strip is the darkest in either setting, so each step toward the current tab lightens.
            let toward_bg = |a: Hsla, b: Hsla| b.l - a.l;
            assert!(toward_bg(palette.tab_strip, palette.tab_hover) >= 0.029, "strip to hovered, dark: {dark}");
            assert!(toward_bg(palette.tab_hover, palette.bg) >= 0.019, "hovered to current, dark: {dark}");
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
                ("the chosen one of a radio group's choices", palette.text, palette.chosen),
                ("a tab not current", palette.text_muted, palette.tab_strip),
                ("a tab not current under the pointer", palette.text_muted, palette.tab_hover),
                ("the close button's glyph under the pointer", palette.on_close, palette.close_hover),
                ("the close button's glyph pressed", palette.on_close, palette.close_pressed),
                ("a filter button's name, its menu open", palette.text, palette.pressed),
            ] {
                let ratio = contrast(ink, ground);
                assert!(ratio >= 4.5, "{what}: {ratio}, dark: {dark}");
            }
        }
    }

    #[test]
    fn a_glyph_on_a_button_with_its_menu_open_reads_at_3_to_1_or_more() {
        for (palette, dark) in [(Palette::light(), false), (Palette::dark(), true)] {
            for (what, ink) in [
                ("the muted glyphs", palette.text_muted),
                ("the faint chevron", palette.text_faint),
                ("a column's own order, or a chosen filter", palette.accent),
            ] {
                let ratio = contrast(ink, palette.pressed);
                assert!(ratio >= 3., "{what}: {ratio}, dark: {dark}");
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
                (&colors.secondary_active, palette.pressed),
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
