//! Rows of home and the folders; search results carry their folder on a second line.
use std::ops::Range;

use colet_core::{entry::Entry, paths::id_text, time};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::*,
    home::{HomeDrag, HomeGhost, movable},
    metrics::{self, Scale, scaled},
    state::{ColumnId, HomeKey, Kind},
    theme::ActivePalette,
    view::*,
};

/// The widths a row's size and modified time take: `1023.9 MB` and `2026/12/31 23:59`, at 12px.
pub(crate) const SIZE_WIDTH: f32 = 56.;
pub(crate) const MODIFIED_WIDTH: f32 = 96.;
/// The room after the sizes, which stand flush right, before the dates, which stand flush left: with the
/// cells' gap alone, `965 B` would run on into `2026/11/24` as one figure.
pub(crate) const SIZE_TRAIL: f32 = 8.;
/// The chevron closing a folder's row, and the room a file's row keeps in its place.
pub(crate) const CHEVRON: f32 = metrics::INLINE_GLYPH;
/// What the pointer is on for a row's icon to turn to its checkbox.
const CHECK_GROUP: &str = "check";

/// A row's id: its path, which tells it from every other.
pub(crate) fn row_id(path: &std::path::Path) -> ElementId {
    ElementId::Name(id_text(path).into())
}

/// The part `part` of the row whose id is `row`: the row's id is shared, not written out again for each part.
pub(crate) fn row_part_id(row: &std::sync::Arc<ElementId>, part: &'static str) -> ElementId {
    ElementId::NamedChild(row.clone(), part.into())
}

/// A row being drawn: its item, its id, which its parts' ids are made from, and the theme's corners its
/// parts round with.
#[derive(Clone, Copy)]
pub(crate) struct DrawnRow<'a> {
    entry: &'a Entry,
    id: &'a std::sync::Arc<ElementId>,
    radii: metrics::Radii,
}

pub(crate) fn row_height(kind: Kind) -> f32 {
    if kind == Kind::Search { 42. } else { 28. }
}

/// The lines of a search result, its name over its folder: set close, as one item's two lines, so the
/// room the row leaves over falls between the rows and the folder reads as its own name's, not the next.
/// Left to the text's own height, the two lines would fill the row, as far apart as the rows are.
const RESULT_NAME_LINE: f32 = 19.;
const RESULT_FOLDER_LINE: f32 = 16.;

/// Where a row's name and a search result's folder under it note how they were drawn, for the row's tip
/// to tell whether either was cut short.
#[derive(Clone, Default)]
struct RowFits {
    name: Fit,
    parent: Fit,
}

/// The tip of the row of `entry`, built as the pointer comes to rest on the row: the name if it was cut
/// short; for a search result, the name over its folder if either was. Neither cut, it shows nothing. A
/// part of the row with a tip of its own shows that one, drawn after the row's.
fn row_tip(entry: &Entry, kind: Kind, fits: RowFits) -> impl Fn(&mut Window, &mut App) -> AnyView + use<> {
    let name = entry.name.clone();
    let parent = (kind == Kind::Search).then(|| entry.parent.clone());
    move |window, cx| {
        let cut = fits.name.cut(&name, window) || parent.as_deref().is_some_and(|parent| fits.parent.cut(parent, window));
        match &parent {
            _ if !cut => cx.new(|_| EmptyView).into(),
            Some(parent) => Tooltip::new(format!("{name}\n{parent}")).build(window, cx),
            None => Tooltip::new(name.clone()).build(window, cx),
        }
    }
}

/// The tip of a text that may be cut short, which `fit` notes the drawing of: the text whole if it was
/// cut, nothing if it fit.
fn cut_tip(text: String, fit: Fit) -> impl Fn(&mut Window, &mut App) -> AnyView + use<> {
    move |window, cx| if fit.cut(&text, window) { Tooltip::new(text.clone()).build(window, cx) } else { cx.new(|_| EmptyView).into() }
}

/// The inks a row's text takes: on the selection's fill while it is `lit`, on the column's otherwise.
#[derive(Clone, Copy)]
struct RowInks {
    ink: Hsla,
    muted: Hsla,
    /// What a search or the column's filter matched.
    mark: Hsla,
}

impl RowInks {
    fn on(p: crate::theme::Palette, lit: bool) -> Self {
        if lit {
            Self { ink: p.on_accent, muted: p.on_accent_muted, mark: p.on_accent_match }
        } else {
            Self { ink: p.text, muted: p.text_muted, mark: p.accent }
        }
    }
}

/// How a row stands, from the column, the clipboard, the menu and a drag in home; see [`Colet::row_look`].
struct RowLook {
    selected: bool,
    /// A search result or favorite points at it, or it is the saved search running.
    highlighted: bool,
    /// Filled with a tint of the accent, apart from the selection's fills: what a search result or
    /// favorite points at, and home's pick while the columns are worked in.
    pointed: bool,
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
    fn row_look(&self, index: usize, entry: &Entry, marked: bool, keys: bool, cx: &App) -> RowLook {
        let c = &self.state.columns[index];
        let kind = c.kind;
        let selected = c.selected.contains(&entry.path);
        let key = HomeKey::of(&entry.path);
        // Home lights the saved search that is running, narrowed and ordered as it was kept.
        let highlighted = c.highlight.as_ref() == Some(&entry.path)
            || matches!(key, HomeKey::Search { query, filter, narrow, order }
                if self.state.pinned()
                    && query == self.state.query
                    && filter == self.state.search_filter.as_deref()
                    && narrow == self.state.search_narrowing().as_deref()
                    && order == self.state.search_order);
        // On the clipboard, cut or copied, whoever put it there.
        let clipboard = &self.operations.read(cx).clipboard;
        let held = clipboard.holds(&entry.path);
        let cut = held && clipboard.files.cut;
        // The menu acts on a row outside the selection, or the folder itself: the selection steps back, so
        // only what the menu acts on stands out.
        let aside = self.menu().is_some_and(|m| m.column == c.id && m.paths.is_some());
        RowLook {
            selected,
            highlighted,
            // The item a search result or favorite points at: its own fill, which wins over the selection but
            // not over the one in the column worked in. Home marks its pick so while the columns are worked in.
            pointed: (highlighted && (!selected || !marked)) || (kind == Kind::Home && selected && !marked),
            lit: selected && marked && !aside,
            // The keys are on it: ringed, selected or not, while they are in this column.
            focused: keys && c.focused.as_ref() == Some(&entry.path),
            // What a right-click opened the menu on, which it leaves unselected: ringed, as in Finder, until
            // it closes.
            target: self.menu().is_some_and(|m| m.column == c.id && m.paths.as_deref() == Some(std::slice::from_ref(&entry.path))),
            aside,
            held,
            cut,
            faded: (entry.hidden.is_hidden() || cut) && !selected,
            dragged: kind == Kind::Home && self.home_drag.read(cx).carried().is_some_and(|d| d.path == entry.path),
            // The filter a saved search runs within, faint beside its name.
            within: match &key {
                HomeKey::Search { filter, .. } => filter.map(str::to_owned),
                HomeKey::Folder { .. } => None,
            },
            // What a favorite or saved search opens narrowed to, fainter after it.
            narrow: match key {
                _ if kind != Kind::Home => None,
                HomeKey::Search { narrow, .. } => narrow.map(str::to_owned),
                HomeKey::Folder { narrow, .. } => narrow.map(std::borrow::Cow::into_owned),
            },
        }
    }

    pub(crate) fn row(&mut self, index: usize, entry: &Entry, marked: bool, keys: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let radii = self.radii(cx);
        let RowLook { selected, highlighted, pointed, lit, focused, target, aside, held, cut, faded, dragged, within, narrow } =
            self.row_look(index, entry, marked, keys, cx);
        let c = &self.state.columns[index];
        let kind = c.kind;
        let name_marks = c.narrow_marks(&entry.name);
        // While a row in home is carried, only the line tells where it goes.
        let dragging = kind == Kind::Home && self.home_carried(cx).is_some();
        let key = HomeKey::of(&entry.path);
        let saved = key.is_search();
        // A narrowed favorite's is its folder's: a drive's own, say.
        let image = match key {
            HomeKey::Folder { path, narrow: Some(_) } => self.icons.get(&Entry { path: path.into_owned(), ..entry.clone() }, cx),
            _ => self.icons.get(entry, cx),
        };
        let muted = if lit { p.on_accent_muted } else { p.text_muted };
        let path = entry.path.clone();
        // A pinned row, favorite or saved search moves among its kind by dragging; it fades where it was meanwhile.
        // Narrowed, home hides some of its kind, and where among them it would land is not to be seen.
        let drag =
            (kind == Kind::Home && c.narrow.is_none()).then(|| movable(self.stored.settings(), &entry.path)).flatten().map(|shelf| {
                (
                    HomeDrag { path: entry.path.clone(), shelf },
                    image.clone(),
                    self.home_drag.read(cx).rows().clone(),
                    cx.entity().downgrade(),
                )
            });
        let name = entry.name.clone();
        let picture = match (saved, image) {
            // In the place of an item's picture, as large.
            (true, _) => icon("search", metrics::ICON, if lit { p.on_accent } else { p.text_muted }).into_any_element(),
            (false, Some(image)) => img(image).size(scaled(metrics::ICON)).flex_none().into_any_element(),
            (false, None) => div().size(scaled(metrics::ICON)).flex_none().into_any_element(),
        };
        let menu_path = entry.path.clone();
        let tab_path = entry.path.clone();
        let press_path = entry.path.clone();
        let focused_row = (marked && c.focused.as_ref() == Some(&entry.path)).then(|| self.key_menu.row.clone());
        // Built once a row: its parts take it as their parent's rather than each writing out the path.
        let id = std::sync::Arc::new(row_id(&entry.path));
        let drawn = DrawnRow { entry, id: &id, radii };
        let fits = RowFits::default();
        // Not while a row is carried or about to be, nor while a menu lies over the rows.
        let tip = (self.home_carried(cx).is_none() && self.press.is_none() && self.overlays.popup().is_none())
            .then(|| row_tip(entry, kind, fits.clone()));
        div()
            .id(ElementId::clone(&id))
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
            // Moved far enough, held down, it is dragged out to another app; home's rows are put in order.
            .when(kind != Kind::Home, |r| {
                r.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| this.press_row(index, press_path.clone(), event.position, cx)),
                )
            })
            .relative()
            .when_some(focused_row, |r, cell| {
                r.child(canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, _, _, _| {}).absolute().size_full())
            })
            .when_some(drag, |r, (drag, image, rows, view)| {
                let drawn = rows.clone();
                let (path, shelf) = (drag.path.clone(), drag.shelf);
                r.child(
                    canvas(move |bounds, _, _| drawn.borrow_mut().push((path.clone(), shelf, bounds)), |_, _, _, _| {})
                        .absolute()
                        .size_full(),
                )
                .on_drag(drag, move |drag, _, window, cx| {
                    view.update(cx, |this, cx| this.start_home_drag(drag, cx)).ok();
                    // As wide as the row was drawn; not drawn yet, as wide as home's pane it lies in.
                    let drawn = rows.borrow().iter().find(|(path, _, _)| *path == drag.path).map(|(_, _, b)| b.size.width);
                    let width = drawn.unwrap_or_else(|| {
                        let pane = view.read_with(cx, |this, _| this.sidebar_width(window)).unwrap_or_default();
                        Scale::of(window).px(pane)
                    });
                    let (name, search, image) = (name.clone(), HomeKey::of(&drag.path).is_search(), image.clone());
                    cx.new(|_| HomeGhost::new(name, search, image, width))
                })
            })
            .when(dragged, |r| r.opacity(0.4))
            .h(scaled(row_height(kind)))
            .w_full()
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(scaled(radii.sm))
            .role(Role::ListBoxOption)
            .aria_label(entry.name.clone())
            .aria_selected(selected)
            .border_2()
            .border_color(transparent_black())
            .when(focused, |r| r.border_color(if lit { p.focus_on_fill } else { p.focus }))
            .when(target, |r| r.border_color(p.accent).bg(p.highlight))
            .when(selected, |r| r.bg(if marked && !aside { p.accent_fill } else { p.selected_inactive }))
            .when(lit, |r| r.text_color(p.on_accent))
            // Told by the row's own fill, as the selection is, not by a line at its edge.
            .when(pointed, |r| r.bg(p.highlight))
            .when(!selected && !highlighted && !dragging, |r| r.hover(|r| r.bg(p.hover)))
            .when(faded, |r| r.text_color(p.text_muted))
            .map(|r| {
                let picture = div().when(faded, |d| d.opacity(if cut { 0.35 } else { 0.6 })).child(picture);
                if kind == Kind::Home { r.child(picture) } else { r.child(self.check(index, drawn, selected, lit, picture, cx)) }
            })
            .child(self.row_name(
                drawn,
                name_marks.as_deref().unwrap_or(&entry.marks.name),
                kind,
                RowInks::on(p, lit),
                &fits,
                (within, narrow),
            ))
            // Out of the name, so the name stays the file's own: a fade alone would read as hidden.
            .when(held, |r| {
                r.child(
                    div()
                        .id(row_part_id(&id, "held"))
                        .tip(if cut { words::column::being_cut() } else { words::column::being_copied() })
                        .child(icon(
                            if cut { "scissors" } else { "copy" },
                            metrics::SMALL_GLYPH,
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
            .when(kind != Kind::Home, |r| r.children(Self::row_cells(&id, entry, lit, muted, p)))
            .when_some(tip, |r, tip| r.tooltip(tip))
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| this.click_row(index, kind, &path, event, window, cx)))
            .test_support()
            .into_any_element()
    }

    /// The icon, which turns to a checkbox while its row is one of several selected or the pointer is on it,
    /// as Explorer's item check boxes do: one alone is the row gone into, not a set being gathered. It takes
    /// the row's edge and the gap after it, to be easy to hit.
    pub(crate) fn check(
        &self,
        index: usize,
        row: DrawnRow,
        selected: bool,
        lit: bool,
        picture: Div,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let p = cx.palette();
        let path = row.entry.path.clone();
        let boxed = selected && self.state.columns[index].selected.len() > 1;
        // Ringed on the row's own fill, filled on any other.
        let (fill, mark) = if lit { (p.on_accent, p.accent_fill) } else { (p.accent_fill, p.on_accent) };
        let check = div().size_3p5().rounded(scaled(row.radii.sm)).flex().items_center().justify_center().map(|b| {
            if selected { b.bg(fill).child(icon("check", metrics::TINY_GLYPH, mark)) } else { b.border_1().border_color(p.text_muted) }
        });
        div()
            .id(row_part_id(row.id, "check"))
            .group(CHECK_GROUP)
            .relative()
            .flex_none()
            .h_full()
            .flex()
            .items_center()
            // Out over the row's padding and ring, and the gap to the name.
            .ml_neg_2p5()
            .pl_2p5()
            .mr_neg_2()
            .pr_2()
            .role(Role::CheckBox)
            .aria_label(words::select())
            .aria_toggled(if selected { Toggled::True } else { Toggled::False })
            .child(picture.when(boxed, |d| d.invisible()).group_hover(CHECK_GROUP, |s| s.invisible()))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .left_2p5()
                    .w_4()
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
    /// `marks`: where the name is marked, by the column's filter or the search; `fits`: where the name and
    /// the folder note how they were drawn.
    fn row_name(
        &self,
        row: DrawnRow,
        marks: &[Range<usize>],
        kind: Kind,
        inks: RowInks,
        fits: &RowFits,
        (within, narrow): (Option<String>, Option<String>),
    ) -> Div {
        let RowInks { ink, muted, mark } = inks;
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
                    .when(kind == Kind::Search, |d| d.line_height(scaled(RESULT_NAME_LINE)))
                    .child(div().relative().min_w_0().truncate().child(with_marks(&row.entry.name, marks, mark)).child(fits.name.note()))
                    // A badge, tinted with the row's own ink so it shows on any of its fills; its own tip
                    // gives the filter it cuts short.
                    .when_some(within, |d, filter| {
                        let fit = Fit::default();
                        d.child(
                            div()
                                .id(row_part_id(row.id, "within"))
                                .flex_shrink_0()
                                .max_w_24()
                                .px_1p5()
                                .rounded(scaled(row.radii.sm))
                                .border_1()
                                .border_color(ink.opacity(0.3))
                                .bg(ink.opacity(0.1))
                                .text_xs()
                                .line_height(scaled(16.))
                                .text_color(ink)
                                .tooltip(cut_tip(filter.clone(), fit.clone()))
                                .child(div().relative().truncate().child(filter).child(fit.note()))
                                .test_support(),
                        )
                    })
                    .when_some(narrow, |d, narrow| {
                        d.child(
                            div()
                                .id(row_part_id(row.id, "narrow"))
                                .min_w_6()
                                .flex_shrink_1()
                                .flex()
                                .items_center()
                                .gap_0p5()
                                .tip(words::column::narrowed_to(&narrow))
                                .child(icon("funnel", metrics::TINY_GLYPH, muted))
                                .child(div().min_w_0().truncate().text_xs().text_color(muted).child(narrow)),
                        )
                    }),
            )
            .when(kind == Kind::Search, |d| {
                d.child(
                    div()
                        .relative()
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis_start()
                        .text_xs()
                        .line_height(scaled(RESULT_FOLDER_LINE))
                        .text_color(muted)
                        .child(with_marks(&row.entry.parent, &row.entry.marks.parent, mark))
                        .child(fits.parent.note()),
                )
            })
    }

    /// The size and the date, each in a set width under its heading, so they read down the column as a
    /// table's would; then the chevron. The size is flush right, as Explorer has it, so sizes are compared
    /// by their length; the date, flush left. A folder has no size of its own; like Explorer, it leaves the
    /// cell blank. A file keeps the chevron's room, so its date lines up with a folder's.
    fn row_cells(id: &std::sync::Arc<ElementId>, entry: &Entry, lit: bool, muted: Hsla, p: crate::theme::Palette) -> [AnyElement; 3] {
        let size = if entry.directory { String::new() } else { time::size(entry.size) };
        let modified = entry.modified.and_then(time::date_minute).unwrap_or_else(|| "—".into());
        let cell = |part: &'static str, width: f32, text: String, flush_right: bool| {
            let trail = if flush_right { SIZE_TRAIL } else { 0. };
            div()
                .flex_none()
                .w(scaled(width + trail))
                .pr(scaled(trail))
                .flex()
                .when(flush_right, |d| d.justify_end())
                .whitespace_nowrap()
                .text_xs()
                .text_color(muted)
                .child(div().id(row_part_id(id, part)).flex_none().child(text).test_support())
                .into_any_element()
        };
        let chevron = if entry.directory {
            icon("chevron-right", CHEVRON, if lit { p.on_accent_muted } else { p.text_faint }).into_any_element()
        } else {
            div().w(scaled(CHEVRON)).flex_none().into_any_element()
        };
        [cell("size", SIZE_WIDTH, size, true), cell("modified", MODIFIED_WIDTH, modified, false), chevron]
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

    /// The rows of a folder or the results: only those in sight are drawn, and the results load their
    /// next page as the list nears the end of what it has (see [`next_page_at_end`]).
    pub(crate) fn column_rows(&mut self, index: usize, marked: bool, keys: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let c = &self.state.columns[index];
        let (id, count) = (c.id, c.entries.len());
        let more_to_come = c.kind == Kind::Search && count < c.total;
        let view = cx.entity();
        let handle = self.list_handle(id);
        // Draws the rows asked for and nothing else: it can be called for any range, to measure as well.
        let mut list = uniform_list(("list", id), count, move |range, _, cx| {
            view.update(cx, |this, cx| {
                let Some(index) = this.state.index_of(id) else { return Vec::new() };
                let c = &this.state.columns[index];
                let entries: Vec<std::sync::Arc<Entry>> =
                    c.entries[range.start.min(c.entries.len())..range.end.min(c.entries.len())].to_vec();
                entries.iter().map(|entry| this.row(index, entry, marked, keys, cx)).collect()
            })
        })
        .track_scroll(&handle)
        .size_full()
        .p_1()
        // The last row scrolls clear of the columns' scrollbar laid over the bottom.
        .pb(scaled(4. + SCROLLBAR_WIDTH));
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
            .when(more_to_come, |d| d.child(next_page_at_end(id, count, &handle, cx)))
            // Clear of the half of the edge that falls inside.
            .child(div().absolute().top_0().bottom_0().left_0().right(scaled(RESIZER_WIDTH / 2.)).child(scrollbar(
                gpui_kit::base::Scrollbar::vertical(&handle),
                p,
                self.scale(),
            )))
            .into_any_element()
    }
}

/// How near the end of what a search's list has its last row in sight brings the next page.
pub(crate) const PAGE_AHEAD: usize = 50;

/// How many of the `count` rows the list laid out last are in sight or above them; `None` before it is
/// laid out.
pub(crate) fn rows_in_sight(handle: &UniformListScrollHandle, count: usize) -> Option<usize> {
    let state = handle.0.borrow();
    let size = state.last_item_size?;
    // A row's height as laid out, not as set: it follows the text's size.
    let row = size.contents.height / count.max(1) as f32;
    if row <= px(0.) {
        return None;
    }
    Some(((-state.base_handle.offset().y + size.item.height) / row).ceil().max(0.) as usize)
}

/// Asks for the next page of the results `id` once the rows in sight come within [`PAGE_AHEAD`] of the
/// `count` it has. Placed after the list, it reads the list's scroll as the list has just laid it out, a
/// row moved to by the keys too, so the rows themselves are only drawn; the page is asked for after the
/// frame, and [`Colet::more`] asks for nothing while one is on its way.
fn next_page_at_end(id: ColumnId, count: usize, handle: &UniformListScrollHandle, cx: &mut Context<Colet>) -> impl IntoElement {
    let (handle, view) = (handle.clone(), cx.entity().downgrade());
    canvas(
        move |_, _, cx| {
            let Some(in_sight) = rows_in_sight(&handle, count) else { return };
            if in_sight + PAGE_AHEAD >= count {
                cx.defer(move |cx| {
                    view.update(cx, |this, cx| this.more(id, cx)).ok();
                });
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_0()
}
