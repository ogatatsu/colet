//! The status bar along the bottom.
use super::*;
use crate::loading::Engine;
use crate::words;

impl Colet {
    pub(super) fn render_statusbar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let history = self.history_button(window, cx);
        let active = &self.state.columns[self.state.active];
        let place = match active.kind {
            Kind::Folder => shown(&active.path),
            Kind::Search if active.error.is_some() => words::status::CANNOT_SEARCH.into(),
            Kind::Search if active.unread() => words::status::SEARCHING.into(),
            Kind::Search => words::SEARCH_RESULTS.into(),
            Kind::Home => words::HOME.into(),
        };
        // What a filter hides is not counted, nor acted on.
        let shown_selected = active.shown_selected();
        // Only one picked row is looked up: each is a search through the rows, done on every frame.
        let picked = match shown_selected.as_ref() {
            [path] => active.entry(path),
            _ => None,
        };
        let detail = match picked {
            _ if shown_selected.len() > 1 => words::status::selected(shown_selected.len()),
            Some(entry) => {
                let what = if entry.directory { words::FOLDER.into() } else { time::size(entry.size) };
                match entry.modified.and_then(time::date_time) {
                    Some(when) => words::status::with_time(&what, when),
                    None => what,
                }
            }
            _ => String::new(),
        };
        let (name, title) = self.loading.engine().describe();
        let (color, dot) = match self.loading.engine() {
            Engine::Unknown | Engine::Connecting => (p.text_faint, None),
            Engine::Connected(_) => (p.text_faint, Some(p.accent)),
            Engine::Failed(_) => (p.danger, Some(p.danger)),
        };
        StatusBar::new()
            .h(scaled(metrics::STATUS_BAR_HEIGHT))
            .flex_none()
            .px_3()
            .gap_4()
            .text_xs()
            .child(
                div()
                    .id("place")
                    .role(Role::Status)
                    .aria_label(place.clone())
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis_start()
                    .child(place),
            )
            .right(div().flex_none().child(detail))
            .right(
                div()
                    .id("engine")
                    .flex_none()
                    .max_w(scaled(200.))
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .text_color(color)
                    .tip(title)
                    .child(
                        div()
                            .size_1p5()
                            .rounded(cx.theme().radius_full())
                            .border_1()
                            .border_color(dot.unwrap_or(p.text_faint))
                            .when_some(dot, |d, c| d.bg(c)),
                    )
                    .child(div().truncate().child(name)),
            )
            .right(history)
            .into_any_element()
    }
}
