//! The address bar: the trail of folders, which turns into a field to type or paste a path in, as
//! Explorer's does. Enter goes there in the tab, Alt+Enter in a new one; a folder opens with its contents
//! beside it, a file where it lives with it picked. Esc, or the keys going elsewhere, lets go of what was
//! typed and the trail comes back.
use std::path::PathBuf;

use colet_core::paths::{address, shown};
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::{
    app::{Colet, Tone},
    state::Kind,
};

/// What the empty address says before and while editing.
pub(crate) const PLACEHOLDER: &str = "パスを入力（Ctrl+L）";

/// The address bar's field, shown in place of the trail while open, and how many times anything has
/// happened to it: a path typed, the field opened or closed. An answer about a path is taken only while
/// nothing has happened since it was asked.
pub struct Address {
    pub field: Entity<InputState>,
    open: bool,
    asked: u64,
}

impl Address {
    pub(crate) fn new(field: Entity<InputState>) -> Self {
        Self { field, open: false, asked: 0 }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Opens or closes the field; an answer still on its way is let go of.
    fn set_open(&mut self, open: bool) {
        self.open = open;
        self.asked += 1;
    }

    /// What the field holds changed: an answer still on its way is no longer about it.
    fn typed(&mut self) {
        self.asked += 1;
    }

    /// Asks about what the field holds now: the number its answer is to come back with.
    fn ask(&mut self) -> u64 {
        self.asked += 1;
        self.asked
    }

    /// Nothing has happened to the field since `asked`.
    fn still_asked(&self, asked: u64) -> bool {
        asked == self.asked
    }
}

impl Colet {
    /// The field, and what it tells: Enter goes to what is typed.
    pub(crate) fn address_field(window: &mut Window, cx: &mut Context<Self>) -> (Entity<InputState>, Subscription) {
        let field = cx.new(|cx| InputState::new(window, cx).placeholder(PLACEHOLDER));
        let typed = cx.subscribe_in(&field, window, |this: &mut Self, _, event, window, cx| {
            match event {
                InputEvent::PressEnter { .. } => this.go_to_address(false, window, cx),
                // What was asked for is no longer what the field holds.
                InputEvent::Change => this.address.typed(),
                _ => {}
            }
        });
        (field, typed)
    }

    /// The deepest folder open, as the field starts with it; none while only home or the results are.
    fn deepest_folder(&self) -> Option<PathBuf> {
        self.state.columns.iter().rev().find(|c| c.kind == Kind::Folder).map(|c| c.path.clone())
    }

    /// Ctrl+L, or a press on the trail away from its crumbs: the field, holding the deepest folder's path
    /// all picked, to be typed over.
    pub fn open_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            return;
        }
        let text = self.deepest_folder().map(shown).unwrap_or_default();
        self.address.set_open(true);
        self.address.field.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
            input.select_all(window, cx);
        });
        cx.notify();
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
        self.address.set_open(true);
        self.address.field.update(cx, |input, cx| {
            input.set_value(text, window, cx);
            input.focus(window, cx);
            input.set_selected_range_with_direction(picked, reversed, cx);
        });
        cx.notify();
    }

    /// Lets go of what was typed; the trail shows again. Any answer still on its way is let go of too.
    pub fn close_address(&mut self, cx: &mut Context<Self>) {
        self.address.set_open(false);
        cx.notify();
    }

    /// Esc: the trail comes back and the keys go to the column worked in.
    pub fn leave_address(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_address(cx);
        self.focus_active(window, cx);
    }

    /// While the field is open, the keys going anywhere else close it, and an answer still on its way is
    /// let go of; its own menu keeps it, and so does a menu, dialog or the history that gives the keys back
    /// to it as it closes, with what was typed and picked there.
    pub(crate) fn follow_address(&mut self, window: &Window, cx: &App) {
        if self.address.is_open()
            && !self.has_keys(&self.address.field, window, cx)
            && !self.keys_go_back_to(&self.address.field.read(cx).focus_handle(cx))
        {
            self.address.set_open(false);
        }
    }

    /// Goes to the path typed, in this tab or, `in_new_tab`, a new one in front. Whether it is there, and
    /// how its names are written on disk, is asked off the UI thread, as a share can keep it waiting. What
    /// cannot be gone to is said, and the field stays as it is to be put right.
    pub fn go_to_address(&mut self, in_new_tab: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.address.field.read(cx).value().to_string();
        if text.trim().is_empty() {
            return;
        }
        let Some(path) = address(&text, |name| std::env::var(name).ok()) else {
            self.notify(
                format!("「{}」は開けません。ドライブ（C:\\…）かサーバー（\\\\server）から始まるパスを入力してください", text.trim()),
                Tone::Error,
                cx,
            );
            return;
        };
        let asked = self.address.ask();
        let backend = self.backend.clone();
        let found = {
            let path = path.clone();
            cx.background_executor().spawn(async move { backend.locate(&path) })
        };
        cx.spawn_in(window, async move |this, cx| {
            let found = found.await;
            this.update_in(cx, |this, window, cx| this.address_found(asked, path, found, in_new_tab, window, cx)).ok();
        })
        .detach();
    }

    /// The answer for the path last typed, unless the field was let go of or typed in anew since.
    fn address_found(
        &mut self,
        asked: u64,
        typed: PathBuf,
        found: Option<(PathBuf, bool)>,
        in_new_tab: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.address.still_asked(asked) {
            return;
        }
        let Some((path, directory)) = found else {
            self.notify(format!("「{}」が見つかりません", shown(&typed)), Tone::Error, cx);
            return;
        };
        self.close_address(cx);
        if in_new_tab {
            self.open_path_in_tab(&path, directory, window, cx);
            return;
        }
        self.state.go_to_path(&path, directory);
        self.sync_search_field(window, cx);
        self.focus_active(window, cx);
    }
}
