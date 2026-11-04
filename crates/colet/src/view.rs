//! How the window looks: home's pane on the left; the search field, the trail of folders, the columns
//! and the status bar beside it.
use std::{ops::Range, rc::Rc, time::Instant};

use colet_core::{entry::{Entry, Sort, direction_label}, paths::{folder_name, id_text, shown}, time};
use gpui_kit::base::input::{self, Input, InputState};
use gpui_kit::component::{button::{Button, ButtonVariants}, status_bar::StatusBar, Disableable, Sizable};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    crumbs::Crumb,
    places::{HomeDrag, movable},
    state::{favorite_key, narrowed_folder, saved_query},
    reveal::{Cell, Slide, on_screen, refit, reveal_target, uncover_target},
    state::{Column, ColumnId, Kind},
};

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

pub trait TipExt {
    /// A line of text shown while the pointer rests on it.
    fn tip(self, text: impl Into<SharedString>) -> Self;
}

impl<E: StatefulInteractiveElement> TipExt for E {
    fn tip(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx))
    }
}

/// The icon named `name` among Colet's, for a button.
pub fn glyph(name: &str) -> gpui_kit::component::Icon {
    gpui_kit::component::Icon::empty().path(format!("icons/{name}.svg"))
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
        s.track(|t| t.bg(transparent_black()).border_color(transparent_black()).width(px(SCROLLBAR_WIDTH)))
            .thumb(|t| t.bg(p.scrollbar).width(px(6.)).inset(px(1.)).radius(px(3.)))
            .thumb_hover(|t| t.bg(p.text_muted))
            .thumb_active(|t| t.bg(p.text_muted))
    })
}

/// How wide the edge a drag widens a column or home's pane from is.
const RESIZER_WIDTH: f32 = 6.;
/// How thick a scrollbar's track is.
pub(crate) const SCROLLBAR_WIDTH: f32 = 8.;

/// Draws its child over everything laid out after it, as `deferred` does, but clipped where it is, for
/// presses as for paint: a column scrolled under home's pane keeps its edge there.
fn on_top(child: impl IntoElement) -> OnTop {
    let clip = Rc::new(std::cell::Cell::new(None));
    OnTop(Some(Clipped { clip: clip.clone(), child: child.into_any_element() }.into_any_element()), clip)
}

struct OnTop(Option<AnyElement>, Rc<std::cell::Cell<Option<ContentMask<Pixels>>>>);

impl Element for OnTop {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.0.as_mut().unwrap().request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, _: &mut App) {
        let (offset, mask) = (window.element_offset(), window.content_mask());
        self.1.set(Some(mask));
        window.defer_draw(self.0.take().unwrap(), offset, 0, Some(mask));
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), _: &mut Window, _: &mut App) {}
}

impl IntoElement for OnTop {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// What [`OnTop`] defers: GPUI clips a deferred draw's paint, but not the hit areas laid out ahead of it.
struct Clipped {
    clip: Rc<std::cell::Cell<Option<ContentMask<Pixels>>>>,
    child: AnyElement,
}

impl Element for Clipped {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, window: &mut Window, cx: &mut App) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), window: &mut Window, cx: &mut App) {
        window.with_content_mask(self.clip.get(), |window| self.child.prepaint(window, cx));
    }

    fn paint(&mut self, _: Option<&GlobalElementId>, _: Option<&InspectorElementId>, _: Bounds<Pixels>, _: &mut (), _: &mut (), window: &mut Window, cx: &mut App) {
        window.with_content_mask(self.clip.get(), |window| self.child.paint(window, cx));
    }
}

impl IntoElement for Clipped {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// The focus ring of what Tab reached: two pixels in the focus color, kept clear until then.
pub fn ring(el: Stateful<Div>, p: crate::theme::Palette) -> Stateful<Div> {
    el.border_2().border_color(transparent_black()).focus_visible(|s| s.border_color(p.focus))
}

/// The ring of a field, or of a button with a border of its own: two pixels in the focus color a pixel
/// off it, laid round it and kept clear until `shown`.
pub fn frame(p: crate::theme::Palette, shown: bool) -> Div {
    div().p(px(1.)).rounded(px(8.)).border_2().border_color(if shown { p.focus } else { transparent_black() })
}

/// Tab, not the mouse, brought the keys to `focus`: when [`ring`] shows.
pub fn keyed(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}

/// A button the mouse presses without taking the keys from where they are; Tab still reaches it.
pub fn keep_focus<E: InteractiveElement>(el: E) -> E {
    el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

/// `text` with what a search matched in it underlined, in the ink of the text around it.
fn with_marks(text: &str, marks: &[Range<usize>]) -> StyledText {
    let underline = UnderlineStyle { thickness: px(1.), ..Default::default() };
    let style = HighlightStyle { underline: Some(underline), ..Default::default() };
    StyledText::new(text.to_owned()).with_highlights(marks.iter().map(|range| (range.clone(), style)))
}

/// Rows of home and the folders; search results carry their folder on a second line.
/// The widths a row's size and modified time take: `1023.9 MB` and `2026/12/31 23:59`, at 12px.
const SIZE_WIDTH: f32 = 56.;
const MODIFIED_WIDTH: f32 = 96.;
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
    /// Home's pane as drawn: as wide as dragged, but narrower in a narrow window, down to its least, so
    /// the rest keeps [`MAIN_MIN`]. The width kept stays as dragged, and comes back as the window widens.
    pub fn sidebar_width(&self, window: &Window) -> f32 {
        let room = f32::from(window.viewport_size().width) - MAIN_MIN;
        self.widths.sidebar.min(room.max(SIDEBAR_WIDTHS.0))
    }

    pub fn on_screen(&self) -> OnScreen {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        let view = f32::from(self.sideways.scroll.bounds().size.width);
        // A wheel moves the offset past its bounds until the next layout brings it back: read it as it
        // will be drawn, so the edges' shades do not flash on.
        let max = f32::from(self.sideways.scroll.max_offset().x).max(self.sideways.hold);
        let from = self.sideways.offset().clamp(0., max);
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
        let row = self.crumbs.row.get()?;
        let bounds = self.crumbs.bounds.borrow();
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

    /// The keys are in home's pane.
    fn in_home(&self) -> bool {
        self.state.columns.get(self.state.active).is_some_and(|c| c.kind == Kind::Home)
    }

    /// Shift+↑↓, with Ctrl adding: picks rows on the way, or in home, which picks one at a time, moves as
    /// the arrows do.
    fn extend_step(&mut self, by: isize, add: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.in_home() {
            self.step(by, window, cx);
            return;
        }
        self.state.extend_step(by, add);
        self.changed(cx);
    }

    fn icon_button(id: &'static str, name: &'static str, label: &'static str, tip: &'static str, enabled: bool) -> Button {
        Button::new(id).ghost().icon(glyph(name)).accessibility_label(label).tooltip(tip).disabled(!enabled)
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
            .on_action(cx.listener(|this, _: &RecycleItems, _, cx| this.delete_picked(false, cx)))
            .on_action(cx.listener(|this, _: &RecycleItemsForGood, _, cx| this.delete_picked(true, cx)))
            // Home picks one row at a time: Ctrl+A picks nothing there, and Shift moves as the arrows do.
            .on_action(cx.listener(|this, _: &SelectAll, _, cx| {
                if !this.in_home() {
                    this.state.select_all(this.state.active);
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ExtendUp, window, cx| this.extend_step(-1, false, window, cx)))
            .on_action(cx.listener(|this, _: &ExtendDown, window, cx| this.extend_step(1, false, window, cx)))
            .on_action(cx.listener(|this, _: &AddUp, window, cx| this.extend_step(-1, true, window, cx)))
            .on_action(cx.listener(|this, _: &AddDown, window, cx| this.extend_step(1, true, window, cx)))
            .on_action(cx.listener(|this, _: &FocusPrevious, _, cx| this.focus_step(-1, cx)))
            .on_action(cx.listener(|this, _: &FocusNext, _, cx| this.focus_step(1, cx)))
            .on_action(cx.listener(|this, _: &FocusPageUp, _, cx| this.focus_step(-10, cx)))
            .on_action(cx.listener(|this, _: &FocusPageDown, _, cx| this.focus_step(10, cx)))
            .on_action(cx.listener(|this, _: &FocusFirst, _, cx| this.focus_step(-isize::MAX / 2, cx)))
            .on_action(cx.listener(|this, _: &FocusLast, _, cx| this.focus_step(isize::MAX / 2, cx)))
            .on_action(cx.listener(|this, _: &TogglePicked, window, cx| {
                this.state.toggle_focused();
                // A saved search picked alone in home runs, which the field shows.
                this.sync_search_field(window, cx);
                this.changed(cx);
            }))
            .on_action(cx.listener(|this, _: &QuickLook, window, cx| {
                if this.home_focus.is_focused(window) || this.columns_focus.is_focused(window) {
                    this.quick_look(cx);
                } else {
                    // A field types it, a button is pressed by it.
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &OpenMenu, window, cx| {
                // Under the focused row, as from the keyboard; or the column's top.
                let at = match this.focused_row.get() {
                    Some(row) => point(row.left() + px(16.), row.bottom()),
                    None => point(px(this.sidebar_width(window)) + px(24.), px(140.)),
                };
                this.open_menu(this.state.active, None, at, window, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenItem, window, cx| {
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
            .on_action(cx.listener(|this, _: &TrimFilter, window, cx| {
                if !this.trim_filter(window, cx) {
                    cx.propagate();
                }
            }))
            .on_action(cx.listener(|this, _: &ClearFilter, window, cx| {
                let c = &this.state.columns[this.state.active];
                if c.filter.is_some() {
                    this.close_filter(c.id, window, cx);
                } else {
                    cx.propagate();
                }
            }))
    }

    /// While `focus`, the rows', has the keys, what is typed there goes to their column's field.
    fn takes_text(&self, focus: &FocusHandle, cx: &mut Context<Self>) -> impl IntoElement {
        let (focus, view) = (focus.clone(), cx.entity());
        canvas(|_, _, _| {}, move |bounds, _, window, cx| window.handle_input(&focus, ElementInputHandler::new(bounds, view), cx))
            .absolute()
            .size_full()
    }

    /// How the row of `entry` in the column at `index` stands, `marked` while that column is worked in.
    fn row_look(&self, index: usize, entry: &Entry, marked: bool) -> RowLook {
        let c = &self.state.columns[index];
        let kind = c.kind;
        let selected = c.selected.contains(&entry.path);
        let saved = saved_query(&entry.path);
        // Home lights the saved search that is running, narrowed as it was kept.
        let highlighted = c.highlight.as_ref() == Some(&entry.path)
            || saved.is_some_and(|(q, f, n)| {
                self.state.pinned() && q == self.state.query && f == self.state.search_filter.as_deref() && n == self.state.search_narrowing().as_deref()
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
            faded: (entry.hidden || cut) && !selected,
            dragged: kind == Kind::Home && self.home_drag.carried.as_ref().is_some_and(|d| d.path == entry.path),
            // The filter a saved search runs within, faint beside its name.
            within: saved.and_then(|(_, f, _)| f).map(str::to_owned),
            // What a favorite or saved search opens narrowed to, fainter after it.
            narrow: (kind == Kind::Home)
                .then(|| saved.map(|(_, _, n)| n.map(str::to_owned)).unwrap_or_else(|| narrowed_folder(&entry.path).map(|(_, n)| n)))
                .flatten(),
        }
    }

    fn row(&mut self, index: usize, entry: &Entry, marked: bool, cx: &mut Context<Self>) -> AnyElement {
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
        let drag = (kind == Kind::Home && c.filter.is_none())
            .then(|| movable(&self.settings, &entry.path))
            .flatten()
            .map(|shelf| (HomeDrag { path: entry.path.clone(), shelf }, image.clone(), self.home_drag.rows.clone(), cx.entity().downgrade()));
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
            .when(barred, |r| {
                r.child(div().absolute().left_0().top_0().bottom_0().w(px(3.)).rounded_l(px(4.)).bg(p.accent))
            })
            .when(selected, |r| r.bg(if marked && !aside { p.accent_fill } else { p.selected_inactive }))
            .when(lit, |r| r.text_color(p.on_accent))
            .when(barred, |r| r.bg(p.highlight))
            .when(!selected && !highlighted, |r| r.hover(|r| r.bg(p.hover)))
            .when(faded, |r| r.text_color(p.text_muted))
            // The selection in home while the columns are worked in: marked at its edge, as in a sidebar.
            .map(|r| {
                let picture = div().when(faded, |d| d.opacity(if cut { 0.35 } else { 0.6 })).child(picture);
                if kind == Kind::Home {
                    r.child(picture)
                } else {
                    r.child(self.check(index, entry, selected, lit, picture, cx))
                }
            })
            .child(self.row_name(entry, kind, barred && highlighted, lit, within, narrow))
            // Out of the name, so the name stays the file's own: a fade alone would read as hidden.
            .when(held, |r| {
                r.child(
                    div()
                        .id(SharedString::from(format!("held:{}", id_text(&entry.path))))
                        .tip(if cut { "切り取り中" } else { "コピー中" })
                        .child(icon(if cut { "scissors" } else { "copy" }, 12., if lit { p.on_accent_muted } else if cut { p.text_faint } else { p.accent }))
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
    fn check(&self, index: usize, entry: &Entry, selected: bool, lit: bool, picture: Div, cx: &mut Context<Self>) -> impl IntoElement + use<> {
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
                d.child(div().w_full().overflow_hidden().whitespace_nowrap().text_ellipsis_start().text_size(px(12.)).text_color(muted).child(with_marks(&entry.parent, &entry.marks.parent)))
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
    fn click_row(&mut self, index: usize, kind: Kind, path: &std::path::Path, event: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
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
    /// The column at `index` under its headings: its rows, or why it has none.
    fn column_body(&mut self, index: usize, marked: bool, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        if let Some(error) = c.error.clone() {
            return self.column_error(id, kind, error, cx);
        }
        if c.unread() {
            return self.loading().into_any_element();
        }
        if c.entries.is_empty() {
            let (name, text) = match kind {
                Kind::Search if c.filter.is_some() => ("funnel", "一致する項目はありません"),
                Kind::Search => ("search", "一致する項目はありません"),
                Kind::Home if c.filter.is_some() => ("funnel", "一致する項目はありません"),
                Kind::Home => ("hard-drive", "場所もお気に入りもありません"),
                Kind::Folder if c.total > 0 && c.filter.is_some() => ("funnel", "一致する項目はありません"),
                Kind::Folder => ("folder", "このフォルダーは空です"),
            };
            return self.message(name, text.into()).into_any_element();
        }
        if kind == Kind::Home {
            return self.home_rows(marked, cx);
        }
        self.column_rows(index, marked, cx)
    }

    /// Why the column `id` could not be listed, with a button to try again; the search's, with one to
    /// the preferences too, where Everything is reconnected.
    fn column_error(&self, id: ColumnId, kind: Kind, error: String, cx: &mut Context<Self>) -> AnyElement {
        self.message("circle-alert", error)
            .child(
                keep_focus(Button::new(("retry", id)).outline().icon(glyph("refresh-cw")).label("再試行")).on_click(cx.listener(
                    move |this, event: &ClickEvent, window, cx| {
                        // The button goes as the column loads: the keys go back to the columns.
                        if event.is_keyboard() {
                            this.focus_active(window, cx);
                        }
                        this.load(id, 0, false, cx);
                    },
                )),
            )
            .when(kind == Kind::Search, |d| {
                d.child(
                    Button::new("search-settings")
                        .outline()
                        .icon(glyph("sliders-horizontal"))
                        .label("設定")
                        .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx))),
                )
            })
            .into_any_element()
    }

    /// Home's rows: a handful of places, so no need to virtualize. The kinds stand under headings, so they
    /// don't read as one list; a drag among them shows where it would land.
    fn home_rows(&mut self, marked: bool, cx: &mut Context<Self>) -> AnyElement {
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
            .on_drag_move(cx.listener(|this, event: &DragMoveEvent<HomeDrag>, _, cx| this.drag_home(event.event.position, event.bounds, cx)))
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
    fn column_rows(&mut self, index: usize, marked: bool, cx: &mut Context<Self>) -> AnyElement {
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
            .child(div().absolute().top_0().bottom_0().left_0().right(px(RESIZER_WIDTH / 2.)).child(scrollbar(gpui_kit::base::Scrollbar::vertical(&handle), p)))
            .into_any_element()
    }

    /// The edge a drag widens a column from: astride its border, so its scrollbar stays clear of it.
    fn resizer(&self, id: &'static str, of: WidthOf, width: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        // Over what lies past the border, drawn after it; none over a dialog, which covers it.
        if self.dialog.is_some() {
            return div().into_any_element();
        }
        let handle = div()
            .id(id)
            .group(id)
            .absolute()
            .top_0()
            // Off the columns' scrollbar along the bottom, which it would take the presses of.
            .bottom(px(SCROLLBAR_WIDTH))
            .right(px(-RESIZER_WIDTH / 2.))
            .w(px(RESIZER_WIDTH))
            .flex()
            .justify_center()
            .cursor_col_resize()
            // A line on the border, not a band over the rows.
            .child(div().w(px(2.)).h_full().group_hover(id, |d| d.bg(p.accent)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.resizing = Some(Resize { of, start_x: event.position.x, start_width: width });
                    cx.stop_propagation();
                }),
            )
            .test_support();
        on_top(handle).into_any_element()
    }

    fn render_column(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let marked = index == self.state.active && !self.search_focused;
        let c: &Column = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        let (width, of) = match kind {
            Kind::Search => (self.widths.search, WidthOf::Search),
            _ => (self.widths.column, WidthOf::Column),
        };
        let title = c.title().into_owned();
        let header_title = title.clone();
        let count = match &c.filter {
            _ if c.error.is_some() || c.unread() => String::new(),
            // Everything narrows the results: how many it lets through, of how many the search finds.
            Some(f) if kind == Kind::Search => format!("{} / {}", time::count(c.total as u64), time::count(f.whole as u64)),
            Some(f) if !f.text.is_empty() => format!("{} / {}", time::count(c.entries.len() as u64), time::count(c.total as u64)),
            _ => time::count(c.total as u64),
        };
        let filtering = c.filter.as_ref().is_some_and(|f| !f.text.is_empty());
        let field = self.filters.get(&id).map(|field| field.input.clone());
        let path = shown(&c.path);
        let (star_key, narrowing) = (favorite_key(&c.path, c.narrowing().as_deref()), c.narrowing());
        let body = self.column_body(index, marked, cx);
        let sort_button = self.sort_button(index, window, cx);
        let headings = self.headings(index, cx);
        div()
            .id(("column", id))
            .role(Role::Group)
            .aria_label(format!("{title} カラム"))
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
                    .id(("head", id))
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
                            .when(!path.is_empty(), |d| d.tip(path))
                            .child(header_title),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_faint).child(count))
                    .child(sort_button)
                    .child({
                        // What it is narrowed to now is part of what is kept.
                        let kept = match kind {
                            Kind::Search => self
                                .settings
                                .saved(&self.state.query, self.state.search_filter.as_deref(), narrowing.as_deref())
                                .is_some(),
                            _ => self.settings.is_favorite(&star_key),
                        };
                        let label = match (kind, kept) {
                            (Kind::Search, true) => "保存した検索を解除（Ctrl+D）",
                            (Kind::Search, false) => "検索を保存（Ctrl+D）",
                            (_, true) => "お気に入りを解除（Ctrl+D）",
                            (_, false) => "お気に入りに追加（Ctrl+D）",
                        };
                        Button::new(("star", id))
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
                            }))
                    })
                    .test_support(),
            )
            .when_some(field, |d, input| {
                d.child(div().flex_none().px_2().py(px(6.)).border_b_1().border_color(p.border).child(self.filter_field(id, filtering, &input, window, cx)))
            })
            .child(headings)
            .child(body)
            .child(self.resizer(if kind == Kind::Search { "search-resizer" } else { "column-resizer" }, of, width, cx))
            .test_support()
            .into_any_element()
    }

    /// The button of the column's own order, beside its star: accented while it is given one apart from
    /// the default, and its menu under it.
    fn sort_button(&mut self, index: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let c = &self.state.columns[index];
        let id = c.id;
        let (order, own) = (self.order_of(c), self.own_order(c).is_some());
        let open = self.column_sort_open() == Some(id);
        let label = format!("並び順: {}{}", order.label(), if own { "" } else { "（既定）" });
        let focus = self.stop(format!("column-sort:{id}"), cx);
        let keyed = keyed(&focus, window);
        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id(("column-sort", id))
                    .test_support()
                    .track_focus(&focus)
                    .key_context(SORT)
                    .on_action(cx.listener(move |this, _: &OpenSort, window, cx| this.open_column_sort(id, window, cx)))
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .aria_expanded(open)
                    .tip(label)
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .border_2()
                    .border_color(if keyed { p.focus } else { transparent_black() })
                    .hover(|d| d.bg(p.hover))
                    .when(open, |d| d.bg(p.hover))
                    .child(icon("arrow-up-down", 14., if own { p.accent } else { p.text_muted }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.column_sort_open() == Some(id) {
                            this.close_menus(true, window, cx);
                        } else {
                            this.open_column_sort(id, window, cx);
                        }
                    })),
            )
            .when_some(self.menu_element("column-sort-menu", "並び順", cx).filter(|_| open), |d, menu| {
                d.child(deferred(div().absolute().top(px(28.)).right_0().w(px(160.)).child(menu)).with_priority(1))
            })
            .into_any_element()
    }

    /// The headings over the column's rows, each sorting it by what it heads; the one it goes by shows
    /// the way it runs. Laid out as a row is, so each stands over its cells.
    fn headings(&self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let c = &self.state.columns[index];
        let id = c.id;
        let order = self.order_of(c);
        let check_all = (c.kind != Kind::Home).then(|| self.check_all(index, cx));
        let heading = |by: Sort, name: &'static str| {
            let chosen = order.by == by;
            let label = if chosen { format!("{}で並べ替え（{}）", by.label(), direction_label(order.descending)) } else { format!("{}で並べ替え", by.label()) };
            div()
                .id((name, id))
                .role(Role::Button)
                .aria_label(label)
                .h_full()
                .px(px(4.))
                .flex()
                .items_center()
                .gap(px(2.))
                .rounded(px(4.))
                .hover(|d| d.bg(p.hover))
                .text_color(if chosen { p.text } else { p.text_muted })
                .child(by.label())
                .when(chosen, |d| d.child(icon(if order.descending { "arrow-down" } else { "arrow-up" }, 11., p.text_muted)))
                .on_click(cx.listener(move |this, _, _, cx| this.sort_by_heading(id, by, cx)))
                .test_support()
        };
        // The row's room: the list's 4px, then the row's 8px and its 2px ring; inside, the gap between cells.
        // Each heading reaches 4px past its cells, over the gap, so its text stands where theirs does.
        div()
            .flex_none()
            .h(px(24.))
            .px(px(10.))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .text_size(px(11.))
            .children(check_all)
            .child(heading(Sort::Name, "sort-by-name").flex_1().min_w_0())
            .child(heading(Sort::Size, "sort-by-size").w(px(SIZE_WIDTH + 8.)).flex_none())
            .child(heading(Sort::Modified, "sort-by-modified").w(px(MODIFIED_WIDTH + 8.)).flex_none())
            .child(div().w(px(13. + 8.)).flex_none())
            .into_any_element()
    }

    /// The checkbox heading the rows' own, over them: checked while every row shown is picked, a click
    /// then picks none, and otherwise all. Faded, and pressed in vain, while there are no rows.
    fn check_all(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let p = self.palette;
        let c = &self.state.columns[index];
        let (id, empty, all) = (c.id, c.entries.is_empty(), c.all_selected());
        let check = div()
            .size(px(14.))
            .rounded(px(3.))
            .flex()
            .items_center()
            .justify_center()
            .map(|b| if all { b.bg(p.accent_fill).child(icon("check", 11., p.on_accent)) } else { b.border_1().border_color(p.text_muted) });
        div()
            .id(("check-all", id))
            .flex_none()
            .h_full()
            // Where the rows' checkboxes stand, then the gap to their names less the heading's own 4px.
            .ml(px(4.))
            .mr(px(4.))
            .w(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .role(Role::CheckBox)
            .aria_label("すべて選択")
            .aria_toggled(if all { Toggled::True } else { Toggled::False })
            .when(empty, |d| d.opacity(0.4))
            .child(check)
            .when(!empty, |d| {
                d.on_click(cx.listener(move |this, _, _, cx| {
                    let Some(index) = this.state.index_of(id) else { return };
                    this.state.toggle_all(index);
                    this.state.set_active(index);
                    this.changed(cx);
                }))
            })
            .test_support()
    }

    /// The field narrowing the column `id` to the names holding what is typed there.
    fn filter_field(&self, id: ColumnId, filtering: bool, input: &Entity<InputState>, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = self.palette;
        let focused = self.has_keys(input, window, cx);
        let has_text = !input.read(cx).value().is_empty();
        // Ringed as the search field is; the ring lies over the gaps around.
        frame(p, focused).m(px(-3.)).child(
            div()
                .id(("filter", id))
                .w_full()
                .h(px(28.))
                .pl(px(8.))
                .pr(px(2.))
                .flex()
                .items_center()
                .gap_2()
                .bg(p.surface)
                .border_1()
                .border_color(p.border_strong)
                .rounded(px(6.))
                // Accented while it has the keys, or narrows the rows from out of them.
                .child(icon("funnel", 13., if focused || filtering { p.accent } else { p.text_muted }))
                .child(div().flex_1().min_w_0().capture_any_mouse_down(Self::right_press_menu(input, cx)).child(Input::new(input)))
                // ↓ picks a row it lets through, as a click does, going there; Esc clears it, then goes there too.
                .capture_action(cx.listener(move |this, _: &input::MoveDown, window, cx| {
                    cx.stop_propagation();
                    this.open_from_filter(id, window, cx);
                }))
                .on_action(cx.listener(move |this, _: &input::Escape, window, cx| {
                    cx.stop_propagation();
                    this.escape_filter(id, window, cx);
                }))
                // ← and → past the text's ends stay here, not going on to the next column; Enter does
                // as ↓ does.
                .on_action(|_: &input::MoveLeft, _, cx| cx.stop_propagation())
                .on_action(|_: &input::MoveRight, _, cx| cx.stop_propagation())
                .on_action(|_: &input::Enter, _, cx| cx.stop_propagation())
                .when(has_text, |b| {
                    b.child(
                        Button::new(("clear-filter", id))
                            .ghost()
                            .xsmall()
                            .icon(glyph("x"))
                            .accessibility_label("フィルタをクリア")
                            .tooltip("フィルタをクリア（Esc）")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                if let Some(field) = this.filters.get(&id) {
                                    let input = field.input.clone();
                                    input.update(cx, |input, cx| {
                                        input.set_value("", window, cx);
                                        input.focus(window, cx);
                                    });
                                    this.filter(id, "", cx);
                                }
                            })),
                    )
                })
                .test_support(),
        )
    }

    fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let home = &self.state.columns[0];
        let (id, filtering) = (home.id, home.filter.is_some());
        let field = self.filters.get(&id).map(|field| field.input.clone());
        let marked = self.state.active == 0 && !self.search_focused;
        let body = self.column_body(0, marked, cx);
        let width = self.sidebar_width(window);
        let keys = div().id("home").track_focus(&self.home_focus).flex_1().min_h_0().relative().flex().flex_col().child(self.takes_text(&self.home_focus, cx)).child(body);
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
            // Headed as a column is, so its filter field stands level with theirs.
            .child(
                div()
                    .id("home-head")
                    .h(px(34.))
                    .flex_none()
                    .pl_3()
                    .pr_2()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .when(marked, |d| d.border_b_2().border_color(p.accent))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(12.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if marked { p.text } else { p.text_muted })
                            .child("ホーム"),
                    ),
            )
            .when_some(field, |d, input| {
                d.child(div().flex_none().px_2().py(px(6.)).border_b_1().border_color(p.border).child(self.filter_field(id, filtering, &input, window, cx)))
            })
            .child(keys)
            .child(self.resizer("sidebar-resizer", WidthOf::Sidebar, width, cx))
            .test_support()
            .into_any_element()
    }

    fn render_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focused = self.search_focused;
        let has_query = !self.state.query.is_empty();
        let sort = self.settings.sort;
        let (back, forward) = (self.state.can_go_back(), self.state.can_go_forward());

        let sort_keyed = keyed(&self.sort_focus, window);
        let filter_button = self.filter_button(window, cx);
        div()
            .h(px(48.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    .child(
                        Self::icon_button("back", "arrow-left", "戻る", "戻る（Alt+←）", back)
                            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx))),
                    )
                    .child(
                        Self::icon_button("forward", "arrow-right", "進む", "進む（Alt+→）", forward)
                            .on_click(cx.listener(|this, _, window, cx| this.forward(window, cx))),
                    ),
            )
            .child(
                // Ringed as a dialog's field is; the ring lies over the gaps around, not in the row. However
                // narrow the window, it keeps room to type in.
                frame(p, focused).flex_1().min_w(px(SEARCH_FIELD_MIN)).m(px(-3.)).child(
                    div()
                        .id("search-box")
                        .w_full()
                        .h(px(32.))
                        .pl(px(10.))
                        .pr(px(4.))
                        .flex()
                        .items_center()
                        .gap_2()
                        .bg(p.surface)
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(6.))
                        .child(icon("search", 15., if focused { p.accent } else { p.text_muted }))
                        .child(div().flex_1().min_w_0().capture_any_mouse_down(Self::right_press_menu(&self.search, cx)).child(Input::new(&self.search)))
                        // ↓ goes into the results, as from a search field in Explorer; Esc clears the search.
                        .capture_action(cx.listener(|this, _: &input::MoveDown, window, cx| {
                            cx.stop_propagation();
                            this.leave_search(window, cx);
                        }))
                        // Esc empties the field, then lets go of the filter, and home shows alone.
                        .on_action(cx.listener(|this, _: &input::Escape, window, cx| {
                            if !this.state.query.is_empty() {
                                this.clear_search(window, cx);
                            } else if this.chosen_filter.is_some() {
                                this.choose_filter(None, cx);
                            }
                        }))
                        .when(has_query, |b| {
                            b.child(
                                Button::new("clear-search")
                                    .ghost()
                                    .xsmall()
                                    .icon(glyph("x"))
                                    .accessibility_label("検索をクリア")
                                    .tooltip("検索をクリア（Esc）")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.clear_search(window, cx);
                                        this.search.update(cx, |input, cx| input.focus(window, cx));
                                    })),
                            )
                        })
                        .test_support(),
                ),
            )
            .child(filter_button)
            .child(
                div()
                    .relative()
                    .child(
                        frame(p, sort_keyed).m(px(-3.)).child(
                            div()
                                .id("sort")
                                .track_focus(&self.sort_focus)
                                .key_context(SORT)
                                .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_sort(window, cx)))
                                .role(Role::Button)
                                .aria_label(format!("既定の並び順: {}", sort.label()))
                                .aria_expanded(self.sort_open())
                                .tip("既定の並び順")
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
                                .hover(|d| d.bg(p.hover))
                                .child(sort.label())
                                .child(icon("chevron-down", 14., p.text_faint))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    if this.sort_open() {
                                        this.close_menus(true, window, cx);
                                    } else {
                                        this.open_sort(window, cx);
                                    }
                                }))
                                .test_support(),
                        ),
                    )
                    .when_some(self.menu_element("sort-menu", "既定の並び順", cx).filter(|_| self.sort_open()), |d, menu| {
                        d.child(deferred(div().absolute().top(px(36.)).right_0().w(px(160.)).child(menu)).with_priority(1))
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_none()
                    .gap(px(2.))
                    .child(
                        keep_focus(Self::icon_button("refresh", "refresh-cw", "更新", "更新（F5）", true))
                            .on_click(cx.listener(|this, _, _, cx| this.refresh(cx))),
                    )
                    .child(
                        Self::icon_button("preferences", "sliders-horizontal", "設定", "設定", true)
                            .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx))),
                    ),
            )
            .into_any_element()
    }

    /// The search filters' button beside the field, naming the one chosen in the accent color, and its menu.
    fn filter_button(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focus = self.stop("search-filter", cx);
        let chosen = self.chosen_filter.clone();
        let on = chosen.is_some();
        let label = format!("検索フィルター: {}", chosen.as_deref().unwrap_or("なし"));
        // In a narrow window its name gives way to the search field, down to the funnel and the chevron;
        // the tip still names it.
        div()
            .relative()
            // The funnel and the chevron with their padding, gaps and border: the button never goes under
            // the sort order's.
            .min_w(px(60.))
            .child(
                frame(p, keyed(&focus, window)).min_w_0().m(px(-3.)).child(
                    div()
                        .id("search-filter")
                        .track_focus(&focus)
                        .key_context(SORT)
                        .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_filter_menu(window, cx)))
                        .role(Role::Button)
                        .aria_label(label.clone())
                        .aria_expanded(self.filter_open())
                        .h(px(32.))
                        .max_w(px(200.))
                        .pl(px(10.))
                        .pr_2()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(if on { p.accent } else { p.text_muted })
                        .bg(p.surface)
                        .border_1()
                        .border_color(if on { p.accent } else { p.border_strong })
                        .rounded(px(6.))
                        .hover(|d| d.bg(p.hover))
                        .tip(format!("{label}（Ctrl+Shift+F）"))
                        .child(icon("funnel", 14., if on { p.accent } else { p.text_muted }))
                        .child(div().min_w_0().truncate().child(chosen.unwrap_or_else(|| "フィルター".into())))
                        .child(icon("chevron-down", 14., p.text_faint))
                        .on_click(cx.listener(|this, _, window, cx| {
                            if this.filter_open() {
                                this.close_menus(true, window, cx);
                            } else {
                                this.open_filter_menu(window, cx);
                            }
                        }))
                        .test_support(),
                ),
            )
            .when_some(self.menu_element("search-filter-menu", "検索フィルター", cx).filter(|_| self.filter_open()), |d, menu| {
                d.child(deferred(div().absolute().top(px(36.)).right_0().min_w(px(200.)).max_w(px(320.)).child(menu)).with_priority(1))
            })
            .into_any_element()
    }

    /// The trail from the search or the drive down to the deepest folder; when it outgrows the bar, the
    /// crumbs furthest from the deepest fold into a menu so the nearest stay readable.
    /// The crumbs of the trail: the results' own while they are pinned, then each folder column's.
    fn trail(&self) -> Vec<Crumb> {
        let mut crumbs = Vec::new();
        if self.state.pinned() {
            let searched = match (&self.state.search_filter, self.state.query.as_str()) {
                (Some(filter), "") => format!("フィルター: {filter}"),
                (Some(filter), query) => format!("{query}（フィルター: {filter}）"),
                (None, query) => query.to_owned(),
            };
            crumbs.push(Crumb { column: 1, name: "検索結果".into(), path: searched });
        }
        for (index, c) in self.state.columns.iter().enumerate().filter(|(_, c)| c.kind == Kind::Folder) {
            crumbs.push(Crumb { column: index, name: shown(folder_name(&c.path)), path: shown(&c.path) });
        }
        crumbs
    }

    /// Each crumb's whole width as drawn: its 12px name, padding, the search icon on the results' own,
    /// the chevron before it and the gap after.
    fn crumb_widths(&self, crumbs: &[Crumb], window: &Window) -> Vec<f32> {
        let results = self.state.pinned();
        let font = font("Segoe UI");
        crumbs
            .iter()
            .enumerate()
            .map(|(k, Crumb { name: label, .. })| {
                let run = TextRun { len: label.len(), font: font.clone(), color: self.palette.text, background_color: None, underline: None, strikethrough: None };
                let shaped = window.text_system().shape_line(label.clone().into(), px(12.), &[run], None);
                let icon = if results && k == 0 { 12. + 4. } else { 0. };
                // The first stands at the bar's start, with no chevron or gap before it.
                let before = if k == 0 { 0. } else { crate::crumbs::SEPARATOR + crate::crumbs::GAP };
                f32::from(shaped.width).ceil() + 12. + icon + before
            })
            .collect()
    }

    fn render_pathbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let file_actions = self.render_actions(cx);
        let p = self.palette;
        let crumbs = self.trail();
        let widths = self.crumb_widths(&crumbs, window);
        // The room the bar was last drawn with; before the first frame, what the window leaves it.
        let room = match self.crumbs.row.get() {
            Some(row) => f32::from(row.size.width),
            None => f32::from(window.viewport_size().width) - 26. - 16. - file_actions.0,
        };
        self.crumbs.room.set(room);
        let folded = crate::crumbs::folded(&widths, room);
        let last = crumbs.len().saturating_sub(1);
        self.crumbs.bounds.borrow_mut().retain(|index, _| crumbs[folded..].iter().any(|c| c.column == *index));
        for Crumb { column: index, .. } in &crumbs[folded..] {
            self.stop(format!("crumb:{index}"), cx);
        }
        let shown: Vec<Crumb> = crumbs[folded..].to_vec();
        let menu_current = crumbs[..folded].iter().any(|c| c.column == self.state.active);
        self.crumbs.folded = crumbs[..folded].to_vec();
        let fold = (folded > 0).then(|| self.crumb_fold(menu_current, cx));
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
                        let (row, room) = (self.crumbs.row.clone(), self.crumbs.room.clone());
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
                    .children(fold)
                    .children(shown.into_iter().enumerate().map(|(k, crumb)| {
                        let max_width = (folded + k == last).then_some(room / 2.);
                        self.crumb(crumb, k > 0 || folded > 0, max_width, cx)
                    })),
            )
            .child(file_actions.1)
            .into_any_element()
    }

    /// The "…" button the crumbs folded off the trail go into, with their menu while it is open;
    /// `current` while the column worked in is among them.
    fn crumb_fold(&mut self, current: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let open = self.crumbs_open();
        let menu = if open { self.menu_element("crumb-menu", "上の階層", cx) } else { None };
        let focus = self.stop("crumb-fold", cx);
        div()
            .relative()
            .child(
                ring(div().id("crumb-fold").track_focus(&focus), p)
                    .role(Role::Button)
                    .aria_label("上の階層")
                    .aria_expanded(open)
                    .size(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .hover(|d| d.bg(p.hover))
                    .tip("上の階層")
                    .child(icon("ellipsis", 14., if current { p.text } else { p.text_muted }))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        if this.crumbs_open() {
                            this.close_crumb_menu(event.is_keyboard(), window, cx);
                        } else {
                            this.open_crumb_menu(window, cx);
                        }
                    })),
            )
            .when_some(menu, |d, menu| d.child(deferred(div().absolute().top(px(28.)).left_0().min_w(px(160.)).child(menu)).with_priority(1)))
            .into_any_element()
    }

    /// A crumb on the trail, a chevron before it unless it stands first; the last is clipped to
    /// `max_width`, the others keep their whole names.
    fn crumb(&self, crumb: Crumb, chevron: bool, max_width: Option<f32>, cx: &mut Context<Self>) -> Div {
        let p = self.palette;
        let Crumb { column: index, name: label, path } = crumb;
        let current = index == self.state.active;
        let query = self.state.pinned() && index == 1;
        let focus = self.stops.get(&format!("crumb:{index}")).cloned().unwrap_or_else(|| self.columns_focus.clone());
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            .when_some(max_width, |d, width| d.min_w_0().max_w(px(width)))
            .when(max_width.is_none(), |d| d.flex_none())
            .when(chevron, |d| d.child(icon("chevron-right", 12., p.text_faint)))
            .child(
                ring(div().id(("crumb", index)).track_focus(&focus), p)
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .relative()
                    .child({
                        let crumbs = self.crumbs.bounds.clone();
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
                    .text_color(if current { p.text } else { p.text_muted })
                    .hover(|d| d.bg(p.hover))
                    .tip(path)
                    .when(query, |d| d.child(icon("search", 12., p.text_faint)))
                    .child(div().truncate().child(label))
                    .on_click(cx.listener(move |this, _, window, cx| this.go_to(index, window, cx)))
                    .test_support(),
            )
    }

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let pinned = self.state.pinned();
        let away = self.state.folded_away();
        let first = if pinned { 2 } else { 1 } + away;
        let only_home = self.state.columns.len() == 1;
        self.filter_fields(window, cx);
        let pinned_column = pinned.then(|| self.render_column(1, window, cx));
        let strip = (away > 0).then(|| self.unfold_strip(away, cx));
        let columns: Vec<AnyElement> = (first..self.state.columns.len()).map(|i| self.render_column(i, window, cx)).collect();
        let scrolling = self.scrolling_columns(columns, cx);
        let main = div()
            .id("main")
            .track_focus(&self.columns_focus)
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .bg(p.bg)
            .child(self.takes_text(&self.columns_focus, cx))
            .children(pinned_column)
            .children(strip)
            .child(scrolling)
            // Nothing searched or opened leaves this side dark; show where to start.
            .when(only_home, |d| d.child(self.home_hint(window, cx)));
        self.column_keys(main, cx).into_any_element()
    }

    /// The strip the `away` folders folded above a result stand in for, which brings them out.
    fn unfold_strip(&mut self, away: usize, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focus = self.stop("unfold", cx);
        div()
            .id("unfold-strip")
            .w(px(FOLD_WIDTH))
            .h_full()
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            .hover(|d| d.bg(p.hover))
            // In line with the columns' headings, where the eye runs, and how many it holds below.
            .child(
                div()
                    .id("unfold-head")
                    .h(px(34.))
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(p.border)
                    .child(icon("chevrons-left", 14., p.text_muted))
                    .test_support(),
            )
            .child(div().id("unfold-count").pt_2().text_size(px(12.)).text_color(p.text_muted).child(away.to_string()).test_support())
            // The button laid over the whole strip: its ring's room would push the heading's line off the
            // columns'.
            .child(
                // Gone once pressed: the keys stay where they were rather than go with it.
                keep_focus(ring(div().id("unfold").track_focus(&focus), p))
                    .role(Role::Button)
                    .aria_label(format!("上の階層を表示（{away}階層）"))
                    .absolute()
                    .inset_0()
                    .tip(format!("上の階層を表示（{away}階層）"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.state.unfold();
                        this.changed(cx);
                    }))
                    .test_support(),
            )
            .into_any_element()
    }

    /// The scrolling `columns` in their sideways scroll, with its scrollbar and a shade on each edge that
    /// cuts a column.
    fn scrolling_columns(&self, columns: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let screen = self.on_screen();
        let content = screen.count as f32 * self.widths.column;
        let hold = self.sideways.hold;
        let fit = cx.entity().downgrade();
        let shade = |angle: f32| linear_gradient(angle, linear_color_stop(p.shade, 0.), linear_color_stop(p.shade.opacity(0.), 1.));
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .relative()
            // Laid out with the columns' view and prepainted ahead of it: where the columns rest follows a
            // change in its width before they are drawn.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        fit.update(cx, |this, cx| this.fit(f32::from(bounds.size.width), cx)).ok();
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(
                div()
                    .id("columns-scroll")
                    .size_full()
                    .overflow_x_scroll()
                    // A wheel turned down scrolls the column under it, not the columns sideways.
                    .restrict_scroll_to_axis()
                    .track_scroll(&self.sideways.scroll)
                    // The crumbs' bar and the edges' shades follow the columns as they scroll.
                    .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                    .child(div().h_full().w(px(content)).flex().children(columns))
                    // The room past the last column is no column's: a press there leaves the keys where they
                    // are, rather than the columns' focus sending them beside home.
                    .child(keep_focus(div().id("columns-rest")).absolute().top_0().bottom_0().left(px(content)).right_0().test_support())
                    // Room up to a view's width past where the columns were brought, so the scroll width
                    // cannot shrink under them while they slide back. A view wide however wide layout makes
                    // it: reckoned here, it would be the last frame's.
                    .child(div().absolute().top_0().left(px(hold)).w_full().h(px(0.)))
                    .test_support(),
            )
            .child(div().absolute().inset_0().child(scrollbar(gpui_kit::base::Scrollbar::horizontal(&self.sideways.scroll), p)))
            .when(screen.hidden_left(), |d| d.child(div().absolute().top_0().bottom_0().left_0().w(px(28.)).bg(shade(90.))))
            .when(screen.hidden_right(), |d| d.child(div().absolute().top_0().bottom_0().right_0().w(px(28.)).bg(shade(270.))))
            .into_any_element()
    }

    fn home_hint(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
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
            // Ringed as a field is; the rings' room makes up most of the gaps. In a narrow window they go on
            // to a second line rather than past the edges.
            .child(div().flex().flex_wrap().justify_center().gap(px(2.)).children(
                [("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")].map(
                    |(query, label)| {
                        let focus = self.stop(format!("hint:{query}"), cx);
                        frame(p, keyed(&focus, window)).child(
                            div()
                                .id(query)
                                .track_focus(&focus)
                                .role(Role::Button)
                                .aria_label(format!("{query} {label}"))
                                .h(px(28.))
                                .px(px(10.))
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .bg(p.surface)
                                .border_1()
                                .border_color(p.border)
                                .rounded(px(6.))
                                .hover(|d| d.bg(p.hover))
                                .child(div().text_color(p.accent).font_family("Consolas").child(query))
                                .child(label)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let filter = this.chosen_filter.clone();
                                    this.search_for(query, filter, window, cx)
                                }))
                                .test_support(),
                        )
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
            Kind::Folder => shown(&active.path),
            Kind::Search if active.error.is_some() => "検索できません".into(),
            Kind::Search if active.unread() => "検索中…".into(),
            Kind::Search => "検索結果".into(),
            Kind::Home => "ホーム".into(),
        };
        // What a filter hides is not counted, nor acted on.
        let shown_selected = active.shown_selected();
        let picked: Vec<&Entry> = shown_selected.iter().filter_map(|path| active.entry(path)).collect();
        let detail = match picked.as_slice() {
            _ if shown_selected.len() > 1 => format!("{} 項目を選択", shown_selected.len()),
            [entry] => {
                let what = if entry.directory { "フォルダー".into() } else { time::size(entry.size) };
                match entry.modified.and_then(time::date_time) {
                    Some(when) => format!("{what} · {when}"),
                    None => what,
                }
            }
            _ => String::new(),
        };
        let (name, title) = self.engine.describe();
        let (color, dot) = match &self.engine {
            Engine::Unknown | Engine::Connecting => (p.text_faint, None),
            Engine::Connected(_) => (p.text_faint, Some(p.accent)),
            Engine::Failed(_) => (p.danger, Some(p.danger)),
        };
        StatusBar::new()
            .h(px(26.))
            .flex_none()
            .px_3()
            .gap_4()
            .text_size(px(12.))
            .child(div().id("place").role(Role::Status).aria_label(place.clone()).flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_ellipsis_start().child(place))
            .right(div().flex_none().child(detail))
            .right(
                div()
                    .id("engine")
                    .flex_none()
                    .max_w(px(200.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .text_color(color)
                    .tip(title)
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
            .right(history)
            .into_any_element()
    }

    /// Slides the columns toward where they should rest, a frame at a time.
    fn reveal(&mut self, window: &mut Window) {
        let view = f32::from(self.sideways.scroll.bounds().size.width);
        let mut from = self.sideways.offset();
        let fold = self.state.fold.map(|f| (f.anchor, self.state.folded_away()));
        let came_out = match (self.sideways.drawn_fold, fold) {
            (Some((was, away)), Some((anchor, 0))) if was == anchor && away > 0 => Some(away),
            _ => None,
        };
        self.sideways.drawn_fold = fold;
        let mut keep = None;
        if let Some(away) = came_out
            && view > 0.
        {
            // The columns on screen stay where they are drawn: the folders come out ahead of them, in the
            // room of the strip and past the left edge, and slide in from there.
            let shift = away as f32 * self.widths.column - FOLD_WIDTH;
            from += shift;
            self.sideways.set_offset(from);
            self.sideways.hold = self.sideways.hold.max(from);
            if let Some(slide) = &mut self.sideways.slide {
                slide.shift(shift);
            }
            // Brought out by the strip rather than by going into one of them, as many as fit stand beside
            // the column worked in, or the anchor while the results or home are.
            if let Some(f) = self.state.folded()
                && !(f.from..f.to).contains(&self.state.active)
            {
                let first = if self.state.pinned() { 2 } else { 1 };
                keep = Some(self.state.active.max(f.to) - first);
            }
            self.sideways.ask();
        }
        if let Some(smooth) = self.sideways.pending.take() {
            if view <= 0. {
                // Not laid out yet: ask again next frame.
                self.sideways.pending = Some(smooth);
                window.request_animation_frame();
                return;
            }
            let (cells, active) = self.scrolling();
            let start = self.sideways.slide.as_ref().map_or(from, |s| s.target);
            let shown: Vec<ColumnId> =
                self.state.columns[self.state.columns.len() - cells.len()..].iter().map(|c| c.id).collect();
            let target = match keep {
                Some(keep) => uncover_target(view, &cells, keep),
                // Home or the pinned results taken up again beside the same columns, as ← brings the keys
                // there, leave the columns where they are.
                None if active.is_none() && shown == self.sideways.revealed => start,
                None => reveal_target(start, view, &cells, active),
            };
            self.sideways.revealed = shown;
            self.sideways.hold = target.max(from);
            self.sideways.fitted = (view, self.widths.column);
            if (target - from).abs() >= 0.5 {
                match &mut self.sideways.slide {
                    Some(slide) => {
                        slide.origin = start;
                        slide.target = target;
                        slide.keep = keep;
                    }
                    None => self.sideways.slide = Some(Slide::new(start, from, target, keep)),
                }
            }
        }
        let Some(slide) = &mut self.sideways.slide else { return };
        let (position, done) = slide.step(Instant::now());
        self.sideways.set_offset(position);
        if done {
            // At rest, the room held for the slide goes: left past the last column, it would scroll the
            // columns off to the left.
            self.sideways.hold = position;
            self.sideways.slide = None;
        } else {
            window.request_animation_frame();
        }
    }

    /// The scrolling columns' places, and which of them is active: `None` for home or the pinned results.
    fn scrolling(&self) -> (Vec<Cell>, Option<usize>) {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        let width = self.widths.column;
        let cells = (0..count).map(|i| Cell { start: i as f32 * width, width }).collect();
        (cells, self.state.active.checked_sub(first))
    }

    /// Layout has made the columns' view `view` wide, ahead of drawing them: when it or the columns' width
    /// changed since they were brought to rest, they follow it in this frame. At rest they go straight
    /// to where [`refit`] puts them, as a window or a width is dragged; sliding, as when the folders
    /// folded away come out and their strip goes, they slide on to where they would now rest.
    pub fn fit(&mut self, view: f32, cx: &mut Context<Self>) {
        let (was, was_width) = self.sideways.fitted;
        if view <= 0. || ((view - was).abs() < 0.5 && was_width == self.widths.column) {
            return;
        }
        self.sideways.fitted = (view, self.widths.column);
        // The first layout: nothing rests anywhere yet.
        if was <= 0. {
            return;
        }
        let (cells, active) = self.scrolling();
        let sideways = &mut self.sideways;
        match &mut sideways.slide {
            Some(slide) => {
                slide.target = match slide.keep {
                    Some(keep) => uncover_target(view, &cells, keep),
                    None => reveal_target(slide.origin, view, &cells, active),
                };
                sideways.hold = slide.target.max(slide.position);
            }
            None => {
                let target = refit(sideways.offset(), was, cells.len() as f32 * was_width, view, &cells, active);
                sideways.set_offset(target);
                sideways.hold = target;
            }
        }
        cx.notify();
    }
}

impl Render for Colet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.search_focused = self.has_keys(&self.search, window, cx);
        // A drag in home let go of anywhere but its list.
        if self.home_drag.carried.is_some() && !cx.has_active_drag() {
            self.end_home_drag();
        }
        self.follow_focus(window, cx);
        self.scroll_rows();
        self.reveal(window);
        let p = self.palette;
        let tabbar = self.render_tabbar(window, cx);
        let sidebar = self.render_sidebar(window, cx);
        let toolbar = self.render_toolbar(window, cx);
        let pathbar = self.render_pathbar(window, cx);
        let main = self.render_main(window, cx);
        div()
            .id("colet")
            .size_full()
            .relative()
            .flex()
            .flex_col()
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
            .on_action(cx.listener(|this, _: &OpenTextMenu, window, cx| this.open_focused_text_menu(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSearchFilters, window, cx| {
                if this.dialog.is_none() {
                    this.open_filter_menu(window, cx);
                }
            }))
            // As the strip's button does: the keys stay where they are.
            .on_action(cx.listener(|this, _: &Unfold, _, cx| {
                if this.dialog.is_none() && this.state.folded_away() > 0 {
                    this.state.unfold();
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusHome, window, cx| {
                if this.dialog.is_none() {
                    this.focus_home(window, cx);
                }
            }))
            // Esc puts down a drag in home, before the keys reach anything else.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.home_drag.carried.is_some() && event.keystroke.key == "escape" {
                    cx.stop_active_drag(window);
                    this.end_home_drag();
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &NewTab, window, cx| this.new_tab(window, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let current = this.tabs.current;
                this.close_tab(current, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.step_tab(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| this.step_tab(-1, window, cx)))
            .on_action(cx.listener(|this, _: &ReopenTab, window, cx| this.reopen_tab(window, cx)))
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
            .child(tabbar)
            .child(toolbar)
            .child(pathbar)
            .child(
                div().flex_1().min_h_0().w_full().flex().child(sidebar).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .h_full()
                        .flex()
                        .flex_col()
                        .child(main)
                        .child(self.render_statusbar(cx)),
                ),
            )
            // Over a dialog, a field's menu is drawn with it.
            .children(if self.dialog.is_none() { self.render_menu(cx) } else { None })
            .children(self.render_history(cx))

    }
}

