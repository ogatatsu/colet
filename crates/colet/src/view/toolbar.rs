//! The bar along the top: back, forward and refresh, the search filters' button and the search field, the
//! address, and the preferences, each drawn by its own feature.
use super::*;

impl Colet {
    fn icon_button(id: &'static str, name: &'static str, label: &'static str, tip: &'static str, enabled: bool) -> Button {
        Button::new(id).ghost().icon(glyph(name)).accessibility_label(label).tooltip(tip).disabled(!enabled)
    }

    pub(super) fn render_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let (back, forward) = (self.state.can_go_back(), self.state.can_go_forward());
        let filter_button = self.filter_button(window, cx);
        let search_box = self.render_search_box(window, cx);
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
        div()
            .h_12()
            .flex_none()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(p.border)
            .child(div().flex().flex_none().gap_0p5().child(back).child(forward).child(refresh))
            .child(filter_button)
            .child(search_box)
            .child(self.field_resizer(cx))
            .child(address)
            .child(preferences)
            .into_any_element()
    }
}
