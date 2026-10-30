//! The menus, the file menu, a text field's, the sort order's and the search filters', as lines that the
//! mouse and the keys pick alike: ↑↓ move, Enter picks, Esc closes, and the keys go back to where they were.
use colet_core::entry::{Sort, direction_label};
use gpui_kit::base::input::{self, InputState};
use gpui_kit::component::{ActiveTheme, menu::{PopupMenu, PopupMenuItem}};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::Colet,
    ops::{Menu, NEEDS_FILE, ONE_ITEM},
    places::Dialog,
    search_filters::FilterEdit,
    state::{ColumnId, Kind, saved_query},
    view::TipExt,
};

/// What a line of a menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Open,
    Explorer,
    Copy,
    Cut,
    Paste,
    CopyPath,
    Rename,
    NewFolder,
    Recycle,
    /// "ごみ箱へ移動" picked with Shift held, as Explorer's Delete deletes outright then.
    Delete,
    Favorite(std::path::PathBuf),
    /// A row of home, pinned or put back.
    Pin(std::path::PathBuf),
    Windows,
    /// A saved search, by its query and filter.
    EditSearch(String, Option<String>),
    Unsave(String, Option<String>),
    /// A sort for the column, or for the default when `None`.
    Sort(Option<ColumnId>, Sort),
    Direction(Option<ColumnId>, bool),
    /// The column goes back to the default order.
    Unsort(ColumnId),
    /// The search filter to search within, or none.
    SearchFilter(Option<String>),
    EditFilters,
    /// An edit of a text field's text.
    Edit(Edit),
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
}

fn line(id: &'static str, glyph: &'static str, text: &'static str, hint: Option<&'static str>, off: Option<&'static str>, command: Command) -> Line {
    Line { id: id.into(), glyph: Some(glyph), text: text.into(), hint, off, command, group: false, chosen: false }
}

/// A line with no glyph, off without a word unless `on`.
fn plain(id: &'static str, text: &'static str, hint: &'static str, on: bool, command: Command) -> Line {
    Line { id: id.into(), glyph: None, text: text.into(), hint: Some(hint), off: (!on).then_some(""), command, group: false, chosen: false }
}

/// A line to choose, checked when it is the one in use.
fn choice(id: impl Into<ElementId>, text: impl Into<SharedString>, command: Command, chosen: bool) -> Line {
    Line { id: id.into(), glyph: None, text: text.into(), hint: None, off: None, command, group: false, chosen }
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
    /// The default order's, from the toolbar.
    Sort,
    /// A column's own order's, from its heading.
    ColumnSort(ColumnId),
    /// The search filters'; `from_search` when the search field had the keys as it opened, to have them
    /// back as it closes.
    Filters { from_search: bool },
    /// The folders folded off the trail.
    Crumbs,
    /// A text field's, opened by a right-click in it or the menu key.
    Text(TextMenu),
}

impl Colet {
    /// The file menu, while it is open.
    pub fn menu(&self) -> Option<&Menu> {
        match &self.popup {
            Some(Popup::File(menu)) => Some(menu),
            _ => None,
        }
    }

    /// Closes the file menu, giving back what it was opened on; any other menu stays.
    pub fn take_menu(&mut self) -> Option<Menu> {
        match self.popup.take() {
            Some(Popup::File(menu)) => Some(menu),
            other => {
                self.popup = other;
                None
            }
        }
    }

    /// The text field's menu, while it is open.
    pub fn text_menu(&self) -> Option<&TextMenu> {
        match &self.popup {
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
        self.popup = Some(Popup::Text(menu));
        self.show_menu(window, cx);
        let focus = self.menu_view.as_ref().map(|(_, focus, _)| focus.clone());
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

    /// The text fields there are: the search's, the columns' filters', and the open dialog's.
    fn text_fields(&self) -> Vec<Entity<InputState>> {
        let mut fields = vec![self.search.clone()];
        fields.extend(self.filters.values().map(|field| field.input.clone()));
        match &self.dialog {
            Some(Dialog::Name { input, .. }) => fields.push(input.clone()),
            Some(Dialog::EditSearch { name, query, .. } | Dialog::Filters { edit: Some(FilterEdit { name, query, .. }), .. }) => {
                fields.extend([name.clone(), query.clone()])
            }
            _ => {}
        }
        fields
    }

    pub fn sort_open(&self) -> bool {
        matches!(self.popup, Some(Popup::Sort))
    }

    /// The column whose own order's menu is open, if one is.
    pub fn column_sort_open(&self) -> Option<ColumnId> {
        match self.popup {
            Some(Popup::ColumnSort(id)) => Some(id),
            _ => None,
        }
    }

    pub fn filter_open(&self) -> bool {
        matches!(self.popup, Some(Popup::Filters { .. }))
    }

    pub fn crumbs_open(&self) -> bool {
        matches!(self.popup, Some(Popup::Crumbs))
    }

    /// The lines of the open menu: a text field's, the sort order's, the search filters', or the file menu's.
    pub fn menu_lines(&self) -> Vec<Line> {
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
        let column = self.column_sort_open().and_then(|id| self.state.index_of(id)).map(|i| &self.state.columns[i]);
        if self.sort_open() || column.is_some() {
            let target = column.map(|c| c.id);
            let sort = column.map_or(self.settings.sort, |c| self.order_of(c));
            let mut lines: Vec<Line> =
                Sort::ALL.map(|by| choice(by.label(), by.label(), Command::Sort(target, by), by == sort.by)).into();
            lines.extend([false, true].map(|descending| {
                let label = direction_label(descending);
                choice(label, label, Command::Direction(target, descending), descending == sort.descending)
            }));
            lines[Sort::ALL.len()].group = true;
            // A column given its own order goes back to the default from here.
            if let Some(c) = column {
                let own = self.own_order(c).is_some();
                lines.push(Line { off: (!own).then_some(""), ..choice("unsort", "既定に戻す", Command::Unsort(c.id), false) }.grouped());
            }
            return lines;
        }
        if self.filter_open() {
            let chosen = self.settings.filter.as_deref();
            let mut lines = vec![choice("filter-none", "なし", Command::SearchFilter(None), chosen.is_none())];
            lines.extend(self.settings.filters.iter().enumerate().map(|(i, f)| {
                choice(("filter", i), f.name.clone(), Command::SearchFilter(Some(f.name.clone())), chosen == Some(f.name.as_str()))
            }));
            lines.push(line("edit-filters", "pencil-line", "フィルターを編集…", None, None, Command::EditFilters).grouped());
            return lines;
        }
        if self.menu().is_none() {
            return Vec::new();
        }
        let reasons = self.reasons();
        let picked = self.picked();
        let entries = self.picked_entries();
        // A row of home is pinned above the rest, or put back, one at a time.
        let in_home = self.menu().and_then(|m| self.state.index_of(m.column)).is_some_and(|i| self.state.columns[i].kind == Kind::Home);
        let pin = match picked.as_slice() {
            [path] if in_home => Some(if self.settings.is_pinned(path) {
                line("menu-unpin", "pin-off", "ピン留めを外す", None, None, Command::Pin(path.clone()))
            } else {
                line("menu-pin", "pin", "ピン留め", None, None, Command::Pin(path.clone()))
            }),
            _ => None,
        };
        if let [path] = picked.as_slice()
            && let Some((query, filter)) = saved_query(path)
        {
            let filter = filter.map(str::to_owned);
            let mut lines = vec![
                line("menu-copy-query", "link", "検索文字列をコピー", Some("Ctrl+Shift+C"), None, Command::CopyPath),
                line("menu-edit-search", "pencil-line", "編集", Some("F2"), None, Command::EditSearch(query.into(), filter.clone())).grouped(),
            ];
            lines.extend(pin);
            lines.push(line("menu-unsave", "star", "保存した検索を解除", None, None, Command::Unsave(query.into(), filter)));
            return lines;
        }
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
        let mut lines = vec![
            line("menu-open", "external-link", "開く", Some("Enter"), open_off, Command::Open),
            line("menu-explorer", "folder-open", "Explorerで表示", None, reasons.explorer, Command::Explorer),
            line("menu-copy", "copy", "コピー", Some("Ctrl+C"), quiet(reasons.copy), Command::Copy).grouped(),
            line("menu-cut", "scissors", "切り取り", Some("Ctrl+X"), quiet(reasons.cut), Command::Cut),
            line("menu-paste", "clipboard-paste", "貼り付け", Some("Ctrl+V"), reasons.paste, Command::Paste),
            line("menu-copy-path", "link", "パスをコピー", Some("Ctrl+Shift+C"), quiet(reasons.copy_path), Command::CopyPath),
            line("menu-rename", "pencil-line", "名前を変更", Some("F2"), quiet(reasons.rename), Command::Rename).grouped(),
            line("menu-new-folder", "folder-plus", "新規フォルダー", Some("Ctrl+Shift+N"), reasons.paste, Command::NewFolder),
            line("menu-recycle", "trash", "ごみ箱へ移動", Some("Delete"), quiet(reasons.delete), Command::Recycle),
        ];
        if one && entries[0].directory {
            let path = entries[0].path.clone();
            let favorite = self.settings.is_favorite(&path);
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
        if let Some(pin) = pin {
            // Beside the favorite's line, in its group.
            let first = !lines.last().is_some_and(|l| matches!(l.command, Command::Favorite(_)));
            lines.push(if first { pin.grouped() } else { pin });
        }
        lines.push(line("menu-windows", "ellipsis", "その他のオプションを確認", None, reasons.shell, Command::Windows).grouped());
        lines
    }

    pub fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        // Done by the field itself, with the keys back in it.
        if let Command::Edit(edit) = command {
            self.close_menus(true, window, cx);
            window.dispatch_action(edit.action(), cx);
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
            Command::Explorer => self.reveal_in_explorer(cx),
            Command::Copy => self.copy(false, cx),
            Command::Cut => self.copy(true, cx),
            Command::Paste => self.paste(cx),
            Command::CopyPath => self.copy_paths(cx),
            Command::Rename => self.ask_rename(window, cx),
            Command::NewFolder => self.ask_new_folder(window, cx),
            Command::Recycle => self.delete_picked(false, cx),
            Command::Delete => self.delete_picked(true, cx),
            Command::Favorite(path) => {
                self.toggle_favorite(&path, cx);
            }
            Command::Pin(path) => self.toggle_pin(&path, cx),
            Command::Windows => {
                self.show_shell_menu(window, cx);
                return;
            }
            Command::EditSearch(query, filter) => self.open_edit(&query, filter.as_deref(), window, cx),
            Command::Unsave(query, filter) => self.toggle_saved(&query, filter, cx),
            Command::Sort(target, by) => self.set_sort(target, by, cx),
            Command::Direction(target, descending) => self.set_direction(target, descending, cx),
            Command::Unsort(id) => self.set_order(Some(id), self.settings.sort, cx),
            Command::SearchFilter(name) => self.choose_filter(name, cx),
            Command::EditFilters => self.open_filters(window, cx),
            Command::Edit(_) => unreachable!("a field's edit is done above"),
        }
        // What the line opened, a dialog, takes the keys; otherwise they go back to the column.
        let to_column = self.dialog.is_none();
        self.close_menus(to_column, window, cx);
    }

    /// Closes whichever menu is open, handing the keys back to the column when `to_column`; a text field's
    /// menu hands them back to its field, and the filters' menu to the search field, when it had them.
    pub fn close_menus(&mut self, to_column: bool, window: &mut Window, cx: &mut Context<Self>) {
        let popup = self.popup.take();
        self.menu_view = None;
        match popup {
            Some(Popup::Text(menu)) => menu.input.update(cx, |input, cx| {
                input.set_selection_focus(None, cx);
                if to_column {
                    input.focus(window, cx);
                }
            }),
            Some(Popup::Filters { from_search: true }) if to_column => self.search.update(cx, |input, cx| input.focus(window, cx)),
            // Esc in the trail's menu goes back to the button that opened it.
            Some(Popup::Crumbs) if to_column => self.stop("crumb-fold", cx).focus(window, cx),
            _ if to_column => self.column_focus(self.state.active).focus(window, cx),
            _ => {}
        }
        cx.notify();
    }

    /// Opens the menu of the folders folded off the trail, as it was last drawn.
    pub fn open_crumb_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.popup = Some(Popup::Crumbs);
        let results = self.state.pinned();
        let active = self.state.active;
        let colet = cx.weak_entity();
        let items = self
            .folded_crumbs
            .iter()
            .map(|(index, label, path)| {
                let (index, label, path) = (*index, label.clone(), path.clone());
                let colet = colet.clone();
                PopupMenuItem::element(move |_, _| div().id(("crumb-item", index)).test_support().tip(path.clone()).child(label.clone()))
                    .icon(crate::view::glyph(if index == 1 && results { "search" } else { "folder" }))
                    .checked(index == active)
                    .on_click(move |_, window, cx| {
                        colet
                            .update(cx, |this, cx| {
                                this.close_crumb_menu(false, window, cx);
                                this.go_to(index, window, cx);
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
            self.popup = None;
            self.menu_view = None;
        }
        if to_button {
            self.stop("crumb-fold", cx).focus(window, cx);
        }
        cx.notify();
    }

    /// Opens the default order's menu.
    pub fn open_sort(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.popup = Some(Popup::Sort);
        self.show_menu(window, cx);
    }

    /// Opens the menu of the column `id`'s own order.
    pub fn open_column_sort(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        self.popup = Some(Popup::ColumnSort(id));
        self.show_menu(window, cx);
    }

    /// Opens the search filters' menu.
    pub fn open_filter_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let from_search = match self.popup {
            Some(Popup::Filters { from_search }) => from_search,
            _ => self.search_focused,
        };
        self.popup = Some(Popup::Filters { from_search });
        self.show_menu(window, cx);
    }

    /// Builds the open menu's lines into the component library's menu and gives it the keys: ↑↓ move,
    /// Enter picks, Esc closes, and the keys go back to where they were.
    pub fn show_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let colet = cx.weak_entity();
        let items: Vec<PopupMenuItem> = self
            .menu_lines()
            .into_iter()
            .flat_map(|line| {
                let Line { id, glyph, text, hint, off, command, group, chosen } = line;
                let colet = colet.clone();
                let item = PopupMenuItem::element(move |_, cx| {
                    div()
                        .id(id.clone())
                        .test_support()
                        .flex_1()
                        .flex()
                        .items_center()
                        .gap_4()
                        .child(div().flex_1().child(text.clone()))
                        .when_some(hint, |d, key| d.child(div().text_xs().text_color(cx.theme().muted_foreground).child(key)))
                })
                .when_some(glyph, |item, glyph| item.icon(crate::view::glyph(glyph)))
                .disabled(off.is_some())
                .checked(chosen)
                .on_click(move |event: &ClickEvent, window, cx| {
                    // "ごみ箱へ移動" with Shift held deletes outright, as Explorer's Delete does.
                    let command = match &command {
                        Command::Recycle if event.modifiers().shift => Command::Delete,
                        command => command.clone(),
                    };
                    colet.update(cx, |this, cx| this.run_command(command, window, cx)).ok();
                });
                group.then(PopupMenuItem::separator).into_iter().chain([item])
            })
            .collect();
        self.show_items(items, window, cx);
    }

    /// Opens a menu of `items`, giving it the keys; it closes as [`Colet::close_menus`] does.
    pub fn show_items(&mut self, items: Vec<PopupMenuItem>, window: &mut Window, cx: &mut Context<Self>) {
        let menu = PopupMenu::build(window, cx, |menu, _, _| items.into_iter().fold(menu, |menu, item| menu.item(item)));
        // Closed by Esc or a press outside it; a line picked closes the menu itself first.
        let dismissed = cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
            if this.menu_view.is_some() {
                this.close_menus(true, window, cx);
            }
        });
        let focus = menu.focus_handle(cx);
        focus.focus(window, cx);
        self.menu_view = Some((menu, focus, dismissed));
        cx.notify();
    }

    /// The open menu has the keys.
    pub fn menu_has_keys(&self, window: &Window, cx: &App) -> bool {
        self.menu_view.as_ref().is_some_and(|(_, focus, _)| focus.contains_focused(window, cx))
    }

    /// The open menu, to lay out where it opened; Shift+F10 and the menu key close it, as they open it.
    pub fn menu_element(&self, id: &'static str, label: &'static str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (menu, _, _) = self.menu_view.as_ref()?;
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
