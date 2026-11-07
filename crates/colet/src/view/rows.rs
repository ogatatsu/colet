//! Rows of home and the folders; search results carry their folder on a second line.
use super::*;

/// What follows the pointer as a row is dragged in home: its row, faint.
pub struct HomeGhost {
    name: String,
    search: bool,
    image: Option<std::sync::Arc<RenderImage>>,
    width: Pixels,
    palette: crate::theme::Palette,
}

impl Render for HomeGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let picture = match (self.search, self.image.clone()) {
            (true, _) => icon("search", 16., p.text_muted).into_any_element(),
            (false, Some(image)) => img(image).size(px(16.)).flex_none().into_any_element(),
            (false, None) => div().size(px(16.)).flex_none().into_any_element(),
        };
        div()
            .w(self.width)
            .h(px(row_height(Kind::Home)))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(px(4.))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            .opacity(0.8)
            .font_family("Segoe UI")
            .text_size(px(13.))
            .text_color(p.text)
            .child(picture)
            .child(div().min_w_0().truncate().child(self.name.clone()))
    }
}

/// The widths a row's size and modified time take: `1023.9 MB` and `2026/12/31 23:59`, at 12px.
pub(super) const SIZE_WIDTH: f32 = 56.;
pub(super) const MODIFIED_WIDTH: f32 = 96.;
/// What the pointer is on for a row's icon to turn to its checkbox.
const CHECK_GROUP: &str = "check";

fn row_height(kind: Kind) -> f32 {
    if kind == Kind::Search { 42. } else { 28. }
}

/// How a row stands, from the column, the clipboard, the menu and a drag in home; see [`Colet::row_look`].
struct RowLook {
    selected: bool,
    /// A search result or favorite points at it, or it is the saved search running.
    highlighted: bool,
    /// Marked by a bar at its edge.
    barred: bool,
    /// Filled with the accent: selected in the column worked in.
    lit: bool,
    /// Ringed: the keys are on it.
    focused: bool,
    /// What the menu was opened on, apart from the selection.
    target: bool,
    /// The menu acts apart from the selection, which steps back.
    aside: bool,
    /// On the clipboard, and cut there.
    held: bool,
    cut: bool,
    /// Hidden, or cut: drawn faint.
    faded: bool,
    /// Being dragged in home: faint where it was.
    dragged: bool,
    within: Option<String>,
    narrow: Option<String>,
}

impl Colet {
    /// How the row of `entry` in the column at `index` stands, `marked` while that column is worked in.
    fn row_look(&self, index: usize, entry: &Entry, marked: bool) -> RowLook {
        let c = &self.state.columns[index];
        let kind = c.kind;
        let selected = c.selected.contains(&entry.path);
        let saved = saved_query(&entry.path);
        // Home lights the saved search that is running, narrowed as it was kept.
        let highlighted = c.highlight.as_ref() == Some(&entry.path)
            || saved.is_some_and(|(q, f, n)| {
                self.state.pinned()
                    && q == self.state.query
                    && f == self.state.search_filter.as_deref()
                    && n == self.state.search_narrowing().as_deref()
            });
        // On the clipboard, cut or copied, whoever put it there.
        let held = self.clipboard.holds(&entry.path);
        let cut = held && self.clipboard.files.cut;
        // The menu acts on a row outside the selection, or the folder itself: the selection steps back, so
        // only what the menu acts on stands out.
        let aside = self.menu().is_some_and(|m| m.column == c.id && m.paths.is_some());
        RowLook {
            selected,
            highlighted,
            // The item a search result or favorite points at: a bar, which wins over the selection but not
            // over the one in the column worked in. Home marks its pick so while the columns are worked in.
            barred: (highlighted && (!selected || !marked)) || (kind == Kind::Home && selected && !marked),
            lit: selected && marked && !aside,
            // The keys are on it: ringed, selected or not, while they are in this column.
            focused: marked && self.keys_in_column && c.focused.as_ref() == Some(&entry.path),
            // What a right-click opened the menu on, which it leaves unselected: ringed, as in Finder, until
            // it closes.
            target: self.menu().is_some_and(|m| m.column == c.id && m.paths.as_deref() == Some(std::slice::from_ref(&entry.path))),
            aside,
            held,
            cut,
            faded: (entry.hidden.is_hidden() || cut) && !selected,
            dragged: kind == Kind::Home && self.home_drag.carried.as_ref().is_some_and(|d| d.path == entry.path),
            // The filter a saved search runs within, faint beside its name.
            within: saved.and_then(|(_, f, _)| f).map(str::to_owned),
            // What a favorite or saved search opens narrowed to, fainter after it.
            narrow: (kind == Kind::Home)
                .then(|| saved.map(|(_, _, n)| n.map(str::to_owned)).unwrap_or_else(|| narrowed_folder(&entry.path).map(|(_, n)| n)))
                .flatten(),
        }
    }

    pub(super) fn row(&mut self, index: usize, entry: &Entry, marked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let RowLook { selected, highlighted, barred, lit, focused, target, aside, held, cut, faded, dragged, within, narrow } =
            self.row_look(index, entry, marked);
        let c = &self.state.columns[index];
        let kind = c.kind;
        let saved = saved_query(&entry.path);
        // A narrowed favorite's is its folder's: a drive's own, say.
        let image = match narrowed_folder(&entry.path) {
            Some((path, _)) => self.icons.get(&Entry { path, ..entry.clone() }, cx),
            None => self.icons.get(entry, cx),
        };
        let muted = if lit { p.on_accent_muted } else { p.text_muted };
        let path = entry.path.clone();
        // A pinned row, favorite or saved search moves among its kind by dragging; it fades where it was meanwhile.
        // Narrowed, home hides some of its kind, and where among them it would land is not to be seen.
        let drag = (kind == Kind::Home && c.narrow.is_none()).then(|| movable(&self.settings, &entry.path)).flatten().map(|shelf| {
            (HomeDrag { path: entry.path.clone(), shelf }, image.clone(), self.home_drag.rows.clone(), cx.entity().downgrade())
        });
        let name = entry.name.clone();
        let picture = match (saved, image) {
            (Some(_), _) => icon("search", 16., if lit { p.on_accent } else { p.text_muted }).into_any_element(),
            (None, Some(image)) => img(image).size(px(16.)).flex_none().into_any_element(),
            (None, None) => div().size(px(16.)).flex_none().into_any_element(),
        };
        let menu_path = entry.path.clone();
        let tab_path = entry.path.clone();
        let focused_row = (marked && c.focused.as_ref() == Some(&entry.path)).then(|| self.focused_row.clone());
        div()
            .id(SharedString::from(id_text(&entry.path)))
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
            // As in a browser, the middle button opens it in a tab of its own, behind this one.
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.open_in_tab(index, &tab_path, cx);
                }),
            )
            .relative()
            .when_some(focused_row, |r, cell| {
                r.child(canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {}).absolute().size_full())
            })
            .when_some(drag, |r, (drag, image, rows, view)| {
                let drawn = rows.clone();
                let path = drag.path.clone();
                r.child(canvas(move |bounds, _, _| drawn.borrow_mut().push((path.clone(), bounds)), |_, _, _, _| {}).absolute().size_full())
                    .on_drag(drag, move |drag, _, _, cx| {
                        view.update(cx, |this, cx| this.start_home_drag(drag, cx)).ok();
                        let width = rows.borrow().iter().find(|(path, _)| *path == drag.path).map_or(px(200.), |(_, b)| b.size.width);
                        let (name, search, image) = (name.clone(), saved_query(&drag.path).is_some(), image.clone());
                        cx.new(|_| HomeGhost { name, search, image, width, palette: p })
                    })
            })
            .when(dragged, |r| r.opacity(0.4))
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
            .when(focused, |r| r.border_color(if lit { p.focus_on_fill } else { p.focus }))
            .when(target, |r| r.border_color(p.accent).bg(p.highlight))
            .when(barred, |r| r.child(div().absolute().left_0().top_0().bottom_0().w(px(3.)).rounded_l(px(4.)).bg(p.accent)))
            .when(selected, |r| r.bg(if marked && !aside { p.accent_fill } else { p.selected_inactive }))
            .when(lit, |r| r.text_color(p.on_accent))
            .when(barred, |r| r.bg(p.highlight))
            .when(!selected && !highlighted, |r| r.hover(|r| r.bg(p.hover)))
            .when(faded, |r| r.text_color(p.text_muted))
            // The selection in home while the columns are worked in: marked at its edge, as in a sidebar.
            .map(|r| {
                let picture = div().when(faded, |d| d.opacity(if cut { 0.35 } else { 0.6 })).child(picture);
                if kind == Kind::Home { r.child(picture) } else { r.child(self.check(index, entry, selected, lit, picture, cx)) }
            })
            .child(self.row_name(entry, kind, barred && highlighted, lit, within, narrow))
            // Out of the name, so the name stays the file's own: a fade alone would read as hidden.
            .when(held, |r| {
                r.child(
                    div()
                        .id(SharedString::from(format!("held:{}", id_text(&entry.path))))
                        .tip(if cut { "切り取り中" } else { "コピー中" })
                        .child(icon(
                            if cut { "scissors" } else { "copy" },
                            12.,
                            if lit {
                                p.on_accent_muted
                            } else if cut {
                                p.text_faint
                            } else {
                                p.accent
                            },
                        ))
                        .test_support(),
                )
            })
            .when(kind != Kind::Home, |r| r.children(Self::row_cells(entry, lit, muted, p)))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| this.click_row(index, kind, &path, event, window, cx)))
            .test_support()
            .into_any_element()
    }

    /// The icon, which turns to a checkbox while its row is one of several selected or the pointer is on it,
    /// as Explorer's item check boxes do: one alone is the row gone into, not a set being gathered. It takes
    /// the row's edge and the gap after it, to be easy to hit.
    pub(super) fn check(
        &self,
        index: usize,
        entry: &Entry,
        selected: bool,
        lit: bool,
        picture: Div,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = self.palette;
        let path = entry.path.clone();
        let boxed = selected && self.state.columns[index].selected.len() > 1;
        // Ringed on the row's own fill, filled on any other.
        let (fill, mark) = if lit { (p.on_accent, p.accent_fill) } else { (p.accent_fill, p.on_accent) };
        let check = div()
            .size(px(14.))
            .rounded(px(3.))
            .flex()
            .items_center()
            .justify_center()
            .map(|b| if selected { b.bg(fill).child(icon("check", 11., mark)) } else { b.border_1().border_color(p.text_muted) });
        div()
            .id(SharedString::from(format!("check:{}", id_text(&entry.path))))
            .group(CHECK_GROUP)
            .relative()
            .flex_none()
            .h_full()
            .flex()
            .items_center()
            // Out over the row's padding and ring, and the gap to the name.
            .ml(px(-10.))
            .pl(px(10.))
            .mr(px(-8.))
            .pr(px(8.))
            .role(Role::CheckBox)
            .aria_label("選択")
            .aria_toggled(if selected { Toggled::True } else { Toggled::False })
            .child(picture.when(boxed, |d| d.invisible()).group_hover(CHECK_GROUP, |s| s.invisible()))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left(px(10.))
                    .w(px(16.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .when(!boxed, |d| d.invisible().group_hover(CHECK_GROUP, |s| s.visible()))
                    .child(check),
            )
            .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.click_check(index, &path, event, cx);
            }))
            .test_support()
    }

    /// A click on the checkbox of `path`: it adds the row to the selection or takes it out, as Ctrl+click
    /// does, and with Shift adds the rows from the anchor, as Ctrl+Shift+click does. Clicked twice, it
    /// does so twice, opening nothing.
    fn click_check(&mut self, index: usize, path: &std::path::Path, event: &ClickEvent, cx: &mut Context<Self>) {
        if event.modifiers().shift {
            self.state.add_range(index, path);
        } else {
            self.state.toggle(index, path);
        }
        self.state.set_active(index);
        self.changed(cx);
    }

    /// The name, with the filter a saved search runs within and what a favorite or saved search opens
    /// narrowed to beside it, and a search result's folder under it.
    fn row_name(&self, entry: &Entry, kind: Kind, accented: bool, lit: bool, within: Option<String>, narrow: Option<String>) -> Div {
        let p = self.palette;
        let ink = if lit { p.on_accent } else { p.text };
        let muted = if lit { p.on_accent_muted } else { p.text_muted };
        div()
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .when(accented, |d| d.text_color(p.accent).font_weight(FontWeight::SEMIBOLD))
                            .child(with_marks(&entry.name, &entry.marks.name)),
                    )
                    // A badge, tinted with the row's own ink so it shows on any of its fills.
                    .when_some(within, |d, filter| {
                        d.child(
                            div()
                                .flex_shrink_0()
                                .max_w(px(96.))
                                .px(px(6.))
                                .rounded(px(4.))
                                .border_1()
                                .border_color(ink.opacity(0.3))
                                .bg(ink.opacity(0.1))
                                .truncate()
                                .text_size(px(11.))
                                .line_height(px(16.))
                                .text_color(ink)
                                .child(filter),
                        )
                    })
                    .when_some(narrow, |d, narrow| {
                        d.child(
                            div()
                                .id(SharedString::from(format!("narrow:{}", id_text(&entry.path))))
                                .min_w(px(24.))
                                .flex_shrink_1()
                                .flex()
                                .items_center()
                                .gap(px(2.))
                                .tip(format!("絞り込み：{narrow}"))
                                .child(icon("funnel", 11., muted))
                                .child(div().min_w_0().truncate().text_size(px(11.)).text_color(muted).child(narrow)),
                        )
                    }),
            )
            .when(kind == Kind::Search, |d| {
                d.child(
                    div()
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .text_size(px(12.))
                        .text_color(muted)
                        .child(with_marks(&entry.parent, &entry.marks.parent)),
                )
            })
    }

    /// The size and the date, each in a set width, flush left under its heading, so they read down the
    /// column as a table's would; then the chevron. A folder has no size of its own; like Explorer, it
    /// leaves the cell blank. A file keeps the chevron's room, so its date lines up with a folder's.
    fn row_cells(entry: &Entry, lit: bool, muted: Hsla, p: crate::theme::Palette) -> [AnyElement; 3] {
        let size = if entry.directory { String::new() } else { time::size(entry.size) };
        let modified = entry.modified.and_then(time::date_minute).unwrap_or_else(|| "—".into());
        let cell = |width: f32, text: String| {
            div().flex_none().w(px(width)).flex().whitespace_nowrap().text_size(px(12.)).text_color(muted).child(text).into_any_element()
        };
        let chevron = if entry.directory {
            icon("chevron-right", 13., if lit { p.on_accent_muted } else { p.text_faint }).into_any_element()
        } else {
            div().w(px(13.)).flex_none().into_any_element()
        };
        [cell(SIZE_WIDTH, size), cell(MODIFIED_WIDTH, modified), chevron]
    }

    /// A click on the row of `path` in the column at `index`: a double one opens it; home picks one row at
    /// a time, whatever is held; elsewhere Ctrl and Shift pick as they do in Explorer.
    fn click_row(
        &mut self,
        index: usize,
        kind: Kind,
        path: &std::path::Path,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.modifiers();
        if event.click_count() >= 2 {
            self.open(index, path, window, cx);
            return;
        }
        if kind == Kind::Home || !(modifiers.control || modifiers.shift) {
            self.pick(index, path.to_owned(), window, cx);
            return;
        }
        if modifiers.control && modifiers.shift {
            self.state.add_range(index, path);
        } else if modifiers.control {
            self.state.toggle(index, path);
        } else {
            self.state.extend(index, path);
        }
        self.state.set_active(index);
        self.changed(cx);
    }

    /// Home's rows: a handful of places, so no need to virtualize. The kinds stand under headings, so they
    /// don't read as one list; a drag among them shows where it would land.
    pub(super) fn home_rows(&mut self, marked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let entries = self.state.columns[0].entries.clone();
        let groups = crate::places::home_groups(&self.settings, &entries);
        // Drawn afresh below, for a drag to find its place.
        self.home_drag.rows.borrow_mut().clear();
        // The line where a drag would land: above the row it goes before, or below its kind's last.
        let landing = self.home_drag.carried.as_ref().zip(self.home_drag.landing.as_ref()).map(|(d, l)| (d.shelf, l.before.clone()));
        let mut rows = div()
            .id("home-rows")
            .role(Role::ListBox)
            .aria_label("ホーム")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.home_scroll)
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<HomeDrag>, _, cx| this.drag_home(event.event.position, event.bounds, cx)),
            )
            .on_drop(cx.listener(|this, drag: &HomeDrag, _, cx| this.drop_home(drag, cx)))
            .test_support();
        for (k, (title, items)) in groups.into_iter().enumerate() {
            rows = rows.child(
                div()
                    // Set off from the kind above, so the kinds read apart.
                    .when(k > 0, |d| d.mt(px(8.)))
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
            for (i, entry) in items.iter().enumerate() {
                let line = landing.as_ref().and_then(|(shelf, before)| match before {
                    Some(before) => (*before == entry.path).then_some(true),
                    None => (i + 1 == items.len() && movable(&self.settings, &entry.path) == Some(*shelf)).then_some(false),
                });
                rows = rows.child(div().px_1().relative().child(self.row(0, entry, marked, cx)).when_some(line, |d, above| {
                    d.child(
                        div()
                            .absolute()
                            .left(px(6.))
                            .right(px(6.))
                            .h(px(2.))
                            .rounded(px(1.))
                            .bg(p.accent)
                            .when(above, |l| l.top(px(-1.)))
                            .when(!above, |l| l.bottom(px(-1.))),
                    )
                }));
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .child(rows)
            .child(div().absolute().inset_0().child(scrollbar(gpui_kit::base::Scrollbar::vertical(&self.home_scroll), p)))
            .into_any_element()
    }

    /// The rows of a folder or the results: only those in sight are drawn, and the results load their
    /// next page as the list nears the end of what it has.
    pub(super) fn column_rows(&mut self, index: usize, marked: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let c = &self.state.columns[index];
        let (id, count) = (c.id, c.entries.len());
        let view = cx.entity();
        let handle = self.list_handle(id);
        let mut list = uniform_list(("list", id), count, move |range, _, cx| {
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
        .p_1()
        // The last row scrolls clear of the columns' scrollbar laid over the bottom.
        .pb(px(4. + SCROLLBAR_WIDTH));
        // Shift and the wheel send the columns sideways; the rows under it stay put. A list has no
        // builder for this, as a div has.
        list.style().restrict_scroll_to_axis = Some(true);
        let title = self.state.columns[index].title().into_owned();
        div()
            .id(("rows", id))
            .role(Role::ListBox)
            .aria_label(title)
            .flex_1()
            .min_h_0()
            .relative()
            .child(list)
            // Clear of the half of the edge that falls inside.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_0()
                    .right(px(RESIZER_WIDTH / 2.))
                    .child(scrollbar(gpui_kit::base::Scrollbar::vertical(&handle), p)),
            )
            .into_any_element()
    }
}
