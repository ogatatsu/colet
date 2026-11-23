//! What is handed to Windows: showing an item in Explorer, its properties, the shell's own menu, and
//! Send To; and the items carried out of the window, faded until they land.
use super::*;
use crate::words;

impl Colet {
    /// Shows the picked items, all in one folder, selected together in it; with nothing picked, the folder
    /// column's own folder, open on what it holds.
    pub fn reveal_in_explorer(&mut self, cx: &mut Context<Self>) {
        let (picked, folder, refused) = (self.picked_entries(), self.folder_here(), self.reasons(cx).explorer);
        self.take_menu();
        if refused.is_some() {
            return;
        }
        let backend = self.backend.clone();
        match folder {
            Some(folder) if picked.is_empty() => {
                let owner = self.owner;
                self.in_background(
                    move || backend.open(&folder, owner).map_err(|e| words::ops::cannot_open(words::windows_failure(e.code()))),
                    None,
                    cx,
                );
            }
            _ if !picked.is_empty() => {
                let paths: Vec<PathBuf> = picked.into_iter().map(|e| HomeKey::on_disk(&e.path).into_owned()).collect();
                self.in_background(move || backend.reveal(&paths).map_err(|e| words::windows_failure(e.code())), None, cx);
            }
            _ => {}
        }
    }

    /// Windows' properties of what is picked, several together in one dialog; with nothing picked, of the
    /// folder column's own folder.
    pub fn show_properties(&mut self, cx: &mut Context<Self>) {
        // What the menu was opened on, read before it closes: after, the selection would stand in.
        let (mut picked, refused) = (self.picked(), self.reasons(cx).properties);
        self.take_menu();
        if let Some(reason) = refused {
            self.refuse(words::properties(), reason, cx);
            return;
        }
        if picked.is_empty() {
            picked.extend(self.folder_here());
        }
        let paths: Vec<PathBuf> = picked.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        let (backend, owner) = (self.backend.clone(), self.owner);
        self.in_background(move || backend.properties(&paths, owner).map_err(|e| words::windows_failure(e.code())), None, cx);
    }

    /// The Windows (Explorer) menu, where this one was opened. Dismisses Colet's menu and returns its
    /// keys even when the column it belonged to has gone.
    pub fn show_shell_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Read while the menu is open: its column and paths are what the reasons are for.
        let refused = self.reasons(cx).shell;
        let Some(menu) = self.take_menu() else { return };
        let index = self.state.index_of(menu.column);
        if let Some(index) = index {
            self.state.set_active(index);
        }
        let to_column = !self.overlays.has_dialog() && self.overlays.menu_has_keys(window, cx);
        self.close_menus(to_column, window, cx);
        let Some(index) = index else { return };
        if let Some(reason) = refused {
            self.notify(reason, Tone::Info, cx);
            return;
        }
        let paths = match &menu.paths {
            Some(paths) => paths.clone(),
            None => self.state.columns[index].shown_selected().into_owned(),
        };
        let paths: Vec<PathBuf> = paths.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        let background = if paths.is_empty() { self.state.columns[index].path.clone() } else { PathBuf::new() };
        let (backend, owner, at) = (self.backend.clone(), self.owner, menu.screen);
        let task = cx.background_executor().spawn(async move { backend.shell_menu(paths, background, at, owner) });
        cx.spawn(async move |this, cx| {
            if let Err(error) = task.await {
                let said = match error.code() {
                    // The items are not all in one folder.
                    code if code == windows::Win32::Foundation::E_INVALIDARG => words::reasons::same_folder().to_string(),
                    // A command from the menu before is still running, as a dialog it opened.
                    code if code == colet_core::shell::MENU_BUSY => words::ops::menu_busy().to_string(),
                    code => words::windows_failure(code),
                };
                this.update(cx, |this, cx| this.notify(said, Tone::Error, cx)).ok();
            }
            // What it ran may have changed the folders.
            this.update(cx, |this, cx| this.refresh(cx)).ok();
        })
        .detach();
    }

    /// Sends the picked items to `target`, one of Windows' Send To, as Explorer does: copied, or with
    /// `shift` moved where it allows it.
    pub fn send_to(&mut self, target: PathBuf, shift: bool, cx: &mut Context<Self>) {
        let (picked, refused) = (self.picked(), self.reasons(cx).send_to);
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = refused {
            self.refuse(words::send_to(), reason, cx);
            return;
        }
        let paths: Vec<PathBuf> = picked.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        // What may be moved goes unwatched first, with the folders inside it, until the send returns.
        let carried = shift.then(|| self.carry(&paths, cx));
        self.operations.update(cx, |operations, cx| operations.send_to(target, paths, shift, carried, cx));
    }

    /// Lets go of the watches on the folders at or inside `paths`, which a send or a drag may move, until
    /// [`put_down`](Self::put_down) is told the number; what hears once they are let go of.
    pub(crate) fn carry(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) -> Carried {
        let id = self.watching.carry(paths);
        Carried { id, released: self.watch(cx) }
    }

    /// What [`carry`](Self::carry) let go of is watched again, by the refresh this makes, which lists what
    /// changed while it was not; folders moved away meanwhile are closed rather than watched.
    pub(crate) fn put_down(&mut self, carry: u64, cx: &mut Context<Self>) {
        self.watching.put_down(carry);
        self.refresh(cx);
    }
}
