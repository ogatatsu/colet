//! Home's pane on the left: its heading, its rows under the headings of their kinds, the line where a row
//! dragged among them would land, and the row that follows the pointer.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    columns::row_height,
    home::{HomeDrag, movable},
    metrics::{self, scaled},
    state::Kind,
    theme::ActivePalette,
    view::*,
};

impl Colet {
    pub(crate) fn render_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
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
            .bg(p.bg)
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

/// The line showing where a row dragged in home lands: drawn across the boundary between two rows, half
/// on either side.
const LANDING_LINE: f32 = 2.;

/// What follows the pointer as a row is dragged in home: its row, faint.
pub struct HomeGhost {
    name: String,
    search: bool,
    image: Option<std::sync::Arc<RenderImage>>,
    width: Pixels,
}

impl HomeGhost {
    /// The row named `name`, a saved search's when `search`, with its shell icon, `width` wide.
    pub fn new(name: String, search: bool, image: Option<std::sync::Arc<RenderImage>>, width: Pixels) -> Self {
        Self { name, search, image, width }
    }
}

impl Render for HomeGhost {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = cx.palette();
        let picture = match (self.search, self.image.clone()) {
            (true, _) => icon("search", 16., p.text_muted).into_any_element(),
            (false, Some(image)) => img(image).size(scaled(metrics::ICON)).flex_none().into_any_element(),
            (false, None) => div().size(scaled(metrics::ICON)).flex_none().into_any_element(),
        };
        div()
            .w(self.width)
            .h(scaled(row_height(Kind::Home)))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .rounded(scaled(metrics::RADIUS))
            .border_1()
            .border_color(p.border_strong)
            .bg(p.raised)
            // Faint enough that the landing line it passes over still shows through.
            .opacity(0.5)
            .font_family(metrics::FONT_UI)
            .text_size(scaled(metrics::TEXT_LARGE))
            .text_color(p.text)
            .child(picture)
            .child(div().min_w_0().truncate().child(self.name.clone()))
    }
}

impl Colet {
    /// Home's rows: a handful of places, so no need to virtualize. The kinds stand under headings, so they
    /// don't read as one list; a drag among them shows where it would land.
    pub(crate) fn home_rows(&mut self, marked: bool, keys: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let entries = self.state.columns[0].entries.clone();
        let groups = crate::home::home_groups(self.stored.settings(), &entries);
        // Drawn afresh below, for a drag to find its place.
        let dragging = self.home_drag.read(cx);
        dragging.rows().borrow_mut().clear();
        dragging.headings().borrow_mut().clear();
        let headings = dragging.headings().clone();
        // The line where a drag would land: above the row it goes before, or below its kind's last.
        let landing = dragging.carried().zip(dragging.landing()).map(|(d, l)| (d.shelf, l.before.clone()));
        let mut rows = div()
            .id("home-rows")
            .role(Role::ListBox)
            .aria_label("ホーム")
            .size_full()
            .overflow_y_scroll()
            .track_scroll(&self.scrolls.home)
            .on_drag_move(
                cx.listener(|this, event: &DragMoveEvent<HomeDrag>, _, cx| this.drag_home(event.event.position, event.bounds, cx)),
            )
            .on_drop(cx.listener(|this, drag: &HomeDrag, _, cx| this.drop_home(drag, cx)))
            .test_support();
        for (k, (title, items)) in groups.into_iter().enumerate() {
            let headings = headings.clone();
            rows = rows.child(
                div()
                    // Set off from the kind above, so the kinds read apart.
                    .when(k > 0, |d| d.mt_2())
                    .relative()
                    .child(canvas(move |bounds, _, _| headings.borrow_mut().push(bounds), |_, _, _, _| {}).absolute().inset_0())
                    .h_7()
                    .px_3()
                    .pb_1p5()
                    .flex()
                    .items_end()
                    .text_size(scaled(metrics::TEXT_BODY))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(p.text_muted)
                    .child(title),
            );
            for (i, entry) in items.iter().enumerate() {
                let line = landing.as_ref().and_then(|(shelf, before)| match before {
                    Some(before) => (*before == entry.path).then_some(true),
                    None => (i + 1 == items.len() && movable(self.stored.settings(), &entry.path) == Some(*shelf)).then_some(false),
                });
                rows = rows.child(div().px_1().relative().child(self.row(0, entry, marked, keys, cx)).when_some(line, |d, above| {
                    d.child(
                        div()
                            .absolute()
                            .left_1p5()
                            .right_1p5()
                            .h(scaled(LANDING_LINE))
                            .rounded_full()
                            .bg(p.accent)
                            .when(above, |l| l.top(scaled(-LANDING_LINE / 2.)))
                            .when(!above, |l| l.bottom(scaled(-LANDING_LINE / 2.))),
                    )
                }));
            }
        }
        div()
            .flex_1()
            .min_h_0()
            .relative()
            .child(rows)
            .child(div().absolute().inset_0().child(scrollbar(gpui_kit::base::Scrollbar::vertical(&self.scrolls.home), p, self.scale())))
            .into_any_element()
    }
}
