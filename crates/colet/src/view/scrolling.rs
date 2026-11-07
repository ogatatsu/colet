//! The columns scrolled sideways: what of them is on screen, and bringing one in sight.
use super::*;

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

    /// The strip the `away` folders folded above a result stand in for, which brings them out.
    pub(super) fn unfold_strip(&mut self, away: usize, cx: &mut Context<Self>) -> AnyElement {
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
                    .h(px(metrics::HEADING_HEIGHT))
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
                    .text_size(px(metrics::TEXT_BODY))
                    .text_color(p.text_muted)
                    .child(away.to_string())
                    .test_support(),
            )
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
    pub(super) fn scrolling_columns(&self, columns: Vec<AnyElement>, cx: &mut Context<Self>) -> AnyElement {
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

    /// Slides the columns toward where they should rest, a frame at a time.
    pub(super) fn reveal(&mut self, window: &mut Window) {
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
            let shown: Vec<ColumnId> = self.state.columns[self.state.columns.len() - cells.len()..].iter().map(|c| c.id).collect();
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
    pub(super) fn scrolling(&self) -> (Vec<Cell>, Option<usize>) {
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
