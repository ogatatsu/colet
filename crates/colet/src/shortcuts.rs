//! The keys, listed in a dialog of their own: opened from the toolbar, or by Ctrl+/, which closes it again.
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    dialogs::{Dialog, DialogParts, label, push},
    metrics::scaled,
    theme::ActivePalette,
};

/// How wide the keys' side of the list is: room for the longest in either language, "Ctrl+Shift+↑↓ / Ctrl+Shift+クリック",
/// which its English, "Ctrl+Shift+↑↓ / Ctrl+Shift+click", is narrower than.
const KEYS_WIDTH: f32 = 240.;

impl Colet {
    /// The close button takes the keys, there being nothing else in the dialog to press.
    pub fn open_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        self.overlays.set_dialog(Dialog::Shortcuts);
        self.stop("close-shortcuts", cx).focus(window, cx);
        cx.notify();
    }

    /// Ctrl+/: opens the list, or closes it while it is open. Over another dialog or a menu it does
    /// nothing, as the window's other keys do.
    pub fn toggle_shortcuts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.overlays.dialog() {
            Some(Dialog::Shortcuts) => self.close_dialog(window, cx),
            Some(_) => {}
            None if self.overlays.has_menu() => {}
            None => self.open_shortcuts(window, cx),
        }
    }

    /// The keys, under a heading for each kind of work.
    pub(crate) fn shortcuts_dialog(&mut self, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        // One by one: an array of them all would lie whole on the stack, past what a debug build has.
        let content = div().flex().flex_col().gap_3().text_xs().children(words::shortcuts::shortcuts().iter().map(|&(heading, keys)| {
            div().flex().flex_col().gap_1().child(label(p, heading)).children(keys.iter().map(|&(key, what)| {
                div()
                    .flex()
                    .gap_3()
                    // As wide as the longest key, so none wraps.
                    .child(div().w(scaled(KEYS_WIDTH)).flex_none().text_color(p.text).child(key))
                    // Wrapped in the room left, rather than cut at the dialog's edge.
                    .child(div().flex_1().min_w_0().text_color(p.text_muted).child(what))
            }))
        }));
        let buttons = vec![(
            "close-shortcuts",
            push("close-shortcuts", words::close(), true, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
        )];
        (words::shortcuts::keys(), content, buttons)
    }
}
