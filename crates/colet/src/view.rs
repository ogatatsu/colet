//! How the window looks: home's pane on the left; the search field, the trail of folders, the columns
//! and the status bar beside it.
use std::{ops::Range, rc::Rc, time::Instant};

use colet_core::{entry::{Entry, Sort}, paths::{folder_name, path_key}, time};
use gpui_kit::base::input::{self, Input};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    state::saved_query,
    reveal::{Cell, on_screen, refit, reveal_target},
    state::{Column, Kind},
};

/// A line of text shown while the pointer rests on an element.
pub struct Tip {
    pub text: SharedString,
    pub palette: crate::theme::Palette,
    /// The element it is on, when a press takes it down.
    owner: Option<ElementId>,
    /// [`Tips::epoch`] when it came up.
    epoch: u64,
}

/// When tips keep away. GPUI keeps a tooltip up through a press, and over the dialog, menu or panel
/// that opens; Windows takes it down until the pointer leaves what was pressed.
#[derive(Default)]
pub struct Tips {
    /// What was pressed: no tip on it until the pointer leaves it.
    pressed: Option<ElementId>,
    /// The press is still down: GPUI reads a pressed element as not hovered while the pointer moves.
    held: bool,
    /// Which of the dialog, menus and panels are open.
    open: u8,
    /// Counts their opening and closing: a tip up from before goes.
    epoch: u64,
}

impl Global for Tips {}

impl Tips {
    /// Notes which of the dialog, menus and panels are open, taking down the tips up when that changes.
    pub fn set_open(open: u8, cx: &mut App) {
        if cx.try_global::<Tips>().map_or(0, |t| t.open) != open {
            let tips = cx.default_global::<Tips>();
            tips.open = open;
            tips.epoch += 1;
        }
    }
}

impl Render for Tip {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        if cx.try_global::<Tips>().is_some_and(|t| t.epoch != self.epoch || (t.pressed.is_some() && t.pressed == self.owner)) {
            return div();
        }
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

pub trait TipExt {
    /// A tip a press takes down, until the pointer leaves.
    fn tip(self, palette: crate::theme::Palette, text: impl Into<SharedString>) -> Self;
    /// A tip that stays through a press, on what a press does nothing to, such as a button that is off.
    fn steady_tip(self, palette: crate::theme::Palette, text: impl Into<SharedString>) -> Self;
}

impl TipExt for Stateful<Div> {
    fn tip(mut self, palette: crate::theme::Palette, text: impl Into<SharedString>) -> Self {
        let Some(owner) = self.interactivity().element_id.clone() else {
            return self.steady_tip(palette, text);
        };
        let text = text.into();
        let released = |_: &MouseUpEvent, _: &mut Window, cx: &mut App| cx.default_global::<Tips>().held = false;
        self.on_mouse_down(MouseButton::Left, {
            let owner = owner.clone();
            move |_, window, cx| {
                let tips = cx.default_global::<Tips>();
                tips.pressed = Some(owner.clone());
                tips.held = true;
                window.refresh();
            }
        })
        .on_mouse_up(MouseButton::Left, released)
        .on_mouse_up_out(MouseButton::Left, released)
        // A key press leaves nothing.
        .hover_listener_mode(HoverListenerMode::InputModalityIndependent)
        .on_hover({
            let owner = owner.clone();
            move |hovered, _, cx| {
                let tips = cx.default_global::<Tips>();
                if !hovered && !tips.held && tips.pressed.as_ref() == Some(&owner) {
                    tips.pressed = None;
                }
            }
        })
        .tooltip(move |_, cx| new_tip(text.clone(), palette, Some(owner.clone()), cx))
    }

    fn steady_tip(self, palette: crate::theme::Palette, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |_, cx| new_tip(text.clone(), palette, None, cx))
    }
}

fn new_tip(text: SharedString, palette: crate::theme::Palette, owner: Option<ElementId>, cx: &mut App) -> AnyView {
    let epoch = cx.try_global::<Tips>().map_or(0, |t| t.epoch);
    cx.new(|_| Tip { text, palette, owner, epoch }).into()
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
pub fn keep_focus(el: Stateful<Div>) -> Stateful<Div> {
    el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

/// `text` with what a search matched in it on a marker, in `ink` when given.
fn with_marks(text: &str, marks: &[Range<usize>], mark: Hsla, ink: Option<Hsla>) -> StyledText {
    let style = HighlightStyle { background_color: Some(mark), color: ink, ..Default::default() };
    StyledText::new(text.to_owned()).with_highlights(marks.iter().map(|range| (range.clone(), style)))
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
        // A wheel moves the offset past its bounds until the next layout brings it back: read it as it
        // will be drawn, so the edges' shades do not flash on.
        let max = f32::from(self.columns_scroll.max_offset().x).max(self.hold);
        let from = (-f32::from(self.columns_scroll.offset().x)).clamp(0., max);
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
            .on_action(cx.listener(|this, _: &OpenItem, window, cx| {
                let index = this.state.active;
                if let Some(path) = this.state.columns[index].focused.clone() {
                    this.open(index, &path, window, cx);
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
        let held = !self.held.is_empty() && self.held.contains(&path_key(&entry.path));
        let cut = held && self.clipboard.cut;
        let faded = (entry.hidden || cut) && !selected;
        // The keys are on it: ringed, selected or not, while they are in this column.
        let focused = marked && self.keys_in_column && c.focused.as_ref() == Some(&entry.path);
        // What a right-click opened the menu on, which it leaves unselected: ringed, as in Finder, until it closes.
        let target = self
            .menu
            .as_ref()
            .is_some_and(|m| m.column == c.id && m.paths.as_deref() == Some(std::slice::from_ref(&entry.path)));
        // The item a search result or favorite points at: a bar, which wins over the selection but not
        // over the one in the column worked in. Home marks its pick so while the columns are worked in.
        let barred = (highlighted && (!selected || !marked)) || (kind == Kind::Home && selected && !marked);
        // The menu acts on a row outside the selection, or the folder itself: the selection steps back, so
        // only what the menu acts on stands out.
        let aside = self.menu.as_ref().is_some_and(|m| m.column == c.id && m.paths.is_some());
        let lit = selected && marked && !aside;
        let image = self.icons.get(entry, cx);
        let muted = if lit { p.on_accent_muted } else { p.text_muted };
        let (mark, ink) = if lit { (p.mark_on_fill, p.on_accent) } else { (p.mark, p.text) };
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
                            .child(with_marks(&entry.name, &entry.marks.name, mark, None)),
                    )
                    .when(kind == Kind::Search, |d| {
                        d.child(div().w_full().overflow_hidden().whitespace_nowrap().text_ellipsis_start().text_size(px(12.)).text_color(muted).child(with_marks(&entry.parent, &entry.marks.parent, mark, Some(ink))))
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
            .when(self.sort.by != Sort::Name && kind != Kind::Home && !(self.sort.by == Sort::Size && entry.directory), |r| {
                let meta = match self.sort.by {
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
                    this.open(index, &path, window, cx);
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
    fn column_body(&mut self, index: usize, marked: bool, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let c = &self.state.columns[index];
        let (id, kind) = (c.id, c.kind);
        if let Some(error) = c.error.clone() {
            let retry = self.stop(format!("retry:{id}"), cx);
            let settings = self.stop("search-settings", cx);
            // Ringed as a field is; the ring lies over the gaps around.
            return self
                .message("circle-alert", error)
                .child(frame(p, keyed(&retry, window)).m(px(-3.)).child(
                    keep_focus(div().id(("retry", id)).track_focus(&retry))
                        .role(Role::Button)
                        .aria_label("再試行")
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
                        .hover(|b| b.bg(p.raised_hover))
                        .child(icon("refresh-cw", 14., p.text_muted))
                        .child("再試行")
                        .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                            // The button goes as the column loads: the keys go back to the columns.
                            if event.is_keyboard() {
                                this.focus_active(window, cx);
                            }
                            this.load(id, 0, false, cx);
                        }))
                        .test_support(),
                ))
                .when(kind == Kind::Search, |d| {
                    d.child(frame(p, keyed(&settings, window)).m(px(-3.)).child(
                        div()
                            .id("search-settings")
                            .track_focus(&settings)
                            .role(Role::Button)
                            .aria_label("設定")
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
                            .hover(|b| b.bg(p.raised_hover))
                            .child(icon("sliders-horizontal", 14., p.text_muted))
                            .child("設定")
                            .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx)))
                            .test_support(),
                    ))
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
            let groups = crate::places::home_groups(&self.settings, &entries);
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
        let title = self.state.columns[index].title().to_owned();
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
        let title = c.title().to_owned();
        let header_title = title.clone();
        let count = if c.error.is_some() || c.unread() { String::new() } else { time::count(c.total as u64) };
        let path = c.path.clone();
        let star_path = c.path.clone();
        let star_focus = self.stop(format!("star:{id}"), cx);
        let body = self.column_body(index, marked, window, cx);
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
                    })
                    .test_support(),
            )
            .child(body)
            .child(self.resizer(if kind == Kind::Search { "search-resizer" } else { "column-resizer" }, of, width, cx))
            .test_support()
            .into_any_element()
    }

    fn render_sidebar(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let (back, forward) = (self.state.can_go_back(), self.state.can_go_forward());
        let marked = self.state.active == 0 && !self.search_focused;
        let body = self.column_body(0, marked, window, cx);
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

    fn render_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focused = self.search_focused;
        let has_query = !self.state.query.is_empty();
        let sort = self.sort;
        let clear_focus = self.stop("clear-search", cx);
        let sort_lines = if self.sort_open { self.render_lines(cx) } else { Vec::new() };
        let sort_keyed = keyed(&self.sort_focus, window);
        div()
            .h(px(48.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .child(
                // Ringed as a dialog's field is; the ring lies over the gaps around, not in the row.
                frame(p, focused).flex_1().min_w_0().m(px(-3.)).child(
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
                ),
            )
            .child(
                div()
                    .relative()
                    .child(
                        frame(p, sort_keyed).m(px(-3.)).child(
                            div()
                                .id("sort")
                                .track_focus(&self.sort_focus)
                                .key_context(SORT)
                                .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_sort(true, window, cx)))
                                .role(Role::Button)
                                .aria_label(format!("並び順: {}", sort.label()))
                                .aria_expanded(self.sort_open)
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
                        ),
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
                        keep_focus(self.icon_button("refresh", "refresh-cw", "更新", true, cx))
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
        let crumb_cursor = self.crumb_cursor;
        let fold_focus = self.stop("crumb-fold", cx);
        // Where ↑↓ and Enter take the menu: the columns its lines go to.
        let menu_targets: Vec<usize> = menu.iter().map(|(index, ..)| *index).collect();
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
                                    ring(div().id("crumb-fold").track_focus(&fold_focus), p)
                                        .role(Role::Button)
                                        .aria_label("上の階層")
                                        .aria_expanded(crumb_menu_open)
                                        .size(px(24.))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .rounded(px(4.))
                                        .cursor_pointer()
                                        .hover(|d| d.bg(p.hover))
                                        .tip(p, "上の階層")
                                        .child(icon("ellipsis", 14., if menu_current { p.text } else { p.text_muted }))
                                        .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                                            if this.crumb_menu_open {
                                                this.close_crumb_menu(event.is_keyboard(), window, cx);
                                            } else {
                                                this.open_crumb_menu(event.is_keyboard(), window, cx);
                                            }
                                        })),
                                )
                                .when(crumb_menu_open, |d| {
                                    d.child(
                                        deferred(
                                            div()
                                                .id("crumb-menu")
                                                .role(Role::Menu)
                                                .aria_label("上の階層")
                                                // ↑↓ move, Enter goes, Esc closes, as in the other menus.
                                                .track_focus(&self.menu_focus)
                                                .key_context(MENU)
                                                .on_action({
                                                    let count = menu_targets.len();
                                                    cx.listener(move |this, _: &MenuUp, _, cx| this.move_crumb_cursor(-1, count, cx))
                                                })
                                                .on_action({
                                                    let count = menu_targets.len();
                                                    cx.listener(move |this, _: &MenuDown, _, cx| this.move_crumb_cursor(1, count, cx))
                                                })
                                                .on_action(cx.listener(move |this, _: &MenuConfirm, window, cx| {
                                                    if let Some(&index) = this.crumb_cursor.and_then(|c| menu_targets.get(c)) {
                                                        this.close_crumb_menu(false, window, cx);
                                                        this.go_to(index, window, cx);
                                                    }
                                                }))
                                                .on_action(cx.listener(|this, _: &CloseMenu, window, cx| this.close_crumb_menu(true, window, cx)))
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
                                                .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_crumb_menu(false, window, cx)))
                                                .children(menu.iter().enumerate().map(|(line, (index, label, path))| {
                                                    let index = *index;
                                                    let lit = crumb_cursor == Some(line);
                                                    div()
                                                        .id(("crumb-item", index))
                                                        .role(Role::MenuItem)
                                                        .aria_label(label.clone())
                                                        .h(px(28.))
                                                        .px(px(10.))
                                                        .flex()
                                                        .items_center()
                                                        .gap_2()
                                                        .rounded(px(4.))
                                                        .cursor_pointer()
                                                        .text_color(if index == active { p.text } else { p.text_muted })
                                                        .hover(|d| d.bg(p.accent_fill).text_color(p.on_accent))
                                                        .when(lit, |d| d.bg(p.accent_fill).text_color(p.on_accent))
                                                        .tip(p, path.clone())
                                                        .child(icon(if index == 1 && results { "search" } else { "folder" }, 14., p.text_faint))
                                                        .child(label.clone())
                                                        .on_click(cx.listener(move |this, _, window, cx| {
                                                            this.close_crumb_menu(false, window, cx);
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
        let screen = self.on_screen();
        let (hidden_left, hidden_right) = (screen.hidden_left(), screen.hidden_right());
        let hold = self.hold;
        let fit = cx.entity().downgrade();
        let pinned_column = pinned.then(|| self.render_column(1, window, cx));
        let columns: Vec<AnyElement> = (first..self.state.columns.len()).map(|i| self.render_column(i, window, cx)).collect();
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
                        .child(
                            div()
                                .id("unfold-count")
                                .pt_2()
                                .text_size(px(12.))
                                .text_color(p.text_muted)
                                .child(away.to_string())
                                .test_support(),
                        )
                        // The button laid over the whole strip: its ring's room would push the heading's line
                        // off the columns'.
                        .child(
                            // Gone once pressed: the keys stay where they were rather than go with it.
                            keep_focus(ring(div().id("unfold").track_focus(&unfold_focus), p))
                                .role(Role::Button)
                                .aria_label(format!("上の階層を表示（{away}階層）"))
                                .absolute()
                                .inset_0()
                                .cursor_pointer()
                                .tip(p, format!("上の階層を表示（{away}階層）"))
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.state.unfold();
                                    this.changed(cx);
                                }))
                                .test_support(),
                        ),
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .relative()
                    // Laid out with the columns' view and prepainted ahead of it: where the columns rest
                    // follows a change in its width before they are drawn.
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
                            .track_scroll(&self.columns_scroll)
                            // The crumbs' bar and the edges' shades follow the columns as they scroll.
                            .on_scroll_wheel(cx.listener(|_, _, _, cx| cx.notify()))
                            .child(div().h_full().w(px(content)).flex().children(columns))
                            // The room past the last column is no column's: a press there leaves the keys where
                            // they are, rather than the columns' focus sending them beside home.
                            .child(keep_focus(div().id("columns-rest")).absolute().top_0().bottom_0().left(px(content)).right_0().test_support())
                            // Room up to a view's width past where the columns were brought, so the scroll
                            // width cannot shrink under them while they slide back. A view wide however
                            // wide layout makes it: reckoned here, it would be the last frame's.
                            .child(div().absolute().top_0().left(px(hold)).w_full().h(px(0.)))
                            .test_support(),
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
            .when(only_home, |d| d.child(self.home_hint(window, cx)));
        let _ = window;
        self.column_keys(main, cx).into_any_element()
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
            // Ringed as a field is; the rings' room makes up most of the gaps.
            .child(div().flex().gap(px(2.)).children(
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
                                .cursor_pointer()
                                .hover(|d| d.bg(p.hover))
                                .child(div().text_color(p.accent).font_family("Consolas").child(query))
                                .child(label)
                                .on_click(cx.listener(move |this, _, window, cx| this.search_for(query, window, cx)))
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
        let (name, title) = self.engine.describe();
        let (color, dot) = match &self.engine {
            Engine::Unknown | Engine::Connecting => (p.text_faint, None),
            Engine::Connected(_) => (p.text_faint, Some(p.accent)),
            Engine::Failed(_) => (p.danger, Some(p.danger)),
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

    fn render_toasts(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let ids: Vec<u64> = self.toasts.iter().map(|t| t.id).collect();
        let stops: Vec<(FocusHandle, FocusHandle)> =
            ids.into_iter().map(|id| (self.stop(format!("toast-details:{id}"), cx), self.stop(format!("close-toast:{id}"), cx))).collect();
        // Over the columns' edges, which are drawn over all laid out after them.
        deferred(div()
            .absolute()
            .bottom(px(36.))
            .right_3()
            .flex()
            .flex_col()
            .gap_2()
            .children(self.toasts.iter().zip(stops).map(|(toast, (details, close))| {
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
                        // The mouse leaves the keys where they are; from the keys, they go on into the history.
                        d.child(
                            keep_focus(ring(div().id(("toast-details", id)).track_focus(&details), p))
                                .role(Role::Button)
                                .aria_label("詳細")
                                .px_2()
                                .h(px(24.))
                                .flex()
                                .items_center()
                                .rounded(px(4.))
                                .text_color(p.accent)
                                .cursor_pointer()
                                .hover(|d| d.bg(p.raised_hover))
                                .child("詳細")
                                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                    this.dismiss(id, cx);
                                    this.show_history(true, Some(entry), window, cx);
                                    if event.is_keyboard() {
                                        this.stop(format!("history-entry:{entry}"), cx).focus(window, cx);
                                    }
                                }))
                                .test_support(),
                        )
                    })
                    .child(
                        keep_focus(ring(div().id(("close-toast", id)).track_focus(&close), p))
                            .role(Role::Button)
                            .aria_label("閉じる")
                            .size(px(24.))
                            .flex()
                            .flex_none()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.raised_hover))
                            .child(icon("x", 14., p.text_muted))
                            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                                // The button goes with the toast: the keys go back to the columns.
                                if event.is_keyboard() {
                                    this.focus_active(window, cx);
                                }
                                this.dismiss(id, cx);
                            }))
                            .test_support(),
                    )
            })))
            .with_priority(1)
            .into_any_element()
    }

    /// Slides the columns toward where they should rest, a frame at a time.
    fn reveal(&mut self, window: &mut Window) {
        let view = f32::from(self.columns_scroll.bounds().size.width);
        let from = -f32::from(self.columns_scroll.offset().x);
        if let Some(smooth) = self.reveal_pending.take() {
            if view <= 0. {
                // Not laid out yet: ask again next frame.
                self.reveal_pending = Some(smooth);
                window.request_animation_frame();
                return;
            }
            let (cells, active) = self.scrolling();
            let start = self.slide.as_ref().map_or(from, |s| s.target);
            let target = reveal_target(start, view, &cells, active);
            self.hold = target.max(from);
            self.fitted = (view, self.widths.column);
            if (target - from).abs() >= 0.5 {
                match &mut self.slide {
                    Some(slide) => {
                        slide.origin = start;
                        slide.target = target;
                    }
                    None => {
                        self.slide = Some(Slide { origin: start, position: from, target, velocity: 0., time: None })
                    }
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
            // At rest, the room held for the slide goes: left past the last column, it would scroll the
            // columns off to the left.
            self.hold = position;
            self.slide = None;
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
        let (was, was_width) = self.fitted;
        if view <= 0. || ((view - was).abs() < 0.5 && was_width == self.widths.column) {
            return;
        }
        self.fitted = (view, self.widths.column);
        // The first layout: nothing rests anywhere yet.
        if was <= 0. {
            return;
        }
        let (cells, active) = self.scrolling();
        match &mut self.slide {
            Some(slide) => {
                slide.target = reveal_target(slide.origin, view, &cells, active);
                self.hold = slide.target.max(slide.position);
            }
            None => {
                let from = -f32::from(self.columns_scroll.offset().x);
                let target = refit(from, was, cells.len() as f32 * was_width, view, &cells, active);
                self.columns_scroll.set_offset(point(px(-target), px(0.)));
                self.hold = target;
            }
        }
        cx.notify();
    }
}

impl Render for Colet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.search_focused = self.search.read(cx).focus_handle(cx).is_focused(window);
        self.follow_focus(window, cx);
        let open = [self.dialog.is_some(), self.menu.is_some(), self.sort_open, self.history_open, self.crumb_menu_open];
        Tips::set_open(open.iter().enumerate().fold(0, |bits, (i, &o)| bits | u8::from(o) << i), cx);
        self.scroll_rows();
        self.reveal(window);
        let p = self.palette;
        let sidebar = self.render_sidebar(window, cx);
        let toolbar = self.render_toolbar(window, cx);
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

