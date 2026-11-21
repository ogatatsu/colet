//! A text field's menu: undo, cut, copy, paste, delete and select all, as the field's own keys do them.
//! A right press in a field, or the menu key, opens it at the caret; the text picked stays lit while the
//! menu has the keys, and they go back to the field as it closes.
use gpui_kit::base::input::{self, InputState};
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    dialogs::Dialog,
    menus::{Line, Popup, plain},
};

/// What a text field's menu does to its text, as the field's own keys do.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edit {
    Undo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

impl Edit {
    fn action(self) -> Box<dyn Action> {
        match self {
            Edit::Undo => Box::new(input::Undo),
            Edit::Cut => Box::new(input::Cut),
            Edit::Copy => Box::new(input::Copy),
            Edit::Paste => Box::new(input::Paste),
            Edit::Delete => Box::new(input::Delete),
            Edit::SelectAll => Box::new(input::SelectAll),
        }
    }
}

/// A text field's menu: the field, where it opened, and what its text allowed as it did.
pub struct TextMenu {
    pub input: Entity<InputState>,
    pub at: Point<Pixels>,
    editable: bool,
    selection: bool,
    copyable: bool,
    /// The clipboard held text.
    paste: bool,
    empty: bool,
}

impl TextMenu {
    /// Its lines, each on as the text allowed as it opened.
    pub fn lines(&self) -> Vec<Line> {
        let edit = self.editable;
        vec![
            plain("text-undo", words::menu::UNDO, "Ctrl+Z", edit, Edit::Undo),
            plain("text-cut", words::CUT, "Ctrl+X", edit && self.copyable, Edit::Cut).grouped(),
            plain("text-copy", words::COPY, "Ctrl+C", self.copyable, Edit::Copy),
            plain("text-paste", words::PASTE, "Ctrl+V", edit && self.paste, Edit::Paste),
            plain("text-delete", words::DELETE, "Delete", edit && self.selection, Edit::Delete),
            plain("text-select-all", words::SELECT_ALL, "Ctrl+A", !self.empty, Edit::SelectAll).grouped(),
        ]
    }
}

impl Colet {
    /// The text field's menu, while it is open.
    pub fn text_menu(&self) -> Option<&TextMenu> {
        match self.overlays.popup() {
            Some(Popup::Text(menu)) => Some(menu),
            _ => None,
        }
    }

    /// The field has the keys, or its menu has them for it.
    pub fn has_keys(&self, input: &Entity<InputState>, window: &Window, cx: &App) -> bool {
        input.read(cx).focus_handle(cx).is_focused(window) || self.text_menu().is_some_and(|menu| menu.input == *input)
    }

    /// For the element round `input`, ahead of the field: a right press opens its menu with the keys in it,
    /// leaving what is picked and where the caret is, as Windows' own fields do. The field never sees the
    /// press, which would move its caret there. The menu key opens it through [`Colet::open_focused_text_menu`].
    pub fn right_press_menu(input: &Entity<InputState>, cx: &Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let field = input.downgrade();
        cx.listener(move |this, event: &MouseDownEvent, window, cx| {
            if event.button != MouseButton::Right {
                return;
            }
            let Some(input) = field.upgrade() else { return };
            cx.stop_propagation();
            input.update(cx, |input, cx| input.focus(window, cx));
            this.open_text_menu(input, event.position, window, cx);
        })
    }

    /// Opens `input`'s menu at `at`. The text picked stays lit while the menu has the keys.
    pub fn open_text_menu(&mut self, input: Entity<InputState>, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        let paste = cx.read_from_clipboard().and_then(|item| item.text()).is_some_and(|text| !text.is_empty());
        let menu = {
            let field = input.read(cx);
            let can = field.context_menu_capabilities();
            TextMenu {
                input: input.clone(),
                at,
                editable: can.is_editable(),
                selection: can.has_selection(),
                copyable: can.is_copyable(),
                paste,
                empty: field.value().is_empty(),
            }
        };
        self.overlays.set_popup(Popup::Text(menu));
        self.show_menu(window, cx);
        let focus = self.overlays.menu_focus().cloned();
        input.update(cx, |input, cx| input.set_selection_focus(focus, cx));
    }

    /// The menu key in a text field opens its menu under the caret.
    pub fn open_focused_text_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.text_fields(cx).into_iter().find(|input| input.read(cx).focus_handle(cx).is_focused(window)) else {
            return;
        };
        let at = {
            let field = input.read(cx);
            field.cursor_layout().map_or_else(|| field.input_bounds().bottom_left(), |(caret, _)| caret.bottom_left())
        };
        self.open_text_menu(input, at, window, cx);
    }

    /// The text fields there are: the search's, the address bar's, the columns' filters', and the open
    /// dialog's.
    fn text_fields(&self, cx: &App) -> Vec<Entity<InputState>> {
        let mut fields = vec![self.search_field(cx), self.address.read(cx).field().clone()];
        fields.extend(self.narrow_fields.values().map(|field| field.input.clone()));
        fields.extend(self.overlays.dialog().into_iter().flat_map(Dialog::fields));
        fields
    }

    /// Done by the field itself, with the keys back in it.
    pub(crate) fn run_edit(&mut self, edit: Edit, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(true, window, cx);
        window.dispatch_action(edit.action(), cx);
    }
}
