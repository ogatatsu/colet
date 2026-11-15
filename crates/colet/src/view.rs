//! How the window looks: home's pane on the left; the search field, the trail of folders, the columns
//! and the status bar beside it.
use std::{ops::Range, rc::Rc, time::Instant};

use colet_core::{
    entry::{Entry, Sort, direction_label},
    paths::{folder_name, id_text, shown},
    time,
};
use gpui_kit::base::input::{self, Input, InputState};
use gpui_kit::component::{
    ActiveTheme, Disableable, FocusableExt, Selectable, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    spinner::Spinner,
    status_bar::StatusBar,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    crumbs::Crumb,
    metrics::{self, Scale, scaled},
    places::{HomeDrag, movable},
    reveal::{Cell, Slide, on_screen, refit, reveal_target, uncover_target},
    state::HomeKey,
    state::{Column, ColumnId, Kind},
    stops::Stop,
    theme::ActivePalette,
};

mod column;
mod elements;
mod pathbar;
mod rows;
mod scrolling;
mod sidebar;
mod statusbar;
mod toolbar;

use elements::on_top;
use rows::{MODIFIED_WIDTH, SIZE_WIDTH};
pub(crate) use rows::{PAGE_AHEAD, rows_in_sight};
#[cfg(test)]
pub(crate) use rows::{row_id, row_part_id};

pub trait TipExt {
    /// A line of text shown while the pointer rests on it.
    fn tip(self, text: impl Into<SharedString>) -> Self;

    /// The tip of a button that opens a menu or a panel, shown only while what it opens is shut: open, it
    /// would lie over what the button opened.
    fn tip_while_shut(self, open: bool, text: impl Into<SharedString>) -> Self;
}

impl<E: StatefulInteractiveElement> TipExt for E {
    fn tip(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx))
    }

    fn tip_while_shut(self, open: bool, text: impl Into<SharedString>) -> Self {
        if open { self } else { self.tip(text) }
    }
}

/// The icon named `name` among Colet's, for a button.
pub fn glyph(name: &str) -> gpui_kit::component::Icon {
    gpui_kit::component::Icon::empty().path(format!("icons/{name}.svg"))
}

/// A thin scrollbar laid over a list, at the zoom `scale` gives; its thumb is Colet's color.
pub fn scrollbar(bar: gpui_kit::base::Scrollbar, p: crate::theme::Palette, scale: Scale) -> gpui_kit::base::Scrollbar {
    bar.mode(gpui_kit::base::ScrollbarMode::Always).styles(|s| {
        s.track(|t| t.bg(transparent_black()).border_color(transparent_black()).width(scale.px(SCROLLBAR_WIDTH)))
            .thumb(|t| {
                t.bg(p.scrollbar)
                    .width(scale.px(SCROLLBAR_THUMB))
                    // Centered in the track, and round at its ends.
                    .inset(scale.px((SCROLLBAR_WIDTH - SCROLLBAR_THUMB) / 2.))
                    .radius(scale.px(SCROLLBAR_THUMB / 2.))
            })
            .thumb_hover(|t| t.bg(p.text_muted))
            .thumb_active(|t| t.bg(p.text_muted))
    })
}

/// How wide the edge a drag widens a column or home's pane from is.
const RESIZER_WIDTH: f32 = 6.;
/// How thick a scrollbar's track is.
pub(crate) const SCROLLBAR_WIDTH: f32 = 8.;
/// How thick a scrollbar's thumb is, within its track.
const SCROLLBAR_THUMB: f32 = 6.;

/// How far a ring laid round what it rings stands out: a pixel's gap and its two pixels.
pub const RING_ROOM: f32 = 3.;

/// The focus ring itself: two pixels in the focus color, `inset` in from the edges of what holds it, its
/// corners `radius`. Laid over what it rings, it takes no room and catches no pointer.
fn ring_line(p: crate::theme::Palette, inset: f32, radius: f32) -> impl IntoElement {
    div()
        .id("focus-ring")
        .test_support()
        .absolute()
        .top(scaled(inset))
        .left(scaled(inset))
        .right(scaled(inset))
        .bottom(scaled(inset))
        .rounded(scaled(radius))
        .border_2()
        .border_color(p.focus)
}

/// The ring of what has neither border nor fill of its own (a crumb, a ghost button, a row in the
/// history): on its edges, its corners `radius`, while it has the keys (`shown`), however they came.
/// Laid last, after what it holds.
pub fn ring<E: ParentElement + FluentBuilder>(el: E, p: crate::theme::Palette, shown: bool, radius: f32) -> E {
    el.when(shown, |el| el.child(ring_line(p, 0., radius)))
}

/// The ring of a field, or of a button with a border or fill of its own: a pixel off it, while it has the
/// keys (`shown`). Its room, [`RING_ROOM`] all round, is kept clear until then; `radius` is the corners'
/// of what it holds.
pub fn frame(p: crate::theme::Palette, shown: bool, radius: f32) -> Div {
    div().relative().p(scaled(RING_ROOM)).when(shown, |d| d.child(ring_line(p, 0., radius + RING_ROOM)))
}

/// The frame round the column worked in, laid over it on the lines round it: from the path bar's line
/// above to its own on the right; the caller sets its left and bottom.
fn active_frame(p: crate::theme::Palette) -> Div {
    div().absolute().top(px(-1.)).right_0().border_1().border_color(p.accent)
}

/// How far in from the window's edges a menu is kept.
pub const MENU_MARGIN: f32 = 8.;
/// How far in from the left of the row, or of the column's heading, the menu opened from the keyboard
/// starts: past the row's own picture, so the menu does not hide what it acts on.
const KEY_MENU_INSET: f32 = 16.;

/// Where the active column's focused row and its heading were drawn in the last frame: the menu opened
/// from the keyboard drops from under the row, or from under the heading when no row has the keys. Both
/// are measured afresh each frame, so a row or column gone leaves nothing behind.
#[derive(Clone, Default)]
pub struct KeyMenuAnchor {
    pub row: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
    pub heading: Rc<std::cell::Cell<Option<Bounds<Pixels>>>>,
}

impl KeyMenuAnchor {
    /// Forgets the last frame's: only what this frame draws is measured.
    pub fn begin_frame(&self) {
        self.row.set(None);
        self.heading.set(None);
    }

    /// Where the menu drops from, at the zoom `scale` gives; `None` when neither was drawn.
    pub fn at(&self, scale: Scale) -> Option<Point<Pixels>> {
        let under = self.row.get().or(self.heading.get())?;
        Some(point(under.left() + scale.px(KEY_MENU_INSET), under.bottom()))
    }
}
/// The room between a button and the menu dropped from it.
const MENU_GAP: f32 = 4.;

/// Which side of its button a menu dropped from it lines up with.
#[derive(Clone, Copy)]
pub enum MenuSide {
    Left,
    Right,
}

/// `menu` dropped from the button its parent holds, the parent `relative` and as tall as the button: just
/// under it whatever its height, lined up with its `side`, `min` to `max` wide, and moved in to keep
/// [`MENU_MARGIN`] at the zoom `scale` gives from the window's edges rather than run off them. Every menu dropped from a button
/// goes through here, so all of them keep to the same edges.
pub fn drop_down(menu: impl IntoElement, side: MenuSide, min: f32, max: Option<f32>, scale: Scale) -> impl IntoElement {
    let (corner, at) = match side {
        MenuSide::Left => (Anchor::TopLeft, div().left_0()),
        MenuSide::Right => (Anchor::TopRight, div().right_0()),
    };
    deferred(
        at.absolute().top_full().mt(scaled(MENU_GAP)).child(
            anchored()
                .anchor(corner)
                .snap_to_window_with_margin(scale.px(MENU_MARGIN))
                .child(div().min_w(scaled(min)).when_some(max, |d, max| d.max_w(scaled(max))).child(menu)),
        ),
    )
    .with_priority(gpui_kit::base::POPUP_PRIORITY)
}

/// A button the mouse presses without taking the keys from where they are; Tab still reaches it.
pub fn keep_focus<E: InteractiveElement>(el: E) -> E {
    el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(scaled(size)).flex_none().text_color(color)
}

/// `text` with what a search matched in it in `color`.
fn with_marks(text: &str, marks: &[Range<usize>], color: Hsla) -> StyledText {
    let style = HighlightStyle { color: Some(color), ..Default::default() };
    StyledText::new(text.to_owned()).with_highlights(marks.iter().map(|range| (range.clone(), style)))
}

/// Where a button's ring lies: on its edges if it has neither border nor fill, a pixel off them if it has.
#[derive(Clone, Copy)]
pub enum RingAt {
    Edge,
    Round,
}

impl Colet {
    /// The library's `button`, Tab reaching it through the stop `key`, ringed as Colet's own controls are
    /// rather than as the library rings it. The library's button leaves the keys where they are when the
    /// mouse presses it. Ringed round it, the ring's room is the caller's to give or lay over the gaps.
    pub(crate) fn ringed(&self, button: Button, key: impl Into<Stop>, at: RingAt, window: &Window, cx: &App) -> Div {
        let p = cx.palette();
        let focus = self.stop(key, cx);
        let shown = focus.is_focused(window);
        // The library's corners stay as many pixels at any zoom: the ring's are given at 100%.
        let radius = Scale::of(window).down(f32::from(cx.theme().radius));
        let button = button.track_focus(&focus).focus_ring(false);
        match at {
            RingAt::Edge => ring(div().flex().flex_none().relative().child(button), p, shown, radius),
            RingAt::Round => frame(p, shown, radius).flex().flex_none().child(button),
        }
    }

    /// A button that opens a menu or the history, `open` telling whether it is, and closes it pressed
    /// again. A press outside what it opened closes that before the button's click comes, so the press
    /// notes whether it was open and [`Colet::pressed_shut`] tells the click.
    pub(crate) fn opener<E: InteractiveElement>(el: E, open: impl Fn(&Colet) -> bool + 'static, cx: &mut Context<Self>) -> E {
        el.capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, _| this.overlays.note_press(open(this))))
    }

    /// The press `event` ends found open what its button opens, and closed it already: the click is not
    /// to open it again. Enter and Space press nothing; a press left noted by no click is noted over by
    /// the next press, the only one a click can end.
    pub(crate) fn pressed_shut(&mut self, event: &ClickEvent) -> bool {
        self.overlays.take_pressed_open() && !event.is_keyboard()
    }

    /// The zoom this frame is drawn at, as [`Render::render`] reads it before anything is drawn.
    pub(crate) fn scale(&self) -> Scale {
        self.scrolls.sideways.scale
    }

    /// The keys are in home's pane.
    fn in_home(&self) -> bool {
        self.state.columns.get(self.state.active).is_some_and(|c| c.kind == Kind::Home)
    }

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let pinned = self.state.pinned();
        let away = self.state.folded_away();
        let first = if pinned { 2 } else { 1 } + away;
        let only_home = self.state.columns.len() == 1;
        let pinned_column = pinned.then(|| self.render_column(1, window, cx));
        let strip = (away > 0).then(|| self.unfold_strip(away, window, cx));
        let columns: Vec<AnyElement> = (first..self.state.columns.len()).map(|i| self.render_column(i, window, cx)).collect();
        let scrolling = self.scrolling_columns(columns, window, cx);
        let measure = cx.entity().downgrade();
        let main = div()
            .id("main")
            .track_focus(&self.columns_focus)
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .bg(p.bg)
            // How wide this side is, for the results to stretch into: a change lays it out again.
            .child(
                canvas(
                    move |bounds, _, cx| {
                        measure
                            .update(cx, |this, cx| {
                                let width = this.scale().down(f32::from(bounds.size.width));
                                if (width - this.scrolls.sideways.main).abs() >= 0.5 {
                                    this.scrolls.sideways.main = width;
                                    cx.notify();
                                }
                            })
                            .ok();
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(self.takes_text(&self.columns_focus, cx))
            .children(pinned_column)
            .children(strip)
            .child(scrolling)
            // Nothing searched or opened leaves this side dark; show where to start.
            .when(only_home, |d| d.child(self.home_hint(window, cx)));
        self.column_keys(main, cx).into_any_element()
    }

    fn home_hint(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .text_size(scaled(metrics::TEXT_BODY))
            .text_color(p.text_muted)
            .child(icon("search", 28., p.text_faint))
            .child("名前やEverythingの検索式で探せます")
            // Ringed as a field is; the rings' room makes up most of the gaps. In a narrow window they go on
            // to a second line rather than past the edges.
            .child(div().flex().flex_wrap().justify_center().gap_0p5().children(
                [("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")].map(
                    |(query, label)| {
                        let button = Button::new(query)
                            .outline()
                            .accessibility_label(format!("{query} {label}"))
                            .h(scaled(metrics::SMALL_CONTROL_HEIGHT))
                            .px_2p5()
                            .gap_1p5()
                            .child(div().text_color(p.accent).font_family(metrics::FONT_CODE).child(query))
                            .child(label)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let filter = this.chosen_filter.clone();
                                this.search_for(query, filter, window, cx)
                            }));
                        self.ringed(button, Stop::Hint(query.to_string()), RingAt::Round, window, cx)
                    },
                ),
            ))
            .into_any_element()
    }
}

impl Colet {
    /// While a row in home or a tab is carried, puts it down as it is let go of anywhere but where it
    /// lands, off the window too. Drawn first, so it hears the press let go of after every place the drag
    /// can land; a landing puts the drag down itself and keeps it from hearing it.
    fn drag_let_go(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.home_drag.carried.is_none() && self.tabs.carried().is_none() {
            return None;
        }
        let view = cx.entity().downgrade();
        let listen = move |_: Bounds<Pixels>, _: (), window: &mut Window, _: &mut App| {
            let view = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    view.update(cx, |this, cx| this.let_go_of_drags(cx)).ok();
                }
            });
        };
        Some(canvas(|_, _, _| {}, listen).absolute().size_0().into_any_element())
    }

    /// The drag in home or of a tab is let go of where it does not land: nothing has moved.
    fn let_go_of_drags(&mut self, cx: &mut Context<Self>) {
        if self.home_drag.carried.is_some() || self.tabs.carried().is_some() {
            self.end_home_drag();
            self.end_tab_drag();
            cx.notify();
        }
    }
}

impl Render for Colet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only the stops asked for over the last frame stay: those of buttons gone go with them.
        self.stops.begin_frame(window);
        self.key_menu.begin_frame();
        // The first frame drawn in front: the keys left the address field on the way, if they did, unheard of.
        if std::mem::take(&mut self.came_to_front) && window.is_window_active() {
            cx.defer_in(window, |this, window, cx| this.follow_address(window, cx));
        }
        // The zoom this frame is drawn at: the widths and the scrolling are reckoned at 100% (see
        // `metrics`), and what layout measures is read through it. Read before anything below reckons.
        self.scrolls.sideways.rescale(Scale::of(window));
        // The columns slide a frame at a time toward where the last frame's layout says they rest.
        // Stepped here, not in prepaint or a next-frame callback, on purpose: the trail of folders and the
        // edges' shades are built below from the offset this sets, so they move with the columns in the
        // same frame, and folders coming out of a fold are shifted in the frame they first appear in.
        // It never notifies; it asks for another frame only while a slide is under way.
        self.reveal(window);
        let p = cx.palette();
        let tabbar = self.render_tabbar(window, cx);
        let sidebar = self.render_sidebar(window, cx);
        let toolbar = self.render_toolbar(window, cx);
        let main = self.render_main(window, cx);
        div()
            .id("colet")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(metrics::FONT_UI)
            .text_size(scaled(metrics::TEXT_LARGE))
            // The window's keys. None of them needs to check for a dialog: the window draws its dialog
            // outside Colet's elements, and the dialog keeps the keys in itself, so while one is open these
            // are never on the way of a key (see `tests::window::leaves_the_window_keys_alone_in_a_dialog`).
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &FocusAddress, window, cx| this.open_address(window, cx)))
            .on_action(cx.listener(|this, _: &OpenTextMenu, window, cx| this.open_focused_text_menu(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSearchFilters, window, cx| this.open_filter_menu(window, cx)))
            // As the strip's button does: the keys stay where they are.
            .on_action(cx.listener(|this, _: &Unfold, _, cx| {
                if this.state.folded_away() > 0 {
                    this.state.unfold();
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusHome, window, cx| this.focus_home(window, cx)))
            .on_action(cx.listener(|this, _: &ToggleHidden, _, cx| this.toggle_hidden(cx)))
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
                let current = this.tabs.current();
                this.close_tab(current, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.step_tab(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| this.step_tab(-1, window, cx)))
            .on_action(cx.listener(|this, _: &ReopenTab, window, cx| this.reopen_tab(window, cx)))
            .on_action(cx.listener(|this, _: &RefreshAll, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &SaveCurrent, window, cx| this.save_current(window, cx)))
            // The mouse's back and forward buttons, as Alt+← and Alt+→.
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Back), cx.listener(|this, _, window, cx| this.back(window, cx)))
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Forward), cx.listener(|this, _, window, cx| this.forward(window, cx)))
            .when(self.widths.resizing().is_some(), |d| {
                d.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                        // Let go of off the window, where the window hears no button come up: the drag
                        // ends at the width it last left, as if let go of there.
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.end_resize(cx);
                            cx.notify();
                        } else if this.widths.drag_to(event.position.x, Scale::of(window)) {
                            cx.notify();
                        }
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.end_resize(cx);
                            cx.notify();
                        }),
                    )
            })
            .children(self.drag_let_go(cx))
            .children(self.press_listener(cx))
            .child(tabbar)
            .child(toolbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .child(sidebar)
                    .child(div().flex_1().min_w_0().h_full().flex().flex_col().child(main).child(self.render_statusbar(window, cx))),
            )
            // Over a dialog, a field's menu is drawn with it.
            .children(if !self.overlays.has_dialog() { self.render_menu(cx) } else { None })
            .when(self.overlays.is_history_open(), |d| d.child(deferred(self.history.clone()).with_priority(2)))
    }
}
