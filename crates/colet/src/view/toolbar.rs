//! The bar along the top: the search field and the buttons beside it.
use super::*;

impl Colet {
    fn icon_button(id: &'static str, name: &'static str, label: &'static str, tip: &'static str, enabled: bool) -> Button {
        Button::new(id).ghost().icon(glyph(name)).accessibility_label(label).tooltip(tip).disabled(!enabled)
    }

    pub(super) fn render_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focused = self.search_has_keys(window, cx);
        let has_query = !self.state.query.is_empty();
        let field_width = self.widths.field;
        let (back, forward) = (self.state.can_go_back(), self.state.can_go_forward());

        let filter_button = self.filter_button(window, cx);
        let address = self.render_address(window, cx);
        let back = Self::icon_button("back", "arrow-left", "戻る", "戻る（Alt+←）", back)
            .on_click(cx.listener(|this, _, window, cx| this.back(window, cx)));
        let back = self.ringed(back, "back", RingAt::Edge, window, cx);
        let forward = Self::icon_button("forward", "arrow-right", "進む", "進む（Alt+→）", forward)
            .on_click(cx.listener(|this, _, window, cx| this.forward(window, cx)));
        let forward = self.ringed(forward, "forward", RingAt::Edge, window, cx);
        let refresh =
            Self::icon_button("refresh", "refresh-cw", "更新", "更新（F5）", true).on_click(cx.listener(|this, _, _, cx| this.refresh(cx)));
        let refresh = self.ringed(refresh, "refresh", RingAt::Edge, window, cx);
        let preferences = Self::icon_button("preferences", "sliders-horizontal", "設定", "設定", true)
            .on_click(cx.listener(|this, _, window, cx| this.open_preferences(window, cx)));
        let preferences = self.ringed(preferences, "preferences", RingAt::Edge, window, cx);
        let clear = has_query.then(|| {
            let clear = Button::new("clear-search")
                .ghost()
                .xsmall()
                .icon(glyph("x"))
                .accessibility_label("検索をクリア")
                .tooltip("検索をクリア（Esc）")
                .on_click(cx.listener(|this, _, window, cx| {
                    this.clear_search(window, cx);
                    this.search.update(cx, |input, cx| input.focus(window, cx));
                }));
            self.ringed(clear, "clear-search", RingAt::Edge, window, cx)
        });
        div()
            .h(px(48.))
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .child(div().flex().flex_none().gap(px(2.)).child(back).child(forward).child(refresh))
            .child(filter_button)
            .child(
                // Ringed as a dialog's field is; the ring lies over the gaps around, not in the row. It keeps
                // its width while the address beside it gives way, then narrows, keeping room to type in.
                frame(p, focused, 6.)
                    .flex_none()
                    .w(px(field_width))
                    .flex_shrink(1.)
                    .min_w(px(SEARCH_FIELD_WIDTHS.0))
                    .m(px(-RING_ROOM))
                    .child(
                        div()
                            .id("search-box")
                            .w_full()
                            .h(px(metrics::CONTROL_HEIGHT))
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
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .capture_any_mouse_down(Self::right_press_menu(&self.search, cx))
                                    .child(Input::new(&self.search)),
                            )
                            // ↓ goes into the results, as from a search field in Explorer; Esc clears the search.
                            .capture_action(cx.listener(|this, _: &input::MoveDown, window, cx| {
                                cx.stop_propagation();
                                this.leave_search(window, cx);
                            }))
                            // Esc empties the field, then lets go of the filter, and home shows alone.
                            .on_action(cx.listener(|this, _: &input::Escape, window, cx| {
                                if !this.state.query.is_empty() {
                                    this.clear_search(window, cx);
                                } else if this.chosen_filter.is_some() {
                                    this.choose_filter(None, cx);
                                }
                            }))
                            .children(clear)
                            .test_support(),
                    ),
            )
            .child(self.field_resizer(cx))
            .child(address)
            .child(preferences)
            .into_any_element()
    }

    /// The chevron from what is searched to where it is, and the edge a drag widens the search field from:
    /// over the gaps on either side, lit while pointed at or dragged.
    fn field_resizer(&self, cx: &mut Context<Self>) -> AnyElement {
        const ID: &str = "search-field-resizer";
        let p = self.palette;
        let dragging = self.widths.resizing() == Some(WidthOf::Field);
        let width = self.widths.field;
        div()
            .id(ID)
            .group(ID)
            .flex_none()
            .h(px(metrics::CONTROL_HEIGHT))
            .px(px(4.))
            .mx(px(-4.))
            .flex()
            .items_center()
            .cursor_col_resize()
            .child(icon("chevron-right", 14., if dragging { p.accent } else { p.text_faint }).group_hover(ID, |s| s.text_color(p.accent)))
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

    /// How the columns show their items, at the file actions' row's end: the default sort order, plain as
    /// the actions are, and whether the hidden items show.
    pub(super) fn view_controls(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let sort = self.stored.settings().sort;
        let sort_focused = self.sort_focus.is_focused(window);
        // Pressed while the hidden items show.
        let show = self.stored.settings().show_hidden;
        let hidden = Self::icon_button(
            "show-hidden",
            if show { "eye" } else { "eye-off" },
            "隠しファイルを表示",
            "隠しファイルを表示（Ctrl+H）",
            true,
        )
        .selected(show)
        .toggled(show)
        .on_click(cx.listener(|this, _, _, cx| this.toggle_hidden(cx)));
        let hidden = self.ringed(hidden, "show-hidden", RingAt::Edge, window, cx);
        div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .child(
                div()
                    .relative()
                    .child(
                        ring(
                            crate::view::keep_focus(div().id("sort"))
                                .track_focus(&self.sort_focus)
                                .key_context(SORT)
                                .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_sort(window, cx)))
                                .role(Role::Button)
                                .aria_label(format!("既定の並び順: {}", sort.label()))
                                .aria_expanded(self.sort_open())
                                .tip("既定の並び順")
                                .h(px(28.))
                                .px_2()
                                .flex()
                                .items_center()
                                .gap(px(6.))
                                .text_size(px(metrics::TEXT_BODY))
                                .text_color(p.text)
                                .rounded(px(6.))
                                .hover(|d| d.bg(p.hover))
                                .child(icon("arrow-up-down", 14., p.text_muted))
                                .child(sort.label())
                                .child(icon("chevron-down", 14., p.text_faint))
                                .map(|d| Self::opener(d, Self::sort_open, cx))
                                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                                    if this.sort_open() {
                                        this.close_menus(true, window, cx);
                                    } else if !this.pressed_shut(event) {
                                        this.open_sort(window, cx);
                                    }
                                })),
                            p,
                            sort_focused,
                            6.,
                        )
                        .test_support(),
                    )
                    .when_some(self.menu_element("sort-menu", "既定の並び順", cx).filter(|_| self.sort_open()), |d, menu| {
                        d.child(
                            deferred(div().absolute().top(px(32.)).right_0().w(px(160.)).child(menu))
                                .with_priority(gpui_kit::base::POPUP_PRIORITY),
                        )
                    }),
            )
            .child(hidden)
            .into_any_element()
    }

    /// The search filters' button beside the field, naming the one chosen in the accent color, and its menu.
    fn filter_button(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let focus = self.stop("search-filter", cx);
        let chosen = self.chosen_filter.clone();
        let on = chosen.is_some();
        let label = format!("検索フィルター: {}", chosen.as_deref().unwrap_or("なし"));
        // In a narrow window its name gives way to the search field, down to the funnel and the chevron;
        // the tip still names it.
        div()
            .relative()
            // Its name gives way before the search field narrows.
            .flex_shrink(100.)
            // The funnel and the chevron with their padding, gaps and border: the button never goes under
            // the sort order's.
            .min_w(px(60.))
            .child(
                frame(p, focus.is_focused(window), 6.).min_w_0().m(px(-RING_ROOM)).child(
                    crate::view::keep_focus(div().id("search-filter"))
                        .test_support()
                        .track_focus(&focus)
                        .key_context(SORT)
                        .on_action(cx.listener(|this, _: &OpenSort, window, cx| this.open_filter_menu(window, cx)))
                        .role(Role::Button)
                        .aria_label(label.clone())
                        .aria_expanded(self.filter_open())
                        .h(px(metrics::CONTROL_HEIGHT))
                        .max_w(px(200.))
                        .pl(px(10.))
                        .pr_2()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(metrics::TEXT_BODY))
                        .text_color(if on { p.accent } else { p.text_muted })
                        .bg(p.surface)
                        .border_1()
                        .border_color(if on { p.accent } else { p.border_strong })
                        .rounded(px(6.))
                        .hover(|d| d.bg(p.hover))
                        .tip(format!("{label}（Ctrl+Shift+F）"))
                        .child(icon("funnel", 14., if on { p.accent } else { p.text_muted }))
                        .child(div().min_w_0().truncate().child(chosen.unwrap_or_else(|| "フィルター".into())))
                        .child(icon("chevron-down", 14., p.text_faint))
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
            .when_some(self.menu_element("search-filter-menu", "検索フィルター", cx).filter(|_| self.filter_open()), |d, menu| {
                d.child(
                    deferred(div().absolute().top(px(36.)).left_0().min_w(px(200.)).max_w(px(320.)).child(menu))
                        .with_priority(gpui_kit::base::POPUP_PRIORITY),
                )
            })
            .into_any_element()
    }
}
