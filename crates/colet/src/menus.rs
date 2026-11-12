//! The menus, the file menu, a text field's, a column's sort order's and the search filters', as lines that the
//! mouse and the keys pick alike: ↑↓ move, Enter picks, Esc closes, and the keys go back to where they were.
use std::path::PathBuf;

use colet_core::{
    entry::{Order, Sort, direction_label},
    shell::IconSource,
};
use gpui_kit::base::input::{self, InputState};
use gpui_kit::component::{
    ActiveTheme,
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::Colet,
    ops::{Menu, NEEDS_FILE, NEEDS_FOLDER, ONE_ITEM, Reasons},
    places::Dialog,
    state::{ColumnId, HomeKey, Kind},
    tabs::TabId,
    view::TipExt,
};

/// What a line of a menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Open,
    /// The row picked opens in a tab of its own, behind this one.
    OpenInTab(std::path::PathBuf),
    Explorer,
    Copy,
    Cut,
    Paste,
    CopyPath,
    Properties,
    Rename,
    NewFolder,
    Recycle,
    /// "ごみ箱へ移動" picked with Shift held, as Explorer's Delete deletes outright then.
    Delete,
    Favorite(std::path::PathBuf),
    /// What is picked, sent to the item of Windows' Send To at `target`: moved when `shift` is held as it
    /// is picked.
    SendTo {
        target: std::path::PathBuf,
        shift: bool,
    },
    /// A row of home, pinned or put back.
    Pin(std::path::PathBuf),
    Windows,
    /// A saved search, by its row's key.
    EditSearch(std::path::PathBuf),
    Unsave(std::path::PathBuf),
    /// A sort for the column.
    Sort(ColumnId, Sort),
    Direction(ColumnId, bool),
    /// Folders before files, or mixed in with them.
    FoldersFirst(ColumnId, bool),
    /// The column goes back to the default order.
    Unsort(ColumnId),
    /// The search filter to search within, or none.
    SearchFilter(Option<String>),
    EditFilters,
    /// An edit of a text field's text.
    Edit(Edit),
    CloseTab(TabId),
    /// Every tab but this one, or those right of it.
    CloseOtherTabs(TabId),
    CloseTabsRight(TabId),
    DuplicateTab(TabId),
    ReopenTab,
}

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

pub struct Line {
    pub id: ElementId,
    pub glyph: Option<&'static str>,
    /// The shell's icon, in place of a glyph.
    pub picture: Option<IconSource>,
    pub text: SharedString,
    /// The key that does the same.
    pub hint: Option<&'static str>,
    /// Why it is off; empty when no word is needed.
    pub off: Option<&'static str>,
    pub command: Command,
    /// A rule above it, starting a group.
    pub group: bool,
    /// The sort in use.
    pub chosen: bool,
    /// The lines of the menu it opens beside it, in place of doing `command`.
    pub sub: Option<Vec<Line>>,
}

fn line(
    id: &'static str,
    glyph: &'static str,
    text: &'static str,
    hint: Option<&'static str>,
    off: Option<&'static str>,
    command: Command,
) -> Line {
    Line { id: id.into(), glyph: Some(glyph), picture: None, text: text.into(), hint, off, command, group: false, chosen: false, sub: None }
}

/// A line with no glyph, off without a word unless `on`.
fn plain(id: &'static str, text: &'static str, hint: &'static str, on: bool, command: Command) -> Line {
    Line {
        id: id.into(),
        glyph: None,
        picture: None,
        text: text.into(),
        hint: Some(hint),
        off: (!on).then_some(""),
        command,
        group: false,
        chosen: false,
        sub: None,
    }
}

/// A line to choose, checked when it is the one in use.
fn choice(id: impl Into<ElementId>, text: impl Into<SharedString>, command: Command, chosen: bool) -> Line {
    Line { id: id.into(), glyph: None, picture: None, text: text.into(), hint: None, off: None, command, group: false, chosen, sub: None }
}

/// The item of Windows' Send To for `target`: named after its path, not its place in the folder.
pub(crate) fn send_to_id(target: &std::path::Path) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new("send-to".into()), SharedString::from(target.to_string_lossy().into_owned()))
}

impl Line {
    fn grouped(mut self) -> Self {
        self.group = true;
        self
    }
}

/// The one menu open at a time, if any.
pub enum Popup {
    /// The file menu, opened by a right-click or the menu key.
    File(Menu),
    /// A column's own order's, from its heading.
    ColumnSort(ColumnId),
    /// The search filters'; `from_search` when the search field had the keys as it opened, to have them
    /// back as it closes.
    Filters { from_search: bool },
    /// The folders folded off the trail.
    Crumbs,
    /// A text field's, opened by a right-click in it or the menu key.
    Text(TextMenu),
    /// A tab's, opened by a right-click on it.
    Tab { tab: TabId, at: Point<Pixels> },
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
        let Some(input) = self.text_fields().into_iter().find(|input| input.read(cx).focus_handle(cx).is_focused(window)) else {
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
    fn text_fields(&self) -> Vec<Entity<InputState>> {
        let mut fields = vec![self.search.clone(), self.address.field().clone()];
        fields.extend(self.narrow_fields.values().map(|field| field.input.clone()));
        fields.extend(self.overlays.dialog().into_iter().flat_map(Dialog::fields));
        fields
    }

    /// The column whose own order's menu is open, if one is.
    pub fn column_sort_open(&self) -> Option<ColumnId> {
        match self.overlays.popup() {
            Some(Popup::ColumnSort(id)) => Some(*id),
            _ => None,
        }
    }

    pub fn filter_open(&self) -> bool {
        matches!(self.overlays.popup(), Some(Popup::Filters { .. }))
    }

    pub fn crumbs_open(&self) -> bool {
        matches!(self.overlays.popup(), Some(Popup::Crumbs))
    }

    /// The lines of the open menu: a text field's, the sort order's, the search filters', or the file menu's.
    pub fn menu_lines(&self, cx: &App) -> Vec<Line> {
        if let Some(menu) = self.text_menu() {
            let edit = menu.editable;
            return vec![
                plain("text-undo", "元に戻す", "Ctrl+Z", edit, Command::Edit(Edit::Undo)),
                plain("text-cut", "切り取り", "Ctrl+X", edit && menu.copyable, Command::Edit(Edit::Cut)).grouped(),
                plain("text-copy", "コピー", "Ctrl+C", menu.copyable, Command::Edit(Edit::Copy)),
                plain("text-paste", "貼り付け", "Ctrl+V", edit && menu.paste, Command::Edit(Edit::Paste)),
                plain("text-delete", "削除", "Delete", edit && menu.selection, Command::Edit(Edit::Delete)),
                plain("text-select-all", "すべて選択", "Ctrl+A", !menu.empty, Command::Edit(Edit::SelectAll)).grouped(),
            ];
        }
        if let Some(column) = self.column_sort_open().and_then(|id| self.state.index_of(id)).map(|i| &self.state.columns[i]) {
            let target = column.id;
            let sort = self.order_of(column);
            // By location only the search results, whose items are in all sorts of folders.
            let search = column.kind == Kind::Search;
            let mut lines: Vec<Line> = Sort::ALL
                .into_iter()
                .filter(|by| search || by.for_folders())
                .map(|by| choice(by.label(), by.label(), Command::Sort(target, by), by == sort.by))
                .collect();
            let sorts = lines.len();
            lines.extend([false, true].map(|descending| {
                let label = direction_label(descending);
                choice(label, label, Command::Direction(target, descending), descending == sort.descending)
            }));
            lines[sorts].group = true;
            let first = sort.folders_first;
            lines.push(choice("folders-first", "フォルダを先に表示", Command::FoldersFirst(target, !first), first).grouped());
            // A column given its own order goes back to the default from here.
            let own = self.own_order(column).is_some();
            lines.push(Line { off: (!own).then_some(""), ..choice("unsort", "既定に戻す", Command::Unsort(target), false) }.grouped());
            return lines;
        }
        if self.filter_open() {
            let chosen = self.chosen_filter.as_deref();
            let mut lines = vec![choice("filter-none", "なし", Command::SearchFilter(None), chosen.is_none())];
            lines.extend(self.stored.settings().filters.iter().map(|f| {
                choice(
                    crate::dialogs::filter_tool_id("filter", &f.name),
                    f.name.clone(),
                    Command::SearchFilter(Some(f.name.clone())),
                    chosen == Some(f.name.as_str()),
                )
            }));
            lines.push(line("edit-filters", "pencil-line", "フィルターを編集…", None, None, Command::EditFilters).grouped());
            return lines;
        }
        if let Some(&Popup::Tab { tab, .. }) = self.overlays.popup() {
            let at = self.tabs.position(tab).unwrap_or_default();
            let count = self.tabs.len();
            let reopen = self.can_reopen_tab();
            return vec![
                line("tab-close", "x", "閉じる", Some("Ctrl+W"), None, Command::CloseTab(tab)),
                plain("tab-close-others", "他のタブを閉じる", "", count > 1, Command::CloseOtherTabs(tab)),
                plain("tab-close-right", "右のタブを閉じる", "", at + 1 < count, Command::CloseTabsRight(tab)),
                line("tab-duplicate", "copy", "複製", None, None, Command::DuplicateTab(tab)).grouped(),
                Line {
                    off: (!reopen).then_some(""),
                    ..line("tab-reopen", "rotate-ccw", "閉じたタブを開き直す", Some("Ctrl+Shift+T"), None, Command::ReopenTab)
                },
            ];
        }
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
            ([entry], _) => {
                line("menu-open-tab", "app-window", "新しいタブで開く", Some("Ctrl+Enter"), None, Command::OpenInTab(entry.path.clone()))
            }
            (_, Some(folder)) => {
                line("menu-open-tab", "app-window", "新しいタブで開く", Some("Ctrl+Enter"), None, Command::OpenInTab(folder))
            }
            _ => {
                let off = if some { ONE_ITEM } else { NEEDS_FOLDER };
                line(
                    "menu-open-tab",
                    "app-window",
                    "新しいタブで開く",
                    Some("Ctrl+Enter"),
                    Some(off),
                    Command::OpenInTab(Default::default()),
                )
            }
        };
        // Each item of Windows' Send To, by the shell's icon for it.
        let targets = self.operations.read(cx).send_targets().iter().map(|target| {
            let command = Command::SendTo { target: target.path.clone(), shift: false };
            Line {
                picture: Some(IconSource::Item(target.path.clone())),
                ..choice(send_to_id(&target.path), target.name.clone(), command, false)
            }
        });
        let send_to = Line {
            sub: Some(targets.collect()),
            ..line("menu-send-to", "send", "送る", None, quiet(reasons.send_to), Command::SendTo { target: PathBuf::new(), shift: false })
        }
        .grouped();
        let mut lines = vec![
            line("menu-open", "external-link", "開く", Some("Enter"), open_off, Command::Open),
            in_tab,
            line("menu-explorer", "folder-open", "Explorerで表示", None, reasons.explorer, Command::Explorer),
            send_to,
            line("menu-copy", "copy", "コピー", Some("Ctrl+C"), quiet(reasons.copy), Command::Copy).grouped(),
            line("menu-cut", "scissors", "切り取り", Some("Ctrl+X"), quiet(reasons.cut), Command::Cut),
            line("menu-paste", "clipboard-paste", "貼り付け", Some("Ctrl+V"), reasons.paste, Command::Paste),
            line("menu-copy-path", "link", "パスをコピー", Some("Ctrl+Shift+C"), reasons.copy_path, Command::CopyPath),
            line("menu-rename", "pencil-line", "名前を変更", Some("F2"), quiet(reasons.rename), Command::Rename).grouped(),
            line("menu-new-folder", "folder-plus", "新規フォルダー", Some("Ctrl+Shift+N"), reasons.paste, Command::NewFolder),
            line("menu-recycle", "trash", "ごみ箱へ移動", Some("Delete"), quiet(reasons.delete), Command::Recycle),
        ];
        if one && entries[0].directory {
            let path = entries[0].path.clone();
            let favorite = self.stored.settings().is_favorite(&path);
            lines.push(
                line(
                    "menu-favorite",
                    if favorite { "star-fill" } else { "star" },
                    if favorite { "お気に入りを解除" } else { "お気に入りに追加" },
                    Some("Ctrl+D"),
                    None,
                    Command::Favorite(path),
                )
                .grouped(),
            );
        }
        lines.push(
            line("menu-properties", "info", "プロパティ", Some("Alt+Enter"), quiet(reasons.properties), Command::Properties).grouped(),
        );
        lines.push(line("menu-windows", "ellipsis", "その他のオプションを確認", None, reasons.shell, Command::Windows));
        lines
    }

    /// The menu of rows of home: opening them, their paths, and keeping them there. What home never does,
    /// as copying, cutting, pasting or deleting, is not offered. Home picks one row at a time.
    fn home_lines(&self, picked: &[PathBuf], reasons: &Reasons) -> Vec<Line> {
        let [path] = picked else { return Vec::new() };
        let path = path.clone();
        // A row of home is pinned above the rest, or put back.
        let pin = if self.stored.settings().is_pinned(&path) {
            line("menu-unpin", "pin-off", "ピン留めを外す", None, None, Command::Pin(path.clone()))
        } else {
            line("menu-pin", "pin", "ピン留め", None, None, Command::Pin(path.clone()))
        };
        let in_tab = line("menu-open-tab", "app-window", "新しいタブで開く", Some("Ctrl+Enter"), None, Command::OpenInTab(path.clone()));
        if HomeKey::of(&path).is_search() {
            return vec![
                line("menu-open", "external-link", "開く", Some("Enter"), None, Command::Open),
                in_tab,
                line("menu-copy-query", "link", "検索文字列をコピー", Some("Ctrl+Shift+C"), None, Command::CopyPath).grouped(),
                line("menu-edit-search", "pencil-line", "編集", Some("F2"), None, Command::EditSearch(path.clone())).grouped(),
                pin,
                line("menu-unsave", "star", "保存した検索を解除", None, None, Command::Unsave(path)),
            ];
        }
        let mut lines = vec![
            line("menu-open", "external-link", "開く", Some("Enter"), None, Command::Open),
            in_tab,
            line("menu-explorer", "folder-open", "Explorerで表示", None, reasons.explorer, Command::Explorer),
            line("menu-copy-path", "link", "パスをコピー", Some("Ctrl+Shift+C"), reasons.copy_path, Command::CopyPath).grouped(),
        ];
        // F2 on a favorite edits it: a place has nothing to edit, and its folder is not renamed from here.
        let favorite = self.stored.settings().is_favorite(&path);
        if favorite {
            lines.push(line("menu-edit-favorite", "pencil-line", "編集", Some("F2"), None, Command::Rename));
        }
        lines.push(pin);
        lines.push(line(
            "menu-favorite",
            if favorite { "star-fill" } else { "star" },
            if favorite { "お気に入りを解除" } else { "お気に入りに追加" },
            Some("Ctrl+D"),
            None,
            Command::Favorite(path),
        ));
        lines[4].group = true;
        lines.push(line("menu-properties", "info", "プロパティ", Some("Alt+Enter"), reasons.properties, Command::Properties).grouped());
        lines.push(line("menu-windows", "ellipsis", "その他のオプションを確認", None, reasons.shell, Command::Windows));
        lines
    }

    pub fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        // Done by the field itself, with the keys back in it.
        if let Command::Edit(edit) = command {
            self.close_menus(true, window, cx);
            window.dispatch_action(edit.action(), cx);
            return;
        }
        // A tab's: the menu goes first, as it holds the tabs where they are.
        if let Some(tab) = match &command {
            Command::CloseTab(tab) | Command::CloseOtherTabs(tab) | Command::CloseTabsRight(tab) | Command::DuplicateTab(tab) => {
                Some(Some(*tab))
            }
            Command::ReopenTab => Some(None),
            _ => None,
        } {
            self.close_menus(true, window, cx);
            let index = tab.and_then(|tab| self.tabs.position(tab));
            match (command, index) {
                (Command::CloseTab(_), Some(index)) => self.close_tab(index, window, cx),
                (Command::CloseOtherTabs(_), Some(index)) => self.close_tabs_but(index, false, window, cx),
                (Command::CloseTabsRight(_), Some(index)) => self.close_tabs_but(index, true, window, cx),
                (Command::DuplicateTab(_), Some(index)) => self.duplicate_tab(index, window, cx),
                (Command::ReopenTab, _) => self.reopen_tab(window, cx),
                _ => {}
            }
            return;
        }
        if let Some(column) = self.menu().map(|menu| menu.column) {
            let Some(index) = self.state.index_of(column) else {
                self.close_menus(true, window, cx);
                return;
            };
            self.state.set_active(index);
        }
        match command {
            Command::Open => self.open_picked(window, cx),
            Command::OpenInTab(path) => {
                let index = self.state.active;
                self.open_in_tab(index, &path, cx);
            }
            Command::Explorer => self.reveal_in_explorer(cx),
            Command::Copy => self.copy(false, cx),
            Command::Cut => self.copy(true, cx),
            Command::Paste => self.paste(cx),
            Command::CopyPath => self.copy_paths(cx),
            Command::Properties => self.show_properties(cx),
            Command::Rename => self.ask_rename(window, cx),
            Command::NewFolder => self.ask_new_folder(window, cx),
            Command::Recycle => self.delete_picked(false, cx),
            Command::Delete => self.delete_picked(true, cx),
            Command::Favorite(path) => {
                self.toggle_favorite(&path, cx);
            }
            Command::SendTo { target, shift } => self.send_to(target, shift, cx),
            Command::Pin(path) => self.toggle_pin(&path, cx),
            Command::Windows => {
                self.show_shell_menu(window, cx);
                return;
            }
            Command::EditSearch(key) => self.open_edit(&key, window, cx),
            Command::Unsave(key) => {
                if let HomeKey::Search { query, filter, narrow } = HomeKey::of(&key) {
                    self.toggle_saved(query, filter.map(str::to_owned), narrow.map(str::to_owned), cx);
                }
            }
            Command::Sort(target, by) => self.set_sort(target, by, cx),
            Command::Direction(target, descending) => self.set_direction(target, descending, cx),
            Command::FoldersFirst(target, first) => self.set_folders_first(target, first, cx),
            Command::Unsort(id) => self.set_order(id, Order::default(), cx),
            Command::SearchFilter(name) => self.choose_filter(name, cx),
            Command::EditFilters => self.open_filters(window, cx),
            Command::Edit(_) => unreachable!("a field's edit is done above"),
            Command::CloseTab(_)
            | Command::CloseOtherTabs(_)
            | Command::CloseTabsRight(_)
            | Command::DuplicateTab(_)
            | Command::ReopenTab => {
                unreachable!("a tab's is done above")
            }
        }
        // What the line opened, a dialog, takes the keys; otherwise they go back to the column.
        let to_column = !self.overlays.has_dialog();
        self.close_menus(to_column, window, cx);
    }

    /// Closes whichever menu is open, handing the keys back when `restore`: to where they were as it opened,
    /// or, in a dialog, to the text field whose menu it was.
    pub fn close_menus(&mut self, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let popup = self.overlays.close_menu();
        if let Some(Popup::Text(menu)) = &popup {
            menu.input.update(cx, |input, cx| input.set_selection_focus(None, cx));
        }
        let within = self.overlays.has_dialog() || self.history_panel.open;
        match popup {
            Some(Popup::Text(menu)) if restore && within => menu.input.update(cx, |input, cx| input.focus(window, cx)),
            _ if restore && !within => self.restore_keys(window, cx),
            _ => {}
        }
        cx.notify();
    }

    /// Opens the menu of the folders folded off the trail, as it was last drawn.
    pub fn open_crumb_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlays.set_popup(Popup::Crumbs);
        let active = self.state.columns.get(self.state.active).map(|c| c.id);
        let colet = cx.weak_entity();
        let items = self
            .crumbs
            .folded
            .iter()
            .map(|crumb| {
                let (id, label, path) = (crumb.column, crumb.name.clone(), crumb.path.clone());
                let colet = colet.clone();
                PopupMenuItem::element(move |_, _| div().id(("crumb-item", id)).test_support().tip(path.clone()).child(label.clone()))
                    .icon(crate::view::glyph("folder"))
                    .checked(Some(id) == active)
                    .on_click(move |_, window, cx| {
                        colet
                            .update(cx, |this, cx| {
                                this.close_crumb_menu(false, window, cx);
                                this.go_to_column(id, window, cx);
                            })
                            .ok();
                    })
            })
            .collect();
        self.show_items(items, window, cx);
    }

    /// Closes the trail's menu; `to_button` hands the keys back to the button that opened it.
    pub fn close_crumb_menu(&mut self, to_button: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.crumbs_open() {
            self.overlays.close_menu();
        }
        if to_button {
            self.stop("crumb-fold", cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Opens the menu of the column `id`'s own order.
    pub fn open_column_sort(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        self.overlays.set_popup(Popup::ColumnSort(id));
        self.show_menu(window, cx);
    }

    /// Opens the search filters' menu.
    pub fn open_filter_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let from_search = match self.overlays.popup() {
            Some(&Popup::Filters { from_search }) => from_search,
            _ => self.search_has_keys(window, cx),
        };
        self.overlays.set_popup(Popup::Filters { from_search });
        self.show_menu(window, cx);
    }

    /// Builds the open menu's lines into the component library's menu and gives it the keys: ↑↓ move,
    /// Enter picks, Esc closes, and the keys go back to where they were.
    pub fn show_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.menu_items(self.menu_lines(cx), window, cx);
        self.show_items(items, window, cx);
    }

    /// `lines` as the component library's, a line with lines of its own opening them in a menu beside it.
    fn menu_items(&mut self, lines: Vec<Line>, window: &mut Window, cx: &mut Context<Self>) -> Vec<PopupMenuItem> {
        let colet = cx.weak_entity();
        let mut items = Vec::new();
        for line in lines {
            let Line { id, glyph, picture, text, hint, off, command, group, chosen, sub } = line;
            if group {
                items.push(PopupMenuItem::separator());
            }
            // Off, it is a line like any other, with the arrow all the same: the library opens a menu beside
            // a line pointed at, off or not.
            let chevron = sub.is_some();
            if let Some(sub) = sub.filter(|_| off.is_none()) {
                let sub = self.menu_items(sub, window, cx);
                let menu = PopupMenu::build(window, cx, |menu, _, _| sub.into_iter().fold(menu, |menu, item| menu.item(item)));
                let item = PopupMenuItem::submenu(text, menu).when_some(glyph, |item, glyph| item.icon(crate::view::glyph(glyph)));
                items.push(item.disabled(off.is_some()));
                continue;
            }
            // Asked for now, and drawn once it has come.
            if let Some(source) = &picture {
                self.icons.of(source.clone(), cx);
            }
            let (colet, drawn) = (colet.clone(), colet.clone());
            let item = PopupMenuItem::element(move |_, cx| {
                let picture = picture.as_ref().map(|source| match drawn.upgrade().and_then(|c| c.read(cx).icons.peek(source)) {
                    Some(image) => img(image).size(px(crate::metrics::ICON)).flex_none().into_any_element(),
                    None => div().size(px(crate::metrics::ICON)).flex_none().into_any_element(),
                });
                // Colet's own colors, as everything else it draws takes them; read as drawn, so a theme
                // changed while the menu is open shows.
                let muted = drawn.upgrade().map_or(cx.theme().muted_foreground, |c| c.read(cx).palette.text_muted);
                div()
                    .id(id.clone())
                    .test_support()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(div().flex_1().flex().items_center().gap_2().children(picture).child(text.clone()))
                    .when_some(hint, |d, key| d.child(div().text_xs().text_color(muted).child(key)))
                    .when(chevron, |d| d.child(svg().path("icons/chevron-right.svg").size(px(12.)).flex_none().text_color(muted)))
            })
            .when_some(glyph, |item, glyph| item.icon(crate::view::glyph(glyph)))
            .disabled(off.is_some())
            .checked(chosen)
            .on_click(move |event: &ClickEvent, window, cx| {
                let command = match &command {
                    // "ごみ箱へ移動" with Shift held deletes outright, as Explorer's Delete does.
                    Command::Recycle if event.modifiers().shift => Command::Delete,
                    // Sent with Shift held moves, as Explorer's Send To does.
                    Command::SendTo { target, .. } => Command::SendTo { target: target.clone(), shift: event.modifiers().shift },
                    command => command.clone(),
                };
                colet.update(cx, |this, cx| this.run_command(command, window, cx)).ok();
            });
            items.push(item);
        }
        items
    }

    /// Opens a menu of `items`, giving it the keys; it closes as [`Colet::close_menus`] does.
    pub fn show_items(&mut self, items: Vec<PopupMenuItem>, window: &mut Window, cx: &mut Context<Self>) {
        let menu = PopupMenu::build(window, cx, |menu, _, _| items.into_iter().fold(menu, |menu, item| menu.item(item)));
        // Closed by Esc or a press outside it; a line picked closes the menu itself first.
        let dismissed = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if this.overlays.has_menu() {
                this.close_menus(true, window, cx);
            }
        });
        let focus = menu.focus_handle(cx);
        self.remember_keys(window, cx);
        focus.focus(window, cx);
        self.overlays.show_menu(menu, focus, dismissed);
        cx.notify();
    }

    /// The open menu has the keys.
    pub fn menu_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.overlays.menu_focus().is_some_and(|focus| focus.contains_focused(window, cx))
    }

    /// The open menu, to lay out where it opened; Shift+F10 and the menu key close it, as they open it.
    pub fn menu_element(&self, id: &'static str, label: &'static str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let menu = self.overlays.menu()?;
        Some(
            div()
                .id(id)
                .aria_label(label)
                .key_context(crate::app::MENU)
                .on_action(cx.listener(|this, _: &crate::app::CloseMenu, window, cx| this.close_menus(true, window, cx)))
                .child(menu.clone())
                .into_any_element(),
        )
    }
}
