//! Colet's colors by role, for the dark and light settings of Windows. The dark ones are the old app's;
//! the light ones keep the same roles, hues and contrasts on a light ground.
use gpui_kit::base::input::{InputEditorStyle, InputState};
use gpui_kit::{App, Entity, Hsla, Window, WindowAppearance, hsla};

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
    pub border: Hsla,
    pub border_strong: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_faint: Hsla,
    pub accent: Hsla,
    pub accent_fill: Hsla,
    pub selected_inactive: Hsla,
    /// Kept well below `accent_fill`, so a pointed-at file never reads as the selection.
    pub highlight: Hsla,
    pub danger: Hsla,
    /// Behind the text of a button that sends to the Recycle Bin.
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
            border: hsl(210., 10., 19.),
            border_strong: hsl(210., 9., 28.),
            text: hsl(210., 14., 90.),
            text_muted: hsl(210., 8., 62.),
            text_faint: hsl(210., 7., 56.),
            accent: hsl(178., 40., 62.),
            accent_fill: hsl(184., 42., 28.),
            selected_inactive: hsl(210., 10., 21.),
            highlight: hsl(184., 20., 16.),
            danger: hsl(0., 75., 70.),
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
            border: hsl(210., 14., 87.),
            border_strong: hsl(210., 12., 76.),
            text: hsl(210., 20., 13.),
            text_muted: hsl(210., 9., 38.),
            text_faint: hsl(210., 8., 45.),
            accent: hsl(184., 60., 30.),
            accent_fill: hsl(184., 50., 33.),
            selected_inactive: hsl(210., 14., 88.),
            highlight: hsl(184., 35., 92.),
            danger: hsl(0., 65., 45.),
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
        let style = InputEditorStyle { selection: self.selection, ..Default::default() };
        input.update(cx, |input, _| input.set_editor_style(style));
    }

    pub fn for_window(window: &Window) -> Self {
        // A debug build can show either without touching the Windows setting.
        #[cfg(debug_assertions)]
        if let Ok(theme) = std::env::var("COLET_THEME") {
            return if theme == "light" { Self::light() } else { Self::dark() };
        }
        match window.appearance() {
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Self::dark(),
            _ => Self::light(),
        }
    }
}
