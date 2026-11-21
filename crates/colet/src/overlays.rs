//! What stands over the window and takes the keys from it: the dialog, the one menu open at a time, the
//! history, and where the keys go back to as the last of them closes. Colet opens and closes them; this keeps each
//! one's parts in step, a dialog with what it listens to and a menu with the line it lights.
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::*;

use crate::{dialogs::Dialog, file_menu::Menu, menus::Popup};

pub struct Overlays {
    dialog: Option<Dialog>,
    /// What the open dialog listens to, its fields: let go of as another opens and as it closes.
    dialog_subscriptions: Vec<Subscription>,
    /// Takes the keys in a dialog with no field.
    dialog_focus: FocusHandle,
    /// The window's dialog is open for `dialog`.
    dialog_shown: bool,
    popup: Option<Popup>,
    /// The open menu's lines, drawn and picked by the component library, its focus, and the closing it
    /// tells of.
    menu: Option<(Entity<PopupMenu>, FocusHandle, Subscription)>,
    history_open: bool,
    /// Where the keys were as the menu, dialog or history open now was opened, the first of them when one
    /// opened another.
    keys_before: Option<WeakFocusHandle>,
    /// The last press on a button that opens a menu or the history found it open.
    pressed_open: bool,
}

impl Overlays {
    pub fn new(cx: &mut App) -> Self {
        Self {
            dialog: None,
            dialog_subscriptions: Vec::new(),
            dialog_focus: cx.focus_handle(),
            dialog_shown: false,
            popup: None,
            menu: None,
            history_open: false,
            keys_before: None,
            pressed_open: false,
        }
    }

    /// A dialog or a menu is open.
    pub fn is_open(&self) -> bool {
        self.dialog.is_some() || self.menu.is_some()
    }

    // ---- The dialog ----

    pub fn dialog(&self) -> Option<&Dialog> {
        self.dialog.as_ref()
    }

    pub fn dialog_mut(&mut self) -> Option<&mut Dialog> {
        self.dialog.as_mut()
    }

    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    /// Makes `dialog` the one open, in place of any before it.
    pub fn set_dialog(&mut self, dialog: Dialog) {
        self.dialog = Some(dialog);
    }

    /// Closes the dialog without letting go of what it listens to, giving back what it asked.
    pub fn take_dialog(&mut self) -> Option<Dialog> {
        self.dialog.take()
    }

    /// Keeps `subscription` while the dialog open now is.
    pub fn listen_in_dialog(&mut self, subscription: Subscription) {
        self.dialog_subscriptions.push(subscription);
    }

    /// Lets go of what the dialog before listened to, as another opens.
    pub fn forget_dialog_listeners(&mut self) {
        self.dialog_subscriptions.clear();
    }

    /// Closes the dialog and lets go of what it listened to.
    pub fn close_dialog(&mut self) {
        self.dialog = None;
        self.dialog_subscriptions.clear();
    }

    pub fn dialog_focus(&self) -> &FocusHandle {
        &self.dialog_focus
    }

    /// Whether the window's dialog is to open (`Some(true)`) or close (`Some(false)`) to follow the dialog;
    /// taken as done.
    pub fn take_dialog_change(&mut self) -> Option<bool> {
        let open = self.dialog.is_some();
        (open != self.dialog_shown).then(|| {
            self.dialog_shown = open;
            open
        })
    }

    // ---- The menu ----

    /// What the one menu open stands for.
    pub fn popup(&self) -> Option<&Popup> {
        self.popup.as_ref()
    }

    /// Makes `popup` the menu to show, in place of any before it.
    pub fn set_popup(&mut self, popup: Popup) {
        self.popup = Some(popup);
    }

    /// Lets go of what the menu stood for, its lines left to close on their own.
    pub fn forget_popup(&mut self) {
        self.popup = None;
    }

    /// Closes the file menu, giving back what it was opened on; any other menu stays.
    pub fn take_file_menu(&mut self) -> Option<Menu> {
        match self.popup.take() {
            Some(Popup::File(menu)) => Some(menu),
            other => {
                self.popup = other;
                None
            }
        }
    }

    /// The menu's lines are drawn: `menu`, with the keys in `focus`, telling of its closing through
    /// `dismissed`.
    pub fn show_menu(&mut self, menu: Entity<PopupMenu>, focus: FocusHandle, dismissed: Subscription) {
        self.menu = Some((menu, focus, dismissed));
    }

    /// A menu's lines are drawn.
    pub fn has_menu(&self) -> bool {
        self.menu.is_some()
    }

    pub fn menu(&self) -> Option<&Entity<PopupMenu>> {
        self.menu.as_ref().map(|(menu, _, _)| menu)
    }

    pub fn menu_focus(&self) -> Option<&FocusHandle> {
        self.menu.as_ref().map(|(_, focus, _)| focus)
    }

    /// Closes the menu, its lines and what it stood for, giving that back.
    pub fn close_menu(&mut self) -> Option<Popup> {
        self.menu = None;
        self.popup.take()
    }

    // ---- The history ----

    /// The history of the file operations is open; what it shows is its own (see
    /// [`HistoryPanel`](crate::history::HistoryPanel)).
    pub fn is_history_open(&self) -> bool {
        self.history_open
    }

    pub fn set_history_open(&mut self, open: bool) {
        self.history_open = open;
    }

    // ---- Where the keys go back to ----

    /// A menu, dialog or the history is open, holding the keys.
    pub fn holds_keys(&self) -> bool {
        self.is_open() || self.history_open
    }

    /// The open menu has the keys.
    pub fn menu_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.menu_focus().is_some_and(|focus| focus.contains_focused(window, cx))
    }

    /// A menu, dialog or the history opening: where the keys are now is where they go back to as it closes.
    /// One opened from another leaves it as the first found it.
    pub fn remember_keys(&mut self, window: &Window, cx: &App) {
        if !self.holds_keys() {
            self.keys_before = window.focused(cx).as_ref().map(FocusHandle::downgrade);
        }
    }

    /// Where the keys go back to, if it is still there.
    fn keys_before(&self) -> Option<FocusHandle> {
        self.keys_before.as_ref().and_then(WeakFocusHandle::upgrade)
    }

    /// A menu, dialog or the history holds the keys, and closing gives them back to `focus`.
    pub fn keys_go_back_to(&self, focus: &FocusHandle) -> bool {
        self.holds_keys() && self.keys_before().is_some_and(|before| before == *focus)
    }

    /// A menu, dialog or the history holds the keys, and closing gives them back to one of `focuses`, or,
    /// with where they were gone, to wherever the caller sends them then.
    pub fn keys_go_back_to_any(&self, focuses: &[&FocusHandle]) -> bool {
        self.holds_keys() && self.keys_before().is_none_or(|before| focuses.contains(&&before))
    }

    /// Where the keys go back to as the last of what is open closes, let go of.
    pub fn take_keys_before(&mut self) -> Option<FocusHandle> {
        self.keys_before.take().and_then(|before| before.upgrade())
    }

    // ---- A button that opens a menu or the history ----

    /// A press on such a button found what it opens `open`.
    pub fn note_press(&mut self, open: bool) {
        self.pressed_open = open;
    }

    /// Whether the last press found it open, let go of: a press is told to one click alone.
    pub fn take_pressed_open(&mut self) -> bool {
        std::mem::take(&mut self.pressed_open)
    }
}
