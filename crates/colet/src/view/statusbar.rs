//! The status bar along the bottom.
use super::*;
use crate::loading::Engine;
use crate::words;

impl Colet {
    pub(super) fn render_statusbar(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let history = self.history_button(window, cx);
        // Home worked in but folded into its strip is out of sight: the bar tells of the columns left to
        // see, the last of them as the address ends at, rather than of a pane that is not there.
        let told = match self.state.active {
            0 if !self.home_open(window, cx) => self.state.columns.len() - 1,
            active => active,
        };
        let active = &self.state.columns[told];
        let place = match active.kind {
            Kind::Folder => shown(&active.path),
            Kind::Search if active.error.is_some() => words::status::cannot_search().into(),
            Kind::Search if active.unread() => words::status::searching().into(),
            Kind::Search => words::search_results().into(),
            Kind::Home => words::home().into(),
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
                let what = if entry.directory { words::folder().into() } else { time::size(entry.size) };
                match entry.modified.and_then(time::date_time) {
                    Some(when) => words::status::with_time(&what, when),
                    None => what,
                }
            }
            _ => String::new(),
        };
        let engine = self.loading.read(cx).engine();
        let (name, title) = engine.describe();
        let (color, dot) = match engine {
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
                    .child(place)
                    .test_support(),
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
