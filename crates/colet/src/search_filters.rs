//! The search filters: Everything queries a search is narrowed by, one at a time, chosen from the menu
//! beside the search field and kept in a dialog of their own.
use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use gpui_kit::component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::prelude::FluentBuilder;

use crate::{
    app::Colet,
    dialogs::{Dialog, DialogParts, field, label, push},
    menus::{Line, Popup, choice, line},
    metrics::{self, scaled},
    settings::SearchFilter,
    state::HomeKey,
    stops::{FilterTool, Stop},
    theme::ActivePalette,
    view::RingAt,
};

/// A filter being added, or edited, found by the name it had as the edit began.
pub struct FilterEdit {
    pub original: Option<String>,
    pub name: Entity<InputState>,
    pub query: Entity<InputState>,
}

impl Colet {
    /// The filter a tab opened afresh starts with.
    pub fn default_filter(&self) -> Option<String> {
        self.stored.settings().default_filter.clone()
    }

    /// Makes the filter named `name` the default, or, the default already, leaves none the default. The
    /// tabs open keep the filter each has.
    pub fn toggle_default_filter(&mut self, name: &str, cx: &mut Context<Self>) {
        let to = (self.stored.settings().default_filter.as_deref() != Some(name)).then(|| name.to_owned());
        self.update_settings(|s| s.default_filter = to, cx);
        cx.notify();
    }

    pub fn open_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.take_menu();
        self.back_to_filters(window, cx);
    }

    /// The list of filters, back from an edit or a deletion asked about.
    pub fn back_to_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        self.overlays.set_dialog(Dialog::Filters { edit: None, deleting: None });
        self.overlays.dialog_focus().focus(window, cx);
        cx.notify();
    }

    /// Esc: from an edit or a deletion back to the list, otherwise out of the dialog.
    pub fn escape_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.overlays.dialog() {
            Some(Dialog::Filters { edit, deleting }) if edit.is_some() || deleting.is_some() => self.back_to_filters(window, cx),
            _ => self.close_dialog(window, cx),
        }
    }

    /// Edits the filter named `original`, or adds one.
    pub fn edit_filter(&mut self, original: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let found = original
            .as_deref()
            .and_then(|name| self.stored.settings().filters.iter().find(|f| f.name == name))
            .cloned()
            .unwrap_or_default();
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("画像").default_value(found.name));
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("ext:jpg;png;gif").default_value(found.query));
        self.begin_dialog(window, cx);
        for input in [&name, &query] {
            let subscription = cx.subscribe_in(input, window, |this: &mut Self, _, event, window, cx| match event {
                InputEvent::PressEnter { .. } => this.submit_filter(window, cx),
                InputEvent::Change | InputEvent::Focus | InputEvent::Blur => cx.notify(),
            });
            self.overlays.listen_in_dialog(subscription);
            cx.palette().style_input(input, cx);
        }
        name.update(cx, |input, cx| input.focus(window, cx));
        self.overlays.set_dialog(Dialog::Filters { edit: Some(FilterEdit { original, name, query }), deleting: None });
        cx.notify();
    }

    /// Why the edited filter cannot be kept as it stands, if it cannot; empty when no word is needed.
    pub fn filter_problem(&self, cx: &App) -> Option<&'static str> {
        let Some(Dialog::Filters { edit: Some(edit), .. }) = self.overlays.dialog() else { return None };
        let name = edit.name.read(cx).value().trim().to_owned();
        if name.is_empty() || edit.query.read(cx).value().trim().is_empty() {
            Some("")
        } else if edit.original.as_deref() != Some(name.as_str()) && self.stored.settings().filter_query(&name).is_some() {
            Some("この名前のフィルターは既にあります")
        } else {
            None
        }
    }

    /// Keeps the filter edited. Renamed, what names it follows: the saved searches, and in every tab the
    /// filter chosen and the way back and forth. The results within it ask again for its new query.
    pub fn submit_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter_problem(cx).is_some() {
            return;
        }
        let Some(Dialog::Filters { edit: Some(edit), .. }) = self.overlays.dialog() else { return };
        let original = edit.original.clone();
        let name = edit.name.read(cx).value().trim().to_owned();
        let query = edit.query.read(cx).value().trim().to_owned();
        let before = original.as_deref().and_then(|from| self.stored.settings().filter_query(from)).map(str::to_owned);
        let edited = SearchFilter { name: name.clone(), query: query.clone() };
        let to = name.clone();
        self.update_settings(
            |s| match &original {
                None => s.filters.push(edited),
                Some(from) => {
                    if let Some(filter) = s.filters.iter_mut().find(|f| &f.name == from) {
                        *filter = edited;
                    }
                    let follow = |named: &mut Option<String>| {
                        if named.as_ref() == Some(from) {
                            *named = Some(to.clone());
                        }
                    };
                    for saved in &mut s.saved_searches {
                        follow(&mut saved.filter);
                    }
                    follow(&mut s.default_filter);
                    // A pinned search keeps its pin under the name.
                    for pin in &mut s.pinned {
                        if let HomeKey::Search { query, filter: Some(filter), narrow, order } = HomeKey::of(pin)
                            && filter == from
                        {
                            *pin = HomeKey::search(query, Some(&to), narrow, order).key();
                        }
                    }
                }
            },
            cx,
        );
        if let Some(from) = &original {
            if *from != name {
                self.rename_filter_in_tabs(from, Some(&name), cx);
            }
            if before.as_deref() != Some(query.as_str()) && self.state.search_filter.as_deref() == Some(name.as_str()) {
                self.state.reset_search();
            }
            self.changed(cx);
        }
        self.back_to_filters(window, cx);
    }

    /// Asks before the filter named `name` goes.
    pub fn ask_delete_filter(&mut self, name: String, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        self.overlays.set_dialog(Dialog::Filters { edit: None, deleting: Some(name) });
        self.overlays.dialog_focus().focus(window, cx);
        cx.notify();
    }

    /// How many saved searches run within the filter named `name`.
    pub fn filter_users(&self, name: &str) -> usize {
        self.stored.settings().saved_searches.iter().filter(|s| s.filter.as_deref() == Some(name)).count()
    }

    /// Deletes the filter asked about: what named it goes without it.
    pub fn delete_filter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Filters { deleting: Some(name), .. }) = self.overlays.dialog() else { return };
        let name = name.clone();
        self.update_settings(
            |s| {
                s.filters.retain(|f| f.name != name);
                if s.default_filter.as_ref() == Some(&name) {
                    s.default_filter = None;
                }
            },
            cx,
        );
        self.rename_filter_in_tabs(&name, None, cx);
        self.changed(cx);
        self.back_to_filters(window, cx);
    }

    /// Moves the filter named `name` up or down the menu by one.
    pub fn move_filter(&mut self, name: &str, by: isize, cx: &mut Context<Self>) {
        let Some(at) = self.stored.settings().filters.iter().position(|f| f.name == name) else { return };
        let to = at as isize + by;
        if to < 0 || to as usize >= self.stored.settings().filters.len() {
            return;
        }
        self.update_settings(|s| s.filters.swap(at, to as usize), cx);
    }
}

/// A small button with a glyph, for each filter in the list.
fn tool(id: ElementId, glyph: &'static str, label: String, enabled: bool) -> Button {
    Button::new(id).ghost().small().icon(crate::view::glyph(glyph)).accessibility_label(label.clone()).tooltip(label).disabled(!enabled)
}

/// The tool `tool` on the search filter `name`: named after the filter, not its place, so it goes with the
/// filter as the list is put in another order, as its stop does.
pub(crate) fn filter_tool_id(tool: &'static str, name: &str) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(tool.into()), SharedString::from(name.to_owned()))
}

impl Colet {
    /// A search filter added or edited.
    pub(crate) fn filter_edit_dialog(&self, edit: &FilterEdit, window: &Window, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        let radii = metrics::Radii::of(window, cx);
        let FilterEdit { name, query, .. } = edit;
        let problem = self.filter_problem(cx);
        let content =
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(label(p, "名前"))
                        .child(field(p, name, self.has_keys(name, window, cx), radii.md, Self::right_press_menu(name, cx)))
                        .when_some(problem.filter(|p| !p.is_empty()), |d, problem| {
                            d.child(div().text_size(scaled(metrics::TEXT_BODY)).text_color(p.danger).child(problem))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(label(p, "検索文字列"))
                        .child(field(p, query, self.has_keys(query, window, cx), radii.md, Self::right_press_menu(query, cx)))
                        .child(div().text_size(scaled(metrics::TEXT_BODY)).text_color(p.text_faint).child(
                            "Everythingの検索式で書きます（例：ext:jpg;png、dm:thisweek）。検索欄に打った語と両方に合うものを探します",
                        )),
                );
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.back_to_filters(window, cx))),
            ),
            (
                "save-filter",
                push("save-filter", "保存", true, problem.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.submit_filter(window, cx))),
            ),
        ];
        (if edit.original.is_none() { "フィルターを追加" } else { "フィルターを編集" }, content, buttons)
    }

    /// Whether the search filter `name` is to go.
    pub(crate) fn filter_delete_dialog(&self, name: &str, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        let users = self.filter_users(name);
        let content =
            div().flex().flex_col().gap_2().text_size(scaled(metrics::TEXT_LARGE)).child(format!("「{name}」を削除しますか？")).when(
                users > 0,
                |d| {
                    d.child(div().text_size(scaled(metrics::TEXT_BODY)).text_color(p.text_muted).child(format!(
                        "このフィルターを使う保存した検索が{users}件あります。削除すると、それらはフィルターなしで検索します。"
                    )))
                },
            );
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.back_to_filters(window, cx))),
            ),
            (
                "delete-filter",
                // Destructive: drawn as danger, not as the primary commit.
                push("delete-filter", "削除", false, true)
                    .danger()
                    .on_click(cx.listener(|this, _, window, cx| this.delete_filter(window, cx))),
            ),
        ];
        ("フィルターを削除", content, buttons)
    }

    /// The search filters, in their order.
    pub(crate) fn filters_dialog(&self, window: &Window, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        let filters = self.stored.settings().filters.clone();
        let default = self.default_filter();
        let last = filters.len().saturating_sub(1);
        let rows: Vec<AnyElement> = filters
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let name = f.name.clone();
                let (up_name, down_name, edit_name, default_name) = (name.clone(), name.clone(), name.clone(), name.clone());
                let is_default = default.as_deref() == Some(f.name.as_str());
                let (default_glyph, default_label) = if is_default {
                    ("circle-check", format!("「{}」を既定から外す", f.name))
                } else {
                    ("circle", format!("「{}」を既定にする", f.name))
                };
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .py_1()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_baseline()
                                    .gap_2()
                                    .min_w_0()
                                    .child(div().truncate().text_size(scaled(metrics::TEXT_LARGE)).child(f.name.clone()))
                                    .when(is_default, |d| {
                                        d.child(
                                            div().flex_none().text_size(scaled(metrics::TEXT_BODY)).text_color(p.text_muted).child("既定"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(scaled(metrics::TEXT_BODY))
                                    .text_color(p.text_muted)
                                    .font_family(metrics::FONT_CODE)
                                    .child(f.query.clone()),
                            ),
                    )
                    // Their stops named after the filter, to go with it.
                    .child(
                        self.ringed(
                            tool(filter_tool_id("filter-default", &f.name), default_glyph, default_label, true)
                                .on_click(cx.listener(move |this, _, _, cx| this.toggle_default_filter(&default_name, cx))),
                            Stop::Filter(FilterTool::Default, f.name.clone()),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(filter_tool_id("filter-up", &f.name), "arrow-up", format!("「{}」を上へ", f.name), i > 0)
                                .when(i > 0, |d| d.on_click(cx.listener(move |this, _, _, cx| this.move_filter(&up_name, -1, cx)))),
                            Stop::Filter(FilterTool::Up, f.name.clone()),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(filter_tool_id("filter-down", &f.name), "arrow-down", format!("「{}」を下へ", f.name), i < last)
                                .when(i < last, |d| d.on_click(cx.listener(move |this, _, _, cx| this.move_filter(&down_name, 1, cx)))),
                            Stop::Filter(FilterTool::Down, f.name.clone()),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(filter_tool_id("filter-edit", &f.name), "pencil-line", format!("「{}」を編集", f.name), true)
                                .on_click(cx.listener(move |this, _, window, cx| this.edit_filter(Some(edit_name.clone()), window, cx))),
                            Stop::Filter(FilterTool::Edit, f.name.clone()),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(filter_tool_id("filter-delete", &f.name), "trash", format!("「{}」を削除", f.name), true)
                                .on_click(cx.listener(move |this, _, window, cx| this.ask_delete_filter(name.clone(), window, cx))),
                            Stop::Filter(FilterTool::Delete, f.name.clone()),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .into_any_element()
            })
            .collect();
        let content = div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(scaled(metrics::TEXT_BODY))
                    .text_color(p.text_muted)
                    .child("検索に加えるEverythingの検索式を登録し、検索欄の左のボタン（Ctrl+Shift+F）で1つ選んで使います。"),
            )
            .child(if rows.is_empty() {
                div()
                    .py_2()
                    .text_size(scaled(metrics::TEXT_BODY))
                    .text_color(p.text_faint)
                    .child("フィルターはまだありません")
                    .into_any_element()
            } else {
                div().flex().flex_col().children(rows).into_any_element()
            });
        let buttons = vec![
            (
                "add-filter",
                push("add-filter", "追加", false, true).on_click(cx.listener(|this, _, window, cx| this.edit_filter(None, window, cx))),
            ),
            (
                "close-filters",
                push("close-filters", "閉じる", true, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
        ];
        ("検索フィルター", content, buttons)
    }
}

/// What a line of the search filters' menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum FilterCommand {
    /// The search filter to search within, or none.
    Choose(Option<String>),
    /// The dialog of the filters opens.
    Edit,
}

impl Colet {
    pub fn filter_open(&self) -> bool {
        matches!(self.overlays.popup(), Some(Popup::Filters { .. }))
    }

    /// Opens the search filters' menu.
    pub fn open_filter_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let from_search = match self.overlays.popup() {
            Some(&Popup::Filters { from_search }) => from_search,
            _ => self.search_has_keys(window, cx),
        };
        self.overlays.set_popup(Popup::Filters { from_search });
        self.show_menu(window, cx);
    }

    /// The search filters' menu: none, each filter, the default marked, and editing them.
    pub(crate) fn filter_lines(&self, cx: &App) -> Vec<Line> {
        let chosen = self.chosen_filter(cx);
        let chosen = chosen.as_deref();
        let mut lines = vec![choice("filter-none", "なし", FilterCommand::Choose(None), chosen.is_none())];
        let default = self.stored.settings().default_filter.as_deref();
        lines.extend(self.stored.settings().filters.iter().map(|f| Line {
            hint: (default == Some(f.name.as_str())).then_some("既定"),
            ..choice(
                filter_tool_id("filter", &f.name),
                f.name.clone(),
                FilterCommand::Choose(Some(f.name.clone())),
                chosen == Some(f.name.as_str()),
            )
        }));
        lines.push(line("edit-filters", "pencil-line", "フィルターを編集…", None, None, FilterCommand::Edit).grouped());
        lines
    }

    /// Does what the line picked in the search filters' menu says.
    pub(crate) fn run_filter_command(&mut self, command: FilterCommand, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            FilterCommand::Choose(name) => self.choose_filter(name, cx),
            FilterCommand::Edit => self.open_filters(window, cx),
        }
    }
}
