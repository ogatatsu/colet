//! The preferences: the language, the theme, the display size, and which hidden items show, in a dialog of
//! their own. Each is taken at once as it is picked, and kept. The keys are listed in a dialog of theirs
//! (`shortcuts.rs`).
use std::rc::Rc;

use gpui_kit::component::{Disableable, checkbox::Checkbox};
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    dialogs::{Choice, Dialog, DialogParts, Pick, label, push},
    settings::{Language, Theme, ZOOMS},
    stops::Stop,
    theme::ActivePalette,
};

impl Colet {
    pub fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        // The language now chosen, the first group, takes the keys, as a radio group's checked one does.
        self.stop(Stop::Language(self.stored.settings().language), cx).focus(window, cx);
        self.overlays.set_dialog(Dialog::Preferences);
        cx.notify();
    }

    /// Speaks `language` at once, and keeps it. A notice already up stays in the words it was given in.
    pub fn set_language(&mut self, language: Language, window: &mut Window, cx: &mut Context<Self>) {
        self.update_settings(|s| s.language = language, cx);
        self.speak(window, cx);
    }

    /// Speaks the language kept, in the fields' placeholders too: the rest is said afresh as it is drawn.
    fn speak(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        words::speak(self.stored.settings().language.spoken(self.backend.windows_speaks()));
        let search = self.search_field(cx);
        search.update(cx, |field, cx| field.set_placeholder(words::search::placeholder(), window, cx));
        let address = self.address.read(cx).field().clone();
        address.update(cx, |field, cx| field.set_placeholder(words::address::placeholder(), window, cx));
        for field in self.narrow_fields.values().map(|field| field.input.clone()).collect::<Vec<_>>() {
            field.update(cx, |field, cx| field.set_placeholder(words::column::narrow_placeholder(), window, cx));
        }
        cx.notify();
    }

    /// Takes `theme`'s colors at once, and keeps it.
    pub fn set_theme(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        self.update_settings(|s| s.theme = theme, cx);
        self.apply_theme(window, cx);
    }

    /// Draws everything at `zoom` percent at once, and keeps it. The columns are brought to rest again
    /// at their new widths as the window is next laid out.
    pub fn set_zoom(&mut self, zoom: u16, cx: &mut Context<Self>) {
        self.update_settings(|s| s.zoom = zoom, cx);
        crate::theme::set_zoom(zoom, cx);
        cx.notify();
    }

    /// The language, the theme, the display, and what shows.
    pub(crate) fn preferences_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        let languages = Language::ALL
            .into_iter()
            .map(|language| {
                let pick: Pick = Rc::new(move |this, window, cx| this.set_language(language, window, cx));
                let chosen = self.stored.settings().language == language;
                Choice::new(("language", language as usize), language.label(), Stop::Language(language), chosen, pick)
            })
            .collect();
        let languages = self.radio_group("languages", words::language(), languages, window, cx);
        let themes = Theme::ALL
            .into_iter()
            .map(|theme| {
                let pick: Pick = Rc::new(move |this, window, cx| this.set_theme(theme, window, cx));
                let chosen = self.stored.settings().theme == theme;
                Choice::new(("theme", theme as usize), theme.label(), Stop::Theme(theme), chosen, pick)
            })
            .collect();
        let themes = self.radio_group("themes", words::theme(), themes, window, cx);
        let zooms = ZOOMS
            .into_iter()
            .map(|zoom| {
                let pick: Pick = Rc::new(move |this, _, cx| this.set_zoom(zoom, cx));
                let chosen = self.stored.settings().zoom == zoom;
                Choice::new(("zoom", zoom as usize), format!("{zoom}%"), Stop::Zoom(zoom), chosen, pick)
            })
            .collect();
        let zooms = self.radio_group("zooms", words::display_size(), zooms, window, cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            // Taken at once as one is picked, and kept.
            .child(div().flex().flex_col().gap_1().child(label(p, words::language())).child(languages))
            .child(div().flex().flex_col().gap_1().child(label(p, words::theme())).child(themes))
            .child(div().flex().flex_col().gap_1().child(label(p, words::display_size())).child(zooms))
            // Taken at once, as the theme is. The protected items hang off the hidden ones: they count only
            // while those show, so they are set only then, and keep what was chosen meanwhile.
            .child({
                let (hidden, protected) = (cx.entity().downgrade(), cx.entity().downgrade());
                let settings = self.stored.settings();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, words::preferences::show()))
                    .child(Checkbox::new("show-hidden").label(words::preferences::show_hidden()).checked(settings.show_hidden).on_click(
                        move |_, _, cx| {
                            hidden.update(cx, |this, cx| this.toggle_hidden(cx)).ok();
                        },
                    ))
                    .child(
                        div().pl_6().child(
                            Checkbox::new("show-protected")
                                .label(words::preferences::show_protected())
                                .checked(settings.show_protected)
                                .disabled(!settings.show_hidden)
                                .on_click(move |_, _, cx| {
                                    protected.update(cx, |this, cx| this.toggle_protected(cx)).ok();
                                }),
                        ),
                    )
            });
        let buttons = vec![(
            "close-preferences",
            push("close-preferences", words::close(), true, true)
                .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
        )];
        (words::settings(), content, buttons)
    }
}
