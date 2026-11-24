//! The columns scrolled sideways: what of them is on screen, and bringing one in sight.
use std::time::Instant;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::*,
    metrics::{self, Scale, scaled},
    reveal::{Cell, Rested, Slide, on_screen, refit, reveal_target, uncover_target},
    state::ColumnId,
    theme::ActivePalette,
    view::*,
};

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
        let room = Scale::of(window).down(f32::from(window.viewport_size().width)) - MAIN_MIN;
        self.widths.sidebar.min(room.max(SIDEBAR_WIDTHS.0))
    }

    pub fn on_screen(&self) -> OnScreen {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        // As this frame lays it out: the strip of folders gone, the view the last frame laid out would
        // still end short of the columns' and shade the right edge for a frame.
        // Home's pane, stepped ahead of this layout as it folds into its strip or comes out, gives the view
        // the room it takes from it, or takes back: the edges' shades would show a frame behind it.
        let view = self.scrolls.sideways.view_ahead() + self.scrolls.sideways.fitted_home - self.home_pane.width();
        // A wheel moves the offset past its bounds until the next layout brings it back: read it as it
        // will be drawn, so the edges' shades do not flash on.
        let max = self.scrolls.sideways.max_offset().max(self.scrolls.sideways.hold);
        let from = self.scrolls.sideways.offset().clamp(0., max);
        let width = self.column_drawn();
        let cells: Vec<Cell> = (0..count).map(|i| Cell { start: i as f32 * width, width }).collect();
        OnScreen { first, count, span: on_screen(from, view, &cells) }
    }

    /// The strip the `away` folders folded above a result stand in for, which brings them out.
    pub(crate) fn unfold_strip(&mut self, away: usize, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let focus = self.stop("unfold", cx);
        div()
            .id("unfold-strip")
            .w(scaled(FOLD_WIDTH))
            .h_full()
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .items_center()
            .bg(p.bg)
            .border_r_1()
            .border_color(p.border)
            .hover(|d| d.bg(p.hover))
            // In line with the columns' headings, where the eye runs, and how many it holds below.
            .child(
                div()
                    .id("unfold-head")
                    .h(scaled(metrics::HEADING_HEIGHT))
                    .w_full()
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .border_b_1()
                    .border_color(p.border)
                    .child(icon("chevrons-left", metrics::GLYPH, p.text_muted))
                    .test_support(),
            )
            .child(
                // Written down the strip, as Japanese is set upright: the count across, as in upright text,
                // then "階層", so it reads as what it counts and not as a bare number.
                div()
                    .id("unfold-count")
                    .pt_2()
                    .flex()
                    .flex_col()
                    .items_center()
                    .text_xs()
                    .text_color(p.text_muted)
                    .font_features(crate::view::written_down())
                    .child(away.to_string())
                    .children(words::column::folded().iter().copied())
                    .test_support(),
            )
            // The button laid over the whole strip: its ring's room would push the heading's line off the
            // columns'.
            .child(
                // Gone once pressed: clicked, the keys stay where they were rather than go with it; pressed
                // with the keys on it, they go to the column worked in.
                keep_focus(ring(div().id("unfold").track_focus(&focus), p, focus.is_focused(window), 0.))
                    .role(Role::Button)
                    .aria_label(words::column::show_levels_above(away))
                    .absolute()
                    .inset_0()
                    .tip(words::column::show_levels_above_key(away))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        this.state.unfold();
                        if event.is_keyboard() {
                            this.come_back_to_active(window, cx);
                        } else {
                            this.changed(cx);
                        }
                    }))
                    .test_support(),
            )
            .into_any_element()
    }

    /// The scrolling `columns` in their sideways scroll, with its scrollbar and a shade on each edge that
    /// cuts a column.
    pub(crate) fn scrolling_columns(&self, columns: Vec<AnyElement>, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let screen = self.on_screen();
        // As wide as the columns are drawn side by side, each rounded on its own.
        let content = screen.count as f32 * self.column_drawn();
        let hold = self.scrolls.sideways.hold;
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
                    move |bounds, window, cx| {
                        let view = Scale::of(window).down(f32::from(bounds.size.width));
                        fit.update(cx, |this, cx| this.fit(view, cx)).ok();
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
                    .track_scroll(&self.scrolls.sideways.scroll)
                    // The crumbs' bar and the edges' shades follow the columns as they scroll.
                    .on_scroll(cx.listener(|_, _, _, cx| cx.notify()))
                    .child(div().h_full().w(scaled(content)).flex().children(columns))
                    // The room past the last column is no column's: a press there leaves the keys where they
                    // are, rather than the columns' focus sending them beside home.
                    .child(
                        keep_focus(div().id("columns-rest")).absolute().top_0().bottom_0().left(scaled(content)).right_0().test_support(),
                    )
                    // Room up to a view's width past where the columns were brought, so the scroll width
                    // cannot shrink under them while they slide back. A view wide however wide layout makes
                    // it: reckoned here, it would be the last frame's.
                    .child(div().absolute().top_0().left(scaled(hold)).w_full().h_0())
                    .test_support(),
            )
            .child(div().absolute().inset_0().child(scrollbar(
                gpui_kit::base::Scrollbar::horizontal(&self.scrolls.sideways.scroll),
                p,
                self.scale(),
            )))
            .when(screen.hidden_left(), |d| {
                d.child(div().id("shade-left").absolute().top_0().bottom_0().left_0().w_7().bg(shade(90.)).test_support())
            })
            .when(screen.hidden_right(), |d| {
                d.child(div().id("shade-right").absolute().top_0().bottom_0().right_0().w_7().bg(shade(270.)).test_support())
            })
            .child(self.scrolling_frame(window, cx))
            .into_any_element()
    }

    /// The frame round the active one of the scrolling columns, drawn over them from outside their scroll
    /// so it lies on the lines round the column: the one before it (or what stands left of the scroll),
    /// the path bar's above, its own, and the status bar's below. Where the column is cut, it runs along
    /// the edge of the view. Read at paint, it stays on the column as it scrolls.
    fn scrolling_frame(&self, window: &Window, cx: &App) -> impl IntoElement {
        let active = if self.columns_marked(window) { self.scrolling().1 } else { None };
        let (scroll, width, color) = (self.scrolls.sideways.scroll.clone(), self.column_drawn(), cx.palette().accent);
        // A pixel past the view on every side but the right, where the window ends; drawn last, or the
        // status bar drawn after the columns would cover its bottom.
        on_top(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let Some(active) = active else { return };
                    // Reckoned as drawn, so the frame's lines stay a pixel wide and on the column's at any
                    // zoom.
                    let width = Scale::of(window).up(width);
                    let view = f32::from(bounds.size.width) - 1.;
                    let start = active as f32 * width + f32::from(scroll.offset().x);
                    let (left, right) = ((start - 1.).max(-1.), (start + width).min(view));
                    if right - left <= 2. {
                        return;
                    }
                    let origin = point(bounds.origin.x + px(1. + left), bounds.origin.y);
                    window.paint_quad(outline(Bounds::new(origin, size(px(right - left), bounds.size.height)), color, BorderStyle::Solid));
                },
            )
            .absolute()
            // A pixel's line, which stays one at any zoom.
            .top(-metrics::HAIRLINE)
            .bottom(-metrics::HAIRLINE)
            .left(-metrics::HAIRLINE)
            .right_0(),
        )
    }

    /// A scrolling column's width as layout draws it (see [`Sideways::drawn`]), which where the columns
    /// stand and rest is reckoned in, so they come to rest on the pixel.
    pub(crate) fn column_drawn(&self) -> f32 {
        self.scrolls.sideways.drawn(self.widths.column)
    }

    /// Slides the columns toward where they should rest, a frame at a time; with motion reduced, as with
    /// Windows' animations off, they are there at once.
    pub(crate) fn reveal(&mut self, window: &mut Window, cx: &App) {
        let mut view = self.scrolls.sideways.view_width();
        let mut from = self.scrolls.sideways.offset();
        let fold = self.state.fold.map(|f| (f.anchor, self.state.folded_away()));
        let came_out = match (self.scrolls.sideways.drawn_fold, fold) {
            (Some((was, away)), Some((anchor, 0))) if was == anchor && away > 0 => Some(away),
            _ => None,
        };
        self.scrolls.sideways.drawn_fold = fold;
        let mut uncover = None;
        if let Some(away) = came_out
            && view > 0.
        {
            // The columns on screen stay where they are drawn: the folders come out ahead of them, in the
            // room of the strip and past the left edge, and slide in from there. In widths as drawn, or
            // at a zoom that draws them a fraction wide the columns would move by the fractions.
            let strip = self.scrolls.sideways.drawn(FOLD_WIDTH);
            let shift = away as f32 * self.column_drawn() - strip;
            from += shift;
            self.scrolls.sideways.set_offset(from);
            self.scrolls.sideways.hold = self.scrolls.sideways.hold.max(from);
            if let Some(slide) = &mut self.scrolls.sideways.slide {
                slide.shift(shift);
            }
            // The strip's room goes to the view in this frame's layout: reckoned with it, so the columns
            // are not left to jump there once laid out.
            view += strip;
            // Brought out by the strip rather than by going into one of them, they slide in only as far as
            // the room past the last column takes.
            if let Some(f) = self.state.folded()
                && !(f.from..f.to).contains(&self.state.active)
            {
                uncover = Some(from);
            }
            self.scrolls.sideways.ask();
        }
        if let Some(smooth) = self.scrolls.sideways.pending.take() {
            if view <= 0. {
                // Not laid out yet: ask again next frame.
                self.scrolls.sideways.pending = Some(smooth);
                window.request_animation_frame();
                return;
            }
            let (cells, active) = self.scrolling();
            let start = self.scrolls.sideways.slide.as_ref().map_or(from, |s| s.target);
            let shown: Vec<ColumnId> = self.state.columns[self.state.columns.len() - cells.len()..].iter().map(|c| c.id).collect();
            // Home's pane coming out of its strip or going back in: they make for where they rest once it
            // stands, as the last layout's view leaves room for it then, so they go one way only. With motion
            // reduced it stands there already, wider or narrower than that layout had it.
            let aim = view + self.scrolls.sideways.fitted_home - self.home_pane.target();
            // Home folding into its strip as the keys leave it: the room it gives the view would be left
            // empty past the last column where they stood, brought there for the view it left them.
            let flush = uncover.is_none() && aim - view >= 0.5;
            let mut target = match uncover {
                Some(at) => uncover_target(at, aim, &cells),
                // Home or the pinned results taken up again beside the same columns, as ← brings the keys
                // there, leave the columns where they are.
                None if active.is_none() && shown == self.scrolls.sideways.revealed => start,
                None => reveal_target(start, aim, &cells, active),
            };
            if flush {
                target = uncover_target(target, aim, &cells);
            }
            self.scrolls.sideways.revealed = shown;
            self.scrolls.sideways.hold = target.max(from);
            self.scrolls.sideways.fitted = (view, self.column_drawn());
            if (target - from).abs() >= 0.5 {
                match &mut self.scrolls.sideways.slide {
                    Some(slide) => {
                        slide.origin = start;
                        slide.target = target;
                        slide.uncover = uncover;
                        slide.flush = flush;
                    }
                    None => self.scrolls.sideways.slide = Some(Slide { flush, ..Slide::new(start, from, target, uncover) }),
                }
            } else if uncover.is_some() {
                // Folders come out with no room past the last column: the columns stay where they are drawn,
                // a slide under way stopped there rather than carried on, or eased back from its momentum.
                self.scrolls.sideways.slide = None;
                self.scrolls.sideways.hold = from;
            }
        }
        let Some(slide) = &mut self.scrolls.sideways.slide else { return };
        let (position, done) = if cx.reduce_motion() { slide.rest() } else { slide.step(Instant::now()) };
        self.scrolls.sideways.set_offset(position);
        if done {
            // At rest, the room held for the slide goes: left past the last column, it would scroll the
            // columns off to the left.
            self.scrolls.sideways.hold = position;
            self.scrolls.sideways.slide = None;
        } else {
            window.request_animation_frame();
        }
    }

    /// The scrolling columns' places, and which of them is active: `None` for home or the pinned results.
    pub(crate) fn scrolling(&self) -> (Vec<Cell>, Option<usize>) {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        let width = self.column_drawn();
        let cells = (0..count).map(|i| Cell { start: i as f32 * width, width }).collect();
        (cells, self.state.active.checked_sub(first))
    }

    /// Layout has made the columns' view `view` wide, ahead of drawing them: when it or the columns' width
    /// changed since they were brought to rest, they follow it in this frame. At rest they go straight
    /// to where [`refit`] puts them, as a window or a width is dragged; sliding, as when the folders
    /// folded away come out and their strip goes, they slide on to where they would now rest.
    pub fn fit(&mut self, view: f32, cx: &mut Context<Self>) {
        let (was, was_width) = self.scrolls.sideways.fitted;
        // The width as drawn, so the columns' end they rested over is the one drawn then, whatever zoom
        // they are drawn at now.
        let width = self.column_drawn();
        let (home, was_home) = (self.home_pane.width(), std::mem::replace(&mut self.scrolls.sideways.fitted_home, self.home_pane.width()));
        if view <= 0. || ((view - was).abs() < 0.5 && was_width == width) {
            return;
        }
        self.scrolls.sideways.fitted = (view, width);
        // The first layout: nothing rests anywhere yet.
        if was <= 0. {
            return;
        }
        // Home's pane coming out of its strip, or going back in, is all that changed the view: the columns
        // stay where they are drawn, pushed aside and cut on the right while it is out. Sliding as the keys
        // came home, they slide on to where they were going, and stand where they would have had the slide
        // ended first; reckoned again with home worked in, they would make for the last column instead.
        // Gone into a column as it folds, they are reckoned with the view it leaves them once it stands, the
        // same however far it has gone, so they go one way only.
        let aim = view + home - self.home_pane.target();
        // However little it moved: as it slows to rest it moves a fraction of a pixel a frame, while the view
        // still changes by a pixel rounded, which taken for the window's would set them flush at once.
        let pushed = self.widths.resizing() != Some(WidthOf::Sidebar) && home != was_home && (view + home - was - was_home).abs() <= 1.;
        let (cells, active) = self.scrolling();
        let sideways = &mut self.scrolls.sideways;
        match &mut sideways.slide {
            Some(_) if pushed && active.is_none() => {}
            Some(slide) => {
                slide.target = match slide.uncover {
                    Some(at) => uncover_target(at, aim, &cells),
                    None if slide.flush => uncover_target(reveal_target(slide.origin, aim, &cells, active), aim, &cells),
                    None => reveal_target(slide.origin, aim, &cells, active),
                };
                sideways.hold = slide.target.max(slide.position);
            }
            None if pushed => {}
            None => {
                let target = refit(Rested { at: sideways.offset(), view: was, end: cells.len() as f32 * was_width }, view, &cells, active);
                sideways.set_offset(target);
                sideways.hold = target;
            }
        }
        cx.notify();
    }
}
