//! The two menus, the file menu and the sort order's, as lines that the mouse and the keys pick alike:
//! ↑↓ move, Enter picks, Esc closes, and the keys go back to the column worked in.
use colet_core::entry::{Sort, direction_label};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{app::Colet, state::saved_query, theme::Palette};

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
    Favorite(String),
    Windows,
    EditSearch(String),
    Unsave(String),
    Sort(Sort),
    Direction(bool),
}

pub struct Line {
    pub id: &'static str,
    pub glyph: Option<&'static str>,
    pub text: &'static str,
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
    Line { id, glyph: Some(glyph), text, hint, off, command, group: false, chosen: false }
}

impl Line {
    fn grouped(mut self) -> Self {
        self.group = true;
        self
    }
}

impl Colet {
    /// The lines of the open menu: the sort order's, or the file menu's.
    pub fn menu_lines(&self) -> Vec<Line> {
        if self.sort_open {
            let choice = |text, command, chosen| Line { id: text, glyph: None, text, hint: None, off: None, command, group: false, chosen };
            let sort = self.settings.sort;
            let mut lines: Vec<Line> =
                Sort::ALL.map(|by| choice(by.label(), Command::Sort(by), by == sort.by)).into();
            lines.extend([false, true].map(|descending| {
                choice(direction_label(descending), Command::Direction(descending), descending == sort.descending)
            }));
            lines[Sort::ALL.len()].group = true;
            return lines;
        }
        if self.menu.is_none() {
            return Vec::new();
        }
        let reasons = self.reasons();
        let picked = self.picked();
        let entries = self.picked_entries();
        if let [path] = picked.as_slice()
            && let Some(query) = saved_query(path)
        {
            return vec![
                line("menu-copy-query", "link", "検索文字列をコピー", Some("Ctrl+Shift+C"), None, Command::CopyPath),
                line("menu-edit-search", "pencil-line", "編集", Some("F2"), None, Command::EditSearch(query.into())).grouped(),
                line("menu-unsave", "star", "保存した検索を解除", None, None, Command::Unsave(query.into())),
            ];
        }
        let one = entries.len() == 1;
        let open_off = (!one).then_some(reasons.open.unwrap_or("1項目を選択したときに使えます"));
        // From empty space nothing is picked, and the actions on items need no word.
        let some = !picked.is_empty();
        let quiet = |reason: Option<&'static str>| if some { reason } else { reason.map(|_| "") };
        let mut lines = vec![
            line("menu-open", "external-link", "開く", Some("Enter"), open_off, Command::Open),
            line("menu-explorer", "folder-open", "Explorerで表示", None, open_off, Command::Explorer),
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
        lines.push(line("menu-windows", "ellipsis", "その他のオプションを確認", None, reasons.shell, Command::Windows).grouped());
        lines
    }

    pub fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(menu) = &self.menu {
            let Some(index) = self.state.index_of(menu.column) else {
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
            Command::Recycle => self.ask_recycle(false, window, cx),
            Command::Favorite(path) => {
                self.toggle_favorite(&path, cx);
            }
            Command::Windows => {
                self.show_shell_menu(window, cx);
                return;
            }
            Command::EditSearch(query) => self.open_edit(&query, window, cx),
            Command::Unsave(query) => self.toggle_saved(&query, cx),
            Command::Sort(by) => self.set_sort(by, cx),
            Command::Direction(descending) => self.set_direction(descending, cx),
        }
        // What the line opened, a dialog, takes the keys; otherwise they go back to the column.
        let to_column = self.dialog.is_none();
        self.close_menus(to_column, window, cx);
    }

    /// Closes whichever menu is open, handing the keys back to the column when `to_column`.
    pub fn close_menus(&mut self, to_column: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.sort_open = false;
        self.menu_cursor = None;
        if to_column {
            let handle = if self.state.active == 0 { &self.home_focus } else { &self.columns_focus };
            handle.focus(window, cx);
        }
        cx.notify();
    }

    /// Opens the menu of the folders folded off the trail; from the keys, with its first line lit.
    pub fn open_crumb_menu(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.sort_open = false;
        self.crumb_menu_open = true;
        self.crumb_cursor = keyboard.then_some(0);
        self.menu_focus.focus(window, cx);
        cx.notify();
    }

    /// Closes the trail's menu; `to_button` hands the keys back to the button that opened it.
    pub fn close_crumb_menu(&mut self, to_button: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.crumb_menu_open = false;
        self.crumb_cursor = None;
        if to_button {
            self.stop("crumb-fold", cx).focus(window, cx);
        }
        cx.notify();
    }

    /// ↑ or ↓ in the trail's menu of `count` lines, round the ends.
    pub fn move_crumb_cursor(&mut self, by: isize, count: usize, cx: &mut Context<Self>) {
        if count == 0 {
            return;
        }
        self.crumb_cursor = Some(match self.crumb_cursor {
            Some(at) => (at as isize + by).rem_euclid(count as isize) as usize,
            None if by > 0 => 0,
            None => count - 1,
        });
        cx.notify();
    }

    /// Opens the sort order's menu; from the keys, with the sort in use lit.
    pub fn open_sort(&mut self, keyboard: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.menu = None;
        self.crumb_menu_open = false;
        self.sort_open = true;
        self.menu_cursor = keyboard.then(|| Sort::ALL.iter().position(|s| *s == self.settings.sort.by).unwrap_or(0));
        self.menu_focus.focus(window, cx);
        cx.notify();
    }

    /// ↑ or ↓ in the open menu: the next line that is on, round the ends.
    pub fn move_cursor(&mut self, by: isize, cx: &mut Context<Self>) {
        let lines = self.menu_lines();
        let on: Vec<usize> = (0..lines.len()).filter(|&i| lines[i].off.is_none()).collect();
        if on.is_empty() {
            return;
        }
        let at = self.menu_cursor.and_then(|c| on.iter().position(|&i| i == c));
        let next = match at {
            Some(at) => (at as isize + by).rem_euclid(on.len() as isize) as usize,
            None if by > 0 => 0,
            None => on.len() - 1,
        };
        self.menu_cursor = Some(on[next]);
        cx.notify();
    }

    pub fn confirm_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let lines = self.menu_lines();
        if let Some(line) = self.menu_cursor.and_then(|c| lines.into_iter().nth(c)).filter(|l| l.off.is_none()) {
            self.run_command(line.command, window, cx);
        }
    }

    /// The lines, drawn alike for both menus.
    pub fn render_lines(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p: Palette = self.palette;
        let cursor = self.menu_cursor;
        let mut out = Vec::new();
        for (i, line) in self.menu_lines().into_iter().enumerate() {
            if line.group {
                out.push(div().my_1().h(px(1.)).bg(p.border).into_any_element());
            }
            let lit = cursor == Some(i);
            let command = line.command.clone();
            out.push(
                div()
                    .id(line.id)
                    .role(Role::MenuItem)
                    .aria_label(line.text)
                    .h(px(28.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .rounded(px(4.))
                    .text_size(px(12.))
                    .when(line.chosen, |d| d.text_color(p.accent))
                    .when(lit, |d| d.bg(p.accent_fill).text_color(p.on_accent))
                    // An item that is off fades; why it is off is there to be read, and does not.
                    .child(
                        div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .gap_2()
                            .when(line.off.is_some(), |d| d.opacity(0.45))
                            .when_some(line.glyph, |d, glyph| {
                                d.child(svg().path(format!("icons/{glyph}.svg")).size(px(14.)).flex_none().text_color(if lit { p.on_accent } else { p.text_muted }))
                            })
                            .child(div().flex_1().child(line.text)),
                    )
                    .when(line.chosen, |d| d.child(svg().path("icons/check.svg").size(px(14.)).text_color(if lit { p.on_accent } else { p.accent })))
                    .when_some(line.off, |d, why| d.child(div().text_color(p.text_muted).child(why)))
                    .when(line.off.is_none(), |d| {
                        d.cursor_pointer()
                            .hover(|d| d.bg(p.accent_fill).text_color(p.on_accent))
                            .when_some(line.hint, |d, key| d.child(div().text_color(if lit { p.on_accent_muted } else { p.text_faint }).child(key)))
                            .on_click(cx.listener(move |this, _, window, cx| this.run_command(command.clone(), window, cx)))
                    })
                    .test_support()
                    .into_any_element(),
            );
        }
        out
    }
}
