//! The address bar: the trail of folders, which turns into a field to type or paste a path in, as
//! Explorer's does. Enter goes there in the tab, Alt+Enter in a new one; a folder opens with its contents
//! beside it, a file where it lives with it picked. Esc or ↓, or the keys going elsewhere, lets go of what
//! was typed and the trail comes back; ↓ picks a row in home standing alone, as from the search field.
//!
//! [`AddressBar`] owns the field, whether it is open, and the asking whether a typed path is there; it
//! tells what came of it through [`AddressEvent`]. Colet owns the rest: the trail drawn in its place,
//! what the field opens holding, where the keys went, and going to what was found.
use std::path::PathBuf;

use colet_core::paths::{address, shown};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::words;
use crate::{
    app::{Colet, Tone},
    backend,
    state::Kind,
};

mod view;

/// The address bar's field, shown in place of the trail while open, and how many times anything has
/// happened to it: a path typed, the field opened or closed. An answer about a path is taken only while
/// nothing has happened since it was asked.
pub struct AddressBar {
    field: Entity<InputState>,
    open: bool,
    asked: u64,
    backend: backend::Shared,
    _typed: Subscription,
}

/// What came of a path typed and gone to. Each is told once, after the bar has settled: closed for one
/// found, still open, as typed, for one to put right.
pub enum AddressEvent {
    /// `path` is there, as written on disk: to be gone to in this tab or, `in_new_tab`, a new one.
    Found { path: PathBuf, directory: bool, in_new_tab: bool },
    /// What was typed cannot be gone to, and why, to be said.
    Problem(String),
}

impl EventEmitter<AddressEvent> for AddressBar {}

impl AddressBar {
    /// Enter in the field goes to what is typed, in this tab.
    pub fn new(backend: backend::Shared, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(words::address::PLACEHOLDER));
        let typed = cx.subscribe_in(&field, window, |this: &mut Self, _, event, _, cx| match event {
            InputEvent::PressEnter { .. } => this.go(false, cx),
            // What was asked for is no longer what the field holds.
            InputEvent::Change => this.asked += 1,
            _ => {}
        });
        Self { field, open: false, asked: 0, backend, _typed: typed }
    }

    pub fn field(&self) -> &Entity<InputState> {
        &self.field
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens or closes the field; an answer still on its way is let go of.
    fn set_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.open = open;
        self.asked += 1;
        cx.notify();
    }

    /// Opens the field holding `text`, all picked to be typed over, with the keys.
    pub fn open(&mut self, text: String, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(true, cx);
        self.field.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
    }

    /// Opens the field holding `text` with `picked` picked, as it was left, with the keys.
    pub fn reopen(&mut self, text: String, picked: std::ops::Range<usize>, reversed: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.set_open(true, cx);
        self.field.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
            input.set_selected_range_with_direction(picked, reversed, cx);
        });
    }

    /// Lets go of what was typed; the trail shows again. Any answer still on its way is let go of too.
    pub fn close(&mut self, cx: &mut Context<Self>) {
        self.set_open(false, cx);
    }

    /// Goes to the path typed, in this tab or, `in_new_tab`, a new one. Whether it is there, and how its
    /// names are written on disk, is asked off the UI thread, as a share can keep it waiting.
    pub fn go(&mut self, in_new_tab: bool, cx: &mut Context<Self>) {
        let text = self.field.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let Some(path) = address(&text, |name| std::env::var(name).ok()) else {
            cx.emit(AddressEvent::Problem(words::address::not_a_path(text.trim())));
            return;
        };
        self.asked += 1;
        let asked = self.asked;
        let backend = self.backend.clone();
        let found = {
            let path = path.clone();
            cx.background_executor().spawn(async move { backend.locate(&path) })
        };
        cx.spawn(async move |this, cx| {
            let found = found.await;
            this.update(cx, |this, cx| this.found(asked, path, found, in_new_tab, cx)).ok();
        })
        .detach();
    }

    /// The answer for the path last typed, unless the field was let go of or typed in anew since. What
    /// is not there leaves the field as it is, to be put right.
    fn found(&mut self, asked: u64, typed: PathBuf, found: Option<(PathBuf, bool)>, in_new_tab: bool, cx: &mut Context<Self>) {
        if asked != self.asked {
            return;
        }
        let Some((path, directory)) = found else {
            cx.emit(AddressEvent::Problem(words::address::not_found(shown(&typed))));
            return;
        };
        self.close(cx);
        cx.emit(AddressEvent::Found { path, directory, in_new_tab });
    }
}

impl Colet {
    /// The deepest folder open, as the field starts with it; none while only home or the results are.
    fn deepest_folder(&self) -> Option<PathBuf> {
        self.state.columns.iter().rev().find(|c| c.kind == Kind::Folder).map(|c| c.path.clone())
    }

    /// Ctrl+L, or a press on the trail away from its crumbs: the field, holding the deepest folder's path
    /// all picked, to be typed over.
    pub fn open_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlays.has_dialog() {
            return;
        }
        let text = self.deepest_folder().map(shown).unwrap_or_default();
        self.address.update(cx, |address, cx| address.open(text, window, cx));
    }

    /// Back to a tab left while typing an address: the field, holding what was typed and picked there.
    pub(crate) fn reopen_address(
        &mut self,
        text: String,
        picked: std::ops::Range<usize>,
        reversed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.address.update(cx, |address, cx| address.reopen(text, picked, reversed, window, cx));
    }

    /// Esc: the trail comes back and the keys go back to the column worked in, picking nothing new.
    pub fn leave_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.address.update(cx, |address, cx| address.close(cx));
        self.come_back_to_active(window, cx);
    }

    /// ↓: as Esc, but home standing alone picks the row the keys land on, as ↓ from the search field does.
    pub fn leave_address_down(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.address.update(cx, |address, cx| address.close(cx));
        self.down_into_columns(window, cx);
    }

    /// Alt+Enter, or Enter: goes to the path typed, in a new tab or this one.
    pub fn go_to_address(&mut self, in_new_tab: bool, cx: &mut Context<Self>) {
        self.address.update(cx, |address, cx| address.go(in_new_tab, cx));
    }

    /// While the field is open, the keys going anywhere else close it, and an answer still on its way is
    /// let go of; its own menu keeps it, and so does a menu, dialog or the history that gives the keys back
    /// to it as it closes, with what was typed and picked there.
    pub(crate) fn follow_address(&mut self, window: &Window, cx: &mut Context<Self>) {
        let address = self.address.read(cx);
        let gone = address.is_open()
            && !self.has_keys(address.field(), window, cx)
            && !self.keys_go_back_to(&address.field().read(cx).focus_handle(cx));
        if gone {
            self.address.update(cx, |address, cx| address.close(cx));
        }
    }

    /// What the bar tells: a path found is gone to, and the keys go to the column worked in; what cannot
    /// be gone to is said.
    pub(crate) fn on_address_event(&mut self, event: &AddressEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            AddressEvent::Problem(problem) => self.notify(problem.clone(), Tone::Error, cx),
            AddressEvent::Found { path, directory, in_new_tab: true } => self.open_path_in_tab(path, *directory, window, cx),
            AddressEvent::Found { path, directory, in_new_tab: false } => {
                self.state.go_to_path(path, *directory);
                self.sync_search_field(window, cx);
                self.focus_active(window, cx);
            }
        }
    }
}
