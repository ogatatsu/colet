//! Home's pane on the left.
use super::*;

impl Colet {
    pub(super) fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let home = &self.state.columns[0];
        let (id, narrowed) = (home.id, home.narrow.is_some());
        let field = self.narrow_fields.get(&id).map(|field| field.input.clone());
        let marked = self.state.active == 0 && self.columns_marked(window);
        let body = self.column_body(0, marked, window, cx);
        let width = self.sidebar_width(window);
        let keys = div()
            .id("home")
            .track_focus(&self.home_focus)
            .flex_1()
            .min_h_0()
            .relative()
            .flex()
            .flex_col()
            .child(self.takes_text(&self.home_focus, cx))
            .child(body);
        let keys = self.column_keys(keys, cx);
        div()
            .id("sidebar")
            .w(scaled(width))
            .flex_none()
            .h_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.sidebar)
            .border_r_1()
            .border_color(p.border)
            // A press anywhere in it works in home, as one in a column works there: its field keeps the
            // keys it took, the rest of it hands them to home's rows.
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !window.default_prevented() {
                        this.home_focus.focus(window, cx);
                        window.prevent_default();
                    }
                    if this.state.active != 0 {
                        this.state.set_active(0);
                        this.changed(cx);
                    }
                }),
            )
            // Headed as a column is, so its filter field stands level with theirs.
            .child(
                div()
                    .id("home-head")
                    .h(scaled(metrics::HEADING_HEIGHT))
                    .flex_none()
                    .pl_3()
                    .pr_2()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(scaled(metrics::TEXT_BODY))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(if marked { p.text } else { p.text_muted })
                            .child("ホーム"),
                    ),
            )
            .when_some(field, |d, input| {
                d.child(
                    div()
                        .id("home-filter")
                        .flex_none()
                        .px_2()
                        .py_1p5()
                        .border_b_1()
                        .border_color(p.border)
                        .child(self.column_narrow_field(id, narrowed, &input, window, cx))
                        .test_support(),
                )
            })
            .child(keys)
            .child(self.resizer("sidebar-resizer", WidthOf::Sidebar, width, cx))
            // Its left and bottom are the window's edges: drawn inside them.
            .when(marked, |d| d.child(on_top(active_frame(p).left_0().bottom_0())))
            .test_support()
            .into_any_element()
    }
}
