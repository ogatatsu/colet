//! The trail of folders above the columns, and the address field typed in in its place.
use super::*;
use gpui_kit::component::menu::PopupMenuItem;

use crate::menus::Popup;

impl Colet {
    /// The bar under the crumbs of the columns on screen, cut where the edges cut the columns; only while
    /// some are out of sight, folded away too. Painted from where the crumbs stand in the same frame: their
    /// places are all known once the frame is laid out, so a crumb just added has its bar at once.
    fn crumb_bar(&self, cx: &App) -> Option<AnyElement> {
        let screen = self.on_screen();
        let out = screen.hidden_left() || screen.hidden_right() || self.state.folded_away() > 0;
        let Some(span) = screen.span.filter(|_| out) else {
            self.crumbs.bar.set(None);
            return None;
        };
        // The crumbs' ids by the columns' places on screen, read against their places once painted.
        let ids: Vec<ColumnId> = self.state.columns.iter().skip(screen.first).map(|c| c.id).collect();
        let (bounds, bar) = (self.crumbs.bounds.clone(), self.crumbs.bar.clone());
        let color = cx.palette().border_strong;
        Some(
            div()
                .id("crumb-bar")
                .absolute()
                .size_full()
                .child(
                    canvas(
                        |_, _, _| {},
                        move |row, _, window, _| {
                            let (bounds, scale) = (bounds.borrow(), Scale::of(window));
                            let drawn = |k: usize| {
                                let b = bounds.get(ids.get(k)?)?;
                                Some((scale.down(f32::from(b.left())), scale.down(f32::from(b.right()))))
                            };
                            let placed = crate::crumbs::bar(span, drawn);
                            bar.set(placed);
                            if let Some((start, end)) = placed {
                                let at = Bounds::new(
                                    point(scale.px(start), row.bottom() - scale.px(2.)),
                                    size(scale.px((end - start).max(0.)), scale.px(2.)),
                                );
                                window.paint_quad(fill(at, color).corner_radii(scale.px(1.)));
                            }
                        },
                    )
                    .size_full(),
                )
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
            .filter(|c| c.kind == Kind::Folder)
            .map(|c| Crumb { column: c.id, name: shown(folder_name(&c.path)), path: shown(&c.path) })
            .collect()
    }

    /// Each crumb's whole width at 100%: its name in the window's text size, padding, the chevron before it and the
    /// gap after.
    fn crumb_widths(&self, crumbs: &[Crumb], window: &Window) -> Vec<f32> {
        let (font, scale) = (font(metrics::FONT_UI), Scale::of(window));
        crumbs
            .iter()
            .enumerate()
            .map(|(k, Crumb { name: label, .. })| {
                let run = TextRun {
                    len: label.len(),
                    font: font.clone(),
                    // Only measured: the color does not change the width.
                    color: Hsla::default(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(label.clone().into(), scale.px(metrics::TEXT_LARGE), &[run], None);
                // The first stands at the bar's start, with no chevron or gap before it.
                let before = if k == 0 { 0. } else { crate::crumbs::SEPARATOR + crate::crumbs::GAP };
                scale.down(f32::from(shaped.width)).ceil() + 12. + before
            })
            .collect()
    }

    /// The address beside the search field: the trail of crumbs, or the field to type a path in.
    pub(super) fn render_address(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let crumbs = self.trail();
        let widths = self.crumb_widths(&crumbs, window);
        // The room the bar was last drawn with; before the first frame, a guess at what the toolbar leaves it.
        let row_width = match self.crumbs.row.get() {
            Some(row) => self.scale().down(f32::from(row.size.width)),
            None => (self.scale().down(f32::from(window.viewport_size().width)) - ADDRESS_GUESS - self.widths.field)
                .max(crate::crumbs::TYPING_ROOM),
        };
        self.crumbs.room.set(row_width);
        // The crumbs leave a strip to press to type a path, however long the trail.
        let room = row_width - crate::crumbs::TYPING_ROOM;
        let folded = crate::crumbs::folded(&widths, room);
        let last = crumbs.len().saturating_sub(1);
        self.crumbs.bounds.borrow_mut().retain(|id, _| crumbs[folded..].iter().any(|c| c.column == *id));
        for Crumb { column: id, .. } in &crumbs[folded..] {
            self.stop(Stop::Crumb(*id), cx);
        }
        let shown: Vec<Crumb> = crumbs[folded..].to_vec();
        let active = self.state.columns.get(self.state.active).map(|c| c.id);
        let menu_current = crumbs[..folded].iter().any(|c| Some(c.column) == active);
        self.crumbs.folded = crumbs[..folded].to_vec();
        let fold = (folded > 0).then(|| self.crumb_fold(menu_current, window, cx));
        let (editing, address_field) = {
            let address = self.address.read(cx);
            (address.is_open(), address.field().clone())
        };
        let trail = div()
            .flex()
            .flex_1()
            .h_full()
            .relative()
            .items_center()
            .gap_0p5()
            .min_w_0()
            .overflow_hidden()
            .child({
                let (row, room) = (self.crumbs.row.clone(), self.crumbs.room.clone());
                canvas(
                    move |bounds, window, _| {
                        row.set(Some(bounds));
                        if (Scale::of(window).down(f32::from(bounds.size.width)) - room.get()).abs() > 0.5 {
                            window.request_animation_frame();
                        }
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            })
            .children(self.crumb_bar(cx))
            .children(fold)
            .children(shown.into_iter().enumerate().map(|(k, crumb)| {
                let max_width = (folded + k == last).then_some(room / 2.);
                self.crumb(crumb, k > 0 || folded > 0, max_width, window, cx)
            }))
            // The room past the crumbs: a press there turns the trail into the field.
            .child(
                div()
                    .id("address-room")
                    .flex_1()
                    .min_w(scaled(crate::crumbs::TYPING_ROOM))
                    .h_full()
                    .cursor_text()
                    .when(crumbs.is_empty(), |d| {
                        d.flex()
                            .items_center()
                            .px_1p5()
                            .child(div().flex_1().min_w_0().truncate().text_color(p.text_muted).child(crate::address::PLACEHOLDER))
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.open_address(window, cx)))
                    .test_support(),
            );
        let field = div()
            .flex_1()
            .min_w_0()
            .px_1p5()
            .key_context(ADDRESS)
            .capture_any_mouse_down(Self::right_press_menu(&address_field, cx))
            .on_action(cx.listener(|this, _: &input::Escape, window, cx| this.leave_address(window, cx)))
            .on_action(cx.listener(|this, _: &GoToAddressInNewTab, _, cx| this.go_to_address(true, cx)))
            .child(Input::new(&address_field));
        // Ringed as the search field is while it takes the keys; the ring lies over the gaps around. It gives
        // way first in a narrow window, folding its crumbs.
        frame(p, editing, metrics::FIELD_RADIUS)
            .flex_1()
            .min_w(scaled(ADDRESS_MIN))
            .m(scaled(-RING_ROOM))
            .text_size(scaled(metrics::TEXT_BODY))
            .child(
                div()
                    .id("address")
                    .w_full()
                    .h(scaled(metrics::CONTROL_HEIGHT))
                    .pl_1()
                    .pr_0p5()
                    .flex()
                    .items_center()
                    .bg(p.surface)
                    .border_1()
                    .border_color(p.border_strong)
                    .rounded(scaled(metrics::FIELD_RADIUS))
                    .when(editing, |d| d.child(field))
                    .when(!editing, |d| d.child(trail))
                    .test_support(),
            )
            .into_any_element()
    }

    /// The "…" button the crumbs folded off the trail go into, with their menu while it is open;
    /// `current` while the column worked in is among them.
    fn crumb_fold(&mut self, current: bool, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let open = self.crumbs_open();
        let menu = if open { self.menu_element("crumb-menu", "上の階層", cx) } else { None };
        let focus = self.stop("crumb-fold", cx);
        div()
            .relative()
            .child(
                ring(crate::view::keep_focus(div().id("crumb-fold")).track_focus(&focus), p, focus.is_focused(window), metrics::RADIUS)
                    .role(Role::Button)
                    .aria_label("上の階層")
                    .aria_expanded(open)
                    .size(scaled(metrics::ICON_BUTTON))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(scaled(metrics::RADIUS))
                    .hover(|d| d.bg(p.hover))
                    .tip_while_shut(open, "上の階層")
                    .child(icon("ellipsis", 15., if current { p.text } else { p.text_muted }))
                    .map(|d| Self::opener(d, Self::crumbs_open, cx))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        if this.crumbs_open() {
                            this.close_crumb_menu(event.is_keyboard(), window, cx);
                        } else if !this.pressed_shut(event) {
                            this.open_crumb_menu(window, cx);
                        }
                    }))
                    .test_support(),
            )
            .when_some(menu, |d, menu| d.child(drop_down(menu, MenuSide::Left, 160., None, self.scale())))
            .into_any_element()
    }

    /// A crumb on the trail, a chevron before it unless it stands first; the last is clipped to
    /// `max_width`, the others keep their whole names.
    pub(super) fn crumb(&self, crumb: Crumb, chevron: bool, max_width: Option<f32>, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = cx.palette();
        let Crumb { column: id, name: label, path } = crumb;
        let current = self.state.index_of(id) == Some(self.state.active);
        let stop = self.stops.get(&Stop::Crumb(id));
        // Without a stop of its own it is never ringed, though it stands for the columns' keys.
        let ringed = stop.as_ref().is_some_and(|s| s.is_focused(window));
        let focus = stop.unwrap_or_else(|| self.columns_focus.clone());
        div()
            .flex()
            .items_center()
            .gap_0p5()
            // As large as the rows' names, though the bar's buttons stay smaller.
            .text_size(scaled(metrics::TEXT_LARGE))
            .when_some(max_width, |d, width| d.min_w_0().max_w(scaled(width)))
            .when(max_width.is_none(), |d| d.flex_none())
            .when(chevron, |d| d.child(icon("chevron-right", 13., p.text_faint)))
            .child(
                // The mouse leaves the keys where they are; the column it goes to takes them.
                ring(crate::view::keep_focus(div().id(("crumb", id))).track_focus(&focus), p, ringed, metrics::RADIUS)
                    .role(Role::Button)
                    .aria_label(label.clone())
                    .relative()
                    .child({
                        let crumbs = self.crumbs.bounds.clone();
                        canvas(
                            move |bounds, _, _| {
                                crumbs.borrow_mut().insert(id, bounds);
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full()
                    })
                    .h_6()
                    .px_1p5()
                    .flex()
                    .items_center()
                    .gap_1()
                    .min_w_0()
                    .rounded(scaled(metrics::RADIUS))
                    .text_color(if current { p.text } else { p.text_muted })
                    .hover(|d| d.bg(p.hover))
                    .tip(path)
                    .child(div().truncate().child(label))
                    .on_click(cx.listener(move |this, _, window, cx| this.go_to_column(id, window, cx)))
                    .test_support(),
            )
    }
}

impl Colet {
    pub fn crumbs_open(&self) -> bool {
        matches!(self.overlays.popup(), Some(Popup::Crumbs))
    }

    /// Opens the menu of the folders folded off the trail, as it was last drawn.
    pub fn open_crumb_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlays.set_popup(Popup::Crumbs);
        let active = self.state.columns.get(self.state.active).map(|c| c.id);
        let colet = cx.weak_entity();
        let items = self
            .crumbs
            .folded
            .iter()
            .map(|crumb| {
                let (id, label, path) = (crumb.column, crumb.name.clone(), crumb.path.clone());
                let colet = colet.clone();
                PopupMenuItem::element(move |_, _| div().id(("crumb-item", id)).test_support().tip(path.clone()).child(label.clone()))
                    .icon(crate::view::glyph("folder"))
                    .checked(Some(id) == active)
                    .on_click(move |_, window, cx| {
                        colet
                            .update(cx, |this, cx| {
                                this.close_crumb_menu(false, window, cx);
                                this.go_to_column(id, window, cx);
                            })
                            .ok();
                    })
            })
            .collect();
        self.show_items(items, window, cx);
    }

    /// Closes the trail's menu; `to_button` hands the keys back to the button that opened it.
    pub fn close_crumb_menu(&mut self, to_button: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.crumbs_open() {
            self.overlays.close_menu();
        }
        if to_button {
            self.stop("crumb-fold", cx).focus(window, cx);
        }
        cx.notify();
    }
}
