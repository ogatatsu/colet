//! The search field as the toolbar draws it, with the button to clear it, the edge a drag widens it from,
//! the search filters' button beside it, and the examples to search for that fill the columns' side while
//! nothing is searched or opened.

use gpui_kit::base::input::{self, Input};
use gpui_kit::component::{
    ActiveTheme as _, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::*,
    metrics::{self, scaled},
    stops::Stop,
    theme::ActivePalette,
    view::*,
};

impl Colet {
    /// The search field, ringed while it has the keys, as wide as the drag left it.
    pub(crate) fn render_search_box(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let focused = self.search_has_keys(window, cx);
        let search = self.search_field(cx);
        let has_query = !self.state.query.is_empty();
        let clear = has_query.then(|| {
            let clear = Button::new("clear-search")
                .ghost()
                .xsmall()
                .icon(glyph("x"))
                .accessibility_label(words::clear_search())
                .tooltip(words::search::clear_key())
                .on_click(cx.listener(|this, _, window, cx| {
                    this.clear_search(window, cx);
                    this.focus_search(window, cx);
                }));
            self.ringed(clear, "clear-search", RingAt::Edge, window, cx)
        });
        // Ringed as a dialog's field is; the ring lies over the gaps around, not in the row. It keeps its
        // width while the address beside it gives way, then narrows, keeping room to type in.
        frame(p, focused, radii.md)
            .flex_none()
            .w(scaled(self.widths.field))
            .flex_shrink(1.)
            .min_w(scaled(SEARCH_FIELD_WIDTHS.0))
            .m(scaled(-RING_ROOM))
            .child(
                div()
                    .id("search-box")
                    .w_full()
                    .h(scaled(metrics::CONTROL_HEIGHT))
                    .pl_2p5()
                    .pr_1()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border_strong)
                    .rounded(scaled(radii.md))
                    .child(icon("search", metrics::FIELD_GLYPH, if focused { p.accent } else { p.text_muted }))
                    .child(div().flex_1().min_w_0().capture_any_mouse_down(Self::right_press_menu(&search, cx)).child(Input::new(&search)))
                    // ↓ goes back to the column worked in, as from the address field.
                    .capture_action(cx.listener(|this, _: &input::MoveDown, window, cx| {
                        cx.stop_propagation();
                        this.down_into_columns(window, cx);
                    }))
                    // Esc empties the field; the filter chosen stays, let go of only from its menu.
                    .on_action(cx.listener(|this, _: &input::Escape, window, cx| {
                        if !this.state.query.is_empty() {
                            this.clear_search(window, cx);
                        }
                    }))
                    .children(clear)
                    .test_support(),
            )
            .into_any_element()
    }

    /// The edge a drag widens the search field from, drawn as a splitter's grip: a short upright bar, not a
    /// chevron, which beside the address would read as a step of the path. It takes the gaps on either
    /// side, and is lit while pointed at or dragged.
    pub(crate) fn field_resizer(&self, cx: &mut Context<Self>) -> AnyElement {
        const ID: &str = "search-field-resizer";
        let p = cx.palette();
        let dragging = self.widths.resizing() == Some(WidthOf::Field);
        let width = self.widths.field;
        div()
            .id(ID)
            .group(ID)
            .flex_none()
            .h(scaled(metrics::CONTROL_HEIGHT))
            .px_1()
            .mx_neg_1()
            .flex()
            .items_center()
            .cursor_col_resize()
            // As wide as the chevron was, so the toolbar keeps its widths.
            .child(
                div().w(scaled(metrics::GLYPH)).flex().justify_center().child(
                    div()
                        .w_0p5()
                        .h_4()
                        .rounded(cx.theme().radius_full())
                        .bg(if dragging { p.accent } else { p.border_strong })
                        .group_hover(ID, |s| s.bg(p.accent)),
                ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    this.widths.start_resize(WidthOf::Field, event.position.x, width);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .test_support()
            .into_any_element()
    }

    /// The search filters' button beside the field, naming the one chosen in the accent color, and its menu.
    pub(crate) fn filter_button(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let focus = self.stop("search-filter", cx);
        let chosen = self.chosen_filter(cx);
        let on = chosen.is_some();
        let label = words::search::filter_label(chosen.as_deref());
        let open = self.filter_open();
        // In a narrow window its name gives way to the search field, down to the funnel and the chevron;
        // the tip still names it.
        div()
            .relative()
            // Its name gives way before the search field narrows.
            .flex_shrink(100.)
            // The funnel and the chevron with their padding, gaps and border: the button never goes under
            // the sort order's.
            .min_w(scaled(60.))
            .child(
                frame(p, focus.is_focused(window), radii.md).min_w_0().m(scaled(-RING_ROOM)).child(
                    crate::view::keep_focus(div().id("search-filter"))
                        .test_support()
                        .track_focus(&focus)
                        .key_context(SORT)
                        .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_filter_menu(window, cx)))
                        .role(Role::Button)
                        .aria_label(label.clone())
                        .aria_expanded(open)
                        .h(scaled(metrics::CONTROL_HEIGHT))
                        .max_w(scaled(200.))
                        .pl_2p5()
                        .pr_2()
                        .flex()
                        .items_center()
                        .gap_1p5()
                        .text_xs()
                        // Open, it stays pressed, under the pointer or not, and its name takes the text's color,
                        // which reads on the pressed fill; the funnel and the border still tell a chosen filter.
                        .text_color(match (open, on) {
                            (true, _) => p.text,
                            (false, true) => p.accent,
                            (false, false) => p.text_muted,
                        })
                        .bg(if open { p.pressed } else { p.surface })
                        .border_1()
                        .border_color(if on { p.accent } else { p.border_strong })
                        .rounded(scaled(radii.md))
                        // Set while open too, as the library's buttons do: GPUI keeps track of the pointer only
                        // for an element with a hover style, and a stale one would light it once shut.
                        .hover(move |d| if open { d } else { d.bg(p.hover) })
                        .tip_while_shut(open, words::search::filter_tip(&label))
                        .child(icon("funnel", metrics::GLYPH, if on { p.accent } else { p.text_muted }))
                        .child(div().min_w_0().truncate().child(chosen.unwrap_or_else(|| words::search::filter().into())))
                        .child(icon("chevron-down", metrics::GLYPH, p.text_faint))
                        .map(|d| Self::opener(d, Self::filter_open, cx))
                        .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                            if this.filter_open() {
                                this.close_menus(true, window, cx);
                            } else if !this.pressed_shut(event) {
                                this.open_filter_menu(window, cx);
                            }
                        })),
                ),
            )
            .when_some(self.menu_element("search-filter-menu", words::search_filter(), cx).filter(|_| open), |d, menu| {
                d.child(drop_down(menu, MenuSide::Left, 200., Some(320.), self.scale()))
            })
            .into_any_element()
    }

    /// The examples to search for, while nothing is searched or opened.
    pub(crate) fn home_hint(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
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
            .text_xs()
            .text_color(p.text_muted)
            .child(icon("search", metrics::MESSAGE_GLYPH, p.text_faint))
            .child(words::search::hint())
            // Ringed as a field is; the rings' room makes up most of the gaps. In a narrow window they go on
            // to a second line rather than past the edges.
            .child(div().flex().flex_wrap().justify_center().gap_0p5().children(words::search::examples().iter().map(|&(query, label)| {
                let button = Button::new(query)
                    .outline()
                    .accessibility_label(format!("{query} {label}"))
                    .h(scaled(metrics::SMALL_CONTROL_HEIGHT))
                    .px_2p5()
                    .gap_1p5()
                    .child(div().text_color(p.accent).font_family(metrics::FONT_CODE).child(query))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let filter = this.chosen_filter(cx);
                        this.search_for(query, filter, window, cx)
                    }));
                self.ringed(button, Stop::Hint(query.to_string()), RingAt::Round, window, cx)
            })))
            .into_any_element()
    }
}
