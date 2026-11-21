//! The file menu: what can be done to the items picked in a column, or to the folder itself from empty
//! space, opened by a right press or the menu key. Over home it offers what home does instead (see
//! [`Colet::home_lines`]). Each line says why it is off when it is.
use std::path::{Path, PathBuf};

use colet_core::shell::IconSource;
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    menus::{Line, Popup, choice, line},
    ops::{NEEDS_FILE, NEEDS_FOLDER, ONE_ITEM},
    state::ColumnId,
    state::Kind,
};

/// What a line of the file menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum FileCommand {
    Open,
    /// The row picked opens in a tab of its own, behind this one.
    OpenInTab(PathBuf),
    Explorer,
    Copy,
    Cut,
    Paste,
    CopyPath,
    Properties,
    Rename,
    NewFolder,
    Recycle,
    /// words::RECYCLE picked with Shift held, as Explorer's Delete deletes outright then.
    Delete,
    Favorite(PathBuf),
    /// What is picked, sent to the item of Windows' Send To at `target`: moved when `shift` is held as it
    /// is picked.
    SendTo {
        target: PathBuf,
        shift: bool,
    },
    Windows,
}

/// Where a menu opened by a right-click or the menu key stands, and what it acts on.
pub struct Menu {
    pub column: ColumnId,
    /// `None` for the column's selection; empty from its empty space, for the folder itself.
    pub paths: Option<Vec<PathBuf>>,
    /// In the window.
    pub at: Point<Pixels>,
    /// On the screen, for the Windows menu.
    pub screen: (i32, i32),
}

impl FileCommand {
    /// As picked with Shift held, or not: words::RECYCLE deletes outright, as Explorer's Delete does, and
    /// Send To moves, as Explorer's does.
    pub fn shifted(&self, shift: bool) -> Self {
        match self {
            FileCommand::Recycle if shift => FileCommand::Delete,
            FileCommand::SendTo { target, .. } => FileCommand::SendTo { target: target.clone(), shift },
            command => command.clone(),
        }
    }
}

/// The item of Windows' Send To for `target`: named after its path, not its place in the folder.
pub(crate) fn send_to_id(target: &Path) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new("send-to".into()), SharedString::from(target.to_string_lossy().into_owned()))
}

impl Colet {
    /// The file menu, while it is open.
    pub fn menu(&self) -> Option<&Menu> {
        match self.overlays.popup() {
            Some(Popup::File(menu)) => Some(menu),
            _ => None,
        }
    }

    /// Closes the file menu, giving back what it was opened on; any other menu stays.
    pub fn take_menu(&mut self) -> Option<Menu> {
        self.overlays.take_file_menu()
    }

    /// Opens the file menu on `paths` (`None` for the selection, empty for the folder itself) in the
    /// column at `index`, at `at` in the window. The keys stay where they were, as the selection does.
    pub fn open_menu(&mut self, index: usize, paths: Option<Vec<PathBuf>>, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlays.has_dialog() {
            return;
        }
        let Some(c) = self.state.columns.get(index) else { return };
        // Home has nothing to offer but on its rows.
        if c.kind == Kind::Home && paths.as_ref().map_or_else(|| c.shown_selected().is_empty(), Vec::is_empty) {
            return;
        }
        let column = c.id;
        self.state.set_active(index);
        self.column_focus(index).focus(window, cx);
        let scale = window.scale_factor();
        let mut point = windows::Win32::Foundation::POINT { x: (f32::from(at.x) * scale) as i32, y: (f32::from(at.y) * scale) as i32 };
        let _ =
            unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(windows::Win32::Foundation::HWND(self.owner as *mut _), &mut point) };
        self.overlays.set_popup(Popup::File(Menu { column, paths, at, screen: (point.x, point.y) }));
        self.show_menu(window, cx);
        // Read afresh for each menu, so one added since is there.
        self.operations.update(cx, |operations, cx| operations.load_send_targets(cx));
    }

    /// The file menu's lines, or home's over home.
    pub(crate) fn file_lines(&self, cx: &App) -> Vec<Line> {
        if self.menu().is_none() {
            return Vec::new();
        }
        let reasons = self.reasons(cx);
        let picked = self.picked();
        if self.menu().and_then(|m| self.state.index_of(m.column)).is_some_and(|i| self.state.columns[i].kind == Kind::Home) {
            return self.home_lines(&picked, &reasons);
        }
        let entries = self.picked_entries();
        let one = entries.len() == 1;
        // From empty space nothing is picked, and the actions on items need no word.
        let some = !picked.is_empty();
        let quiet = |reason: Option<&'static str>| if some { reason } else { reason.map(|_| "") };
        // Several open as files in their apps, the folders among them passed over.
        let open_off = match entries.len() {
            0 => quiet(Some(ONE_ITEM)),
            1 => None,
            _ => entries.iter().all(|e| e.directory).then_some(NEEDS_FILE),
        };
        // From empty space, the folder column's own folder, narrowed as it is.
        let in_tab = match (entries.as_slice(), self.folder_here().filter(|_| !some)) {
            ([entry], _) => line(
                "menu-open-tab",
                "app-window",
                words::OPEN_IN_NEW_TAB,
                Some("Ctrl+Enter"),
                None,
                FileCommand::OpenInTab(entry.path.clone()),
            ),
            (_, Some(folder)) => {
                line("menu-open-tab", "app-window", words::OPEN_IN_NEW_TAB, Some("Ctrl+Enter"), None, FileCommand::OpenInTab(folder))
            }
            _ => {
                let off = if some { ONE_ITEM } else { NEEDS_FOLDER };
                line(
                    "menu-open-tab",
                    "app-window",
                    words::OPEN_IN_NEW_TAB,
                    Some("Ctrl+Enter"),
                    Some(off),
                    FileCommand::OpenInTab(Default::default()),
                )
            }
        };
        // Each item of Windows' Send To, by the shell's icon for it.
        let targets = self.operations.read(cx).send_targets().iter().map(|target| {
            let command = FileCommand::SendTo { target: target.path.clone(), shift: false };
            Line {
                picture: Some(IconSource::Item(target.path.clone())),
                ..choice(send_to_id(&target.path), target.name.clone(), command, false)
            }
        });
        let send_to = Line {
            sub: Some(targets.collect()),
            ..line(
                "menu-send-to",
                "send",
                words::SEND_TO,
                None,
                quiet(reasons.send_to),
                FileCommand::SendTo { target: PathBuf::new(), shift: false },
            )
        }
        .grouped();
        let mut lines = vec![
            line("menu-open", "external-link", words::OPEN, Some("Enter"), open_off, FileCommand::Open),
            in_tab,
            line("menu-explorer", "folder-open", words::SHOW_IN_EXPLORER, None, reasons.explorer, FileCommand::Explorer),
            send_to,
            line("menu-copy", "copy", words::COPY, Some("Ctrl+C"), quiet(reasons.copy), FileCommand::Copy).grouped(),
            line("menu-cut", "scissors", words::CUT, Some("Ctrl+X"), quiet(reasons.cut), FileCommand::Cut),
            line("menu-paste", "clipboard-paste", words::PASTE, Some("Ctrl+V"), reasons.paste, FileCommand::Paste),
            line("menu-copy-path", "link", words::COPY_PATH, Some("Ctrl+Shift+C"), reasons.copy_path, FileCommand::CopyPath),
            line("menu-rename", "pencil-line", words::RENAME, Some("F2"), quiet(reasons.rename), FileCommand::Rename).grouped(),
            line("menu-new-folder", "folder-plus", words::NEW_FOLDER, Some("Ctrl+Shift+N"), reasons.paste, FileCommand::NewFolder),
            line("menu-recycle", "trash", words::RECYCLE, Some("Delete"), quiet(reasons.delete), FileCommand::Recycle),
        ];
        if one && entries[0].directory {
            let path = entries[0].path.clone();
            let favorite = self.stored.settings().is_favorite(&path);
            lines.push(
                line(
                    "menu-favorite",
                    if favorite { "star-fill" } else { "star" },
                    if favorite { words::REMOVE_FAVORITE } else { words::ADD_FAVORITE },
                    Some("Ctrl+D"),
                    None,
                    FileCommand::Favorite(path),
                )
                .grouped(),
            );
        }
        lines.push(
            line("menu-properties", "info", words::PROPERTIES, Some("Alt+Enter"), quiet(reasons.properties), FileCommand::Properties)
                .grouped(),
        );
        lines.push(line("menu-windows", "ellipsis", words::MORE_OPTIONS, None, reasons.shell, FileCommand::Windows));
        lines
    }

    /// Does what the line picked in the file menu says, in the column it was opened in; false when the
    /// menu is to stay as it is, as Windows' own menu opens in its place.
    pub(crate) fn run_file_command(&mut self, command: FileCommand, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match command {
            FileCommand::Open => self.open_picked(window, cx),
            FileCommand::OpenInTab(path) => {
                let index = self.state.active;
                self.open_in_tab(index, &path, cx);
            }
            FileCommand::Explorer => self.reveal_in_explorer(cx),
            FileCommand::Copy => self.copy(false, cx),
            FileCommand::Cut => self.copy(true, cx),
            FileCommand::Paste => self.paste(cx),
            FileCommand::CopyPath => self.copy_paths(cx),
            FileCommand::Properties => self.show_properties(cx),
            FileCommand::Rename => self.ask_rename(window, cx),
            FileCommand::NewFolder => self.ask_new_folder(window, cx),
            FileCommand::Recycle => self.delete_picked(false, cx),
            FileCommand::Delete => self.delete_picked(true, cx),
            FileCommand::Favorite(path) => self.toggle_favorite(&path, cx),
            FileCommand::SendTo { target, shift } => self.send_to(target, shift, cx),
            FileCommand::Windows => {
                self.show_shell_menu(window, cx);
                return false;
            }
        }
        true
    }
}
