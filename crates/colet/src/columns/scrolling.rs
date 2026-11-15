//! The columns scrolled sideways: what of them is on screen, and bringing one in sight.
use std::time::Instant;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    metrics::{self, Scale, scaled},
    reveal::{Cell, Slide, on_screen, refit, reveal_target, uncover_target},
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

    /// How wide the search results are to be: their own width, or all the window leaves past the room of
    /// the folders on screen beside them and of the fold strip, if that is more. The folders' room is
    /// never less than one column's, so moving between a folder, which opens beside them, and a file,
    /// which opens nothing, leaves the results as wide as they were. They narrow back to their own width
    /// only as more folders open than the window holds beside them.
    pub fn results_width(&self) -> f32 {
        let beside = self.on_screen().count.max(1) as f32 * self.widths.column + FOLD_WIDTH;
        self.widths.search.max((self.scrolls.sideways.main - beside).floor())
    }

    /// The results are drawn wider than their own width.
    pub(crate) fn results_stretched(&self) -> bool {
        self.scrolls.sideways.results > self.widths.search
    }

    /// Takes the results a frame's step toward [`Colet::results_width`], sliding as the columns do, and
    /// tells how far they narrowed (widened, below naught) from as they were drawn. They take their width
    /// at once as they first show and while the window or home's pane changes width; then they have not
    /// moved under the columns (see [`Colet::fit`]), and the step is naught.
    fn step_results(&mut self, window: &mut Window) -> f32 {
        let target = if self.state.pinned() { self.results_width() } else { 0. };
        let sideways = &mut self.scrolls.sideways;
        let drawn = sideways.results;
        let resized = (sideways.main - sideways.results_main).abs() >= 0.5;
        sideways.results_main = sideways.main;
        if target <= 0. || drawn <= 0. || resized {
            sideways.results = target;
            sideways.results_slide = None;
            return 0.;
        }
        if (target - drawn).abs() < 0.5 && sideways.results_slide.is_none() {
            return 0.;
        }
        let slide = sideways.results_slide.get_or_insert_with(|| Slide::new(drawn, drawn, target, None));
        slide.target = target;
        let (width, done) = slide.step(Instant::now());
        if done {
            sideways.results_slide = None;
        } else {
            window.request_animation_frame();
        }
        sideways.results = width;
        drawn - width
    }

    pub fn on_screen(&self) -> OnScreen {
        let first = if self.state.pinned() { 2 } else { 1 } + self.state.folded_away();
        let count = self.state.columns.len().saturating_sub(first);
        let view = self.scrolls.sideways.view_width();
        // A wheel moves the offset past its bounds until the next layout brings it back: read it as it
        // will be drawn, so the edges' shades do not flash on.
        let max = self.scrolls.sideways.max_offset().max(self.scrolls.sideways.hold);
        let from = self.scrolls.sideways.offset().clamp(0., max);
        let cells: Vec<Cell> = (0..count).map(|i| Cell { start: i as f32 * self.widths.column, width: self.widths.column }).collect();
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
                    .text_size(scaled(metrics::TEXT_BODY))
                    .text_color(p.text_muted)
                    .child(away.to_string())
                    .child("階")
                    .child("層")
                    .test_support(),
            )
            // The button laid over the whole strip: its ring's room would push the heading's line off the
            // columns'.
            .child(
                // Gone once pressed: clicked, the keys stay where they were rather than go with it; pressed
                // with the keys on it, they go to the column worked in.
                keep_focus(ring(div().id("unfold").track_focus(&focus), p, focus.is_focused(window), 0.))
                    .role(Role::Button)
                    .aria_label(format!("上の階層を表示（{away}階層）"))
                    .absolute()
                    .inset_0()
                    .tip(format!("上の階層を表示（{away}階層）"))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        this.state.unfold();
                        if event.is_keyboard() {
                            this.focus_active(window, cx);
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
        let content = screen.count as f32 * self.widths.column;
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
            .when(screen.hidden_left(), |d| d.child(div().absolute().top_0().bottom_0().left_0().w_7().bg(shade(90.))))
            .when(screen.hidden_right(), |d| d.child(div().absolute().top_0().bottom_0().right_0().w_7().bg(shade(270.))))
            .child(self.scrolling_frame(window, cx))
            .into_any_element()
    }

    /// The frame round the active one of the scrolling columns, drawn over them from outside their scroll
    /// so it lies on the lines round the column: the one before it (or what stands left of the scroll),
    /// the path bar's above, its own, and the status bar's below. Where the column is cut, it runs along
    /// the edge of the view. Read at paint, it stays on the column as it scrolls.
    fn scrolling_frame(&self, window: &Window, cx: &App) -> impl IntoElement {
        let active = if self.columns_marked(window) { self.scrolling().1 } else { None };
        let (scroll, width, color) = (self.scrolls.sideways.scroll.clone(), self.widths.column, cx.palette().accent);
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
            .top(px(-1.))
            .bottom(px(-1.))
            .left(px(-1.))
            .right_0(),
        )
    }

    /// Slides the columns toward where they should rest, a frame at a time.
    pub(crate) fn reveal(&mut self, window: &mut Window) {
        let mut view = self.scrolls.sideways.view_width();
        let mut from = self.scrolls.sideways.offset();
        // The results beside the columns narrow or widen, as folders open or close: the view's left edge
        // moves by as much, and its width with it. The columns on screen stay where they are drawn, so the
        // folders come out from under the results' edge, and are brought on screen in the view as it is
        // about to be laid out.
        let narrowed = self.step_results(window);
        if narrowed.abs() > 0. && view > 0. {
            view += narrowed;
            from -= narrowed;
            self.scrolls.sideways.set_offset(from.max(0.));
            self.scrolls.sideways.hold = self.scrolls.sideways.hold.max(from);
            if let Some(slide) = &mut self.scrolls.sideways.slide {
                slide.shift(-narrowed);
            }
        }
        let fold = self.state.fold.map(|f| (f.anchor, self.state.folded_away()));
        let came_out = match (self.scrolls.sideways.drawn_fold, fold) {
            (Some((was, away)), Some((anchor, 0))) if was == anchor && away > 0 => Some(away),
            _ => None,
        };
        self.scrolls.sideways.drawn_fold = fold;
        let mut keep = None;
        if let Some(away) = came_out
            && view > 0.
        {
            // The columns on screen stay where they are drawn: the folders come out ahead of them, in the
            // room of the strip and past the left edge, and slide in from there.
            let shift = away as f32 * self.widths.column - FOLD_WIDTH;
            from += shift;
            self.scrolls.sideways.set_offset(from);
            self.scrolls.sideways.hold = self.scrolls.sideways.hold.max(from);
            if let Some(slide) = &mut self.scrolls.sideways.slide {
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
            let target = match keep {
                Some(keep) => uncover_target(view, &cells, keep),
                // Home or the pinned results taken up again beside the same columns, as ← brings the keys
                // there, leave the columns where they are.
                None if active.is_none() && shown == self.scrolls.sideways.revealed => start,
                None => reveal_target(start, view, &cells, active),
            };
            self.scrolls.sideways.revealed = shown;
            self.scrolls.sideways.hold = target.max(from);
            self.scrolls.sideways.fitted = (view, self.widths.column);
            if (target - from).abs() >= 0.5 {
                match &mut self.scrolls.sideways.slide {
                    Some(slide) => {
                        slide.origin = start;
                        slide.target = target;
                        slide.keep = keep;
                    }
                    None => self.scrolls.sideways.slide = Some(Slide::new(start, from, target, keep)),
                }
            }
        }
        let Some(slide) = &mut self.scrolls.sideways.slide else { return };
        let (position, done) = slide.step(Instant::now());
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
        let width = self.widths.column;
        let cells = (0..count).map(|i| Cell { start: i as f32 * width, width }).collect();
        (cells, self.state.active.checked_sub(first))
    }

    /// Layout has made the columns' view `view` wide, ahead of drawing them: when it or the columns' width
    /// changed since they were brought to rest, they follow it in this frame. At rest they go straight
    /// to where [`refit`] puts them, as a window or a width is dragged; sliding, as when the folders
    /// folded away come out and their strip goes, they slide on to where they would now rest.
    pub fn fit(&mut self, view: f32, cx: &mut Context<Self>) {
        let (was, was_width) = self.scrolls.sideways.fitted;
        if view <= 0. || ((view - was).abs() < 0.5 && was_width == self.widths.column) {
            return;
        }
        self.scrolls.sideways.fitted = (view, self.widths.column);
        // The first layout: nothing rests anywhere yet.
        if was <= 0. {
            return;
        }
        let (cells, active) = self.scrolling();
        let sideways = &mut self.scrolls.sideways;
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
