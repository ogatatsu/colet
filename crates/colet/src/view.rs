//! How the window looks: home's pane on the left; the search field, the trail of folders, the columns
//! and the status bar beside it.
use std::time::Instant;

use colet_core::{entry::{Entry, Sort}, paths::folder_name, time};
use gpui_kit::base::input::{self, Input};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    places::HomeMenu,
    state::saved_query,
    reveal::{Cell, on_screen, reveal_target},
    state::{Column, Kind},
};

/// A line of text shown while the pointer rests on an element.
struct Tip {
    text: SharedString,
    palette: crate::theme::Palette,
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

fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

/// Rows of home and the folders; search results carry their folder on a second line.
fn row_height(kind: Kind) -> f32 {
    if kind == Kind::Search { 42. } else { 28. }
}

impl Colet {
    fn icon_button(&self, id: &'static str, name: &'static str, enabled: bool) -> Stateful<Div> {
        let p = self.palette;
        div()
            .id(id)
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
            .on_action(cx.listener(|this, _: &EditItem, window, cx| this.edit_item(window, cx)))
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
        let kept = kind == Kind::Home && (saved.is_some() || self.settings.is_favorite(&entry.path));
        let focused = marked && c.focused.as_ref() == Some(&entry.path) && !selected;
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
        div()
            .id(SharedString::from(entry.path.clone()))
            // A favorite or saved search in home is taken off from its menu.
            .when(kept, |r| {
                r.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        this.home_menu = Some(HomeMenu { path: menu_path.clone(), at: event.position });
                        cx.notify();
                    }),
                )
            })
            .h(px(row_height(kind)))
            .w_full()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(transparent_black())
            .when(focused, |r| r.border_color(p.accent))
            .when(selected, |r| r.bg(if marked { p.accent_fill } else { p.selected_inactive }))
            .when(lit, |r| r.text_color(p.on_accent))
            .when(!selected && highlighted, |r| r.bg(p.highlight))
            .when(!selected && !highlighted, |r| r.hover(|r| r.bg(p.hover)))
            .when(entry.hidden && !selected, |r| r.text_color(p.text_muted))
            // The selection in home while the columns are worked in: marked at its edge, as in a sidebar.
            .when(kind == Kind::Home && selected && !marked, |r| r.bg(p.highlight).border_l_2().border_color(p.accent))
            .child(div().when(entry.hidden && !selected, |d| d.opacity(0.6)).child(picture))
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
                            .when(highlighted && !selected, |d| d.text_color(p.accent).font_weight(FontWeight::SEMIBOLD))
                            .child(entry.name.clone()),
                    )
                    .when(kind == Kind::Search, |d| {
                        d.child(div().w_full().truncate().text_size(px(12.)).text_color(muted).child(entry.parent.clone()))
                    }),
            )
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
                if event.click_count() >= 2 {
                    this.open(index, &path, cx);
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
            .child(icon("loader-circle", 15., p.text_muted))
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
            let mut rows = div().id("home-rows").flex_1().overflow_y_scroll();
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
            return rows.into_any_element();
        }
        let count = c.entries.len();
        let view = cx.entity();
        let handle = self.list_handle(id);
        uniform_list(("list", id), count, move |range, _, cx| {
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
        .flex_1()
        .p_1()
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
        let count = if c.error.is_some() || c.unread() { String::new() } else { c.total.to_string() };
        let path = c.path.clone();
        let star_path = c.path.clone();
        let body = self.column_body(index, marked, cx);
        div()
            .id(("column", id))
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
                            .child(title),
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
                        div()
                            .id(("star", id))
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
                    .px_3()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        self.icon_button("back", "arrow-left", back)
                            .tip(p, "戻る（Alt+←）")
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)))
                            .test_support(),
                    )
                    .child(
                        self.icon_button("forward", "arrow-right", forward)
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
                            div()
                                .id("clear-search")
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
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.sort_open = !this.sort_open;
                                cx.notify();
                            }))
                            .test_support(),
                    )
                    .when(self.sort_open, |d| {
                        d.child(
                            deferred(
                                div()
                                    .id("sort-menu")
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
                                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                        this.sort_open = false;
                                        cx.notify();
                                    }))
                                    .children(Sort::ALL.map(|option| {
                                        let chosen = option == sort;
                                        div()
                                            .id(option.label())
                                            .h(px(28.))
                                            .px(px(10.))
                                            .flex()
                                            .items_center()
                                            .rounded(px(4.))
                                            .text_size(px(12.))
                                            .cursor_pointer()
                                            .when(chosen, |d| d.text_color(p.accent))
                                            .hover(|d| d.bg(p.accent_fill).text_color(p.on_accent))
                                            .child(div().flex_1().child(option.label()))
                                            .when(chosen, |d| d.child(icon("check", 14., p.accent)))
                                            .on_click(cx.listener(move |this, _, _, cx| this.set_sort(option, cx)))
                                            .test_support()
                                    })),
                            )
                            .with_priority(1),
                        )
                    }),
            )
            .child(
                self.icon_button("preferences", "sliders-horizontal", true)
                    .tip(p, "設定")
                    .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx)))
                    .test_support(),
            )
            .child(
                self.icon_button("refresh", "refresh-cw", true)
                    .tip(p, "更新（F5）")
                    .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
            )
            .into_any_element()
    }

    /// The trail from the search or the drive down to the deepest folder; when it outgrows the bar, the
    /// crumbs furthest from the deepest fold into a menu so the nearest stay readable.
    fn render_pathbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
        // Measured as drawn: 12px text, its padding, and the separator before it.
        let font = font("Segoe UI");
        let widths: Vec<f32> = crumbs
            .iter()
            .map(|(_, label, _)| {
                let run = TextRun { len: label.len(), font: font.clone(), color: p.text, background_color: None, underline: None, strikethrough: None };
                let shaped = window.text_system().shape_line(label.clone().into(), px(12.), &[run], None);
                f32::from(shaped.width) + 12. + 16.
            })
            .collect();
        let room = f32::from(window.viewport_size().width) - self.widths.sidebar - 26.;
        let mut folded = 0;
        if widths.iter().sum::<f32>() > room && !widths.is_empty() {
            let mut left = room - 30. - widths.last().unwrap().min(room / 2.);
            folded = widths.len() - 1;
            while folded > 0 && widths[folded - 1] <= left {
                folded -= 1;
                left -= widths[folded];
            }
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
            .border_b_1()
            .border_color(p.border)
            .text_size(px(12.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .min_w_0()
                    .overflow_hidden()
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
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .min_w_0()
                            .when(k > 0 || folded > 0, |d| d.child(icon("chevron-right", 12., p.text_faint)))
                            .child(
                                div()
                                    .id(("crumb", index))
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
            .into_any_element()
    }

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let pinned = self.state.pinned();
        let away = self.state.folded_away();
        let first = if pinned { 2 } else { 1 } + away;
        let only_home = self.state.columns.len() == 1;
        let count = self.state.columns.len().saturating_sub(first);
        let content = count as f32 * self.widths.column;
        let view = f32::from(self.columns_scroll.bounds().size.width);
        let from = -f32::from(self.columns_scroll.offset().x);
        let cells: Vec<Cell> = (0..count).map(|i| Cell { start: i as f32 * self.widths.column, width: self.widths.column }).collect();
        let span = on_screen(from, view, &cells);
        let hidden_left = span.is_some_and(|s| s.0 > 0.);
        let hidden_right = span.is_some_and(|s| s.1 < count as f32);
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
                    div()
                        .id("unfold")
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
                            .child(div().h_full().w(px(content + hold)).flex().children(columns)),
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

    fn render_statusbar(&self) -> AnyElement {
        let p = self.palette;
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
            .child(div().id("place").flex_1().min_w_0().truncate().child(place))
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
                    .child(self.render_statusbar()),
            )
            .children(self.render_home_menu(cx))
            .children(self.render_dialog(window, cx))
            .child(self.render_toasts(cx))
    }
}

