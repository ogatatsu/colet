//! The strip of tabs over the window: how it is drawn, scrolled and carried along. [`TabStrip`] keeps what
//! lasts between frames there, where the strip is scrolled and the tab carried; [`TabBar`] draws it from the
//! tabs Colet hands it each frame and tells it, through a [`TabStripEvent`], what was asked for. Which tabs
//! there are, and what each holds, stay Colet's.
use gpui_kit::component::{
    ActiveTheme as _, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::{TabId, Title};
use crate::words;
use crate::{
    caption,
    metrics::{self, Scale, scaled},
    theme::{ActivePalette, Palette},
    view::TipExt,
};

/// A tab's width while the strip has room, and the least it narrows to before the strip scrolls.
const TAB_WIDTH: f32 = 260.;
const TAB_MIN: f32 = 100.;
const TAB_HEIGHT: f32 = 32.;
/// Above the tabs, while the window is not maximized: maximized, they reach the screen's top edge, where
/// the pointer stops on them.
const TAB_TOP: f32 = 4.;
const TAB_CURVE: f32 = 8.;
/// The button for another tab, square, and the gap left of it.
const NEW_TAB_BUTTON: f32 = 26.;
const NEW_TAB_GAP: f32 = 4.;
/// The least room kept right of the tabs to take the window by, however many there are.
const ROOM: f32 = 100.;
/// What the bar leaves beside the tabs: the button for another, the room to take the window by, the
/// window's own buttons and the curves at the strip's ends.
const BESIDE_TABS: f32 = NEW_TAB_GAP + NEW_TAB_BUTTON + ROOM + caption::WIDTH + TAB_CURVE * 2.;

/// What the strip asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum TabStripEvent {
    /// The tab is to come to the front.
    Select(TabId),
    Close(TabId),
    /// The tab's menu is to open at the pointer.
    Menu(TabId, Point<Pixels>),
    /// Another tab is to open.
    New,
    /// The tab carried was let go of in the gap `gap`: 0 before the first tab, up to the count after the
    /// last.
    Move {
        tab: TabId,
        gap: usize,
    },
}

/// Where the strip is scrolled, the tab it last brought in sight, and the tab carried along it.
#[derive(Default)]
pub struct TabStrip {
    scroll: ScrollHandle,
    /// The tab in front the strip last brought in sight: another coming to the front is brought in sight
    /// in turn.
    revealed: Option<TabId>,
    drag: TabDrag,
}

/// A tab carried along the strip, and the gap between tabs it would land in.
#[derive(Default)]
struct TabDrag {
    carried: Option<TabId>,
    /// 0 before the first tab, up to the count after the last; none while the pointer is off the strip.
    gap: Option<usize>,
}

impl EventEmitter<TabStripEvent> for TabStrip {}

impl TabStrip {
    pub fn scroll(&self) -> &ScrollHandle {
        &self.scroll
    }

    /// Brings the tab `id`, at `ix`, in sight as it comes to the front; false while the strip is not laid
    /// out yet to scroll, to be tried again on the next frame.
    fn bring_in_sight(&mut self, id: TabId, ix: usize) -> bool {
        if self.revealed == Some(id) {
            return true;
        }
        // Before the first prepaint the handle does not yet know that it scrolls horizontally;
        // scroll_to_item would consume the request without moving it.
        if self.scroll.bounds().size.width <= px(0.) {
            return false;
        }
        self.revealed = Some(id);
        self.scroll.scroll_to_item(ix);
        true
    }

    // ---- A tab carried along the strip ----

    /// The tab `id` is picked up.
    pub fn start_drag(&mut self, id: TabId) {
        self.drag = TabDrag { carried: Some(id), gap: None };
    }

    /// The tab carried, if one is.
    pub fn carried(&self) -> Option<TabId> {
        self.drag.carried
    }

    /// The gap the tab carried would land in.
    pub fn gap(&self) -> Option<usize> {
        self.drag.gap.filter(|_| self.drag.carried.is_some())
    }

    /// The tab carried would land in `gap` now; true when that moved.
    pub fn aim(&mut self, gap: Option<usize>) -> bool {
        if self.drag.carried.is_none() || self.drag.gap == gap {
            return false;
        }
        self.drag.gap = gap;
        true
    }

    /// The tab carried is put down: the gap it would have landed in.
    pub fn end_drag(&mut self) -> Option<usize> {
        std::mem::take(&mut self.drag).gap
    }

    /// The pointer at `at` in a tab's drag, the bar drawn at `bar` with `count` tabs `width` wide: the gap
    /// it is over, none off the bar. Over a tab's left half it goes before it, over its right half after;
    /// past the last, last.
    fn gap_at(&self, at: Point<Pixels>, bar: Bounds<Pixels>, count: usize, width: f32, scale: Scale) -> Option<usize> {
        bar.contains(&at).then(|| {
            let strip = self.scroll.bounds();
            let along = scale.down(f32::from(at.x - strip.left() - self.scroll.offset().x)) - TAB_CURVE;
            ((along / width).round().max(0.) as usize).min(count)
        })
    }
}

/// A tab as the strip draws it.
pub struct TabLook {
    pub id: TabId,
    pub title: Title,
}

/// The strip of tabs, drawn from `tabs` with the one at `current` in front, and right of them the button
/// for another, room to take the window by and the window's own buttons: the window's title bar.
#[derive(IntoElement)]
pub struct TabBar {
    strip: Entity<TabStrip>,
    tabs: Vec<TabLook>,
    current: usize,
}

impl TabBar {
    pub fn new(strip: &Entity<TabStrip>, tabs: Vec<TabLook>, current: usize) -> Self {
        Self { strip: strip.clone(), tabs, current }
    }
}

/// Tells `strip`'s owner of `event`.
fn ask(strip: &Entity<TabStrip>, event: TabStripEvent, cx: &mut App) {
    strip.update(cx, |_, cx| cx.emit(event));
}

impl RenderOnce for TabBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = cx.palette();
        let scale = Scale::of(window);
        let radius = cx.theme().radius_tokens().sm;
        let Self { strip, tabs, current } = self;
        let count = tabs.len();
        let top = if window.is_maximized() { 0. } else { TAB_TOP };
        // The tabs share the room there is, down to their least; past that the strip scrolls.
        let room = scale.down(f32::from(window.viewport_size().width)) - BESIDE_TABS;
        let width = (room / count.max(1) as f32).clamp(TAB_MIN, TAB_WIDTH);
        if let Some(front) = tabs.get(current)
            && !strip.update(cx, |strip, _| strip.bring_in_sight(front.id, current))
        {
            window.request_animation_frame();
        }
        let (carried, gap) = {
            let strip = strip.read(cx);
            (strip.carried(), strip.gap())
        };
        let dragging = carried.is_some();
        let tab = |ix: usize, look: &TabLook| -> Stateful<Div> {
            Tab { strip: &strip, look, ix, count, current: ix == current, width, carried, p, radius }.render()
        };
        let mut cells = Vec::with_capacity(count * 2);
        for (ix, look) in tabs.iter().enumerate() {
            // Keep one cell per tab, in order, so scroll_to_item includes both curves. The selected
            // tab is painted last, over its neighbors' hover fills, within this same scroll clip.
            let cell = div()
                .flex_none()
                .w(scaled(width + TAB_CURVE * 2.))
                .h_full()
                .pt(scaled(top))
                .px(scaled(TAB_CURVE))
                .when(ix + 1 < count, |d| d.mr(scaled(-TAB_CURVE * 2.)))
                .when(ix != current, |d| d.child(tab(ix, look).test_support()));
            cells.push(cell.into_any_element());
        }
        for (ix, look) in tabs.iter().enumerate().take(count.saturating_sub(1)) {
            if ix == current || ix + 1 == current {
                continue;
            }
            // This passive hover region spans the two neighboring tab bodies. Its opacity hides
            // their shared separator without changing hitboxes or storing pointer state.
            cells.push(
                div()
                    .id(("tab-separator", look.id as usize))
                    .absolute()
                    .left(scaled(ix as f32 * width + TAB_CURVE))
                    .top(scaled(top))
                    .w(scaled(width * 2.))
                    .h(scaled(TAB_HEIGHT))
                    // While a tab is carried, only the line tells where it goes.
                    .when(!dragging, |d| d.hover(|s| s.opacity(0.)))
                    .child(div().absolute().left(scaled(width)).ml(-metrics::HAIRLINE).top_2().w(metrics::HAIRLINE).h_4().bg(p.border))
                    .into_any_element(),
            );
        }
        if let Some(look) = tabs.get(current) {
            cells.push(
                tab(current, look)
                    .absolute()
                    .left(scaled(current as f32 * width + TAB_CURVE))
                    .top(scaled(top))
                    .test_support()
                    .into_any_element(),
            );
        }
        if let Some(gap) = gap {
            // Over every tab, at the edge between the bodies the carried one would land between.
            cells.push(
                div()
                    .id("tab-landing")
                    .absolute()
                    .left(scaled(TAB_CURVE + gap as f32 * width - 1.))
                    .top(scaled(top))
                    .w_0p5()
                    .h(scaled(TAB_HEIGHT))
                    // A line two pixels wide, not a surface: its ends round off by half its width, not by a tier.
                    .rounded(scaled(1.))
                    .bg(p.accent)
                    .test_support()
                    .into_any_element(),
            );
        }
        let scroll = strip.read(cx).scroll().clone();
        let (aiming, landing, new) = (strip.clone(), strip.clone(), strip.clone());
        div()
            .id("tabbar")
            .on_drag_move(move |event: &DragMoveEvent<DraggedTab>, window, cx| {
                aiming.update(cx, |strip, cx| {
                    let gap = strip.gap_at(event.event.position, event.bounds, count, width, Scale::of(window));
                    if strip.aim(gap) {
                        cx.notify();
                    }
                });
            })
            .on_drop(move |dragged: &DraggedTab, _, cx| {
                landing.update(cx, |strip, cx| {
                    if let Some(gap) = strip.end_drag() {
                        cx.emit(TabStripEvent::Move { tab: dragged.id, gap });
                    }
                    cx.notify();
                });
            })
            .h(scaled(TAB_HEIGHT + top))
            .flex_none()
            .w_full()
            .relative()
            .flex()
            .items_center()
            .bg(p.tab_strip)
            .role(Role::TabList)
            .aria_label(words::TAB)
            // The selected tab paints over this line to join the toolbar below.
            .child(div().absolute().left_0().right_0().bottom_0().h(metrics::HAIRLINE).bg(p.border))
            .child(
                div()
                    .id("tab-strip")
                    .relative()
                    .flex()
                    .min_w_0()
                    .h_full()
                    .items_end()
                    .overflow_x_scroll()
                    .track_scroll(&scroll)
                    .children(cells),
            )
            .child(
                tab_button(p, radius, "new-tab", "plus", words::NEW_TAB, words::tabs::NEW_TAB_KEY)
                    .small()
                    .ml(scaled(NEW_TAB_GAP))
                    .size(scaled(NEW_TAB_BUTTON))
                    .on_click(move |_, _, cx| ask(&new, TabStripEvent::New, cx)),
            )
            // The room left takes the window, as a title bar would.
            .child(caption::takes_the_window(div().id("tab-room").flex_1().min_w(scaled(ROOM)).h_full()).test_support())
            .child(caption::buttons(window, cx))
            // So does the gap above the tabs, up to the window's buttons.
            .when(top > 0., |d| {
                d.child(caption::takes_the_window(
                    div().id("tab-top").absolute().top_0().left_0().right(scaled(caption::WIDTH)).h(scaled(top)),
                ))
            })
    }
}

/// One tab on the strip, at `ix` of `count`.
struct Tab<'a> {
    strip: &'a Entity<TabStrip>,
    look: &'a TabLook,
    ix: usize,
    count: usize,
    current: bool,
    width: f32,
    carried: Option<TabId>,
    p: Palette,
    /// The corners of its close button: the theme's.
    radius: Pixels,
}

impl Tab<'_> {
    fn render(self) -> Stateful<Div> {
        let Self { strip, look, ix, count, current, width, carried, p, radius } = self;
        let id = look.id;
        let Title { glyph, name, tip } = &look.title;
        let group = SharedString::from(format!("tab-{id}"));
        let dragged = DraggedTab { id, name: name.clone(), glyph, width };
        let dragging = carried.is_some();
        let (closing, selecting, middle, menu, picking) = (strip.clone(), strip.clone(), strip.clone(), strip.clone(), strip.clone());
        div()
            .id(("tab", id as usize))
            .group(group.clone())
            .h(scaled(TAB_HEIGHT))
            .flex_none()
            .w(scaled(width))
            .relative()
            .pl_2p5()
            .pr_1()
            .flex()
            .items_center()
            .gap_1p5()
            .role(Role::Tab)
            .aria_label(name.clone())
            .aria_selected(current)
            .aria_position_in_set(ix + 1)
            .aria_size_of_set(count)
            .text_color(if current { p.text } else { p.text_muted })
            .when(current, |d| d.child(tab_shape(p.bg, Some(p.border))))
            .when(!current && !dragging, |d| {
                d.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0p5()
                        .left_0()
                        .right_0()
                        .invisible()
                        .group_hover(group, |s| s.visible())
                        .child(tab_shape(p.tab_hover, None)),
                )
            })
            .when(carried == Some(id), |d| d.opacity(0.4))
            .tip(tip.clone())
            .child(svg().path(format!("icons/{glyph}.svg")).size(scaled(metrics::GLYPH)).flex_none().text_color(if current {
                p.text
            } else {
                p.text_muted
            }))
            .child(div().flex_1().min_w_0().truncate().child(name.clone()))
            .child(tab_button(p, radius, ("tab-close", id as usize), "x", words::CLOSE_TAB, words::tabs::CLOSE_TAB_KEY).xsmall().on_click(
                move |_, _, cx| {
                    cx.stop_propagation();
                    ask(&closing, TabStripEvent::Close(id), cx);
                },
            ))
            .on_click(move |_, _, cx| ask(&selecting, TabStripEvent::Select(id), cx))
            .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
                cx.stop_propagation();
                ask(&middle, TabStripEvent::Close(id), cx);
            })
            .on_mouse_down(MouseButton::Right, move |event: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                ask(&menu, TabStripEvent::Menu(id, event.position), cx);
            })
            .on_drag(dragged, move |dragged, _, _, cx| {
                picking.update(cx, |strip, cx| {
                    strip.start_drag(dragged.id);
                    cx.notify();
                });
                cx.new(|_| dragged.clone())
            })
    }
}

/// A tab carried along the strip.
#[derive(Clone)]
pub struct DraggedTab {
    id: TabId,
    name: String,
    glyph: &'static str,
    /// The tab's width on the strip, curves aside.
    width: f32,
}

impl Render for DraggedTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        // As wide as the tab and held where it was picked up; the curves reach out beside it. Faint
        // enough that the landing line it passes over still shows through.
        div()
            .relative()
            .w(scaled(self.width))
            .h(scaled(TAB_HEIGHT))
            .pl_2p5()
            .pr_1()
            .flex()
            .items_center()
            .gap_1p5()
            .child(tab_shape(p.bg, Some(p.border)))
            .child(svg().path(format!("icons/{}.svg", self.glyph)).size(scaled(metrics::GLYPH)).flex_none().text_color(p.text))
            .child(div().min_w_0().truncate().child(self.name.clone()))
            .opacity(0.5)
            .font_family(metrics::FONT_UI)
            .text_sm()
            .text_color(p.text)
    }
}

/// A small button with a glyph on the tab bar, which the mouse alone presses: Tab passes it by.
fn tab_button(p: Palette, radius: Pixels, id: impl Into<ElementId>, glyph: &str, label: &'static str, tip: &'static str) -> Button {
    Button::new(id)
        .ghost()
        .rounded(radius)
        .icon(crate::view::glyph(glyph))
        .accessibility_label(label)
        .tooltip(tip)
        .tab_stop(false)
        .flex_none()
        .text_color(p.text_muted)
}

/// Decoration only: the tab's hitbox stays rectangular and the curves have no mouse listeners.
fn tab_shape(background: Hsla, border: Option<Hsla>) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let curve = Scale::of(window).px(TAB_CURVE);
            let mut fill = PathBuilder::fill().with_style(PathStyle::Fill(FillOptions::tolerance(0.02)));
            trace_tab(&mut fill, bounds, curve);
            fill.close();
            if let Ok(path) = fill.build() {
                window.paint_path(path, background);
            }
            if let Some(border) = border {
                let mut outline = PathBuilder::stroke(metrics::HAIRLINE);
                trace_tab(&mut outline, bounds, curve);
                // Leave the bottom open to the toolbar.
                if let Ok(path) = outline.build() {
                    window.paint_path(path, border);
                }
            }
        },
    )
    .absolute()
    .top_0()
    .bottom_0()
    .left(scaled(-TAB_CURVE))
    .right(scaled(-TAB_CURVE))
}

fn trace_tab(path: &mut PathBuilder, bounds: Bounds<Pixels>, curve: Pixels) {
    // Keep the one-pixel outline inside the canvas, including at the scroll viewport's ends.
    let left = bounds.left() + metrics::HAIRLINE / 2.;
    let right = bounds.right() - metrics::HAIRLINE / 2.;
    let top = bounds.top() + metrics::HAIRLINE / 2.;
    let bottom = bounds.bottom();
    let tab_left = left + curve;
    let tab_right = right - curve;
    path.move_to(point(left, bottom));
    tab_corner(path, point(left, bottom), point(tab_left, bottom), point(tab_left, bottom - curve));
    path.line_to(point(tab_left, top + curve));
    tab_corner(path, point(tab_left, top + curve), point(tab_left, top), point(tab_left + curve, top));
    path.line_to(point(tab_right - curve, top));
    tab_corner(path, point(tab_right - curve, top), point(tab_right, top), point(tab_right, top + curve));
    path.line_to(point(tab_right, bottom - curve));
    tab_corner(path, point(tab_right, bottom - curve), point(tab_right, bottom), point(right, bottom));
}

fn tab_corner(path: &mut PathBuilder, from: Point<Pixels>, corner: Point<Pixels>, to: Point<Pixels>) {
    const K: f32 = 0.552_284_8;
    path.cubic_bezier_to(to, from + (corner - from) * K, to + (corner - to) * K);
}

#[cfg(test)]
mod tests {
    use super::TabStrip;

    #[test]
    fn carries_a_tab_and_puts_it_down_with_the_gap_it_was_over() {
        let mut strip = TabStrip::default();
        assert!(!strip.aim(Some(1)), "nothing carried");
        strip.start_drag(7);
        assert_eq!((strip.carried(), strip.gap()), (Some(7), None));
        assert!(strip.aim(Some(3)));
        assert!(!strip.aim(Some(3)));
        assert_eq!(strip.end_drag(), Some(3));
        assert_eq!((strip.carried(), strip.gap()), (None, None));
    }
}
