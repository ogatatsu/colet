//! The window's dialog: which one is open, its frame, and the parts every dialog is built of (a field, a
//! label, a button, a radio group). Each dialog's contents, title and buttons are drawn by the feature it
//! belongs to: a name by the file operations, a favorite or saved search by home, the search filters by
//! theirs, and the preferences by theirs.
use std::rc::Rc;

use colet_core::{entry::Order, operations::Request};
use gpui_kit::base::input::{Input, InputState};
use gpui_kit::component::{
    Disableable, Selectable, WindowExt,
    button::{Button, ButtonVariants},
    dialog::Dialog as DialogFrame,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::Colet,
    metrics::{self, Scale, scaled},
    search_filters::FilterEdit,
    settings::{Favorite, SavedSearch},
    stops::Stop,
    theme::Palette,
    view::{RING_ROOM, RingAt},
};

/// The dialog open over the window, with what it is editing.
pub enum Dialog {
    /// A rename's new name, or a new folder's: the request it sends, its name taken from `input`. `taken`: the
    /// name tried last that the rename's folder already has. `checking`: the name tried whose folder is being
    /// asked, off the UI thread, whether it has it already; typing lets go of it.
    Name { request: Request, input: Entity<InputState>, tried: bool, taken: Option<String>, checking: Option<String> },
    /// A saved search's name, query, filter, narrowing and order, found by those it had as the dialog opened.
    EditSearch {
        original: SavedSearch,
        name: Entity<InputState>,
        query: Entity<InputState>,
        filter: Option<String>,
        narrow: Entity<InputState>,
        order: Order,
    },
    /// A favorite's name and narrowing, found by those it had as the dialog opened; its folder stays.
    EditFavorite { original: Favorite, name: Entity<InputState>, narrow: Entity<InputState> },
    /// The search filters, to add to, put in order, edit or delete: one at a time, in `edit`, or the one
    /// asked about before it goes.
    Filters { edit: Option<FilterEdit>, deleting: Option<String> },
    /// The theme, the display, and the keys.
    Preferences,
    /// The files to open together, asked about first when they are many.
    Open { paths: Vec<std::path::PathBuf> },
}

impl Dialog {
    /// Its text fields, in their order.
    pub fn fields(&self) -> Vec<Entity<InputState>> {
        match self {
            Dialog::Name { input, .. } => vec![input.clone()],
            Dialog::EditSearch { name, query, narrow, .. } => vec![name.clone(), query.clone(), narrow.clone()],
            Dialog::EditFavorite { name, narrow, .. } => vec![name.clone(), narrow.clone()],
            Dialog::Filters { edit: Some(edit), .. } => vec![edit.name.clone(), edit.query.clone()],
            Dialog::Filters { .. } | Dialog::Preferences | Dialog::Open { .. } => Vec::new(),
        }
    }
}

/// How wide every dialog is: room for a filter's name and search side by side, and for the keys' list.
const DIALOG_WIDTH: f32 = 640.;

/// A field, ringed while it has the keys (`focused`), or its menu has them for it; `menu` opens that.
pub(crate) fn field(
    p: Palette,
    input: &Entity<InputState>,
    focused: bool,
    // The field's corners at 100%: the theme's `md`, from `metrics::Radii`.
    radius: f32,
    menu: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    // The ring, two pixels in the focus color a pixel off the field.
    crate::view::frame(p, focused, radius).child(
        div()
            .h(scaled(metrics::CONTROL_HEIGHT))
            .px_2p5()
            .flex()
            .items_center()
            .bg(p.surface)
            .border_1()
            .border_color(p.border_strong)
            .rounded(scaled(radius))
            // Its padding too, round the text.
            .capture_any_mouse_down(menu)
            .child(div().flex_1().min_w_0().child(Input::new(input))),
    )
}

/// A dialog's buttons, each with the name of the stop Tab reaches it by.
pub(crate) type Buttons = Vec<(&'static str, Button)>;

/// A dialog as its feature draws it: its title, what lies between the title and the buttons, and the
/// buttons.
pub(crate) type DialogParts = (&'static str, Div, Buttons);

/// The name over a field or a group of choices.
pub(crate) fn label(p: Palette, text: &'static str) -> Div {
    div().text_xs().text_color(p.text_muted).child(text)
}

/// A dialog's button: the primary one is what Enter does.
pub(crate) fn push(id: impl Into<ElementId>, text: impl Into<SharedString>, primary: bool, enabled: bool) -> Button {
    Button::new(id).label(text).when(primary, |b| b.primary()).disabled(!enabled)
}

/// What picking a choice of a [`Colet::radio_group`] does.
pub(crate) type Pick = Rc<dyn Fn(&mut Colet, &mut Window, &mut Context<Colet>)>;

/// A choice of a [`Colet::radio_group`].
pub(crate) struct Choice {
    id: ElementId,
    text: SharedString,
    stop: Stop,
    chosen: bool,
    pick: Pick,
}

impl Choice {
    pub(crate) fn new(id: impl Into<ElementId>, text: impl Into<SharedString>, stop: Stop, chosen: bool, pick: Pick) -> Self {
        Choice { id: id.into(), text: text.into(), stop, chosen, pick }
    }
}

/// How far an arrow moves among the choices of a radio group: back or on, whichever way they run.
fn arrow_step(keystroke: &Keystroke) -> Option<isize> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "left" | "up" => Some(-1),
        "right" | "down" => Some(1),
        _ => None,
    }
}

impl Colet {
    /// `choices` as a radio group named `label`: the chosen one drawn selected, as a pressed button is,
    /// not as the primary button, which is what Enter does in the dialog. Tab stops at the chosen one only,
    /// or the first while none is, and the arrows move among them, picking as they go, as in a radio group.
    pub(crate) fn radio_group(
        &self,
        id: &'static str,
        label: &'static str,
        choices: Vec<Choice>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let tab_at = choices.iter().position(|c| c.chosen).unwrap_or(0);
        let stops: Rc<[Stop]> = choices.iter().map(|c| c.stop.clone()).collect();
        let picks: Rc<[Pick]> = choices.iter().map(|c| c.pick.clone()).collect();
        let buttons: Vec<Div> = choices
            .into_iter()
            .enumerate()
            .map(|(k, Choice { id, text, stop, chosen, pick })| {
                let (ring_stop, in_group) = (stop.clone(), stops.clone());
                // Told to the button, not to the stop's handle: the button writes its own onto the handle it
                // is given as it is drawn.
                let button =
                    Button::new(id).label(text).selected(chosen).role(Role::RadioButton).toggled(chosen).tab_stop(k == tab_at).on_click(
                        cx.listener(move |this, _, window, cx| {
                            // The mouse leaves the keys where they are, but within the group they go with the
                            // pick, so the ring does not stay on the last.
                            if in_group.iter().any(|s| this.stop(s.clone(), cx).is_focused(window)) {
                                this.stop(stop.clone(), cx).focus(window, cx);
                            }
                            pick(this, window, cx);
                        }),
                    );
                self.ringed(button, ring_stop, RingAt::Round, window, cx)
            })
            .collect();
        div()
            .id(id)
            .flex()
            .flex_wrap()
            .gap_0p5()
            .role(Role::RadioGroup)
            .aria_label(label)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let Some(by) = arrow_step(&event.keystroke) else { return };
                let Some(at) = stops.iter().position(|s| this.stop(s.clone(), cx).is_focused(window)) else { return };
                let to = (at as isize + by).rem_euclid(stops.len() as isize) as usize;
                cx.stop_propagation();
                this.stop(stops[to].clone(), cx).focus(window, cx);
                (picks[to])(this, window, cx);
            }))
            // Ringed as the fields are; the rings' room makes up most of the gaps.
            .children(buttons)
    }
}

impl Colet {
    /// Lets go of what the dialog before listened to. Called first by whatever opens one, before it makes
    /// the fields the new dialog listens to, and before the keys go into it: where they are is where they go
    /// back to as it closes.
    pub(crate) fn begin_dialog(&mut self, window: &Window, cx: &App) {
        self.remember_keys(window, cx);
        self.overlays.forget_dialog_listeners();
    }

    /// The keys go back to where they were as the dialog opened, or the menu that opened it.
    pub fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlays.close_dialog();
        self.restore_keys(window, cx);
        cx.notify();
    }

    /// Opens the window's dialog as [`Overlays::dialog`](crate::overlays::Overlays::dialog) comes, and
    /// closes it as that goes; the keys stay where Colet put them, in a field of the dialog or back in the
    /// columns.
    pub fn sync_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = self.overlays.take_dialog_change() else { return };
        let focused = window.focused(cx);
        if open {
            let colet = cx.weak_entity();
            window.open_dialog(cx, move |dialog, window, cx| match colet.upgrade() {
                Some(colet) => colet.update(cx, |this, cx| this.dialog_frame(dialog, window, cx)),
                None => dialog,
            });
        } else {
            window.close_dialog(cx);
        }
        if let Some(focused) = focused {
            focused.focus(window, cx);
        }
    }

    /// The open dialog's title, contents and buttons, as the feature it belongs to draws them.
    fn dialog_parts(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<DialogParts> {
        // Each drawn by a function of its own: in a debug build one function drawing them all takes the room
        // of every one of them at once, past what the window's thread has.
        let parts = match self.overlays.dialog()? {
            Dialog::Name { request, input, checking, .. } => self.name_dialog(request, input, checking.is_some(), window, cx),
            Dialog::EditFavorite { original, name, narrow } => self.favorite_dialog(original, name, narrow, window, cx),
            Dialog::EditSearch { name, query, filter, narrow, order, .. } => {
                self.search_dialog(name, query, filter, narrow, *order, window, cx)
            }
            Dialog::Filters { edit: Some(edit), .. } => self.filter_edit_dialog(edit, window, cx),
            Dialog::Filters { deleting: Some(name), .. } => self.filter_delete_dialog(name, cx),
            Dialog::Filters { .. } => self.filters_dialog(window, cx),
            Dialog::Preferences => self.preferences_dialog(window, cx),
            Dialog::Open { paths } => {
                let count = paths.len();
                self.open_dialog(count, cx)
            }
        };
        Some(parts)
    }

    /// The dialog as the window draws it: the heading and the buttons stay, and what lies between scrolls
    /// when the window is too low. Esc steps back as [`Colet::escape_dialog`] does; the dialog closes only as
    /// [`Overlays::dialog`](crate::overlays::Overlays::dialog) goes.
    pub fn dialog_frame(&mut self, dialog: DialogFrame, window: &mut Window, cx: &mut Context<Self>) -> DialogFrame {
        let Some((title, content, buttons)) = self.dialog_parts(window, cx) else { return dialog };
        // Each ringed round as the fields are, Tab reaching it through a stop of its id; the rings lie over
        // the gaps between them.
        let buttons: Vec<Div> =
            buttons.into_iter().map(|(key, button)| self.ringed(button, key, RingAt::Round, window, cx).m(scaled(-RING_ROOM))).collect();
        let colet = cx.weak_entity();
        dialog
            .w(Scale::of(window).px(DIALOG_WIDTH))
            .close_button(false)
            .overlay_closable(false)
            .on_cancel(move |_, window, cx| {
                colet.update(cx, |this, cx| this.escape_dialog(window, cx)).ok();
                false
            })
            // Enter is the fields' and the buttons' own: it goes on to whichever has the keys.
            .on_ok(|_, _, cx| {
                cx.propagate();
                false
            })
            .title(div().text_base().font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .id("dialog-content")
                    .test_support()
                    .aria_label(title)
                    .track_focus(self.overlays.dialog_focus())
                    // The window's dialog lies outside Colet's own elements: the menu key in its fields is heard here.
                    .on_action(cx.listener(|this, _: &crate::app::OpenTextMenu, window, cx| this.open_focused_text_menu(window, cx)))
                    .child(content),
            )
            .footer(div().flex().items_center().justify_end().gap_2().children(buttons))
            // A field's menu, over the dialog it is in.
            .children(self.render_menu(cx))
    }
}
