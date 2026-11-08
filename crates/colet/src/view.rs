//! How the window looks: home's pane on the left; the search field, the trail of folders, the columns
//! and the status bar beside it.
use std::{ops::Range, rc::Rc, time::Instant};

use colet_core::{
    entry::{Entry, Sort, direction_label},
    paths::{folder_name, id_text, shown},
    time,
};
use gpui_kit::base::input::{self, Input, InputState};
use gpui_kit::component::{
    Disableable, FocusableExt, Selectable, Sizable,
    button::{Button, ButtonVariants},
    status_bar::StatusBar,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::*,
    crumbs::Crumb,
    metrics,
    places::{HomeDrag, movable},
    reveal::{Cell, Slide, on_screen, refit, reveal_target, uncover_target},
    state::{Column, ColumnId, Kind},
    state::{favorite_key, narrowed_folder, saved_query},
};

mod column;
mod elements;
mod pathbar;
mod rows;
mod scrolling;
mod sidebar;
mod statusbar;
mod toolbar;

use elements::on_top;
use rows::{MODIFIED_WIDTH, SIZE_WIDTH};

pub trait TipExt {
    /// A line of text shown while the pointer rests on it.
    fn tip(self, text: impl Into<SharedString>) -> Self;
}

impl<E: StatefulInteractiveElement> TipExt for E {
    fn tip(self, text: impl Into<SharedString>) -> Self {
        let text = text.into();
        self.tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(text.clone()).build(window, cx))
    }
}

/// The icon named `name` among Colet's, for a button.
pub fn glyph(name: &str) -> gpui_kit::component::Icon {
    gpui_kit::component::Icon::empty().path(format!("icons/{name}.svg"))
}

/// The loading icon, turning once a second.
pub fn spinner(id: impl Into<ElementId>, size: f32, color: Hsla) -> AnimationElement<Svg> {
    icon("loader-circle", size, color).with_animation(id, Animation::new(std::time::Duration::from_secs(1)).repeat(), |svg, delta| {
        svg.with_transformation(Transformation::rotate(percentage(delta)))
    })
}

/// A thin scrollbar laid over a list, as the old app's; its thumb is Colet's color.
pub fn scrollbar(bar: gpui_kit::base::Scrollbar, p: crate::theme::Palette) -> gpui_kit::base::Scrollbar {
    bar.mode(gpui_kit::base::ScrollbarMode::Always).styles(|s| {
        s.track(|t| t.bg(transparent_black()).border_color(transparent_black()).width(px(SCROLLBAR_WIDTH)))
            .thumb(|t| t.bg(p.scrollbar).width(px(6.)).inset(px(1.)).radius(px(3.)))
            .thumb_hover(|t| t.bg(p.text_muted))
            .thumb_active(|t| t.bg(p.text_muted))
    })
}

/// How wide the edge a drag widens a column or home's pane from is.
const RESIZER_WIDTH: f32 = 6.;
/// How thick a scrollbar's track is.
pub(crate) const SCROLLBAR_WIDTH: f32 = 8.;

/// The focus ring of what Tab reached: two pixels in the focus color, kept clear until then.
pub fn ring(el: Stateful<Div>, p: crate::theme::Palette) -> Stateful<Div> {
    el.border_2().border_color(transparent_black()).focus_visible(|s| s.border_color(p.focus))
}

/// The ring of a field, or of a button with a border of its own: two pixels in the focus color a pixel
/// off it, laid round it and kept clear until `shown`.
pub fn frame(p: crate::theme::Palette, shown: bool) -> Div {
    div().p(px(1.)).rounded(px(8.)).border_2().border_color(if shown { p.focus } else { transparent_black() })
}

/// The frame round the column worked in, laid over it on the lines round it: from the path bar's line
/// above to its own on the right; the caller sets its left and bottom.
fn active_frame(p: crate::theme::Palette) -> Div {
    div().absolute().top(px(-1.)).right_0().border_1().border_color(p.accent)
}

/// Tab, not the mouse, brought the keys to `focus`: when [`ring`] shows.
pub fn keyed(focus: &FocusHandle, window: &Window) -> bool {
    focus.is_focused(window) && window.last_input_was_keyboard()
}

/// A button the mouse presses without taking the keys from where they are; Tab still reaches it.
pub fn keep_focus<E: InteractiveElement>(el: E) -> E {
    el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
}

fn icon(name: &'static str, size: f32, color: Hsla) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

/// `text` with what a search matched in it in `color`.
fn with_marks(text: &str, marks: &[Range<usize>], color: Hsla) -> StyledText {
    let style = HighlightStyle { color: Some(color), ..Default::default() };
    StyledText::new(text.to_owned()).with_highlights(marks.iter().map(|range| (range.clone(), style)))
}

impl Colet {
    /// A button that opens a menu or the history, `open` telling whether it is, and closes it pressed
    /// again. A press outside what it opened closes that before the button's click comes, so the press
    /// notes whether it was open and [`Colet::pressed_shut`] tells the click.
    pub(crate) fn opener<E: InteractiveElement>(el: E, open: impl Fn(&Colet) -> bool + 'static, cx: &mut Context<Self>) -> E {
        el.capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, _| this.pressed_open = open(this)))
    }

    /// The press `event` ends found open what its button opens, and closed it already: the click is not
    /// to open it again. Enter and Space press nothing; a press left noted by no click is noted over by
    /// the next press, the only one a click can end.
    pub(crate) fn pressed_shut(&mut self, event: &ClickEvent) -> bool {
        std::mem::take(&mut self.pressed_open) && !event.is_keyboard()
    }

    /// The keys are in home's pane.
    fn in_home(&self) -> bool {
        self.state.columns.get(self.state.active).is_some_and(|c| c.kind == Kind::Home)
    }

    fn render_main(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let pinned = self.state.pinned();
        let away = self.state.folded_away();
        let first = if pinned { 2 } else { 1 } + away;
        let only_home = self.state.columns.len() == 1;
        self.sync_narrow_fields(window, cx);
        let pinned_column = pinned.then(|| self.render_column(1, window, cx));
        let strip = (away > 0).then(|| self.unfold_strip(away, cx));
        let columns: Vec<AnyElement> = (first..self.state.columns.len()).map(|i| self.render_column(i, window, cx)).collect();
        let scrolling = self.scrolling_columns(columns, cx);
        let main = div()
            .id("main")
            .track_focus(&self.columns_focus)
            .flex_1()
            .min_h_0()
            .flex()
            .relative()
            .bg(p.bg)
            .child(self.takes_text(&self.columns_focus, cx))
            .children(pinned_column)
            .children(strip)
            .child(scrolling)
            // Nothing searched or opened leaves this side dark; show where to start.
            .when(only_home, |d| d.child(self.home_hint(window, cx)));
        self.column_keys(main, cx).into_any_element()
    }

    fn home_hint(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_3()
            .p_6()
            .text_size(px(metrics::TEXT_BODY))
            .text_color(p.text_muted)
            .child(icon("search", 28., p.text_faint))
            .child("名前やEverythingの検索式で探せます")
            // Ringed as a field is; the rings' room makes up most of the gaps. In a narrow window they go on
            // to a second line rather than past the edges.
            .child(div().flex().flex_wrap().justify_center().gap(px(2.)).children(
                [("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")].map(
                    |(query, label)| {
                        let focus = self.stop(format!("hint:{query}"), cx);
                        frame(p, keyed(&focus, window)).child(
                            Button::new(query)
                                .outline()
                                .track_focus(&focus)
                                .focus_ring(false)
                                .accessibility_label(format!("{query} {label}"))
                                .h(px(28.))
                                .px(px(10.))
                                .gap(px(6.))
                                .child(div().text_color(p.accent).font_family("Consolas").child(query))
                                .child(label)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    let filter = this.chosen_filter.clone();
                                    this.search_for(query, filter, window, cx)
                                })),
                        )
                    },
                ),
            ))
            .into_any_element()
    }
}

impl Render for Colet {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.search_focused = self.has_keys(&self.search, window, cx);
        self.follow_address(window, cx);
        // A drag in home let go of anywhere but its list.
        if self.home_drag.carried.is_some() && !cx.has_active_drag() {
            self.end_home_drag();
        }
        // And a tab's let go of off the tab bar.
        if self.tabs.drag.carried.is_some() && !cx.has_active_drag() {
            self.end_tab_drag();
        }
        self.follow_focus(window, cx);
        self.scroll_rows();
        self.reveal(window);
        let p = self.palette;
        let tabbar = self.render_tabbar(window, cx);
        let sidebar = self.render_sidebar(window, cx);
        let toolbar = self.render_toolbar(window, cx);
        let pathbar = self.render_pathbar(window, cx);
        let main = self.render_main(window, cx);
        div()
            .id("colet")
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .bg(p.bg)
            .text_color(p.text)
            .font_family("Segoe UI")
            .text_size(px(metrics::TEXT_LARGE))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| {
                this.search.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(window, cx);
                });
            }))
            .on_action(cx.listener(|this, _: &FocusAddress, window, cx| this.open_address(window, cx)))
            .on_action(cx.listener(|this, _: &OpenTextMenu, window, cx| this.open_focused_text_menu(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSearchFilters, window, cx| {
                if this.dialog.is_none() {
                    this.open_filter_menu(window, cx);
                }
            }))
            // As the strip's button does: the keys stay where they are.
            .on_action(cx.listener(|this, _: &Unfold, _, cx| {
                if this.dialog.is_none() && this.state.folded_away() > 0 {
                    this.state.unfold();
                    this.changed(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &FocusHome, window, cx| {
                if this.dialog.is_none() {
                    this.focus_home(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleHidden, _, cx| {
                if this.dialog.is_none() {
                    this.toggle_hidden(cx);
                }
            }))
            // Esc puts down a drag in home, before the keys reach anything else.
            .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if this.home_drag.carried.is_some() && event.keystroke.key == "escape" {
                    cx.stop_active_drag(window);
                    this.end_home_drag();
                    cx.notify();
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|this, _: &NewTab, window, cx| this.new_tab(window, cx)))
            .on_action(cx.listener(|this, _: &CloseTab, window, cx| {
                let current = this.tabs.current;
                this.close_tab(current, window, cx);
            }))
            .on_action(cx.listener(|this, _: &NextTab, window, cx| this.step_tab(1, window, cx)))
            .on_action(cx.listener(|this, _: &PreviousTab, window, cx| this.step_tab(-1, window, cx)))
            .on_action(cx.listener(|this, _: &ReopenTab, window, cx| this.reopen_tab(window, cx)))
            .on_action(cx.listener(|this, _: &RefreshAll, _, cx| this.refresh(cx)))
            .on_action(cx.listener(|this, _: &SaveCurrent, _, cx| {
                if this.dialog.is_none() {
                    this.save_current(cx);
                }
            }))
            // The mouse's back and forward buttons, as Alt+← and Alt+→.
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Back), cx.listener(|this, _, window, cx| this.back(window, cx)))
            .on_mouse_down(MouseButton::Navigate(NavigationDirection::Forward), cx.listener(|this, _, window, cx| this.forward(window, cx)))
            .when(self.resizing.is_some(), |d| {
                d.cursor_col_resize()
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        let Some(resize) = &this.resizing else { return };
                        let (min, max) = match resize.of {
                            WidthOf::Sidebar => SIDEBAR_WIDTHS,
                            WidthOf::Field => SEARCH_FIELD_WIDTHS,
                            WidthOf::Search | WidthOf::Column => COLUMN_WIDTHS,
                        };
                        // Display scaling makes the deltas fractional; widths stay whole pixels.
                        let width = (resize.start_width + f32::from(event.position.x - resize.start_x)).clamp(min, max).round();
                        match resize.of {
                            WidthOf::Sidebar => this.widths.sidebar = width,
                            WidthOf::Search => this.widths.search = width,
                            WidthOf::Column => this.widths.column = width,
                            WidthOf::Field => this.widths.field = width,
                        }
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.end_resize(cx);
                            cx.notify();
                        }),
                    )
            })
            .child(tabbar)
            .child(toolbar)
            .child(pathbar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .child(sidebar)
                    .child(div().flex_1().min_w_0().h_full().flex().flex_col().child(main).child(self.render_statusbar(cx))),
            )
            // Over a dialog, a field's menu is drawn with it.
            .children(if self.dialog.is_none() { self.render_menu(cx) } else { None })
            .children(self.render_history(cx))
    }
}
