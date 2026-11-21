//! Why each action on the items picked is off, if it is, as its button and its menu item say; and what
//! the actions act on: the column the menu opened in, or the one worked in, and the items picked there.
use super::*;
use crate::file_menu::Menu;
use crate::words;

/// Why an action is off, as its button and menu item say; `None` while it is on.
pub struct Reasons {
    pub copy: Option<&'static str>,
    pub cut: Option<&'static str>,
    pub paste: Option<&'static str>,
    pub rename: Option<&'static str>,
    pub delete: Option<&'static str>,
    pub explorer: Option<&'static str>,
    pub shell: Option<&'static str>,
    pub copy_path: Option<&'static str>,
    pub send_to: Option<&'static str>,
    pub properties: Option<&'static str>,
}

pub use crate::words::reasons::{
    BUSY, NEEDS_FILE, NEEDS_FOLDER, NEEDS_SELECTION, NO_COPY_DRIVE, NO_COPY_SAVED, NO_SEND_DRIVE, NO_SEND_TARGETS, NOT_AT_HOME,
    NOT_AT_SERVER, NOT_FOR_SAVED, ONE_ITEM, SAME_FOLDER,
};

impl Colet {
    /// A menu keeps the column it opened on even if another column takes its place in the chain.
    pub(super) fn action_column(&self) -> Option<&Column> {
        let index = match self.menu() {
            Some(menu) => self.state.index_of(menu.column)?,
            None => self.state.active,
        };
        self.state.columns.get(index)
    }

    /// What the file actions work on: the selection shown, or while the menu is open, what it was opened on.
    pub fn picked(&self) -> Vec<PathBuf> {
        self.picked_paths().into_owned()
    }

    /// [`picked`](Self::picked), lent rather than copied where it can be, for what is asked each frame.
    pub(super) fn picked_paths(&self) -> Cow<'_, [PathBuf]> {
        let Some(column) = self.action_column() else { return Cow::Borrowed(&[]) };
        match self.menu() {
            Some(Menu { paths: Some(paths), .. }) => Cow::Borrowed(paths),
            _ => column.shown_selected(),
        }
    }

    pub fn picked_entries(&self) -> Vec<Entry> {
        let Some(c) = self.action_column() else { return Vec::new() };
        let picked: HashSet<PathBuf> = self.picked().into_iter().collect();
        c.entries.iter().filter(|e| picked.contains(&e.path)).map(|e| Entry::clone(e)).collect()
    }

    /// The folder column worked in's own folder, or server.
    pub fn folder_here(&self) -> Option<PathBuf> {
        let c = self.action_column()?;
        (c.kind == ColumnKind::Folder).then(|| c.path.clone())
    }

    /// Where a paste or a new folder goes: the folder column worked in, unless it lists a server's shares,
    /// which are only looked at.
    pub fn destination(&self) -> Option<PathBuf> {
        self.folder_here().filter(|path| !is_server(path))
    }

    /// The column worked in lists a server's shares.
    pub(super) fn at_server(&self) -> bool {
        self.action_column().is_some_and(|c| c.kind == ColumnKind::Folder && is_server(&c.path))
    }

    /// Asked as the buttons are drawn, each frame: it walks the selection, which may be thousands, without
    /// making a string, and stops at the first answer.
    pub fn reasons(&self, cx: &App) -> Reasons {
        fn parent(p: &Path) -> &Path {
            parent_folder(p).unwrap_or(Path::new(""))
        }

        let operations = self.operations.read(cx);
        let picked = self.picked_paths();
        let at_home = self.action_column().is_none_or(|c| c.kind == ColumnKind::Home);
        let at_server = self.at_server();
        let search = picked.iter().any(|p| HomeKey::of(p).is_search());
        let none = picked.is_empty();
        let drive = picked.iter().any(|p| !p.as_os_str().is_empty() && parent_folder(&HomeKey::on_disk(p)).is_none());
        let apart = picked.first().map(|p| parent(p)).is_some_and(|first| picked.iter().any(|p| parent(p) != first));

        // What works on the items picked needs some, and not in home or a server's shares.
        let item = none.then_some(NEEDS_SELECTION).or(at_home.then_some(NOT_AT_HOME)).or(at_server.then_some(NOT_AT_SERVER));
        let busy = operations.is_running().then_some(BUSY);
        // What works on a place: with nothing picked, the folder column's own folder; else the items', all
        // in one folder.
        let place = if none { self.folder_here().is_none().then_some(NEEDS_FOLDER) } else { apart.then_some(SAME_FOLDER) };
        Reasons {
            copy: item.or(search.then_some(NO_COPY_SAVED)).or(drive.then_some(NO_COPY_DRIVE)),
            cut: item,
            paste: busy.or(at_server.then_some(NOT_AT_SERVER)).or_else(|| self.destination().is_none().then_some(NEEDS_FOLDER)),
            // One saved search picked alone can always be renamed.
            rename: if picked.len() == 1 && search { None } else { busy.or(item).or((picked.len() != 1).then_some(ONE_ITEM)) },
            delete: busy.or(item),
            explorer: place,
            shell: search.then_some(NOT_FOR_SAVED).or(at_server.then_some(NOT_AT_SERVER)).or(place),
            // With nothing picked, the folder column's own folder.
            copy_path: if none { place } else { None },
            // From several folders too, as a drag from them.
            send_to: item.or(drive.then_some(NO_SEND_DRIVE)).or(operations.send_targets().is_empty().then_some(NO_SEND_TARGETS)),
            // Only looked at: in home and while an operation runs too, and from several folders. With
            // nothing picked, the folder column's own folder, a server's not.
            properties: if none {
                at_server.then_some(NOT_AT_SERVER).or(self.folder_here().is_none().then_some(NEEDS_FOLDER))
            } else {
                search.then_some(NOT_FOR_SAVED)
            },
        }
    }

    /// Says why a shortcut did nothing, as the buttons and menu items say it.
    pub(super) fn refuse(&mut self, title: &str, reason: &str, cx: &mut Context<Self>) {
        self.notify(words::reasons::refused(title, reason), Tone::Info, cx);
    }
}
