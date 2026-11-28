//! A column: its heading, narrowing field, headings, rows and the edge that widens it.

use colet_core::{entry::Sort, paths::shown, time};
use gpui_kit::base::input::{self, Input, InputState};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    spinner::Spinner,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::*,
    metrics::{self, scaled},
    state::{Column, ColumnId, Kind},
    stops::Stop,
    theme::ActivePalette,
    view::*,
};

use super::{MODIFIED_WIDTH, SIZE_TRAIL, SIZE_WIDTH};

/// How many the column holds, in its heading: narrowed, how many show of how many in all. Nothing while it
/// cannot be listed or has not been yet.
fn count_text(c: &Column) -> String {
    match &c.narrow {
        _ if c.error.is_some() || c.unread() => String::new(),
        // Everything narrows the results: how many it lets through, of how many the search finds.
        Some(n) if c.kind == Kind::Search => format!("{} / {}", time::count(c.total as u64), time::count(n.whole as u64)),
        Some(n) if !n.text().is_empty() => format!("{} / {}", time::count(c.entries.len() as u64), time::count(c.shown_total() as u64)),
        _ => time::count(c.shown_total() as u64),
    }
}

impl Colet {
    /// Shift+↑↓, with Ctrl adding: picks rows on the way, or in home, which picks one at a time, moves as
    /// the arrows do.
    pub(crate) fn extend_step(&mut self, by: isize, add: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.in_home() {
            self.step(by, window, cx);
            return;
        }
        self.state.extend_step(by, add);
        self.changed(cx);
    }

    /// The keys are on the rows, home's or the columns', not on a field or a button among them.
    fn keys_on_rows(&self, window: &Window) -> bool {
        self.home_focus.is_focused(window) || self.columns_focus.is_focused(window)
    }

    /// The keys are on one of the buttons that are stops for Tab.
    fn stop_focused(&self, window: &Window) -> bool {
        self.stops.has_keys(window)
    }

    /// `act` on `A`, a key that moves through the rows or between columns, while the keys are on the rows.
    /// On a field or a button among them, it is theirs to take or leave: it moves nothing.
    fn on_rows<A: Action>(
        el: Stateful<Div>,
        cx: &mut Context<Self>,
        act: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        el.on_action(cx.listener(move |this, _: &A, window, cx| {
            if this.keys_on_rows(window) {
                act(this, window, cx);
            } else {
                cx.propagate();
            }
        }))
    }

    /// The keys that move through a column and between them, for home's pane and the columns alike.
    pub(crate) fn column_keys(&self, el: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        let el = el.key_context(COLUMNS);
        let el = Self::on_rows::<SelectPrevious>(el, cx, |this, window, cx| this.step(-1, window, cx));
        let el = Self::on_rows::<SelectNext>(el, cx, |this, window, cx| this.step(1, window, cx));
        let el = Self::on_rows::<SelectPageUp>(el, cx, |this, window, cx| this.step(-10, window, cx));
        let el = Self::on_rows::<SelectPageDown>(el, cx, |this, window, cx| this.step(10, window, cx));
        let el = Self::on_rows::<SelectFirst>(el, cx, |this, window, cx| this.step(-isize::MAX / 2, window, cx));
        let el = Self::on_rows::<SelectLast>(el, cx, |this, window, cx| this.step(isize::MAX / 2, window, cx));
        let el = Self::on_rows::<ColumnLeft>(el, cx, |this, window, cx| this.move_column(false, window, cx));
        let el = Self::on_rows::<ColumnRight>(el, cx, |this, window, cx| this.move_column(true, window, cx));
        // Shift+↑↓ pick on the way, Ctrl moves the keys alone, both together add.
        let el = Self::on_rows::<ExtendUp>(el, cx, |this, window, cx| this.extend_step(-1, false, window, cx));
        let el = Self::on_rows::<ExtendDown>(el, cx, |this, window, cx| this.extend_step(1, false, window, cx));
        let el = Self::on_rows::<AddUp>(el, cx, |this, window, cx| this.extend_step(-1, true, window, cx));
        let el = Self::on_rows::<AddDown>(el, cx, |this, window, cx| this.extend_step(1, true, window, cx));
        let el = Self::on_rows::<FocusPrevious>(el, cx, |this, _, cx| this.focus_step(-1, cx));
        let el = Self::on_rows::<FocusNext>(el, cx, |this, _, cx| this.focus_step(1, cx));
        let el = Self::on_rows::<FocusPageUp>(el, cx, |this, _, cx| this.focus_step(-10, cx));
        let el = Self::on_rows::<FocusPageDown>(el, cx, |this, _, cx| this.focus_step(10, cx));
        let el = Self::on_rows::<FocusFirst>(el, cx, |this, _, cx| this.focus_step(-isize::MAX / 2, cx));
        let el = Self::on_rows::<FocusLast>(el, cx, |this, _, cx| this.focus_step(isize::MAX / 2, cx));
        let el = Self::on_rows::<TogglePicked>(el, cx, |this, window, cx| {
            this.state.toggle_focused();
            // A saved search picked alone in home runs, which the field shows.
            this.sync_search_field(window, cx);
            this.changed(cx);
        });
        el.on_action(cx.listener(|this, _: &RenameItem, window, cx| this.ask_rename(window, cx)))
            .on_action(cx.listener(|this, _: &CopyItems, _, cx| this.copy(false, cx)))
            .on_action(cx.listener(|this, _: &CutItems, _, cx| this.copy(true, cx)))
            .on_action(cx.listener(|this, _: &PasteItems, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &CopyPaths, _, cx| this.copy_paths(cx)))
            .on_action(cx.listener(|this, _: &ShowProperties, _, cx| this.show_properties(cx)))
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| this.ask_new_folder(window, cx)))
            .on_action(cx.listener(|this, _: &RecycleItems, _, cx| this.delete_picked(false, cx)))
            .on_action(cx.listener(|this, _: &RecycleItemsForGood, _, cx| this.delete_picked(true, cx)))
            // Home picks one row at a time: Ctrl+A picks nothing there, and Shift moves as the arrows do.
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                if !this.in_home() {
                    this.state.select_all(this.state.active);
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &QuickLook, window, cx| {
                if this.keys_on_rows(window) {
                    this.quick_look(cx);
                } else {
                    // A field types it, a button is pressed by it.
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenMenu, window, cx| {
                // Under the focused row, as from the keyboard; or the column's top.
                // Neither drawn only before the first frame: the window's corner, which the menu is kept in from.
                let at = this.key_menu.at(this.scale()).unwrap_or_default();
                this.open_menu(this.state.active, None, at, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenItem, window, cx| {
                // A button among the rows is pressed by it, as by space.
                if this.stop_focused(window) {
                    cx.propagate();
                    return;
                }
                let index = this.state.active;
                // Several picked open together, as the menu's 開く does.
                if this.state.columns[index].shown_selected().len() > 1 {
                    this.open_picked(window, cx);
                } else if let Some(path) = this.state.columns[index].focused.clone() {
                    this.open(index, &path, window, cx);
                }
            }))
            // The row the keys are on opens in a tab of its own, behind this one; with none, the folder
            // column's own folder.
            .on_action(cx.listener(|this, _: &OpenInNewTab, _, cx| {
                let index = this.state.active;
                let c = &this.state.columns[index];
                let own = (c.kind == Kind::Folder).then(|| c.path.clone());
                if let Some(path) = c.focused.clone().or_else(|| c.highlight.clone()).or(own) {
                    this.open_in_tab(index, &path, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &TrimNarrowing, window, cx| {
                if !this.trim_narrowing(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ClearNarrowing, window, cx| {
                let c = &this.state.columns[this.state.active];
                if c.is_narrowed() {
                    this.clear_narrowing(c.id, window, cx);
                } else {
                    cx.propagate();
                }
            }))
    }

    /// While `focus`, the rows', has the keys, what is typed there goes to their column's field.
    pub(crate) fn takes_text(&self, focus: &FocusHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let (focus, view) = (focus.clone(), cx.entity());
        canvas(|_, _, _| {}, move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, view), cx))
            .absolute()
            .size_full()
    }

    pub(crate) fn message(&self, icon_name: &'static str, text: String, cx: &App) -> Div {
        let p = cx.palette();
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .px_4()
            .py_8()
            .text_xs()
            .text_color(p.text_muted)
            .child(icon(icon_name, metrics::MESSAGE_GLYPH, p.text_faint))
            .child(div().text_center().child(text))
    }

    pub(crate) fn loading(&self, cx: &App) -> Div {
        let p = cx.palette();
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .text_xs()
            .text_color(p.text_muted)
            .child(Spinner::new().icon(glyph("loader-circle")).color(p.text_muted).with_size(self.scale().px(metrics::GLYPH)))
            .child(words::column::loading())
    }

    /// What a column lists, or why it lists nothing.
    /// The column at `index` under its headings: its rows, or why it has none.
    pub(crate) fn column_body(&mut self, index: usize, marked: bool, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        if let Some(error) = c.error.clone() {
            return self.column_error(id, error, window, cx);
        }
        if c.unread() {
            return self.loading(cx).into_any_element();
        }
        if c.entries.is_empty() {
            let (name, text) = match kind {
                Kind::Search if c.narrow.is_some() => ("funnel", words::no_match()),
                Kind::Search => ("search", words::no_match()),
                Kind::Home if c.narrow.is_some() => ("funnel", words::no_match()),
                Kind::Home => ("hard-drive", words::home::empty()),
                Kind::Folder if c.shown_total() > 0 && c.is_narrowed() => ("funnel", words::no_match()),
                Kind::Folder if c.total > 0 => ("eye-off", words::column::only_hidden()),
                Kind::Folder => ("folder", words::column::empty_folder()),
            };
            return self.message(name, text.into(), cx).into_any_element();
        }
        // The keys are on the column worked in: its focused row is ringed.
        let keys = marked && self.keys_in_column(window);
        if kind == Kind::Home {
            return self.home_rows(marked, keys, cx);
        }
        self.column_rows(index, marked, keys, cx)
    }

    /// Why the column `id` could not be listed, with a button to try again.
    fn column_error(&mut self, id: ColumnId, error: String, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let retry = Button::new(("retry", id)).outline().icon(glyph("refresh-cw")).label(words::column::retry()).on_click(cx.listener(
            move |this, event: &ClickEvent, window, cx| {
                // The button goes as the column loads: the keys go back to the columns.
                if event.is_keyboard() {
                    this.come_back_to_active(window, cx);
                }
                this.load(id, 0, false, cx);
            },
        ));
        let retry = self.ringed(retry, Stop::Retry(id), RingAt::Round, window, cx).m(scaled(-RING_ROOM));
        self.message("circle-alert", words::sentence(error), cx).child(retry).into_any_element()
    }

    /// The edge a drag widens a column from: astride its border, so its scrollbar stays clear of it.
    pub(crate) fn resizer(&self, id: &'static str, of: WidthOf, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        // Over what lies past the border, drawn after it; none over a dialog, which covers it.
        if self.overlays.has_dialog() {
            return div().into_any_element();
        }
        let handle = div()
            .id(id)
            .group(id)
            .absolute()
            .top_0()
            // Off the columns' scrollbar along the bottom, which it would take the presses of.
            .bottom(scaled(SCROLLBAR_WIDTH))
            .right(scaled(-RESIZER_WIDTH / 2.))
            .w(scaled(RESIZER_WIDTH))
            .flex()
            .justify_center()
            .cursor_col_resize()
            // A line on the border, not a band over the rows.
            .child(div().w_0p5().h_full().group_hover(id, |d| d.bg(p.accent)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.widths.start_resize(of, event.position.x, width);
                    cx.stop_propagation();
                }),
            )
            .test_support();
        on_top(handle).into_any_element()
    }

    pub(crate) fn render_column(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let marked = index == self.state.active && self.columns_marked(window);
        let c: &Column = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        let (width, of) = match kind {
            Kind::Search => (self.widths.search, WidthOf::Search),
            _ => (self.widths.column, WidthOf::Column),
        };
        let title = c.title().into_owned();
        let header_title = title.clone();
        let count = count_text(c);
        let narrowed = c.is_narrowed();
        let field = self.narrow_fields.input(id);
        let path = shown(&c.path);
        let star = self.star_button(index, window, cx);
        let body = self.column_body(index, marked, window, cx);
        let sort_button = self.sort_button(index, window, cx);
        let headings = self.headings(index, cx);
        div()
            .id(("column", id))
            .role(Role::Group)
            .aria_label(words::column::label(&title))
            // A press anywhere in it takes the keys there, before the columns' focus would send them to
            // the first one beside home.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if let Some(index) = this.state.index_of(id) {
                        this.state.set_active(index);
                        cx.notify();
                    }
                }),
            )
            // Let go, it slides on screen as a crumb brings it, even when it was already active: sliding
            // under the press would carry a button in it away from the pointer.
            .on_click(cx.listener(|this, _, _, cx| this.changed(cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    if let Some(index) = this.state.index_of(id) {
                        this.open_menu(index, Some(Vec::new()), event.position, window, cx);
                    }
                }),
            )
            .w(scaled(width))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.surface)
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .id(("head", id))
                    .h(scaled(metrics::HEADING_HEIGHT))
                    .flex_none()
                    .pl_3()
                    .pr_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .id(("title", id))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if marked { p.text } else { p.text_muted })
                            .when(!path.is_empty(), |d| d.tip(path))
                            .child(header_title),
                    )
                    .child(div().text_xs().text_color(p.text_faint).child(count))
                    .child(sort_button)
                    .child(star)
                    .when(index == self.state.active, |d| {
                        let cell = self.key_menu.heading.clone();
                        d.relative().child(canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {}).absolute().size_full())
                    })
                    .test_support(),
            )
            .when_some(field, |d, input| {
                d.child(
                    div()
                        .flex_none()
                        .px_2()
                        .py_1p5()
                        .border_b_1()
                        .border_color(p.border)
                        .child(self.column_narrow_field(id, narrowed, &input, window, cx)),
                )
            })
            .child(headings)
            .child(body)
            .child(self.resizer(if kind == Kind::Search { "search-resizer" } else { "column-resizer" }, of, width, cx))
            // The scrolling columns' frame is drawn over their scroll, which would cut this one's edges.
            // Out by a line on the left and below, so it falls on the lines there.
            .when(marked && kind == Kind::Search, |d| d.child(on_top(active_frame(p).left(-metrics::HAIRLINE).bottom(-metrics::HAIRLINE))))
            .test_support()
            .into_any_element()
    }

    /// The star in the column's heading: keeps the folder as a favorite, or the search as a saved one, as
    /// it is narrowed now, or lets go of it.
    fn star_button(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = cx.palette();
        let c = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        // What it is narrowed to now is part of what is kept, and so for the results is their order.
        let narrowing = c.narrowing();
        let kept = match kind {
            Kind::Search => self
                .stored
                .settings()
                .saved(&self.state.query, self.state.search_filter.as_deref(), narrowing.as_deref(), self.state.search_order)
                .is_some(),
            _ => self.stored.settings().favorite(&c.path, narrowing.as_deref()).is_some(),
        };
        let label = match (kind, kept) {
            (Kind::Search, true) => words::column::unsave_search_key(),
            (Kind::Search, false) => words::column::save_search_key(),
            (_, true) => words::column::remove_favorite_key(),
            (_, false) => words::column::add_favorite_key(),
        };
        let star = Button::new(("star", id))
            .ghost()
            .xsmall()
            .icon(if kept { glyph("star-fill").text_color(p.accent) } else { glyph("star") })
            .accessibility_label(label)
            .tooltip(label)
            .on_click(cx.listener(move |this, _, _, cx| {
                if kind == Kind::Search {
                    this.toggle_saved_current(cx);
                } else if let Some(index) = this.state.index_of(id) {
                    let key = this.favorite_key_of(index);
                    this.toggle_favorite(&key, cx);
                }
            }));
        self.ringed(star, Stop::Star(id), RingAt::Edge, window, cx)
    }

    /// The button of the column's own order, beside its star: accented while it is given one apart from
    /// the default, and its menu under it.
    fn sort_button(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let c = &self.state.columns[index];
        let id = c.id;
        let (order, own) = (self.order_of(c), self.own_order(c).is_some());
        let open = self.column_sort_open() == Some(id);
        let label = words::column::sort_order(words::order(order), own);
        let button = Button::new(("column-sort", id))
            .ghost()
            .xsmall()
            .icon(glyph("arrow-up-down").text_color(if own { p.accent } else { p.text_muted }))
            .accessibility_label(label.clone())
            // Pressed while its menu is open, and told so.
            .open(open)
            .aria_expanded(open)
            // Shown only while the menu is shut: open, the tip would lie over it.
            .when(!open, |b| b.tooltip(label))
            // ↓ opens the menu as well as Enter and Space, as with the search filter's button.
            .key_context(SORT)
            .on_action(cx.listener(move |this, _: &OpenSort, window, cx| this.open_column_sort(id, window, cx)))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                if this.column_sort_open() == Some(id) {
                    this.close_menus(true, window, cx);
                } else if !this.pressed_shut(event) {
                    this.open_column_sort(id, window, cx);
                }
            }));
        // The button takes no listener for the press, so the box round it notes it.
        let button = self.ringed(button, Stop::ColumnSort(id), RingAt::Edge, window, cx);
        div()
            .relative()
            .flex_none()
            .child(Self::opener(div().flex(), move |this| this.column_sort_open() == Some(id), cx).child(button))
            .when_some(self.menu_element("column-sort-menu", words::sort_order(), cx).filter(|_| open), |d, menu| {
                d.child(drop_down(menu, MenuSide::Right, 160., Some(160.), self.scale()))
            })
            .into_any_element()
    }

    /// The headings over the column's rows, each sorting it by what it heads; the one it goes by shows
    /// the way it runs. Laid out as a row is, so each stands over its cells.
    pub(crate) fn headings(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let c = &self.state.columns[index];
        let id = c.id;
        let order = self.order_of(c);
        let check_all = (c.kind != Kind::Home).then(|| self.check_all(index, cx));
        let search = c.kind == Kind::Search;
        let heading = |by: Sort, name: &'static str| {
            let chosen = order.by == by;
            let label = if chosen {
                words::column::sorted_by_in(words::sort_name(by), words::direction(order.descending))
            } else {
                words::column::sorted_by(words::sort_name(by))
            };
            // The sizes stand flush right, and their heading with them: its arrow goes before the name, so
            // the name ends where the sizes do.
            let flush_right = by == Sort::Size;
            let arrow = chosen.then(|| icon(if order.descending { "arrow-down" } else { "arrow-up" }, 11., p.text_muted));
            let heading_id = std::sync::Arc::new(ElementId::from((name, id)));
            div()
                .id(ElementId::clone(&heading_id))
                .role(Role::Button)
                .aria_label(label)
                .h_full()
                .px_1()
                .flex()
                .items_center()
                .when(flush_right, |d| d.pr(scaled(4. + SIZE_TRAIL)).justify_end())
                .gap_0p5()
                .rounded(scaled(self.radii(cx).sm))
                .hover(|d| d.bg(p.hover))
                .text_color(if chosen { p.text } else { p.text_muted })
                .map(|d| {
                    let name = div()
                        .id(ElementId::NamedChild(heading_id.clone(), "name".into()))
                        .flex_none()
                        .child(words::sort_name(by))
                        .test_support();
                    if flush_right { d.children(arrow).child(name) } else { d.child(name).children(arrow) }
                })
                .on_click(cx.listener(move |this, _, _, cx| this.sort_by_heading(id, by, cx)))
                .test_support()
        };
        // The row's room: the list's 4px, then the row's 8px and its 2px ring; inside, the gap between cells.
        // Each heading reaches 4px past its cells, over the gap, so its text stands where theirs does: at
        // their left, or for the sizes, at their right.
        div()
            .flex_none()
            .h_6()
            .px_2p5()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .text_xs()
            .children(check_all)
            // The search results' items are in all sorts of folders, shown under their names: they sort by
            // those too, from a heading beside the name's, the rest of the room left bare.
            .map(|d| {
                if search {
                    d.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .flex()
                            .child(heading(Sort::Name, "sort-by-name").flex_none())
                            .child(heading(Sort::Location, "sort-by-location").flex_none()),
                    )
                } else {
                    d.child(heading(Sort::Name, "sort-by-name").flex_1().min_w_0())
                }
            })
            .child(heading(Sort::Size, "sort-by-size").w(scaled(SIZE_WIDTH + SIZE_TRAIL + 8.)).flex_none())
            .child(heading(Sort::Modified, "sort-by-modified").w(scaled(MODIFIED_WIDTH + 8.)).flex_none())
            .child(div().w(scaled(13. + 8.)).flex_none())
            .into_any_element()
    }

    /// The checkbox heading the rows' own, over them: checked while every row shown is picked, a click
    /// then picks none, and otherwise all. Disabled while there are no rows. A Tab stop, as the library's
    /// checkbox is: Space presses it.
    fn check_all(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let c = &self.state.columns[index];
        let (id, empty, all) = (c.id, c.entries.is_empty(), c.all_selected());
        div()
            .flex_none()
            .h_full()
            // Where the rows' checkboxes stand, then the gap to their names less the heading's own 4px.
            .ml_1()
            .mr_1()
            .w_4()
            .flex()
            .items_center()
            .justify_center()
            .child(Checkbox::new(("check-all", id)).small().accessibility_label(words::select_all()).checked(all).disabled(empty).on_click(
                cx.listener(move |this, _, _, cx| {
                    let Some(index) = this.state.index_of(id) else { return };
                    this.state.toggle_all(index);
                    this.state.set_active(index);
                    this.changed(cx);
                }),
            ))
    }

    /// The field narrowing the column `id` to the names holding what is typed there.
    pub(crate) fn column_narrow_field(
        &mut self,
        id: ColumnId,
        narrowed: bool,
        input: &Entity<InputState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let focused = self.has_keys(input, window, cx);
        let has_text = !input.read(cx).value().is_empty();
        let clear = has_text.then(|| {
            let clear = Button::new(("clear-filter", id))
                .ghost()
                .xsmall()
                .icon(glyph("x"))
                .accessibility_label(words::column::clear_filter())
                .tooltip(words::column::clear_filter_key())
                .on_click(cx.listener(move |this, _, window, cx| {
                    if let Some(field) = this.narrow_fields.get(&id) {
                        let input = field.input.clone();
                        input.update(cx, |input, cx| {
                            input.set_value("", window, cx);
                            input.focus(window, cx);
                        });
                        this.narrow(id, "", cx);
                    }
                }));
            self.ringed(clear, Stop::ClearFilter(id), RingAt::Edge, window, cx)
        });
        // Ringed as the search field is; the ring lies over the gaps around.
        frame(p, focused, radii.md).m(scaled(-RING_ROOM)).child(
            div()
                .id(("filter", id))
                .w_full()
                .h(scaled(metrics::SMALL_CONTROL_HEIGHT))
                .pl_2()
                .pr_0p5()
                .flex()
                .items_center()
                .gap_2()
                .bg(p.surface)
                .border_1()
                .border_color(p.border_strong)
                .rounded(scaled(radii.md))
                // Accented while it has the keys, or narrows the rows from out of them.
                .child(icon("funnel", 13., if focused || narrowed { p.accent } else { p.text_muted }))
                .child(div().flex_1().min_w_0().capture_any_mouse_down(Self::right_press_menu(input, cx)).child(Input::new(input)))
                // ↓ picks a row it lets through, as a click does, going there; Esc clears it, then goes there too.
                .capture_action(cx.listener(move |this, _: &input::MoveDown, window, cx| {
                    cx.stop_propagation();
                    this.open_from_narrow_field(id, window, cx);
                }))
                .on_action(cx.listener(move |this, _: &input::Escape, window, cx| {
                    cx.stop_propagation();
                    this.escape_narrow_field(id, window, cx);
                }))
                // ← and → past the text's ends stay here, not going on to the next column; Enter does
                // as ↓ does.
                .on_action(|_: &input::MoveLeft, _, cx| cx.stop_propagation())
                .on_action(|_: &input::MoveRight, _, cx| cx.stop_propagation())
                .on_action(|_: &input::Enter, _, cx| cx.stop_propagation())
                .children(clear)
                .test_support(),
        )
    }
}
