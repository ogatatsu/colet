//! The trail of folders above the columns, and the address field typed in in its place.
use super::*;

impl Colet {
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

    /// The trail from the search or the drive down to the deepest folder; when it outgrows the bar, the
    /// crumbs furthest from the deepest fold into a menu so the nearest stay readable.
    /// The crumbs of the trail: each folder column's. The search results have none: the search field beside
    /// it stands for them.
    pub(super) fn trail(&self) -> Vec<Crumb> {
        self.state
            .columns
            .iter()
            .enumerate()
            .filter(|(_, c)| c.kind == Kind::Folder)
            .map(|(index, c)| Crumb { column: index, name: shown(folder_name(&c.path)), path: shown(&c.path) })
            .collect()
    }

    /// Each crumb's whole width as drawn: its 13px name, padding, the chevron before it and the gap after.
    fn crumb_widths(&self, crumbs: &[Crumb], window: &Window) -> Vec<f32> {
        let font = font("Segoe UI");
        crumbs
            .iter()
            .enumerate()
            .map(|(k, Crumb { name: label, .. })| {
                let run = TextRun {
                    len: label.len(),
                    font: font.clone(),
                    color: self.palette.text,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(label.clone().into(), px(13.), &[run], None);
                // The first stands at the bar's start, with no chevron or gap before it.
                let before = if k == 0 { 0. } else { crate::crumbs::SEPARATOR + crate::crumbs::GAP };
                f32::from(shaped.width).ceil() + 12. + before
            })
            .collect()
    }

    /// The row under the toolbar: the file actions, and at its end how the columns show their items.
    pub(super) fn render_pathbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let controls = self.view_controls(window, cx);
        div()
            .h(px(40.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .border_b_1()
            .border_color(p.border)
            .child(self.render_actions(cx))
            .child(controls)
            .into_any_element()
    }

    /// The address beside the search field: the trail of crumbs, or the field to type a path in.
    pub(super) fn render_address(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let crumbs = self.trail();
        let widths = self.crumb_widths(&crumbs, window);
        // The room the bar was last drawn with; before the first frame, a guess at what the toolbar leaves it.
        let row_width = match self.crumbs.row.get() {
            Some(row) => f32::from(row.size.width),
            None => (f32::from(window.viewport_size().width) - ADDRESS_GUESS).max(crate::crumbs::TYPING_ROOM),
        };
        self.crumbs.room.set(row_width);
        // The crumbs leave a strip to press to type a path, however long the trail.
        let room = row_width - crate::crumbs::TYPING_ROOM;
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
        let editing = self.address_open;
        let trail = div()
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
            }))
            // The room past the crumbs: a press there turns the trail into the field.
            .child(
                div()
                    .id("address-room")
                    .flex_1()
                    .min_w(px(crate::crumbs::TYPING_ROOM))
                    .h_full()
                    .cursor_text()
                    .when(crumbs.is_empty(), |d| {
                        d.flex()
                            .items_center()
                            .px(px(6.))
                            .child(div().flex_1().min_w_0().truncate().text_color(p.text_muted).child(crate::address::PLACEHOLDER))
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.open_address(window, cx)))
                    .test_support(),
            );
        let field = div()
            .flex_1()
            .min_w_0()
            .px(px(6.))
            .key_context(ADDRESS)
            .capture_any_mouse_down(Self::right_press_menu(&self.address, cx))
            .on_action(cx.listener(|this, _: &input::Escape, window, cx| this.leave_address(window, cx)))
            .on_action(cx.listener(|this, _: &GoToAddressInNewTab, window, cx| this.go_to_address(true, window, cx)))
            .child(Input::new(&self.address));
        // Ringed as the search field is while it takes the keys; the ring lies over the gaps around. It gives
        // way first in a narrow window, folding its crumbs.
        frame(p, editing)
            .flex_1()
            .min_w(px(ADDRESS_MIN))
            .m(px(-3.))
            .text_size(px(12.))
            .child(
                div()
                    .id("address")
                    .w_full()
                    .h(px(32.))
                    .pl(px(4.))
                    .pr(px(2.))
                    .flex()
                    .items_center()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border_strong)
                    .rounded(px(6.))
                    .when(editing, |d| d.child(field))
                    .when(!editing, |d| d.child(trail))
                    .test_support(),
            )
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
                    .child(icon("ellipsis", 15., if current { p.text } else { p.text_muted }))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        if this.crumbs_open() {
                            this.close_crumb_menu(event.is_keyboard(), window, cx);
                        } else {
                            this.open_crumb_menu(window, cx);
                        }
                    })),
            )
            .when_some(menu, |d, menu| {
                d.child(deferred(div().absolute().top(px(28.)).left_0().min_w(px(160.)).child(menu)).with_priority(1))
            })
            .into_any_element()
    }

    /// A crumb on the trail, a chevron before it unless it stands first; the last is clipped to
    /// `max_width`, the others keep their whole names.
    pub(super) fn crumb(&self, crumb: Crumb, chevron: bool, max_width: Option<f32>, cx: &mut Context<Self>) -> Div {
        let p = self.palette;
        let Crumb { column: index, name: label, path } = crumb;
        let current = index == self.state.active;
        let focus = self.stops.get(&format!("crumb:{index}")).cloned().unwrap_or_else(|| self.columns_focus.clone());
        div()
            .flex()
            .items_center()
            .gap(px(2.))
            // As large as the rows' names, though the bar's buttons stay smaller.
            .text_size(px(13.))
            .when_some(max_width, |d, width| d.min_w_0().max_w(px(width)))
            .when(max_width.is_none(), |d| d.flex_none())
            .when(chevron, |d| d.child(icon("chevron-right", 13., p.text_faint)))
            .child(
                ring(div().id(("crumb", index)).track_focus(&focus), p)
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .relative()
                    .child({
                        let crumbs = self.crumbs.bounds.clone();
                        canvas(
                            move |bounds, _, _| {
                                crumbs.borrow_mut().insert(index, bounds);
                            },
                            |_, _, _, _| {},
                        )
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
                    .child(div().truncate().child(label))
                    .on_click(cx.listener(move |this, _, window, cx| this.go_to(index, window, cx)))
                    .test_support(),
            )
    }
}
