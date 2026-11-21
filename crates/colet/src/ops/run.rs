//! Running a file operation, one at a time, in the tab it started in: what the columns show meanwhile,
//! following what it moved, and what is said and picked once it ends. Deleting the items picked is one.
use super::reasons::{BUSY, NOT_AT_HOME};
use super::*;
use crate::words;

impl Colet {
    /// Delete sends to the Recycle Bin and Shift+Delete deletes outright, as in Explorer: whether to ask
    /// first, and what to do with an item the bin cannot take, is the shell's.
    pub fn delete_picked(&mut self, permanent: bool, cx: &mut Context<Self>) {
        let (picked, reasons) = (self.picked(), self.reasons(cx));
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        let kind = if permanent { Kind::Delete } else { Kind::Recycle };
        if let Some(reason) = reasons.delete {
            let hint = if reason != NOT_AT_HOME {
                ""
            } else if picked.iter().any(|p| HomeKey::of(p).is_search()) {
                words::reasons::UNSAVE_HINT
            } else {
                words::reasons::UNFAVORITE_HINT
            };
            self.notify(words::reasons::refused_with_hint(kind.title(), reason, hint), Tone::Info, cx);
            return;
        }
        self.run(Request { kind, paths: picked, destination: PathBuf::new(), name: String::new() }, cx);
    }

    /// What the file operations tell of on their own, as what they set off ends.
    pub(crate) fn on_operations_event(&mut self, event: &OperationsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            OperationsEvent::Notice(text, tone) => self.notify(text.clone(), *tone, cx),
            OperationsEvent::PasteReady(paste) => self.run_paste(paste.clone(), cx),
            OperationsEvent::SendTargetsChanged => {
                if self.menu().is_some() {
                    self.show_menu(window, cx);
                }
            }
            // What it moved has left its folder.
            OperationsEvent::Sent(Some(carried)) => self.put_down(*carried, cx),
            OperationsEvent::Sent(None) => self.refresh(cx),
        }
    }

    /// Runs one file operation and keeps it in the history; a second waits for none, it is refused.
    pub fn run(&mut self, request: Request, cx: &mut Context<Self>) {
        let tab = self.current_tab();
        self.run_in(request, tab, cx);
    }

    /// Runs `request` for the tab `tab`, which picks what it makes.
    pub(super) fn run_in(&mut self, request: Request, tab: crate::tabs::TabId, cx: &mut Context<Self>) {
        let Some(events) = self.operations.update(cx, |operations, cx| operations.begin(&request, tab, cx)) else {
            self.refuse(request.kind.title(), BUSY, cx);
            return;
        };
        let (backend, owner, asked) = (self.backend.clone(), self.owner, request.clone());
        cx.spawn(async move |this, cx| {
            // A paste reads the clipboard once, and both lets go of and pastes what it read: what was last
            // seen on it may be a moment old.
            let clipboard = if asked.kind == Kind::Paste {
                let backend = backend.clone();
                cx.background_executor().spawn(async move { backend.clipboard_files() }).await
            } else {
                None
            };
            // What leaves its place goes unwatched first, with the folders inside it.
            let Ok(released) = this.update(cx, |this, cx| {
                let operations = this.operations.read(cx);
                let leaving = match asked.kind {
                    Kind::Recycle | Kind::Delete | Kind::Rename => asked.paths.as_slice(),
                    Kind::Paste => match &clipboard {
                        Some(files) if files.cut => files.paths.as_slice(),
                        Some(_) => &[],
                        // Unread, as while another app holds it: the paste reads it again, and most likely
                        // fails as this did.
                        None if operations.clipboard.files.cut => operations.clipboard.files.paths.as_slice(),
                        None => &[],
                    },
                    Kind::NewFolder => &[],
                };
                this.loading.watching.released = leaving.iter().map(|p| path_key(p)).collect();
                this.watch(cx)
            }) else {
                return;
            };
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some(released) = released {
                        let _ = released.recv();
                    }
                    backend.operate(
                        asked,
                        clipboard,
                        owner,
                        Box::new(move |event| events.lock().unwrap_or_else(|e| e.into_inner()).push(event)),
                    )
                })
                .await;
            this.update(cx, |this, cx| this.finished(request, result, cx)).ok();
        })
        .detach();
    }

    /// Home's favorites and pins at `from` or inside it, whatever their case, move to `to`; a narrowed
    /// favorite's folder moves under its narrowing. Two that then name one row keep the first.
    pub(crate) fn follow_move(&mut self, from: &Path, to: &Path, cx: &mut Context<Self>) {
        let moved_path = |p: &Path| colet_core::paths::moved(p, from, to);
        let moved_key = |key: &PathBuf| {
            match HomeKey::of(key) {
                HomeKey::Folder { path, narrow } => moved_path(&path).map(|path| HomeKey::Folder { path: path.into(), narrow }.key()),
                HomeKey::Search { .. } => None,
            }
            .unwrap_or_else(|| key.clone())
        };
        let pinned: Vec<PathBuf> = self.stored.settings().pinned.iter().map(moved_key).collect();
        let pinned = (pinned != self.stored.settings().pinned).then_some(pinned);
        let favorites: Vec<Favorite> = self
            .stored
            .settings()
            .favorites
            .iter()
            .map(|f| Favorite { path: moved_path(&f.path).unwrap_or_else(|| f.path.clone()), ..f.clone() })
            .collect();
        let favorites = (favorites != self.stored.settings().favorites).then_some(favorites);
        if favorites.is_some() || pinned.is_some() {
            // Settling the settings keeps the first of any two that now name one row.
            self.update_settings(
                |s| {
                    if let Some(favorites) = favorites {
                        s.favorites = favorites;
                    }
                    if let Some(pinned) = pinned {
                        s.pinned = pinned;
                    }
                },
                cx,
            );
        }
    }

    pub(super) fn finished(&mut self, request: Request, result: Result<(), OperationError>, cx: &mut Context<Self>) {
        let Some(Running { id, tab, made, placed: pasted, .. }) = self.operations.update(cx, |operations, cx| operations.end(cx)) else {
            return;
        };
        // Watched again by the refresh below, which lists what changed while they were not.
        self.loading.watching.released.clear();
        let Request { kind, paths, destination, name } = request;
        let (cancelled, problem) = match &result {
            Ok(()) => (false, None),
            Err(OperationError::Cancelled) => (true, None),
            Err(error) => (false, Some(error.to_string())),
        };
        // Where the shell says it put it, under the name it got, which may not be the one asked; one that
        // says nothing of it is taken at the name asked, once its item is done.
        let told = kind == Kind::Rename && !pasted.is_empty();
        let renamed = (kind == Kind::Rename && result.is_ok())
            .then(|| {
                pasted
                    .first()
                    .cloned()
                    .or_else(|| made.as_ref().and(paths.first()).map(|p| child(parent_folder(p).unwrap_or(Path::new("")), &name)))
            })
            .flatten();
        if let Some(to) = &renamed {
            // Carry the selection over to the new name before the listing drops the old one, in the tab it
            // was renamed from; a tab closed since has no selection to carry.
            if let Some(state) = self.state_in(tab) {
                state.renamed(&paths[0], to);
            }
            // Home's favorites and pins at it or inside it follow it, as the refresh below lists home again.
            self.follow_move(&paths[0], to, cx);
        }
        if kind == Kind::NewFolder && result.is_ok() {
            // The shell says where it made the folder, which a name clash may have changed.
            if let Some(state) = self.state_in(tab) {
                state.created(&destination, &made.unwrap_or_else(|| child(&destination, &name)));
            }
        }
        // Told where it went, the item already says so.
        let renamed = renamed.filter(|_| !told);
        let problem =
            self.operations.update(cx, |operations, cx| operations.record_end(Ended { id, kind, cancelled, problem, renamed }, cx));
        if let Some(problem) = problem {
            self.notify_entry(words::ops::problem_of(kind.title(), problem), id, cx);
        }
        // Like Explorer, what was pasted is picked once listed, under the names it got.
        if kind == Kind::Paste
            && !pasted.is_empty()
            && let Some(state) = self.state_in(tab)
        {
            state.pasted(&destination, &pasted);
        }
        self.refresh(cx);
        self.changed(cx);
    }
}
