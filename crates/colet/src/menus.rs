//! The menus, as lines that the mouse and the keys pick alike: ↑↓ move, Enter picks, Esc closes, and the
//! keys go back to where they were. One is open at a time ([`Popup`]). Each feature draws its own menu's
//! lines and does what they say: the file menu and home's, a column's order's, the search filters', a
//! tab's and a text field's; the folders folded off the trail are a menu of the trail's own.
use colet_core::shell::IconSource;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::Colet,
    column_order::OrderCommand,
    file_menu::FileCommand,
    file_menu::Menu,
    home::HomeCommand,
    metrics::scaled,
    search_filters::FilterCommand,
    state::ColumnId,
    tabs::{TabCommand, TabId},
    text_menu::{Edit, TextMenu},
    theme::ActivePalette,
};

/// What a line of a menu does, by the feature whose menu it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    File(FileCommand),
    Home(HomeCommand),
    Order(OrderCommand),
    Filter(FilterCommand),
    Tab(TabCommand),
    /// An edit of a text field's text.
    Edit(Edit),
}

impl From<FileCommand> for Command {
    fn from(command: FileCommand) -> Self {
        Command::File(command)
    }
}

impl From<HomeCommand> for Command {
    fn from(command: HomeCommand) -> Self {
        Command::Home(command)
    }
}

impl From<OrderCommand> for Command {
    fn from(command: OrderCommand) -> Self {
        Command::Order(command)
    }
}

impl From<FilterCommand> for Command {
    fn from(command: FilterCommand) -> Self {
        Command::Filter(command)
    }
}

impl From<TabCommand> for Command {
    fn from(command: TabCommand) -> Self {
        Command::Tab(command)
    }
}

impl From<Edit> for Command {
    fn from(edit: Edit) -> Self {
        Command::Edit(edit)
    }
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

impl Line {
    /// With a rule above it, starting a group.
    pub(crate) fn grouped(mut self) -> Self {
        self.group = true;
        self
    }
}

/// A line with a glyph, the key that does the same, and why it is off, if it is.
pub(crate) fn line(
    id: &'static str,
    glyph: &'static str,
    text: &'static str,
    hint: Option<&'static str>,
    off: Option<&'static str>,
    command: impl Into<Command>,
) -> Line {
    Line {
        id: id.into(),
        glyph: Some(glyph),
        picture: None,
        text: text.into(),
        hint,
        off,
        command: command.into(),
        group: false,
        chosen: false,
        sub: None,
    }
}

/// A line with no glyph, off without a word unless `on`.
pub(crate) fn plain(id: &'static str, text: &'static str, hint: &'static str, on: bool, command: impl Into<Command>) -> Line {
    Line {
        id: id.into(),
        glyph: None,
        picture: None,
        text: text.into(),
        hint: Some(hint),
        off: (!on).then_some(""),
        command: command.into(),
        group: false,
        chosen: false,
        sub: None,
    }
}

/// A line to choose, checked when it is the one in use.
pub(crate) fn choice(id: impl Into<ElementId>, text: impl Into<SharedString>, command: impl Into<Command>, chosen: bool) -> Line {
    Line {
        id: id.into(),
        glyph: None,
        picture: None,
        text: text.into(),
        hint: None,
        off: None,
        command: command.into(),
        group: false,
        chosen,
        sub: None,
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
    /// The lines of the open menu, as the feature whose menu it is draws them. The trail's menu is built of
    /// its own items, not of lines.
    pub fn menu_lines(&self, cx: &App) -> Vec<Line> {
        match self.overlays.popup() {
            Some(Popup::Text(menu)) => menu.lines(),
            Some(Popup::ColumnSort(id)) => self.order_lines(*id),
            Some(Popup::Filters { .. }) => self.filter_lines(cx),
            Some(Popup::Tab { tab, .. }) => self.tab_lines(*tab),
            Some(Popup::File(_)) => self.file_lines(cx),
            Some(Popup::Crumbs) | None => Vec::new(),
        }
    }

    /// Does what the line picked says, by the feature whose menu it is, and closes the menu.
    pub fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            // Each closes the menu itself first.
            Command::Edit(edit) => return self.run_edit(edit, window, cx),
            Command::Tab(command) => return self.run_tab_command(command, window, cx),
            _ => {}
        }
        // The file menu's and home's act in the column it was opened in.
        if let Some(column) = self.menu().map(|menu| menu.column) {
            let Some(index) = self.state.index_of(column) else {
                self.close_menus(true, window, cx);
                return;
            };
            self.state.set_active(index);
        }
        let close = match command {
            Command::File(command) => self.run_file_command(command, window, cx),
            Command::Home(command) => {
                self.run_home_command(command, window, cx);
                true
            }
            Command::Order(command) => {
                self.run_order_command(command, cx);
                true
            }
            Command::Filter(command) => {
                self.run_filter_command(command, window, cx);
                true
            }
            Command::Edit(_) | Command::Tab(_) => unreachable!("done above"),
        };
        if close {
            // What the line opened, a dialog, takes the keys; otherwise they go back to the column.
            let to_column = !self.overlays.has_dialog();
            self.close_menus(to_column, window, cx);
        }
    }

    /// Closes whichever menu is open, handing the keys back when `restore`: to where they were as it opened,
    /// or, in a dialog, to the text field whose menu it was.
    pub fn close_menus(&mut self, restore: bool, window: &mut Window, cx: &mut Context<Self>) {
        let popup = self.overlays.close_menu();
        if let Some(Popup::Text(menu)) = &popup {
            menu.input.update(cx, |input, cx| input.set_selection_focus(None, cx));
        }
        let within = self.overlays.has_dialog() || self.overlays.is_history_open();
        match popup {
            Some(Popup::Text(menu)) if restore && within => menu.input.update(cx, |input, cx| input.focus(window, cx)),
            _ if restore && !within => self.restore_keys(window, cx),
            _ => {}
        }
        cx.notify();
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
                    Some(image) => img(image).size(crate::metrics::scaled(crate::metrics::ICON)).flex_none().into_any_element(),
                    None => div().size(crate::metrics::scaled(crate::metrics::ICON)).flex_none().into_any_element(),
                });
                // Colet's own colors, as everything else it draws takes them; read as drawn, so a theme
                // changed while the menu is open shows.
                let muted = cx.palette().text_muted;
                div()
                    .id(id.clone())
                    .test_support()
                    .flex_1()
                    .flex()
                    .items_center()
                    .gap_4()
                    .child(div().flex_1().flex().items_center().gap_2().children(picture).child(text.clone()))
                    .when_some(hint, |d, key| d.child(div().text_xs().text_color(muted).child(key)))
                    .when(chevron, |d| d.child(svg().path("icons/chevron-right.svg").size_3().flex_none().text_color(muted)))
            })
            .when_some(glyph, |item, glyph| item.icon(crate::view::glyph(glyph)))
            .disabled(off.is_some())
            .checked(chosen)
            .on_click(move |event: &ClickEvent, window, cx| {
                // Shift held changes what some of the file menu's lines do, as in Explorer.
                let command = match &command {
                    Command::File(command) => Command::File(command.shifted(event.modifiers().shift)),
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

/// The least width of the file menu, whose lines carry their keys beside them.
const FILE_MENU_WIDTH: f32 = 240.;
/// The least width of a text field's menu, whose lines are short.
const TEXT_MENU_WIDTH: f32 = 200.;
/// The least width of a tab's menu.
const TAB_MENU_WIDTH: f32 = 220.;

impl Colet {
    /// The file menu, or a text field's, where it was opened; over a dialog too, for its fields'. GPUI
    /// paints every deferred draw in the order of its priority, nested or not, so the menu takes one over
    /// the dialog's (ten and up) to be drawn in front of it as it is clicked in front of it.
    pub fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (at, id, label, width) = match self.overlays.popup()? {
            Popup::File(menu) => (menu.at, "menu", "ファイル操作", FILE_MENU_WIDTH),
            Popup::Text(menu) => (menu.at, "text-menu", "編集", TEXT_MENU_WIDTH),
            Popup::Tab { at, .. } => (*at, "tab-menu", "タブ", TAB_MENU_WIDTH),
            _ => return None,
        };
        let menu = self.menu_element(id, label, cx)?;
        Some(
            deferred(
                anchored()
                    .position(at)
                    .snap_to_window_with_margin(self.scale().px(crate::view::MENU_MARGIN))
                    .child(div().min_w(scaled(width)).child(menu)),
            )
            .with_priority(gpui_kit::base::POPUP_PRIORITY)
            .into_any_element(),
        )
    }
}
