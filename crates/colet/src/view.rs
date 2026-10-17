//! How the window looks: home's pane on the left; the search field, the trail of folders, the columns
//! and the status bar beside it.
use std::time::Instant;

use colet_core::{entry::{Entry, Sort}, paths::folder_name, time};
use gpui_kit::base::input::{self, Input};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    state::saved_query,
    reveal::{Cell, on_screen, reveal_target},
    state::{Column, Kind},
};

/// A line of text shown while the pointer rests on an element.
pub struct Tip {
    pub text: SharedString,
    pub palette: crate::theme::Palette,
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        div()
            .px_2()
            .py_1()
            .text_size(px(12.))
            .text_color(p.text)
            .bg(p.raised)
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(4.))
            .child(self.text.clone())
    }
}

trait TipExt {
    fn tip(self, palette: crate::theme::Palette, text: impl Into<SharedString>) -> Self;
}

impl TipExt for Stateful<Div> {
    fn tip(self, palette: crate::theme::Palette, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |_, cx| cx.new(|_| Tip { text: text.clone(), palette }).into())
    }
}

/// The loading icon, turning once a second.
pub fn spinner(id: impl Into<ElementId>, size: f32, color: Hsla) -> AnimationElement<Svg> {
    icon("loader-circle", size, color).with_animation(
        id,
        Animation::new(std::time::Duration::from_secs(1)).repeat(),
        |svg, delta| svg.with_transformation(Transformation::rotate(percentage(delta))),
    )
}

/// A thin scrollbar laid over a list, as the old app's; its thumb is Colet's color.
pub fn scrollbar(bar: gpui_kit::base::Scrollbar, p: crate::theme::Palette) -> gpui_kit::base::Scrollbar {
    bar.mode(gpui_kit::base::ScrollbarMode::Always).styles(|s| {
        s.track(|t| t.bg(transparent_black()).border_color(transparent_black()).width(px(8.)))
            .thumb(|t| t.bg(p.scrollbar).width(px(6.)).inset(px(1.)).radius(px(3.)))
            .thumb_hover(|t| t.bg(p.text_muted))
            .thumb_active(|t| t.bg(p.text_muted))
    })
}

/// The focus ring of what Tab reached: two pixels in the focus color, kept clear until then.
pub fn ring(el: Stateful<Div>, p: crate::theme::Palette) -> Stateful<Div> {
    el.border_2().border_color(transparent_black()).focus_visible(|s| s.border_color(p.focus))
}

fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

/// Rows of home and the folders; search results carry their folder on a second line.
fn row_height(kind: Kind) -> f32 {
    if kind == Kind::Search { 42. } else { 28. }
}

/// What of the scrolling columns is on screen.
pub struct OnScreen {
    /// The column index the first scrolling column has.
    pub first: usize,
    pub count: usize,
    /// In columns, as [`on_screen`] measures it.
    pub span: Option<(f32, f32)>,
}

impl OnScreen {
    pub fn hidden_left(&self) -> bool {
        self.span.is_some_and(|s| s.0 > 0.)
    }

    pub fn hidden_right(&self) -> bool {
        self.span.is_some_and(|s| s.1 < self.count as f32)
    }
}

impl Colet {
    pub fn on_screen(&self) -> OnScreen {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        let view = f32::from(self.columns_scroll.bounds().size.width);
        let from = -f32::from(self.columns_scroll.offset().x);
        let cells: Vec<Cell> = (0..count).map(|i| Cell { start: i as f32 * self.widths.column, width: self.widths.column }).collect();
        OnScreen { first, count, span: on_screen(from, view, &cells) }
    }

    /// The bar under the crumbs of the columns on screen, cut where the edges cut the columns; only while
    /// some are out of sight, folded away too. Placed from where the crumbs were last drawn.
    fn crumb_bar(&self) -> Option<AnyElement> {
        let screen = self.on_screen();
        let (s0, s1) = screen.span?;
        if !(screen.hidden_left() || screen.hidden_right() || self.state.folded_away() > 0) {
            return None;
        }
        let row = self.crumbs_row.get()?;
        let bounds = self.crumb_bounds.borrow();
        let head = screen.first + s0.floor() as usize;
        let tail = screen.first + (s1.ceil() as usize).saturating_sub(1);
        // Crumbs folded into the menu have no place to mark: start at the first one shown.
        let from = (head..=tail).find(|i| bounds.contains_key(i))?;
        let (first, last) = (bounds.get(&from)?, bounds.get(&tail)?);
        let into = if from == head { s0 - s0.floor() } else { 0. };
        let start = f32::from(first.left()) + f32::from(first.size.width) * into;
        let reach = s1 - (tail - screen.first) as f32;
        let end = f32::from(last.left()) + f32::from(last.size.width) * reach.clamp(0., 1.);
        Some(
            div()
                .id("crumb-bar")
                .absolute()
                .left(px(start - f32::from(row.left())))
                .bottom_0()
                .w(px((end - start).max(0.)))
                .h(px(2.))
                .rounded(px(1.))
                .bg(self.palette.border_strong)
                .test_support()
                .into_any_element(),
        )
    }

    fn icon_button(&mut self, id: &'static str, name: &'static str, label: &'static str, enabled: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = self.palette;
        let focus = self.stop(id, cx);
        ring(div().id(id).track_focus(&focus), p)
            .role(Role::Button)
            .aria_label(label)
            .size(px(30.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .child(icon(name, 16., p.text_muted))
            .when(enabled, |b| b.cursor_pointer().hover(|b| b.bg(p.hover)))
            .when(!enabled, |b| b.opacity(0.35))
    }

    /// The keys that move through a column and between them, for home's pane and the columns alike.
    fn column_keys(&self, el: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        el.key_context(COLUMNS)
            .on_action(cx.listener(|this, _: &SelectPrevious, window, cx| this.step(-1, window, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, window, cx| this.step(1, window, cx)))
            .on_action(cx.listener(|this, _: &SelectPageUp, window, cx| this.step(-10, window, cx)))
            .on_action(cx.listener(|this, _: &SelectPageDown, window, cx| this.step(10, window, cx)))
            .on_action(cx.listener(|this, _: &SelectFirst, window, cx| this.step(-isize::MAX / 2, window, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, window, cx| this.step(isize::MAX / 2, window, cx)))
            .on_action(cx.listener(|this, _: &ColumnLeft, window, cx| this.move_column(false, window, cx)))
            .on_action(cx.listener(|this, _: &ColumnRight, window, cx| this.move_column(true, window, cx)))
            .on_action(cx.listener(|this, _: &GoBack, window, cx| this.back(window, cx)))
            .on_action(cx.listener(|this, _: &GoForward, window, cx| this.forward(window, cx)))
            .on_action(cx.listener(|this, _: &RenameItem, window, cx| this.ask_rename(window, cx)))
            .on_action(cx.listener(|this, _: &CopyItems, _, cx| this.copy(false, cx)))
            .on_action(cx.listener(|this, _: &CutItems, _, cx| this.copy(true, cx)))
            .on_action(cx.listener(|this, _: &PasteItems, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &CopyPaths, _, cx| this.copy_paths(cx)))
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| this.ask_new_folder(window, cx)))
            .on_action(cx.listener(|this, _: &RecycleItems, window, cx| this.ask_recycle(false, window, cx)))
            .on_action(cx.listener(|this, _: &RecycleItemsForGood, window, cx| this.ask_recycle(true, window, cx)))
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                this.state.select_all(this.state.active);
                this.changed(cx);
            }))
            .on_action(cx.listener(|this, _: &ExtendUp, _, cx| {
                this.state.extend_step(-1);
                this.changed(cx);
            }))
            .on_action(cx.listener(|this, _: &ExtendDown, _, cx| {
                this.state.extend_step(1);
                this.changed(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenMenu, window, cx| {
                // Under the focused row, as from the keyboard; or the column's top.
                let at = match this.focused_row.get() {
                    Some(row) => point(row.left() + px(16.), row.bottom()),
                    None => point(px(this.widths.sidebar) + px(24.), px(140.)),
                };
                this.open_menu(this.state.active, None, at, window, cx);
                // From the keys, the first line is lit.
                this.move_cursor(1, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenItem, _, cx| {
                let index = this.state.active;
                if let Some(path) = this.state.columns[index].focused.clone() {
                    this.open(index, &path, cx);
                }
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                let keystroke = &event.keystroke;
                let m = keystroke.modifiers;
                if m.control || m.alt || m.platform {
                    return;
                }
                if let Some(text) = keystroke.key_char.as_deref()
                    && !text.trim().is_empty()
                    && text.chars().count() == 1
                {
                    this.type_ahead(text, window, cx);
                    cx.stop_propagation();
                }
            }))
    }

    fn row(&mut self, index: usize, entry: &Entry, marked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let c = &self.state.columns[index];
        let kind = c.kind;
        let selected = c.selected.contains(&entry.path);
        let saved = saved_query(&entry.path);
        // Home lights the saved search that is running.
        let highlighted = c.highlight.as_ref() == Some(&entry.path)
            || saved.is_some_and(|q| !self.state.query.is_empty() && q == self.state.query);
        // On the clipboard, cut or copied, whoever put it there.
        let held = self.clipboard.paths.contains(&entry.path);
        let cut = held && self.clipboard.cut;
        let faded = (entry.hidden || cut) && !selected;
        // The keys are on it: ringed, selected or not, while they are in this column.
        let focused = marked && self.keys_in_column && c.focused.as_ref() == Some(&entry.path);
        // What a right-click opened the menu on, which it leaves unselected: ringed, as in Finder, until it closes.
        let target = self
            .menu
            .as_ref()
            .is_some_and(|m| m.index == index && m.paths.as_deref() == Some(std::slice::from_ref(&entry.path)));
        // The item a search result or favorite points at: a bar, which wins over the selection but not
        // over the one in the column worked in. Home marks its pick so while the columns are worked in.
        let barred = (highlighted && (!selected || !marked)) || (kind == Kind::Home && selected && !marked);
        let lit = selected && marked;
        let image = self.icons.get(entry, cx, |this: &mut Colet| &mut this.icons);
        let muted = if lit { p.on_accent_muted } else { p.text_muted };
        let path = entry.path.clone();
        let picture = match (saved, image) {
            (Some(_), _) => icon("search", 16., if lit { p.on_accent } else { p.text_muted }).into_any_element(),
            (None, Some(image)) => img(image).size(px(16.)).flex_none().into_any_element(),
            (None, None) => div().size(px(16.)).flex_none().into_any_element(),
        };
        let menu_path = entry.path.clone();
        let focused_row = (marked && c.focused.as_ref() == Some(&entry.path)).then(|| self.focused_row.clone());
        div()
            .id(SharedString::from(entry.path.clone()))
            // Like Finder, the selection stays: the menu acts on the row clicked, or the whole selection if
            // that is part of it.
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let selected = this.state.columns.get(index).is_some_and(|c| c.selected.contains(&menu_path));
                    let paths = (!selected).then(|| vec![menu_path.clone()]);
                    this.open_menu(index, paths, event.position, window, cx);
                }),
            )
            .relative()
            .when_some(focused_row, |r, cell| {
                r.child(canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {}).absolute().size_full())
            })
            .h(px(row_height(kind)))
            .w_full()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .role(Role::ListBoxOption)
            .aria_label(entry.name.clone())
            .aria_selected(selected)
            .border_2()
            .border_color(transparent_black())
            .when(focused, |r| r.border_color(p.focus))
            .when(target, |r| r.border_color(p.accent))
            .when(barred, |r| {
                r.child(div().absolute().left_0().top_0().bottom_0().w(px(3.)).rounded_l(px(4.)).bg(p.accent))
            })
            .when(selected, |r| r.bg(if marked { p.accent_fill } else { p.selected_inactive }))
            .when(lit, |r| r.text_color(p.on_accent))
            .when(barred, |r| r.bg(p.highlight))
            .when(!selected && !highlighted, |r| r.hover(|r| r.bg(p.hover)))
            .when(faded, |r| r.text_color(p.text_muted))
            // The selection in home while the columns are worked in: marked at its edge, as in a sidebar.
            .child(div().when(faded, |d| d.opacity(if cut { 0.35 } else { 0.6 })).child(picture))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .w_full()
                            .truncate()
                            .when(barred && highlighted, |d| d.text_color(p.accent).font_weight(FontWeight::SEMIBOLD))
                            .child(entry.name.clone()),
                    )
                    .when(kind == Kind::Search, |d| {
                        d.child(div().w_full().overflow_hidden().whitespace_nowrap().text_ellipsis_start().text_size(px(12.)).text_color(muted).child(entry.parent.clone()))
                    }),
            )
            // Out of the name, so type-ahead stays the file's own: a fade alone would read as hidden.
            .when(held, |r| {
                r.child(
                    div()
                        .id(SharedString::from(format!("held:{}", entry.path)))
                        .tip(p, if cut { "切り取り中" } else { "コピー中" })
                        .child(icon(if cut { "scissors" } else { "copy" }, 12., if lit { p.on_accent_muted } else if cut { p.text_faint } else { p.accent }))
                        .test_support(),
                )
            })
            // A folder has no size of its own; like Explorer, leave it blank.
            .when(self.sort != Sort::Name && kind != Kind::Home && !(self.sort == Sort::Size && entry.directory), |r| {
                let meta = match self.sort {
                    Sort::Size => time::size(entry.size),
                    _ => entry.modified.and_then(time::date).unwrap_or_else(|| "—".into()),
                };
                r.child(div().flex_none().text_size(px(12.)).text_color(muted).child(meta))
            })
            // A file keeps the chevron's room, so its date lines up with a folder's.
            .when(kind != Kind::Home, |r| {
                r.child(if entry.directory {
                    icon("chevron-right", 13., if lit { p.on_accent_muted } else { p.text_faint }).into_any_element()
                } else {
                    div().w(px(13.)).flex_none().into_any_element()
                })
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                let modifiers = event.modifiers();
                if event.click_count() >= 2 {
                    this.open(index, &path, cx);
                } else if modifiers.control {
                    this.state.toggle(index, &path);
                    this.state.set_active(index);
                    this.changed(cx);
                } else if modifiers.shift {
                    this.state.extend(index, &path);
                    this.state.set_active(index);
                    this.changed(cx);
                } else {
                    this.pick(index, path.clone(), window, cx);
                }
            }))
            .test_support()
            .into_any_element()
    }

    fn message(&self, icon_name: &'static str, text: String) -> Div {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .px_4()
            .py_8()
            .text_size(px(12.))
            .text_color(p.text_muted)
            .child(icon(icon_name, 28., p.text_faint))
            .child(div().text_center().child(text))
    }

    fn loading(&self) -> Div {
        let p = self.palette;
        div()
            .flex()
            .items_center()
            .gap_2()
            .px_4()
            .py_3()
            .text_size(px(12.))
            .text_color(p.text_muted)
            .child(spinner("loading", 15., p.text_muted))
            .child("読み込み中…")
    }

    /// What a column lists, or why it lists nothing.
    fn column_body(&mut self, index: usize, marked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let c = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        if let Some(error) = &c.error {
            return self
                .message("circle-alert", error.clone())
                .child(
                    div()
                        .id(("retry", id))
                        .h(px(30.))
                        .px_3()
                        .flex()
                        .items_center()
                        .gap_2()
                        .rounded(px(6.))
                        .border_1()
                        .border_color(p.border_strong)
                        .bg(p.raised)
                        .cursor_pointer()
                        .hover(|b| b.bg(p.hover))
                        .child(icon("refresh-cw", 14., p.text_muted))
                        .child("再試行")
                        .on_click(cx.listener(move |this, _, _, cx| this.load(id, 0, false, cx)))
                        .test_support(),
                )
                .when(kind == Kind::Search, |d| {
                    d.child(
                        div()
                            .id("search-settings")
                            .h(px(30.))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_2()
                            .rounded(px(6.))
                            .border_1()
                            .border_color(p.border_strong)
                            .bg(p.raised)
                            .cursor_pointer()
                            .hover(|b| b.bg(p.hover))
                            .child(icon("sliders-horizontal", 14., p.text_muted))
                            .child("設定")
                            .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx)))
                            .test_support(),
                    )
                })
                .into_any_element();
        }
        if c.unread() {
            return self.loading().into_any_element();
        }
        if c.entries.is_empty() {
            let (name, text) = match kind {
                Kind::Search => ("search", "一致する項目はありません"),
                Kind::Home => ("hard-drive", "ドライブもお気に入りもありません"),
                Kind::Folder => ("folder", "このフォルダーは空です"),
            };
            return self.message(name, text.into()).into_any_element();
        }
        if kind == Kind::Home {
            // A handful of places: no need to virtualize. The kinds stand under headings, so they don't
            // read as one list.
            let entries = c.entries.clone();
            let favorite = |e: &&Entry| self.settings.is_favorite(&e.path);
            let groups: Vec<(&str, Vec<Entry>)> = [
                ("お気に入り", entries.iter().filter(favorite).cloned().collect::<Vec<_>>()),
                ("保存した検索", entries.iter().filter(|e| saved_query(&e.path).is_some()).cloned().collect()),
                (
                    "ドライブ",
                    entries.iter().filter(|e| saved_query(&e.path).is_none() && !favorite(e)).cloned().collect(),
                ),
            ]
            .into_iter()
            .filter(|(_, items)| !items.is_empty())
            .collect();
            let mut rows = div()
                .id("home-rows")
                .role(Role::ListBox)
                .aria_label("ホーム")
                .size_full()
                .overflow_y_scroll()
                .track_scroll(&self.home_scroll);
            for (title, items) in groups {
                rows = rows.child(
                    div()
                        .h(px(28.))
                        .px_3()
                        .pb(px(6.))
                        .flex()
                        .items_end()
                        .text_size(px(12.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(p.text_muted)
                        .child(title),
                );
                for entry in &items {
                    rows = rows.child(div().px_1().child(self.row(0, entry, marked, cx)));
                }
            }
            return div()
                .flex_1()
                .min_h_0()
                .relative()
                .child(rows)
                .child(div().absolute().inset_0().child(scrollbar(gpui_kit::base::Scrollbar::vertical(&self.home_scroll), p)))
                .into_any_element();
        }
        let count = c.entries.len();
        let view = cx.entity();
        let handle = self.list_handle(id);
        let list = uniform_list(("list", id), count, move |range, _, cx| {
            view.update(cx, |this, cx| {
                let Some(index) = this.state.index_of(id) else { return Vec::new() };
                let c = &this.state.columns[index];
                // The next page of results, once the list nears the end of what it has.
                if c.kind == Kind::Search && range.end + 50 >= c.entries.len() && c.entries.len() < c.total {
                    let view = cx.entity();
                    cx.defer(move |cx| view.update(cx, |this, cx| this.more(id, cx)));
                }
                let entries: Vec<Entry> = c.entries[range.start.min(c.entries.len())..range.end.min(c.entries.len())].to_vec();
                entries.iter().map(|entry| this.row(index, entry, marked, cx)).collect()
            })
        })
        .track_scroll(&handle)
        .size_full()
        .p_1();
        let title = self.state.columns[index].title().to_owned();
        div()
            .id(("rows", id))
            .role(Role::ListBox)
            .aria_label(title)
            .flex_1()
            .min_h_0()
            .relative()
            .child(list)
            .child(div().absolute().inset_0().child(scrollbar(gpui_kit::base::Scrollbar::vertical(&handle), p)))
            .into_any_element()
    }

    /// The edge a drag widens a column from.
    fn resizer(&self, id: &'static str, of: WidthOf, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .id(id)
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(6.))
            .cursor_col_resize()
            .hover(|d| d.bg(p.accent.opacity(0.4)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.resizing = Some(Resize { of, start_x: event.position.x, start_width: width });
                    cx.stop_propagation();
                }),
            )
            .test_support()
            .into_any_element()
    }

    fn render_column(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let marked = index == self.state.active && !self.search_focused;
        let c: &Column = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        let (width, of) = match kind {
            Kind::Search => (self.widths.search, WidthOf::Search),
            _ => (self.widths.column, WidthOf::Column),
        };
        let title = c.title().to_owned();
        let header_title = title.clone();
        let count = if c.error.is_some() || c.unread() { String::new() } else { c.total.to_string() };
        let path = c.path.clone();
        let star_path = c.path.clone();
        let star_focus = self.stop(format!("star:{id}"), cx);
        let body = self.column_body(index, marked, cx);
        div()
            .id(("column", id))
            .role(Role::Group)
            .aria_label(format!("{title} カラム"))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if let Some(index) = this.state.index_of(id) {
                        this.open_menu(index, Some(Vec::new()), event.position, window, cx);
                    }
                }),
            )
            .w(px(width))
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
                    .h(px(34.))
                    .flex_none()
                    .pl_3()
                    .pr_2()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(p.border)
                    .when(marked, |d| d.border_b_2().border_color(p.accent))
                    .child(
                        div()
                            .id(("title", id))
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if marked { p.text } else { p.text_muted })
                            .when(!path.is_empty(), |d| d.tip(p, path))
                            .child(header_title),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_faint).child(count))
                    .child({
                        let kept = match kind {
                            Kind::Search => self.settings.saved(&self.state.query).is_some(),
                            _ => self.settings.is_favorite(&star_path),
                        };
                        let label = match (kind, kept) {
                            (Kind::Search, true) => "保存した検索を解除（Ctrl+D）",
                            (Kind::Search, false) => "検索を保存（Ctrl+D）",
                            (_, true) => "お気に入りを解除（Ctrl+D）",
                            (_, false) => "お気に入りに追加（Ctrl+D）",
                        };
                        ring(div().id(("star", id)).track_focus(&star_focus), p)
                            .role(Role::Button)
                            .aria_label(label)
                            .size(px(24.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.hover))
                            .tip(p, label)
                            .child(icon(if kept { "star-fill" } else { "star" }, 14., if kept { p.accent } else { p.text_muted }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if kind == Kind::Search {
                                    let query = this.state.query.clone();
                                    this.toggle_saved(&query, cx);
                                } else {
                                    this.toggle_favorite(&star_path, cx);
                                }
                            }))
                            .test_support()
                    }),
            )
            .child(body)
            .child(self.resizer(if kind == Kind::Search { "search-resizer" } else { "column-resizer" }, of, width, cx))
            .test_support()
            .into_any_element()
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (back, forward) = (self.state.can_go_back(), self.state.can_go_forward());
        let marked = self.state.active == 0 && !self.search_focused;
        let body = self.column_body(0, marked, cx);
        let width = self.widths.sidebar;
        let keys = div().id("home").track_focus(&self.home_focus).flex_1().min_h_0().flex().flex_col().child(body);
        let keys = self.column_keys(keys, cx);
        div()
            .id("sidebar")
            .w(px(width))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .child(
                div()
                    .h(px(48.))
                    .flex_none()
                    .when(marked, |d| d.border_b_2().border_color(p.accent))
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        self.icon_button("back", "arrow-left", "戻る", back, cx)
                            .tip(p, "戻る（Alt+←）")
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                            .test_support(),
                    )
                    .child(
                        self.icon_button("forward", "arrow-right", "進む", forward, cx)
                            .tip(p, "進む（Alt+→）")
                            .on_click(cx.listener(|this, _, window, cx| this.forward(window, cx)))
                            .test_support(),
                    ),
            )
            .child(keys)
            .child(self.resizer("sidebar-resizer", WidthOf::Sidebar, width, cx))
            .into_any_element()
    }

    fn render_toolbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focused = self.search_focused;
        let has_query = !self.state.query.is_empty();
        let sort = self.sort;
        let clear_focus = self.stop("clear-search", cx);
        let sort_lines = if self.sort_open { self.render_lines(cx) } else { Vec::new() };
        div()
            .h(px(48.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("search-box")
                    .flex_1()
                    .min_w_0()
                    .h(px(32.))
                    .pl(px(10.))
                    .pr(px(4.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(p.surface)
                    .border_1()
                    .border_color(if focused { p.accent } else { p.border_strong })
                    .rounded(px(6.))
                    .child(icon("search", 15., if focused { p.accent } else { p.text_muted }))
                    .child(div().flex_1().min_w_0().child(Input::new(&self.search)))
                    // ↓ goes into the results, as from a search field in Explorer; Esc clears the search.
                    .capture_action(cx.listener(|this, _: &input::MoveDown, window, cx| {
                        cx.stop_propagation();
                        this.leave_search(window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &input::Escape, window, cx| {
                        if !this.state.query.is_empty() {
                            this.clear_search(window, cx);
                        }
                    }))
                    .when(has_query, |b| {
                        b.child(
                            ring(div().id("clear-search").track_focus(&clear_focus), p)
                                .role(Role::Button)
                                .aria_label("検索をクリア")
                                .size(px(24.))
                                .flex()
                                .items_center()
                                .justify_center()
                                .rounded(px(4.))
                                .cursor_pointer()
                                .hover(|d| d.bg(p.hover))
                                .tip(p, "検索をクリア（Esc）")
                                .child(icon("x", 14., p.text_muted))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.clear_search(window, cx);
                                    this.search.update(cx, |input, cx| input.focus(window, cx));
                                })),
                        )
                    }),
            )
            .child(
                div()
                    .relative()
                    .child(
                        div()
                            .id("sort")
                            .track_focus(&self.sort_focus)
                            .key_context(SORT)
                            .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_sort(true, window, cx)))
                            .focus_visible(|s| s.border_color(p.focus))
                            .h(px(32.))
                            .pl(px(10.))
                            .pr_2()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .text_size(px(12.))
                            .text_color(p.text_muted)
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border_strong)
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.hover))
                            .child(sort.label())
                            .child(icon("chevron-down", 14., p.text_faint))
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.sort_open {
                                    this.close_menus(true, window, cx);
                                } else {
                                    this.open_sort(false, window, cx);
                                }
                            }))
                            .test_support(),
                    )
                    .when(self.sort_open, |d| {
                        d.child(
                            deferred(
                                div()
                                    .id("sort-menu")
                                    .track_focus(&self.menu_focus)
                                    .key_context(MENU)
                                    .on_action(cx.listener(|this, _: &MenuUp, _, cx| this.move_cursor(-1, cx)))
                                    .on_action(cx.listener(|this, _: &MenuDown, _, cx| this.move_cursor(1, cx)))
                                    .on_action(cx.listener(|this, _: &MenuConfirm, window, cx| this.confirm_cursor(window, cx)))
                                    .on_action(cx.listener(|this, _: &CloseMenu, window, cx| this.close_menus(true, window, cx)))
                                    .absolute()
                                    .top(px(36.))
                                    .right_0()
                                    .w(px(140.))
                                    .p_1()
                                    .bg(p.raised)
                                    .border_1()
                                    .border_color(p.border_strong)
                                    .rounded(px(8.))
                                    .shadow_lg()
                                    .occlude()
                                    .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_menus(false, window, cx)))
                                    .children(sort_lines),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    .child(
                        self.icon_button("refresh", "refresh-cw", "更新", true, cx)
                            .tip(p, "更新（F5）")
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx)))
                            .test_support(),
                    )
                    .child(
                        self.icon_button("preferences", "sliders-horizontal", "設定", true, cx)
                            .tip(p, "設定")
                            .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx)))
                            .test_support(),
                    ),
            )
            .into_any_element()
    }

    /// The trail from the search or the drive down to the deepest folder; when it outgrows the bar, the
    /// crumbs furthest from the deepest fold into a menu so the nearest stay readable.
    fn render_pathbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let file_actions = self.render_actions(cx);
        let p = self.palette;
        let results = self.state.pinned();
        let mut crumbs: Vec<(usize, String, String)> = Vec::new();
        if results {
            crumbs.push((1, "検索結果".into(), self.state.query.clone()));
        }
        for (index, c) in self.state.columns.iter().enumerate().filter(|(_, c)| c.kind == Kind::Folder) {
            crumbs.push((index, folder_name(&c.path).to_owned(), c.path.clone()));
        }
        let active = self.state.active;
        // Each crumb's whole width as drawn: its 12px name, padding, the search icon on the results' own,
        // the chevron before it and the gap after.
        let font = font("Segoe UI");
        let widths: Vec<f32> = crumbs
            .iter()
            .enumerate()
            .map(|(k, (_, label, _))| {
                let run = TextRun { len: label.len(), font: font.clone(), color: p.text, background_color: None, underline: None, strikethrough: None };
                let shaped = window.text_system().shape_line(label.clone().into(), px(12.), &[run], None);
                let icon = if results && k == 0 { 12. + 4. } else { 0. };
                // The first stands at the bar's start, with no chevron or gap before it.
                let before = if k == 0 { 0. } else { crate::crumbs::SEPARATOR + crate::crumbs::GAP };
                f32::from(shaped.width).ceil() + 12. + icon + before
            })
            .collect();
        // The room the bar was last drawn with; before the first frame, what the window leaves it.
        let room = match self.crumbs_row.get() {
            Some(row) => f32::from(row.size.width),
            None => f32::from(window.viewport_size().width) - self.widths.sidebar - 26. - 16. - file_actions.0,
        };
        self.crumbs_room.set(room);
        let folded = crate::crumbs::folded(&widths, room);
        let last = crumbs.len().saturating_sub(1);
        self.crumb_bounds.borrow_mut().retain(|index, _| crumbs[folded..].iter().any(|c| c.0 == *index));
        for (index, ..) in &crumbs[folded..] {
            self.stop(format!("crumb:{index}"), cx);
        }
        let menu: Vec<(usize, String, String)> = crumbs[..folded].to_vec();
        let shown: Vec<(usize, String, String)> = crumbs[folded..].to_vec();
        let menu_current = menu.iter().any(|(index, ..)| *index == active);
        let crumb_menu_open = self.crumb_menu_open;
        div()
            .h(px(36.))
            .flex_none()
            .pl(px(14.))
            .pr_3()
            .flex()
            .items_center()
            .gap_4()
            .border_b_1()
            .border_color(p.border)
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .h_full()
                    .relative()
                    .items_center()
                    .gap(px(2.))
                    .min_w_0()
                    .overflow_hidden()
                    .child({
                        let (row, room) = (self.crumbs_row.clone(), self.crumbs_room.clone());
                        canvas(
                            move |bounds, window, _| {
                                row.set(Some(bounds));
                                if (f32::from(bounds.size.width) - room.get()).abs() > 0.5 {
                                    window.request_animation_frame();
                                }
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    })
                    .children(self.crumb_bar())
                    .when(folded > 0, |d| {
                        d.child(
                            div()
                                .relative()
                                .child(
                                    div()
                                        .id("crumb-fold")
                                        .size(px(24.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .hover(|d| d.bg(p.hover))
                                        .tip(p, "上の階層")
                                        .child(icon("ellipsis", 14., if menu_current { p.text } else { p.text_muted }))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.crumb_menu_open = !this.crumb_menu_open;
                                            cx.notify();
                                        })),
                                )
                                .when(crumb_menu_open, |d| {
                                    d.child(
                                        deferred(
                                            div()
                                                .id("crumb-menu")
                                                .absolute()
                                                .top(px(28.))
                                                .left_0()
                                                .min_w(px(160.))
                                                .p_1()
                                                .bg(p.raised)
                                                .border_1()
                                                .border_color(p.border_strong)
                                                .rounded(px(8.))
                                                .shadow_lg()
                                                .occlude()
                                                .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                                    this.crumb_menu_open = false;
                                                    cx.notify();
                                                }))
                                                .children(menu.iter().map(|(index, label, path)| {
                                                    let index = *index;
                                                    div()
                                                        .id(("crumb-item", index))
                                                        .h(px(28.))
                                                        .px(px(10.))
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(if index == active { p.text } else { p.text_muted })
                                                        .hover(|d| d.bg(p.accent_fill).text_color(p.on_accent))
                                                        .tip(p, path.clone())
                                                        .child(icon(if index == 1 && results { "search" } else { "folder" }, 14., p.text_faint))
                                                        .child(label.clone())
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            this.crumb_menu_open = false;
                                                            this.go_to(index, window, cx);
                                                        }))
                                                })),
                                        )
                                        .with_priority(1),
                                    )
                                }),
                        )
                    })
                    .children(shown.into_iter().enumerate().map(|(k, (index, label, path))| {
                        let current = index == active;
                        let query = results && index == 1;
                        let crumb_focus = self.stops.get(&format!("crumb:{index}")).cloned().unwrap_or_else(|| self.columns_focus.clone());
                        let is_last = folded + k == last;
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .when(is_last, |d| d.min_w_0().max_w(px(room / 2.)))
                            .when(!is_last, |d| d.flex_none())
                            .when(k > 0 || folded > 0, |d| d.child(icon("chevron-right", 12., p.text_faint)))
                            .child(
                                ring(div().id(("crumb", index)).track_focus(&crumb_focus), p)
                                    .role(Role::Button)
                                    .aria_label(label.clone())
                                    .relative()
                                    .child({
                                        let crumbs = self.crumb_bounds.clone();
                                        canvas(move |bounds, _, _| {
                                            crumbs.borrow_mut().insert(index, bounds);
                                        }, |_, _, _, _| {})
                                        .absolute()
                                        .size_full()
                                    })
                                    .h(px(24.))
                                    .px(px(6.))
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .min_w_0()
                                    .rounded(px(4.))
                                    .cursor_pointer()
                                    .text_color(if current { p.text } else { p.text_muted })
                                    .hover(|d| d.bg(p.hover))
                                    .tip(p, path)
                                    .when(query, |d| d.child(icon("search", 12., p.text_faint)))
                                    .child(div().truncate().child(label))
                                    .on_click(cx.listener(move |this, _, window, cx| this.go_to(index, window, cx)))
                                    .test_support(),
                            )
                    })),
            )
            .child(file_actions.1)
            .into_any_element()
    }

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let pinned = self.state.pinned();
        let away = self.state.folded_away();
        let first = if pinned { 2 } else { 1 } + away;
        let only_home = self.state.columns.len() == 1;
        let unfold_focus = self.stop("unfold", cx);
        let count = self.state.columns.len().saturating_sub(first);
        let content = count as f32 * self.widths.column;
        let view = f32::from(self.columns_scroll.bounds().size.width);
        let screen = self.on_screen();
        let (hidden_left, hidden_right) = (screen.hidden_left(), screen.hidden_right());
        // Room past the last column, so the scroll width cannot shrink under where the columns were brought.
        let hold = (self.hold + view - content).max(0.);
        let pinned_column = pinned.then(|| self.render_column(1, cx));
        let columns: Vec<AnyElement> = (first..self.state.columns.len()).map(|i| self.render_column(i, cx)).collect();
        let main = div()
            .id("main")
            .track_focus(&self.columns_focus)
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .bg(p.bg)
            .children(pinned_column)
            .when(away > 0, |d| {
                d.child(
                    ring(div().id("unfold").track_focus(&unfold_focus), p)
                        .role(Role::Button)
                        .aria_label(format!("上の階層を表示（{away}階層）"))
                        .w(px(FOLD_WIDTH))
                        .h_full()
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(p.surface)
                        .border_r_1()
                        .border_color(p.border)
                        .cursor_pointer()
                        .hover(|d| d.bg(p.hover))
                        .tip(p, format!("上の階層を表示（{away}階層）"))
                        .child(icon("chevrons-left", 14., p.text_muted))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.state.unfold();
                            this.changed(cx);
                        }))
                        .test_support(),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .relative()
                    .child(
                        div()
                            .id("columns-scroll")
                            .size_full()
                            .overflow_x_scroll()
                            .track_scroll(&self.columns_scroll)
                            // The crumbs' bar and the edges' shades follow the columns as they scroll.
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .child(div().h_full().w(px(content + hold)).flex().children(columns)),
                    )
                    .child(
                        div()
                            .absolute()
                            .inset_0()
                            .child(scrollbar(gpui_kit::base::Scrollbar::horizontal(&self.columns_scroll), p)),
                    )
                    .when(hidden_left, |d| {
                        d.child(
                            div().absolute().top_0().bottom_0().left_0().w(px(28.)).bg(linear_gradient(
                                90.,
                                linear_color_stop(p.shade, 0.),
                                linear_color_stop(p.shade.opacity(0.), 1.),
                            )),
                        )
                    })
                    .when(hidden_right, |d| {
                        d.child(
                            div().absolute().top_0().bottom_0().right_0().w(px(28.)).bg(linear_gradient(
                                270.,
                                linear_color_stop(p.shade, 0.),
                                linear_color_stop(p.shade.opacity(0.), 1.),
                            )),
                        )
                    }),
            )
            // Nothing searched or opened leaves this side dark; show where to start.
            .when(only_home, |d| d.child(self.home_hint(cx)));
        let _ = window;
        self.column_keys(main, cx).into_any_element()
    }

    fn home_hint(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .text_size(px(12.))
            .text_color(p.text_muted)
            .child(icon("search", 28., p.text_faint))
            .child("名前やEverythingの検索式で探せます")
            .child(div().flex().gap_2().children(
                [("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")].map(
                    |(query, label)| {
                        div()
                            .id(query)
                            .h(px(28.))
                            .px(px(10.))
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .bg(p.surface)
                            .border_1()
                            .border_color(p.border)
                            .rounded(px(6.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.hover))
                            .child(div().text_color(p.accent).font_family("Consolas").child(query))
                            .child(label)
                            .on_click(cx.listener(move |this, _, window, cx| this.search_for(query, window, cx)))
                            .test_support()
                    },
                ),
            ))
            .into_any_element()
    }

    fn render_statusbar(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let history = self.history_button(cx);
        let active = &self.state.columns[self.state.active];
        let place = match active.kind {
            Kind::Folder => active.path.clone(),
            Kind::Search if active.error.is_some() => "検索できません".into(),
            Kind::Search if active.unread() => "検索中…".into(),
            Kind::Search => "検索結果".into(),
            Kind::Home => "ホーム".into(),
        };
        let picked: Vec<&Entry> = active.selected.iter().filter_map(|path| active.entry(path)).collect();
        let detail = match picked.as_slice() {
            _ if active.selected.len() > 1 => format!("{} 項目を選択", active.selected.len()),
            [entry] => {
                let what = if entry.directory { "フォルダー".into() } else { time::size(entry.size) };
                match entry.modified.and_then(time::date_time) {
                    Some(when) => format!("{what} · {when}"),
                    None => what,
                }
            }
            _ => String::new(),
        };
        let (name, color, dot, title) = match &self.engine {
            Engine::Unknown => ("Everything 1.5".to_owned(), p.text_faint, None, "検索エンジン: Everything 1.5（検索すると接続します）".to_owned()),
            Engine::Connected(name) => (name.clone(), p.text_faint, Some(p.accent), format!("検索エンジン: {name}（接続済み）")),
            Engine::Failed(error) => ("Everything 1.5 · 要確認".to_owned(), p.danger, Some(p.danger), error.clone()),
        };
        div()
            .h(px(26.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_4()
            .text_size(px(12.))
            .text_color(p.text_muted)
            .border_t_1()
            .border_color(p.border)
            .child(div().id("place").role(Role::Status).aria_label(place.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis_start().child(place))
            .child(div().flex_none().child(detail))
            .child(
                div()
                    .id("engine")
                    .flex_none()
                    .max_w(px(200.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(color)
                    .tip(p, title)
                    .child(
                        div()
                            .size(px(6.))
                            .rounded_full()
                            .border_1()
                            .border_color(dot.unwrap_or(p.text_faint))
                            .when_some(dot, |d, c| d.bg(c)),
                    )
                    .child(div().truncate().child(name)),
            )
            .child(history)
            .into_any_element()
    }

    fn render_toasts(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .absolute()
            .bottom(px(36.))
            .right_3()
            .flex()
            .flex_col()
            .gap_2()
            .children(self.toasts.iter().map(|toast| {
                let id = toast.id;
                let error = toast.tone == Tone::Error;
                div()
                    .id(("toast", id))
                    .role(if error { Role::Alert } else { Role::Status })
                    .aria_label(toast.text.clone())
                    .max_w(px(420.))
                    .pl_3()
                    .pr_1()
                    .py_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_size(px(12.))
                    .bg(p.raised)
                    .border_1()
                    .border_color(if error { p.danger } else { p.border_strong })
                    .rounded(px(8.))
                    .shadow_lg()
                    .occlude()
                    .when(error, |d| d.child(icon("circle-alert", 14., p.danger)))
                    .child(div().flex_1().py_1().child(toast.text.clone()))
                    .when_some(toast.entry, |d, entry| {
                        d.child(
                            div()
                                .id(("toast-details", id))
                                .px_2()
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .text_color(p.accent)
                                .cursor_pointer()
                                .hover(|d| d.bg(p.hover))
                                .child("詳細")
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.dismiss(id, cx);
                                    this.show_history(true, Some(entry), window, cx);
                                }))
                                .test_support(),
                        )
                    })
                    .child(
                        div()
                            .id(("close-toast", id))
                            .size(px(24.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.hover))
                            .child(icon("x", 14., p.text_muted))
                            .on_click(cx.listener(move |this, _, _, cx| this.dismiss(id, cx)))
                            .test_support(),
                    )
            }))
            .into_any_element()
    }

    /// Slides the columns toward where they should rest, a frame at a time.
    fn reveal(&mut self, window: &mut Window) {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        let view = f32::from(self.columns_scroll.bounds().size.width);
        let from = -f32::from(self.columns_scroll.offset().x);
        if let Some(smooth) = self.reveal_pending.take() {
            if view <= 0. {
                // Not laid out yet: ask again next frame.
                self.reveal_pending = Some(smooth);
                window.request_animation_frame();
                return;
            }
            let cells: Vec<Cell> =
                (0..count).map(|i| Cell { start: i as f32 * self.widths.column, width: self.widths.column }).collect();
            let active = self.state.active.checked_sub(first);
            let start = self.slide.as_ref().map_or(from, |s| s.target);
            let target = reveal_target(start, view, &cells, active);
            self.hold = target.max(from);
            if (target - from).abs() >= 0.5 {
                match &mut self.slide {
                    Some(slide) => slide.target = target,
                    None => self.slide = Some(Slide { position: from, target, velocity: 0., time: None }),
                }
            }
        }
        let Some(slide) = &mut self.slide else { return };
        // Critically damped: settles in about 350ms without overshooting. A frame held up counts as at
        // most two, so the slide slows there rather than leaping most of the way.
        let stiffness = 27.;
        let now = Instant::now();
        let t = slide.time.map_or(1. / 60., |then| (now - then).as_secs_f32().clamp(0., 1. / 30.));
        slide.time = Some(now);
        let x = slide.position - slide.target;
        let k = slide.velocity + stiffness * x;
        let decay = (-stiffness * t).exp();
        slide.position = slide.target + (x + k * t) * decay;
        slide.velocity = (slide.velocity - stiffness * k * t) * decay;
        let done = (slide.position - slide.target).abs() < 0.5 && slide.velocity.abs() < 20.;
        let position = if done { slide.target } else { slide.position };
        self.columns_scroll.set_offset(point(px(-position), px(0.)));
        if done {
            self.slide = None;
        } else {
            window.request_animation_frame();
        }
    }
}

impl Render for Colet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.search_focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        self.follow_focus(window);
        self.scroll_rows();
        self.reveal(window);
        let p = self.palette;
        let sidebar = self.render_sidebar(cx);
        let toolbar = self.render_toolbar(cx);
        let pathbar = self.render_pathbar(window, cx);
        let main = self.render_main(window, cx);
        div()
            .id("colet")
            .size_full()
            .relative()
            .flex()
            .bg(p.bg)
            .text_color(p.text)
            .font_family("Segoe UI")
            .text_size(px(13.))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &RefreshAll, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &SaveCurrent, _, cx| {
                if this.dialog.is_none() {
                    this.save_current(cx);
                }
            }))
            // The mouse's back and forward buttons, as Alt+← and Alt+→.
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Back),
                cx.listener(|this, _, window, cx| this.back(window, cx)),
            )
            .on_mouse_down(
                MouseButton::Navigate(NavigationDirection::Forward),
                cx.listener(|this, _, window, cx| this.forward(window, cx)),
            )
            .when(self.resizing.is_some(), |d| {
                d.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        let Some(resize) = &this.resizing else { return };
                        let (min, max) = if resize.of == WidthOf::Sidebar { SIDEBAR_WIDTHS } else { COLUMN_WIDTHS };
                        // Display scaling makes the deltas fractional; widths stay whole pixels.
                        let width = (resize.start_width + f32::from(event.position.x - resize.start_x)).clamp(min, max).round();
                        match resize.of {
                            WidthOf::Sidebar => this.widths.sidebar = width,
                            WidthOf::Search => this.widths.search = width,
                            WidthOf::Column => this.widths.column = width,
                        }
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.end_resize(cx);
                            cx.notify();
                        }),
                    )
            })
            .child(sidebar)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(toolbar)
                    .child(pathbar)
                    .child(main)
                    .child(self.render_statusbar(cx)),
            )
            .children(self.render_menu(cx))
            .children(self.render_history(cx))
            .children(self.render_dialog(window, cx))
            .child(self.render_toasts(cx))
    }
}

