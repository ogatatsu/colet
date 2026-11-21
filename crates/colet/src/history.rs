//! The history of the file operations as the window shows it: which operations are shown opened, the
//! focus of each row and item Tab stops at, and the asking whether an item pressed is still there. Colet
//! opens and closes it, as it does a menu or a dialog, and hears from it what it asks for.
use std::{collections::HashSet, path::PathBuf};

use colet_core::paths::{folder_name, shown};
use gpui_kit::component::{
    ActiveTheme as _, Sizable,
    button::{Button, ButtonVariants},
    spinner::Spinner,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::words;
use crate::{
    app::{Colet, Tone},
    backend::{self, PathKind},
    dialogs::push,
    metrics::{self, scaled},
    ops::{ItemStatus, Operations, Status},
    stops::{Stop, Stops},
    theme::ActivePalette,
    view::{RingAt, keep_focus, ring},
};

/// What the history asks of the window.
#[derive(Clone, Debug, PartialEq)]
pub enum HistoryEvent {
    /// Pressed outside it: it is to close.
    Dismiss,
    /// The item pressed is still there: it is to close, and the item to be opened in the columns.
    Reveal { path: PathBuf, folder: bool },
    /// The item pressed has moved on or been deleted since.
    Gone(PathBuf),
}

pub struct HistoryPanel {
    operations: Entity<Operations>,
    backend: backend::Shared,
    /// The operations shown opened.
    expanded: HashSet<u64>,
    /// The item last pressed, while the disk is asked whether it is still there.
    revealing: Option<PathBuf>,
    /// The rows and items Tab stops at, kept only while they are drawn.
    stops: Stops,
    _operations_changed: Subscription,
}

impl EventEmitter<HistoryEvent> for HistoryPanel {}

impl HistoryPanel {
    pub fn new(operations: Entity<Operations>, backend: backend::Shared, cx: &mut Context<Self>) -> Self {
        let changed = cx.observe(&operations, |_, _, cx| cx.notify());
        Self { operations, backend, expanded: HashSet::new(), revealing: None, stops: Stops::default(), _operations_changed: changed }
    }

    /// Opening, with `entry` shown opened and the rest closed.
    pub fn opened(&mut self, entry: Option<u64>, cx: &mut Context<Self>) {
        self.expanded = entry.into_iter().collect();
        cx.notify();
    }

    /// Closing: an item pressed and still being asked about is let go of, and opening it again does not
    /// revive it.
    pub fn closed(&mut self) {
        self.expanded.clear();
        self.revealing = None;
    }

    pub fn is_expanded(&self, entry: u64) -> bool {
        self.expanded.contains(&entry)
    }

    /// Shows `entry` opened, or closed if it was.
    pub fn toggle(&mut self, entry: u64, cx: &mut Context<Self>) {
        if !self.expanded.remove(&entry) {
            self.expanded.insert(entry);
        }
        cx.notify();
    }

    /// The focus of the row of `entry`, for the keys to go to it.
    pub fn entry_focus(&self, entry: u64, cx: &App) -> FocusHandle {
        self.stops.get_or_make(Stop::HistoryEntry(entry), cx)
    }

    /// Asks whether `path` is still there, off the UI thread, as a share can keep it waiting; only the last
    /// item pressed, while the history stays open, is answered for.
    pub fn reveal(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.revealing = Some(path.clone());
        let backend = self.backend.clone();
        let asked = {
            let path = path.clone();
            cx.background_executor().spawn(async move { backend.path_kind(&path) })
        };
        cx.spawn_in(window, async move |this, cx| {
            let kind = asked.await;
            this.update(cx, |this, cx| this.found(path, kind, cx)).ok();
        })
        .detach();
    }

    fn found(&mut self, path: PathBuf, kind: PathKind, cx: &mut Context<Self>) {
        if self.revealing.as_ref() != Some(&path) {
            return;
        }
        self.revealing = None;
        cx.emit(match kind {
            PathKind::Gone => HistoryEvent::Gone(path),
            PathKind::Folder | PathKind::File => HistoryEvent::Reveal { path, folder: kind == PathKind::Folder },
        });
    }

    fn render_entries(&self, window: &Window, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let history = &self.operations.read(cx).history;
        history
            .entries
            .iter()
            .map(|entry| {
                let id = entry.id;
                let open = self.is_expanded(id);
                let problems = entry.has_problems();
                let focus = self.stops.get_or_make(Stop::HistoryEntry(id), cx);
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        keep_focus(ring(div().id(("history-entry", id)).track_focus(&focus), p, focus.is_focused(window), 0.))
                            .role(Role::Button)
                            .aria_label(format!("{} {}", entry.request.kind.title(), entry.target()))
                            .aria_expanded(open)
                            .px_3()
                            .py_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_xs()
                            .hover(|d| d.bg(p.raised_hover))
                            .child(
                                svg()
                                    .path(if open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" })
                                    .size_3()
                                    .text_color(p.text_faint),
                            )
                            .child(div().text_color(p.text_faint).child(clock(entry.started)))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(entry.request.kind.title()))
                            .child(div().flex_1().min_w_0().truncate().text_color(p.text_muted).child(entry.target()))
                            .child(div().text_color(if problems { p.danger } else { p.text_muted }).child(entry.outcome()))
                            .on_click(cx.listener(move |this, _, _, cx| this.toggle(id, cx)))
                            .test_support(),
                    )
                    .when(open, |d| {
                        d.when_some(entry.error.clone(), |d, error| {
                            d.child(div().px_3().pb_1().text_xs().text_color(p.danger).child(error))
                        })
                        .children(entry.items.iter().enumerate().map(|(k, item)| {
                            let path = item.path.clone();
                            let name = shown(folder_name(&item.path));
                            let color = match item.status {
                                ItemStatus::Done | ItemStatus::Destroyed | ItemStatus::Handed => p.text_muted,
                                ItemStatus::Failed => p.danger,
                                _ => p.text_faint,
                            };
                            div()
                                .px_3()
                                .pl_8()
                                .py(scaled(3.))
                                .flex()
                                .flex_col()
                                .text_xs()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(if item.gone {
                                            // Deleted, it has no place to open.
                                            div().child(name).into_any_element()
                                        } else {
                                            let focus = self.stops.get_or_make(Stop::HistoryItem(id, k), cx);
                                            keep_focus(ring(
                                                div().id(history_item_id(id, k)).track_focus(&focus),
                                                p,
                                                focus.is_focused(window),
                                                4.,
                                            ))
                                            // Opened in Colet, not in a browser: a button, its name in the accent color.
                                            .role(Role::Button)
                                            .aria_label(name.clone())
                                            .rounded(scaled(radii.sm))
                                            .text_color(p.accent)
                                            .hover(|d| d.bg(p.raised_hover))
                                            .child(name)
                                            .on_click(cx.listener(move |this, _, window, cx| this.reveal(path.clone(), window, cx)))
                                            .test_support()
                                            .into_any_element()
                                        })
                                        .child(div().text_color(color).child(item.status.label())),
                                )
                                .child(div().truncate().text_color(p.text_faint).child(match &item.from {
                                    Some(from) => format!("← {}", shown(from)),
                                    None => shown(&item.path),
                                }))
                                .when_some(item.error.clone(), |d, error| d.child(div().text_color(p.danger).child(error)))
                                .into_any_element()
                        }))
                        .when(entry.more > 0, |d| {
                            d.child(div().pl_8().py_1().text_xs().text_color(p.text_faint).child(words::more_items(entry.more)))
                        })
                    })
                    .into_any_element()
            })
            .collect()
    }
}

impl Render for HistoryPanel {
    /// The file operations run since Colet opened, newest first. Colet lays it over the window, deferred,
    /// only while it is open.
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Only the stops of the rows and items drawn below stay; the mouse leaves the keys where they are.
        // Frame bookkeeping, as Colet's own (see `Colet::begin_frame`): it never notifies.
        self.stops.begin_frame(window);
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let entries = self.render_entries(window, cx);
        let clearable = self.operations.read(cx).history.entries.iter().any(|e| e.status != Status::Running);
        div()
            .id("history")
            .absolute()
            .right_3()
            .bottom_8()
            .w(scaled(440.))
            .max_h(scaled(520.))
            .flex()
            .flex_col()
            .bg(p.raised)
            .border_1()
            .border_color(p.border_strong)
            .rounded(scaled(radii.lg))
            .shadow_lg()
            .occlude()
            .on_mouse_down_out(cx.listener(|_, _, _, cx| cx.emit(HistoryEvent::Dismiss)))
            .child(
                div()
                    .px_3()
                    .h_10()
                    .flex()
                    .items_center()
                    .border_b_1()
                    .border_color(p.border)
                    .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child(words::history::TITLE))
                    .child(
                        keep_focus(push("clear-history", words::history::CLEAR, false, clearable).ghost().small()).on_click(
                            cx.listener(|this, _, _, cx| this.operations.update(cx, |operations, cx| operations.clear_history(cx))),
                        ),
                    ),
            )
            .child(if entries.is_empty() {
                div().p_4().text_xs().text_color(p.text_muted).child(words::history::EMPTY).into_any_element()
            } else {
                div().id("history-list").flex_1().overflow_y_scroll().children(entries).into_any_element()
            })
    }
}

/// `HH:MM`, local.
pub(crate) fn clock(unix_ms: u64) -> String {
    colet_core::time::local(unix_ms).map_or_else(String::new, |t| format!("{:02}:{:02}", t.wHour, t.wMinute))
}

/// The item at `k` of the step `entry` in the history.
pub(crate) fn history_item_id(entry: u64, k: usize) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(("history-item", entry).into()), SharedString::from(k.to_string()))
}

impl Colet {
    /// The history's button in the status bar: turning while an operation runs, with the count of those
    /// with problems not seen yet.
    pub fn history_button(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = cx.palette();
        let operations = self.operations.read(cx);
        let (unseen, running) = (operations.history.unseen, operations.is_running());
        let label = if unseen > 0 { words::history::title_with_problems(unseen) } else { words::history::TITLE.to_owned() };
        let button = Button::new("history-button")
            .ghost()
            .xsmall()
            .map(|b| if running { b.icon(Spinner::new()) } else { b.icon(crate::view::glyph("clock")) })
            .accessibility_label(label.clone())
            // Shown only while the history is shut: open, the tip would lie over it.
            .when(!self.overlays.is_history_open(), |b| b.tooltip(label))
            .when(unseen > 0, |d| {
                d.child(
                    div()
                        .min_w_4()
                        .h_4()
                        .px(scaled(3.))
                        .rounded(cx.theme().radius_full())
                        // White on the deep red: past 4.5:1 in either theme.
                        .bg(p.danger_fill)
                        .text_color(p.on_accent)
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(unseen.to_string()),
                )
            })
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                if this.overlays.is_history_open() {
                    this.show_history(false, None, window, cx);
                } else if !this.pressed_shut(event) {
                    this.show_history(true, None, window, cx);
                }
            }));
        // The button takes no listener for the press, so the box round it notes it.
        let button = self.ringed(button, "history-button", RingAt::Edge, window, cx);
        Self::opener(div().flex(), |this| this.overlays.is_history_open(), cx).child(button).into_any_element()
    }
}

impl Colet {
    /// Opened, the history has been seen, with `entry` shown opened; closed, the keys go back to where they
    /// were as it opened.
    pub fn show_history(&mut self, open: bool, entry: Option<u64>, window: &mut Window, cx: &mut Context<Self>) {
        let was_open = self.overlays.is_history_open();
        if open && !was_open {
            self.remember_keys(window, cx);
        }
        self.overlays.set_history_open(open);
        if open {
            self.history.update(cx, |history, cx| history.opened(entry, cx));
            self.operations.update(cx, |operations, cx| operations.mark_history_seen(cx));
        } else {
            self.history.update(cx, |history, _| history.closed());
        }
        if !open && was_open {
            self.restore_keys(window, cx);
        }
        cx.notify();
    }

    /// What the history asks for. An item pressed in it is answered for only while it stays open, so one
    /// still there closes it and opens in the columns.
    pub(crate) fn on_history_event(&mut self, event: &HistoryEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            HistoryEvent::Dismiss => self.show_history(false, None, window, cx),
            HistoryEvent::Gone(path) => self.notify(words::history::gone(shown(folder_name(path))), Tone::Info, cx),
            HistoryEvent::Reveal { path, folder } => {
                self.show_history(false, None, window, cx);
                self.state.go_to_path(path, *folder);
                self.sync_search_field(window, cx);
                self.focus_active(window, cx);
            }
        }
    }
}
