//! The window as a whole: the tabs, the toolbar ([`toolbar`]), home's pane beside the columns, and the
//! status bar ([`statusbar`]), each region drawn by its own feature (home, the columns, the search bar,
//! the address); the keys and drags that span the window; and the parts every feature draws with: rings
//! round what has the keys, tips, glyphs, scrollbars and menus dropped from a button.
use std::{ops::Range, rc::Rc};

use colet_core::{paths::shown, time};
use gpui_kit::component::{
    ActiveTheme, Disableable, FocusableExt,
    button::{Button, ButtonVariants},
    status_bar::StatusBar,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    metrics::{self, Scale, scaled},
    state::Kind,
    stops::Stop,
    theme::ActivePalette,
};

mod elements;
mod statusbar;
mod toolbar;

pub(crate) use elements::on_top;

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

/// How a line of text that may be cut short with an ellipsis was last drawn: the box it was cut to and
/// the style it was drawn in, noted as it is drawn, so whether it fit can be told when the pointer comes
/// to rest on it. A text box tells neither by itself.
#[derive(Clone, Default)]
pub(crate) struct Fit(Rc<std::cell::RefCell<Option<(Pixels, TextStyle)>>>);

impl Fit {
    /// Laid over the box the text is cut to, it notes the box's width and the text's style as they are
    /// drawn; the box is to be `relative`.
    pub(crate) fn note(&self) -> impl IntoElement + use<> {
        let drawn = self.0.clone();
        canvas(move |bounds, window, _| *drawn.borrow_mut() = Some((bounds.size.width, window.text_style())), |_, _, _, _| {})
            .absolute()
            .size_full()
    }

    /// Whether `text`, as last drawn, was cut short: wider, shaped whole, than its box. A box sized to
    /// text that fits is a whole pixel wide, rounded, so half a pixel over is still taken to fit. Not
    /// drawn yet, it was not cut.
    pub(crate) fn cut(&self, text: &str, window: &Window) -> bool {
        let Some((width, style)) = &*self.0.borrow() else { return false };
        let size = style.font_size.to_pixels(window.rem_size());
        let line = window.text_system().shape_line(text.to_owned().into(), size, &[style.to_run(text.len())], None);
        line.width > *width + px(0.5)
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
pub(crate) const RESIZER_WIDTH: f32 = 6.;
/// How thick a scrollbar's track is.
pub(crate) const SCROLLBAR_WIDTH: f32 = 8.;
/// How thick a scrollbar's thumb is, within its track.
pub(crate) const SCROLLBAR_THUMB: f32 = 6.;

/// How far a ring laid round what it rings stands out: a pixel's gap and its two pixels.
pub const RING_ROOM: f32 = 3.;

/// The focus ring itself: two pixels in the focus color, `inset` in from the edges of what holds it, its
/// corners `radius`. Laid over what it rings, it takes no room and catches no pointer.
pub(crate) fn ring_line(p: crate::theme::Palette, inset: f32, radius: f32) -> impl IntoElement {
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
pub(crate) fn active_frame(p: crate::theme::Palette) -> Div {
    // Out by a line, so its top falls on the path bar's.
    div().absolute().top(-metrics::HAIRLINE).right_0().border_1().border_color(p.accent)
}

/// How far in from the window's edges a menu is kept.
pub const MENU_MARGIN: f32 = 8.;
/// How far in from the left of the row, or of the column's heading, the menu opened from the keyboard
/// starts: past the row's own picture, so the menu does not hide what it acts on.
pub(crate) const KEY_MENU_INSET: f32 = 16.;

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
pub(crate) const MENU_GAP: f32 = 4.;

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

pub(crate) fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(scaled(size)).flex_none().text_color(color)
}

/// `text` with what a search matched in it in `color`.
pub(crate) fn with_marks(text: &str, marks: &[Range<usize>], color: Hsla) -> StyledText {
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
        // The library's own corners, as the ring takes them.
        let radius = metrics::Radii::of(window, cx).md;
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

    /// Readies what lives for a frame only, before the frame is drawn. The one thing Colet changes in
    /// [`Render::render`], and on purpose: it never touches what the window shows of the app's state, and
    /// it never notifies (a slide under way asks for its next frame, and nothing more).
    ///
    /// - The stops asked for over the last frame stay, and those of buttons gone go. Colet keeps them, not
    ///   GPUI's keyed state, as the keys are sent to some from outside drawing (see [`crate::stops`]).
    /// - Where the menu key's menu drops from is measured afresh.
    /// - The zoom this frame is drawn at is read: the widths and the scrolling are reckoned at 100% (see
    ///   [`metrics`]), and what layout measures is read through it. Read before anything below reckons.
    /// - Home's pane comes a frame further out of its strip, or back in, as the keys come to it or leave:
    ///   before the columns, which reckon with the room it leaves them.
    /// - The columns slide a frame toward where the last frame's layout says they rest. Stepped here, not
    ///   in prepaint or a next-frame callback: the trail of folders and the edges' shades are built from
    ///   the offset this sets, so they move with the columns in the same frame, and folders coming out of
    ///   a fold are shifted in the frame they first appear in.
    fn begin_frame(&mut self, window: &mut Window, cx: &App) {
        self.stops.begin_frame(window);
        self.key_menu.begin_frame();
        self.scrolls.sideways.rescale(Scale::of(window), window.scale_factor());
        self.step_home_pane(window, cx);
        self.reveal(window, cx);
    }

    /// The zoom this frame is drawn at, as [`Render::render`] reads it before anything is drawn.
    pub(crate) fn scale(&self) -> Scale {
        self.scrolls.sideways.scale
    }

    /// The theme's corners at the zoom this frame is drawn at.
    pub(crate) fn radii(&self, cx: &App) -> metrics::Radii {
        metrics::Radii::at(self.scale(), cx)
    }

    /// The keys are in home's pane.
    pub(crate) fn in_home(&self) -> bool {
        self.state.columns.get(self.state.active).is_some_and(|c| c.kind == Kind::Home)
    }
}

impl Colet {
    /// While a row in home or a tab is carried, puts it down as it is let go of anywhere but where it
    /// lands, off the window too. Drawn first, so it hears the press let go of after every place the drag
    /// can land; a landing puts the drag down itself and keeps it from hearing it.
    fn drag_let_go(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.home_carried(cx).is_none() && self.tab_strip.read(cx).carried().is_none() {
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
        if self.home_carried(cx).is_some() || self.tab_strip.read(cx).carried().is_some() {
            self.end_home_drag(cx);
            self.end_tab_drag(cx);
            cx.notify();
        }
    }
}

impl Render for Colet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.begin_frame(window, cx);
        let p = cx.palette();
        let tabbar = self.tab_bar();
        let sidebar = self.render_sidebar(window, cx);
        let toolbar = self.render_toolbar(window, cx);
        let main = self.render_columns(window, cx);
        div()
            .id("colet")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .font_family(metrics::FONT_UI)
            .text_sm()
            // The window's keys. None of them needs to check for a dialog: the window draws its dialog
            // outside Colet's elements, and the dialog keeps the keys in itself, so while one is open these
            // are never on the way of a key (see `tests::window::leaves_the_window_keys_alone_in_a_dialog`).
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| this.search.update(cx, |search, cx| search.focus_all(window, cx))))
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
            .on_action(cx.listener(|this, _: &ToggleShortcuts, window, cx| this.toggle_shortcuts(window, cx)))
            // Esc puts down a drag in home, before the keys reach anything else.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.home_carried(cx).is_some() && event.keystroke.key == "escape" {
                    cx.stop_active_drag(window);
                    this.end_home_drag(cx);
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            // Before anything pressed takes the keys from home: its pane stays out until let go of.
            .capture_any_mouse_down(cx.listener(|this, event: &MouseDownEvent, _, cx| this.hold_home(event, cx)))
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
            .children(self.home_hold_listener(cx))
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
