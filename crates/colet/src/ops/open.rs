//! Opening the items picked: a folder beside, a file in its app, and many files only once asked.
use super::*;
use crate::words;

/// The fewest files opened together that are asked about first, as Explorer stops offering to open them
/// past 15.
pub const ASK_TO_OPEN: usize = 16;

impl Colet {
    /// Opens the picked item: a folder beside it, a file in its app. Of several, the files each open in
    /// their apps and the folders are passed over.
    pub fn open_picked(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let index = self.action_column().and_then(|c| self.state.index_of(c.id));
        let picked = self.picked_entries();
        self.take_menu();
        match (index, picked.as_slice()) {
            (Some(index), [entry]) => self.open(index, &entry.path, window, cx),
            _ => {
                let files: Vec<PathBuf> = picked.into_iter().filter(|e| !e.directory).map(|e| e.path).collect();
                if files.len() >= ASK_TO_OPEN {
                    self.begin_dialog(window, cx);
                    self.overlays.set_dialog(Dialog::Open { paths: files });
                    self.stop("confirm-open", cx).focus(window, cx);
                    cx.notify();
                } else {
                    self.open_files(files, cx);
                }
            }
        }
    }

    /// Opens the files asked about.
    pub fn confirm_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Open { paths }) = self.overlays.take_dialog() else { return };
        self.close_dialog(window, cx);
        self.open_files(paths, cx);
    }

    /// Opens `paths` one after another in their apps, in the order given; one that fails leaves the rest to
    /// open, and those that failed are told of together.
    pub(super) fn open_files(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        if paths.is_empty() {
            return;
        }
        let (backend, owner) = (self.backend.clone(), self.owner);
        self.in_background(
            move || {
                let failed: Vec<(String, String)> = paths
                    .iter()
                    .filter_map(|path| {
                        backend.open(path, owner).err().map(|error| (shown(folder_name(path)), words::windows_failure(error.code())))
                    })
                    .collect();
                match failed.as_slice() {
                    [] => Ok(()),
                    [(name, error)] => Err(words::ops::cannot_open_named(name, error)),
                    [(_, error), ..] => {
                        let names: Vec<String> = failed.iter().take(3).map(|(name, _)| words::quoted(name)).collect();
                        let more = if failed.len() > 3 { words::more_items(failed.len() - 3) } else { String::new() };
                        Err(words::ops::cannot_open_many(failed.len(), names.join(""), more, error))
                    }
                }
            },
            None,
            cx,
        );
    }

    /// Whether to open `count` files together.
    pub(crate) fn open_dialog(&mut self, count: usize, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        let content = div()
            .flex()
            .flex_col()
            .gap_2()
            .text_sm()
            .child(words::ops::open_many(count))
            .child(div().text_xs().text_color(p.text_muted).child(words::ops::open_many_hint()));
        let buttons = vec![
            (
                "cancel",
                push("cancel", words::cancel(), false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            // It takes the keys as the dialog opens, so Enter goes on as it was pressed to open them; its
            // stop is the one opening the dialog sends them to.
            (
                "confirm-open",
                push("confirm-open", words::open(), true, true).on_click(cx.listener(|this, _, window, cx| this.confirm_open(window, cx))),
            ),
        ];
        (words::open(), content, buttons)
    }
}
