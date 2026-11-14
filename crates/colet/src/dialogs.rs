//! What opens over the window: the dialogs, the file menu, and the history of file operations.
use colet_core::{
    entry::{Order, Sort, direction_label},
    operations::{Kind as Operation, Request},
    paths::{ancestors, folder_name, shown},
};
use std::{path::PathBuf, rc::Rc};

use gpui_kit::base::input::{Input, InputState};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    dialog::Dialog as DialogFrame,
    spinner::Spinner,
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::Colet,
    menus::Popup,
    metrics,
    ops::{ItemStatus, Menu, Status},
    places::Dialog,
    search_filters::FilterEdit,
    settings::{Favorite, Theme},
    state::Kind,
    stops::{FilterTool, OrderChoice, Stop},
    theme::Palette,
    view::{RING_ROOM, RingAt},
};

/// The keys the preferences list, under a heading for each kind of work.
const SHORTCUTS: [(&str, &[(&str, &str)]); 6] = [
    (
        "移動",
        &[
            ("← →", "カラムを移動"),
            ("PageUp / PageDown", "1 画面ずつ移動"),
            ("Home / End", "先頭 / 末尾へ移動"),
            ("Enter", "開く"),
            ("Alt+← / Alt+→", "戻る / 進む"),
            ("マウスの戻る / 進むボタン", "戻る / 進む"),
            ("Ctrl+0", "ホームへ移動"),
            ("Ctrl+L", "パスを入力して移動（Alt+Enter で新しいタブに開く）"),
        ],
    ),
    (
        "選択",
        &[
            ("↑ ↓", "項目を選択（Shift で範囲選択）"),
            ("Ctrl+クリック / Shift+クリック", "複数選択"),
            ("Ctrl+↑↓ / Ctrl+Space", "離れた項目を選択"),
            ("Ctrl+Shift+↑↓ / Ctrl+Shift+クリック", "範囲を選択に足す"),
            ("Ctrl+A", "読み込み済み項目を全選択"),
        ],
    ),
    (
        "検索と絞り込み",
        &[
            ("Ctrl+F", "検索"),
            ("↓ / Enter", "検索欄から結果へ移動"),
            ("Esc", "検索をクリア"),
            ("Ctrl+Shift+F", "検索フィルターを選ぶ"),
            ("Ctrl+D", "検索を保存 / フォルダーをお気に入りに追加"),
            ("文字キー", "カラムを名前で絞り込む（Backspace で 1 文字消す、Esc で解除）"),
        ],
    ),
    (
        "タブ",
        &[
            ("Ctrl+T", "新しいタブ"),
            ("Ctrl+Enter", "新しいタブで開く"),
            ("Ctrl+W", "タブを閉じる"),
            ("Ctrl+Tab / Ctrl+Shift+Tab", "次 / 前のタブ"),
            ("Ctrl+Shift+T", "閉じたタブを開き直す"),
        ],
    ),
    (
        "ファイル操作",
        &[
            ("Ctrl+C / X / V", "コピー / 切り取り / 貼り付け"),
            ("Ctrl+Shift+C", "パスをコピー"),
            ("Ctrl+Shift+N", "新規フォルダー"),
            ("F2", "名前を変更（保存した検索は編集）"),
            ("Delete", "ごみ箱へ移動"),
            ("Shift+Delete", "完全に削除"),
            ("Alt+Enter", "プロパティ"),
            ("Shift+F10 / メニューキー", "メニューを開く"),
        ],
    ),
    (
        "プレビューと表示",
        &[("Space", "QuickLook でプレビュー"), ("Ctrl+H", "隠しファイルの表示を切り替え"), ("Ctrl+U", "上の階層を表示"), ("F5", "更新")],
    ),
];

/// How wide the keys' side of the list is: room for the longest, "Ctrl+Shift+↑↓ / Ctrl+Shift+クリック".
const KEYS_WIDTH: f32 = 240.;

/// `HH:MM`, local.
fn clock(unix_ms: u64) -> String {
    colet_core::time::local(unix_ms).map_or_else(String::new, |t| format!("{:02}:{:02}", t.wHour, t.wMinute))
}

/// A field, ringed while it has the keys (`focused`), or its menu has them for it; `menu` opens that.
fn field(
    p: Palette,
    input: &Entity<gpui_kit::base::input::InputState>,
    focused: bool,
    menu: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    // The ring, two pixels in the focus color a pixel off the field.
    crate::view::frame(p, focused, metrics::FIELD_RADIUS).child(
        div()
            .h(px(metrics::CONTROL_HEIGHT))
            .px(px(10.))
            .flex()
            .items_center()
            .bg(p.surface)
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(metrics::FIELD_RADIUS))
            // Its padding too, round the text.
            .capture_any_mouse_down(menu)
            .child(div().flex_1().min_w_0().child(Input::new(input))),
    )
}

/// A dialog's buttons, each with the name of the stop Tab reaches it by.
type Buttons = Vec<(&'static str, Button)>;

fn label(p: Palette, text: &'static str) -> Div {
    div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_muted).child(text)
}

/// A dialog's button: the primary one is what Enter does.
fn push(id: impl Into<ElementId>, text: impl Into<SharedString>, primary: bool, enabled: bool) -> Button {
    Button::new(id).label(text).when(primary, |b| b.primary()).disabled(!enabled)
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

/// The item at `k` of the step `entry` in the history.
pub(crate) fn history_item_id(entry: u64, k: usize) -> ElementId {
    ElementId::NamedChild(std::sync::Arc::new(("history-item", entry).into()), SharedString::from(k.to_string()))
}

/// What picking a choice of a [`Colet::radio_group`] does.
type Pick = Rc<dyn Fn(&mut Colet, &mut Window, &mut Context<Colet>)>;

/// A choice of a [`Colet::radio_group`].
struct Choice {
    id: ElementId,
    text: SharedString,
    stop: Stop,
    chosen: bool,
    pick: Pick,
}

impl Choice {
    fn new(id: impl Into<ElementId>, text: impl Into<SharedString>, stop: Stop, chosen: bool, pick: Pick) -> Self {
        Choice { id: id.into(), text: text.into(), stop, chosen, pick }
    }
}

/// How far an arrow moves among the choices of a radio group: back or on, whichever way they run.
fn arrow_step(keystroke: &Keystroke) -> Option<isize> {
    if keystroke.modifiers.modified() {
        return None;
    }
    match keystroke.key.as_str() {
        "left" | "up" => Some(-1),
        "right" | "down" => Some(1),
        _ => None,
    }
}

impl Colet {
    /// `choices` as a radio group named `label`: the chosen one drawn selected, as a pressed button is,
    /// not as the primary button, which is what Enter does in the dialog. Tab stops at the chosen one only,
    /// or the first while none is, and the arrows move among them, picking as they go, as in a radio group.
    fn radio_group(
        &self,
        id: &'static str,
        label: &'static str,
        choices: Vec<Choice>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let tab_at = choices.iter().position(|c| c.chosen).unwrap_or(0);
        let stops: Rc<[Stop]> = choices.iter().map(|c| c.stop.clone()).collect();
        let picks: Rc<[Pick]> = choices.iter().map(|c| c.pick.clone()).collect();
        let buttons: Vec<Div> = choices
            .into_iter()
            .enumerate()
            .map(|(k, Choice { id, text, stop, chosen, pick })| {
                let (ring_stop, in_group) = (stop.clone(), stops.clone());
                // Told to the button, not to the stop's handle: the button writes its own onto the handle it
                // is given as it is drawn.
                let button =
                    Button::new(id).label(text).selected(chosen).role(Role::RadioButton).toggled(chosen).tab_stop(k == tab_at).on_click(
                        cx.listener(move |this, _, window, cx| {
                            // The mouse leaves the keys where they are, but within the group they go with the
                            // pick, so the ring does not stay on the last.
                            if in_group.iter().any(|s| this.stop(s.clone(), cx).is_focused(window)) {
                                this.stop(stop.clone(), cx).focus(window, cx);
                            }
                            pick(this, window, cx);
                        }),
                    );
                self.ringed(button, ring_stop, RingAt::Round, window, cx)
            })
            .collect();
        div()
            .id(id)
            .flex()
            .flex_wrap()
            .gap(px(2.))
            .role(Role::RadioGroup)
            .aria_label(label)
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let Some(by) = arrow_step(&event.keystroke) else { return };
                let Some(at) = stops.iter().position(|s| this.stop(s.clone(), cx).is_focused(window)) else { return };
                let to = (at as isize + by).rem_euclid(stops.len() as isize) as usize;
                cx.stop_propagation();
                this.stop(stops[to].clone(), cx).focus(window, cx);
                (picks[to])(this, window, cx);
            }))
            // Ringed as the fields are; the rings' room makes up most of the gaps.
            .children(buttons)
    }
    /// The field for what a favorite or saved search opens narrowed to, and why it cannot be kept, if it
    /// cannot.
    fn narrow_field(&self, input: &Entity<InputState>, problem: Option<&'static str>, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = self.palette;
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(label(p, "絞り込み"))
            .child(field(p, input, self.has_keys(input, window, cx), Self::right_press_menu(input, cx)))
            .child(match problem {
                Some(problem) => div().text_size(px(metrics::TEXT_BODY)).text_color(p.danger).child(problem),
                None => div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_faint).child("開いたとき、この文字で名前を絞り込みます"),
            })
    }

    /// The order a saved search lists its results in: what it sorts by, the way it runs, and whether folders
    /// come first, each a row of choices as the sort order's menu has them.
    fn order_field(&self, order: Order, window: &Window, cx: &mut Context<Self>) -> Div {
        let p = self.palette;
        let mut row = |id: &'static str, label: &'static str, choices: Vec<(OrderChoice, &'static str)>| {
            let choices = choices
                .into_iter()
                .map(|(choice, text)| {
                    let pick: Pick = Rc::new(move |this, _, cx| this.change_edit_order(|order| choice.apply(order), cx));
                    let item = ElementId::NamedChild(std::sync::Arc::new(id.into()), SharedString::from(text));
                    Choice::new(item, text, Stop::EditSearchOrder(choice), choice.is_in(order), pick)
                })
                .collect();
            self.radio_group(id, label, choices, window, cx)
        };
        let by = Sort::ALL.into_iter().map(|by| (OrderChoice::By(by), by.label())).collect();
        let direction = [false, true].into_iter().map(|d| (OrderChoice::Descending(d), direction_label(d))).collect();
        let folders = vec![(OrderChoice::FoldersFirst(true), "フォルダを先に"), (OrderChoice::FoldersFirst(false), "混在")];
        let (by, direction, folders) = (
            row("edit-search-sort", "並べ替え", by),
            row("edit-search-direction", "向き", direction),
            row("edit-search-folders", "フォルダ", folders),
        );
        div().flex().flex_col().gap_1().child(label(p, "並び順")).child(by).child(direction).child(folders)
    }

    fn dialog_parts(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<(&'static str, Div, Buttons)> {
        let title = match self.overlays.dialog()? {
            Dialog::Name { request: Request { kind: Operation::NewFolder, .. }, .. } => "新規フォルダー",
            Dialog::Name { .. } => "名前を変更",
            Dialog::EditSearch { .. } => "保存した検索を編集",
            Dialog::EditFavorite { .. } => "お気に入りを編集",
            Dialog::Filters { edit: Some(FilterEdit { original: None, .. }), .. } => "フィルターを追加",
            Dialog::Filters { edit: Some(_), .. } => "フィルターを編集",
            Dialog::Filters { deleting: Some(_), .. } => "フィルターを削除",
            Dialog::Filters { .. } => "検索フィルター",
            Dialog::Preferences => "設定",
            Dialog::Open { .. } => "開く",
        };
        // Each drawn by a function of its own: in a debug build one function drawing them all takes the room
        // of every one of them at once, past what the window's thread has.
        let (content, buttons) = match self.overlays.dialog()? {
            Dialog::Name { request, input, checking, .. } => self.name_dialog(request, input, checking.is_some(), window, cx),
            Dialog::EditFavorite { original, name, narrow } => self.favorite_dialog(original, name, narrow, window, cx),
            Dialog::EditSearch { name, query, filter, narrow, order, .. } => {
                self.search_dialog(name, query, filter, narrow, *order, window, cx)
            }
            Dialog::Filters { edit: Some(edit), .. } => self.filter_edit_dialog(edit, window, cx),
            Dialog::Filters { deleting: Some(name), .. } => self.filter_delete_dialog(name, cx),
            Dialog::Filters { .. } => self.filters_dialog(window, cx),
            Dialog::Preferences => self.preferences_dialog(window, cx),
            Dialog::Open { paths } => {
                let count = paths.len();
                self.open_dialog(count, cx)
            }
        };
        Some((title, content, buttons))
    }

    /// A new folder's name, or a rename's.
    fn name_dialog(
        &self,
        request: &Request,
        input: &Entity<InputState>,
        checking: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Div, Buttons) {
        let p = self.palette;
        let Request { kind, paths, destination, .. } = request;
        let problem = self.name_problem(cx);
        let empty = input.read(cx).value().trim().is_empty();
        // Where the name goes: a rename may come from the results, far from its folder.
        let (place, submit) = match kind {
            Operation::NewFolder => (format!("作成先：{}", shown(destination)), "作成"),
            _ => (
                format!("場所：{}", paths.first().and_then(|p| ancestors(p).into_iter().rev().nth(1)).map(shown).unwrap_or_default()),
                "保存",
            ),
        };
        let content =
            div().flex().flex_col().gap_3().child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_faint).child(place)).child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "名前"))
                    .child(field(p, input, self.has_keys(input, window, cx), Self::right_press_menu(input, cx)))
                    .when_some(problem, |d, problem| d.child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.danger).child(problem))),
            );
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            // While the folder is asked whether the name is free, the press is in and waits.
            (
                "submit-name",
                push("submit-name", if checking { "確認中…" } else { submit }, true, !empty && problem.is_none() && !checking)
                    .on_click(cx.listener(|this, _, window, cx| this.submit_name(window, cx))),
            ),
        ];
        (content, buttons)
    }

    /// A favorite's name and narrowing.
    fn favorite_dialog(
        &self,
        original: &Favorite,
        name: &Entity<InputState>,
        narrow: &Entity<InputState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Div, Buttons) {
        let p = self.palette;
        let problem = self.favorite_edit_problem(cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_faint).child(format!("フォルダー：{}", shown(&original.path))))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "名前"))
                    .child(field(p, name, self.has_keys(name, window, cx), Self::right_press_menu(name, cx)))
                    .child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_faint).child("空欄ならフォルダー名を表示します")),
            )
            .child(self.narrow_field(narrow, problem, window, cx));
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            (
                "save-favorite",
                push("save-favorite", "保存", true, problem.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.submit_favorite_edit(window, cx))),
            ),
        ];
        (content, buttons)
    }

    /// A saved search's name, query, filter, narrowing and order.
    #[allow(clippy::too_many_arguments)]
    fn search_dialog(
        &self,
        name: &Entity<InputState>,
        query: &Entity<InputState>,
        filter: &Option<String>,
        narrow: &Entity<InputState>,
        order: Order,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Div, Buttons) {
        let p = self.palette;
        // The saved search's choice of filter, none first.
        let choices: Vec<Option<String>> =
            std::iter::once(None).chain(self.stored.settings().filters.iter().map(|f| Some(f.name.clone()))).collect();
        let problem = self.edit_problem(cx);
        let chosen = filter.clone();
        let filters = choices
            .into_iter()
            .map(|choice| {
                let text = choice.clone().unwrap_or_else(|| "なし".into());
                let id = match &choice {
                    Some(name) => filter_tool_id("edit-search-filter", name),
                    None => "edit-search-filter-none".into(),
                };
                let picked = choice.clone();
                let pick: Pick = Rc::new(move |this, _, cx| this.pick_edit_filter(picked.clone(), cx));
                let is_chosen = choice == chosen;
                Choice::new(id, text, Stop::EditSearchFilter(choice), is_chosen, pick)
            })
            .collect();
        let filters = self.radio_group("edit-search-filters", "検索フィルター", filters, window, cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "名前"))
                    .child(field(p, name, self.has_keys(name, window, cx), Self::right_press_menu(name, cx)))
                    .child(
                        div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_faint).child("空欄なら検索文字列をそのまま表示します"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "検索文字列"))
                    .child(field(p, query, self.has_keys(query, window, cx), Self::right_press_menu(query, cx)))
                    .when_some(problem.filter(|p| !p.is_empty()), |d, problem| {
                        d.child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.danger).child(problem))
                    }),
            )
            .child(div().flex().flex_col().gap_1().child(label(p, "検索フィルター")).child(filters))
            .child(self.narrow_field(narrow, None, window, cx))
            .child(self.order_field(order, window, cx));
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            (
                "save-search",
                push("save-search", "保存", true, problem.is_none())
                    .on_click(cx.listener(|this, _, window, cx| this.submit_edit(window, cx))),
            ),
        ];
        (content, buttons)
    }

    /// A search filter added or edited.
    fn filter_edit_dialog(&self, edit: &FilterEdit, window: &Window, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
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
                        .child(field(p, name, self.has_keys(name, window, cx), Self::right_press_menu(name, cx)))
                        .when_some(problem.filter(|p| !p.is_empty()), |d, problem| {
                            d.child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.danger).child(problem))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(label(p, "検索文字列"))
                        .child(field(p, query, self.has_keys(query, window, cx), Self::right_press_menu(query, cx)))
                        .child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_faint).child(
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
        (content, buttons)
    }

    /// Whether the search filter `name` is to go.
    fn filter_delete_dialog(&self, name: &str, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
        let users = self.filter_users(name);
        let content = div().flex().flex_col().gap_2().text_size(px(metrics::TEXT_LARGE)).child(format!("「{name}」を削除しますか？")).when(
            users > 0,
            |d| {
                d.child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_muted).child(format!(
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
        (content, buttons)
    }

    /// The search filters, in their order.
    fn filters_dialog(&self, window: &Window, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
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
                                    .child(div().truncate().text_size(px(metrics::TEXT_LARGE)).child(f.name.clone()))
                                    .when(is_default, |d| {
                                        d.child(div().flex_none().text_size(px(metrics::TEXT_BODY)).text_color(p.text_muted).child("既定"))
                                    }),
                            )
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(metrics::TEXT_BODY))
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
                    .text_size(px(metrics::TEXT_BODY))
                    .text_color(p.text_muted)
                    .child("検索に加えるEverythingの検索式を登録し、検索欄の左のボタン（Ctrl+Shift+F）で1つ選んで使います。"),
            )
            .child(if rows.is_empty() {
                div()
                    .py_2()
                    .text_size(px(metrics::TEXT_BODY))
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
        (content, buttons)
    }

    /// The theme, the display, and the keys.
    fn preferences_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
        let themes = Theme::ALL
            .into_iter()
            .map(|theme| {
                let pick: Pick = Rc::new(move |this, window, cx| this.set_theme(theme, window, cx));
                let chosen = self.stored.settings().theme == theme;
                Choice::new(("theme", theme as usize), theme.label(), Stop::Theme(theme), chosen, pick)
            })
            .collect();
        let themes = self.radio_group("themes", "テーマ", themes, window, cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            // Taken at once as one is picked, and kept.
            .child(div().flex().flex_col().gap_1().child(label(p, "テーマ")).child(themes))
            // Taken at once, as the theme is. It counts only while the hidden items show, but is set at any time.
            .child({
                let colet = cx.entity().downgrade();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "表示"))
                    .child(
                        Checkbox::new("show-protected")
                            .label("保護されたシステムファイルも表示する")
                            .checked(self.stored.settings().show_protected)
                            .on_click(move |_, _, cx| {
                                colet.update(cx, |this, cx| this.toggle_protected(cx)).ok();
                            }),
                    )
                    .child(
                        div()
                            .text_size(px(metrics::TEXT_BODY))
                            .text_color(p.text_muted)
                            .child("隠しファイルを表示しているときに効きます。"),
                    )
            })
            .child(div().pt_1().text_size(px(metrics::TEXT_LARGE)).font_weight(FontWeight::SEMIBOLD).child("キーボード操作"))
            // One by one: an array of them all would lie whole on the stack, past what a debug build has.
            .child(div().flex().flex_col().gap_3().text_size(px(metrics::TEXT_BODY)).children(SHORTCUTS.iter().map(|&(heading, keys)| {
                div().flex().flex_col().gap_1().child(label(p, heading)).children(keys.iter().map(|&(key, what)| {
                    div()
                        .flex()
                        .gap_3()
                        // As wide as the longest key, so none wraps.
                        .child(div().w(px(KEYS_WIDTH)).flex_none().text_color(p.text).child(key))
                        // Wrapped in the room left, rather than cut at the dialog's edge.
                        .child(div().flex_1().min_w_0().text_color(p.text_muted).child(what))
                }))
            })));
        let buttons = vec![(
            "close-preferences",
            push("close-preferences", "閉じる", true, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
        )];
        (content, buttons)
    }

    /// Whether to open `count` files together.
    fn open_dialog(&mut self, count: usize, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
        let content = div()
            .flex()
            .flex_col()
            .gap_2()
            .text_size(px(metrics::TEXT_LARGE))
            .child(format!("{count} 項目を開きますか？"))
            .child(div().text_size(px(metrics::TEXT_BODY)).text_color(p.text_muted).child("それぞれ関連付けられたアプリで開きます。"));
        let buttons = vec![
            (
                "cancel",
                push("cancel", "キャンセル", false, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
            ),
            // It takes the keys as the dialog opens, so Enter goes on as it was pressed to open them; its
            // stop is the one opening the dialog sends them to.
            (
                "confirm-open",
                push("confirm-open", "開く", true, true).on_click(cx.listener(|this, _, window, cx| this.confirm_open(window, cx))),
            ),
        ];
        (content, buttons)
    }

    /// The dialog as the window draws it: the heading and the buttons stay, and what lies between scrolls
    /// when the window is too low. Esc steps back as [`Colet::escape_dialog`] does; the dialog closes only as
    /// [`Overlays::dialog`](crate::overlays::Overlays::dialog) goes.
    pub fn dialog_frame(&mut self, dialog: DialogFrame, window: &mut Window, cx: &mut Context<Self>) -> DialogFrame {
        let Some((title, content, buttons)) = self.dialog_parts(window, cx) else { return dialog };
        // Each ringed round as the fields are, Tab reaching it through a stop of its id; the rings lie over
        // the gaps between them.
        let buttons: Vec<Div> =
            buttons.into_iter().map(|(key, button)| self.ringed(button, key, RingAt::Round, window, cx).m(px(-RING_ROOM))).collect();
        let colet = cx.weak_entity();
        dialog
            .w(px(640.))
            .close_button(false)
            .overlay_closable(false)
            .on_cancel(move |_, window, cx| {
                colet.update(cx, |this, cx| this.escape_dialog(window, cx)).ok();
                false
            })
            // Enter is the fields' and the buttons' own: it goes on to whichever has the keys.
            .on_ok(|_, _, cx| {
                cx.propagate();
                false
            })
            .title(div().text_size(px(metrics::TEXT_TITLE)).font_weight(FontWeight::SEMIBOLD).child(title))
            .child(
                div()
                    .id("dialog-content")
                    .test_support()
                    .aria_label(title)
                    .track_focus(self.overlays.dialog_focus())
                    // The window's dialog lies outside Colet's own elements: the menu key in its fields is heard here.
                    .on_action(cx.listener(|this, _: &crate::app::OpenTextMenu, window, cx| this.open_focused_text_menu(window, cx)))
                    .child(content),
            )
            .footer(div().flex().items_center().justify_end().gap_2().children(buttons))
            // A field's menu, over the dialog it is in.
            .children(self.render_menu(cx))
    }

    /// Opens the file menu on `paths` (`None` for the selection, empty for the folder itself) in the
    /// column at `index`, at `at` in the window. The keys stay where they were, as the selection does.
    pub fn open_menu(&mut self, index: usize, paths: Option<Vec<PathBuf>>, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlays.has_dialog() {
            return;
        }
        let Some(c) = self.state.columns.get(index) else { return };
        // Home has nothing to offer but on its rows.
        if c.kind == Kind::Home && paths.as_ref().map_or_else(|| c.shown_selected().is_empty(), Vec::is_empty) {
            return;
        }
        let column = c.id;
        self.state.set_active(index);
        self.column_focus(index).focus(window, cx);
        let scale = window.scale_factor();
        let mut point = windows::Win32::Foundation::POINT { x: (f32::from(at.x) * scale) as i32, y: (f32::from(at.y) * scale) as i32 };
        let _ =
            unsafe { windows::Win32::Graphics::Gdi::ClientToScreen(windows::Win32::Foundation::HWND(self.owner as *mut _), &mut point) };
        self.overlays.set_popup(Popup::File(Menu { column, paths, at, screen: (point.x, point.y) }));
        self.show_menu(window, cx);
        // Read afresh for each menu, so one added since is there.
        self.operations.update(cx, |operations, cx| operations.load_send_targets(cx));
    }

    /// The file menu, or a text field's, where it was opened; over a dialog too, for its fields'. GPUI
    /// paints every deferred draw in the order of its priority, nested or not, so the menu takes one over
    /// the dialog's (ten and up) to be drawn in front of it as it is clicked in front of it.
    pub fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (at, id, label, width) = match self.overlays.popup()? {
            Popup::File(menu) => (menu.at, "menu", "ファイル操作", 240.),
            Popup::Text(menu) => (menu.at, "text-menu", "編集", 200.),
            Popup::Tab { at, .. } => (*at, "tab-menu", "タブ", 220.),
            _ => return None,
        };
        let menu = self.menu_element(id, label, cx)?;
        Some(
            deferred(
                anchored().position(at).snap_to_window_with_margin(px(crate::view::MENU_MARGIN)).child(div().min_w(px(width)).child(menu)),
            )
            .with_priority(gpui_kit::base::POPUP_PRIORITY)
            .into_any_element(),
        )
    }

    /// The file actions under the toolbar.
    pub fn render_actions(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let reasons = self.reasons(cx);
        let action = |id: &'static str, glyph: &'static str, label: &'static str, key: &'static str, off: Option<&'static str>| {
            let tip = match off {
                Some(why) => format!("{label}（{why}）"),
                None => format!("{label}（{key}）"),
            };
            Button::new(id).ghost().icon(crate::view::glyph(glyph)).accessibility_label(label).tooltip(tip).disabled(off.is_some())
        };
        let row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            // Those done on the spot leave the keys where they were; the ones asking in a dialog take them.
            .child(self.ringed(
                action("copy", "copy", "コピー", "Ctrl+C", reasons.copy).on_click(cx.listener(|this, _, _, cx| this.copy(false, cx))),
                "copy",
                RingAt::Edge,
                window,
                cx,
            ))
            .child(self.ringed(
                action("cut", "scissors", "切り取り", "Ctrl+X", reasons.cut).on_click(cx.listener(|this, _, _, cx| this.copy(true, cx))),
                "cut",
                RingAt::Edge,
                window,
                cx,
            ))
            .child(
                self.ringed(
                    action("paste", "clipboard-paste", "貼り付け", "Ctrl+V", reasons.paste)
                        .on_click(cx.listener(|this, _, _, cx| this.paste(cx))),
                    "paste",
                    RingAt::Edge,
                    window,
                    cx,
                ),
            )
            .child(
                self.ringed(
                    action("copy-path", "link", "パスをコピー", "Ctrl+Shift+C", reasons.copy_path)
                        .on_click(cx.listener(|this, _, _, cx| this.copy_paths(cx))),
                    "copy-path",
                    RingAt::Edge,
                    window,
                    cx,
                ),
            )
            .child(div().mx(px(6.)).w(px(1.)).h(px(16.)).bg(p.border))
            .child(
                self.ringed(
                    action("rename", "pencil-line", "名前を変更", "F2", reasons.rename)
                        .on_click(cx.listener(|this, _, window, cx| this.ask_rename(window, cx))),
                    "rename",
                    RingAt::Edge,
                    window,
                    cx,
                ),
            )
            .child(
                self.ringed(
                    action("new-folder", "folder-plus", "新規フォルダー", "Ctrl+Shift+N", reasons.paste)
                        .on_click(cx.listener(|this, _, window, cx| this.ask_new_folder(window, cx))),
                    "new-folder",
                    RingAt::Edge,
                    window,
                    cx,
                ),
            )
            // With Shift held it deletes outright, as Explorer's Delete does.
            .child(
                self.ringed(
                    action("recycle", "trash", "ごみ箱へ移動", "Delete／Shift+クリックで完全に削除", reasons.delete)
                        .on_click(cx.listener(|this, event: &ClickEvent, _, cx| this.delete_picked(event.modifiers().shift, cx))),
                    "recycle",
                    RingAt::Edge,
                    window,
                    cx,
                ),
            )
            .child(div().mx(px(6.)).w(px(1.)).h(px(16.)).bg(p.border))
            .child(
                self.ringed(
                    action("properties", "info", "プロパティ", "Alt+Enter", reasons.properties)
                        .on_click(cx.listener(|this, _, _, cx| this.show_properties(cx))),
                    "properties",
                    RingAt::Edge,
                    window,
                    cx,
                ),
            );
        row.into_any_element()
    }

    /// The history's button in the status bar: turning while an operation runs, with the count of those
    /// with problems not seen yet.
    pub fn history_button(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let operations = self.operations.read(cx);
        let (unseen, running) = (operations.history.unseen, operations.is_running());
        let label = if unseen > 0 { format!("操作履歴（問題のある操作 {unseen} 件）") } else { "操作履歴".to_owned() };
        let button = Button::new("history-button")
            .ghost()
            .xsmall()
            .map(|b| if running { b.icon(Spinner::new()) } else { b.icon(crate::view::glyph("clock")) })
            .accessibility_label(label.clone())
            // Shown only while the history is shut: open, the tip would lie over it.
            .when(!self.history_panel.open, |b| b.tooltip(label))
            .when(unseen > 0, |d| {
                d.child(
                    div()
                        .min_w(px(14.))
                        .h(px(14.))
                        .px(px(3.))
                        .rounded(px(7.))
                        // White on the deep red: past 4.5:1 in either theme.
                        .bg(p.danger_fill)
                        .text_color(p.on_accent)
                        .text_size(px(metrics::TEXT_TINY))
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(unseen.to_string()),
                )
            })
            .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                if this.history_panel.open {
                    this.show_history(false, None, window, cx);
                } else if !this.pressed_shut(event) {
                    this.show_history(true, None, window, cx);
                }
            }));
        // The button takes no listener for the press, so the box round it notes it.
        let button = self.ringed(button, "history-button", RingAt::Edge, window, cx);
        Self::opener(div().flex(), |this| this.history_panel.open, cx).child(button).into_any_element()
    }

    /// The file operations run since Colet opened, newest first.
    pub fn render_history(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.history_panel.open {
            return None;
        }
        let p = self.palette;
        // Each row and each item that opens a stop for Tab; the mouse leaves the keys
        // where they are, as before.
        let mut keys = Vec::new();
        for entry in &self.operations.read(cx).history.entries {
            keys.push(Stop::HistoryEntry(entry.id));
            if self.history_panel.expanded.contains(&entry.id) {
                keys.extend(entry.items.iter().enumerate().filter(|(_, item)| !item.gone).map(|(k, _)| Stop::HistoryItem(entry.id, k)));
            }
        }
        let stops: std::collections::HashMap<Stop, FocusHandle> = keys.into_iter().map(|key| (key.clone(), self.stop(key, cx))).collect();
        let history = &self.operations.read(cx).history;
        let entries: Vec<AnyElement> = history
            .entries
            .iter()
            .map(|entry| {
                let id = entry.id;
                let open = self.history_panel.expanded.contains(&id);
                let problems = entry.has_problems();
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        crate::view::keep_focus(crate::view::ring(
                            div().id(("history-entry", id)).track_focus(&stops[&Stop::HistoryEntry(id)]),
                            p,
                            stops[&Stop::HistoryEntry(id)].is_focused(window),
                            0.,
                        ))
                        .role(Role::Button)
                        .aria_label(format!("{} {}", entry.request.kind.title(), entry.target()))
                        .aria_expanded(open)
                        .px_3()
                        .py_2()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(metrics::TEXT_BODY))
                        .hover(|d| d.bg(p.raised_hover))
                        .child(
                            svg()
                                .path(if open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" })
                                .size(px(12.))
                                .text_color(p.text_faint),
                        )
                        .child(div().text_color(p.text_faint).child(clock(entry.started)))
                        .child(div().font_weight(FontWeight::SEMIBOLD).child(entry.request.kind.title()))
                        .child(div().flex_1().min_w_0().truncate().text_color(p.text_muted).child(entry.target()))
                        .child(div().text_color(if problems { p.danger } else { p.text_muted }).child(entry.outcome()))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.history_panel.toggle(id);
                            cx.notify();
                        }))
                        .test_support(),
                    )
                    .when(open, |d| {
                        d.when_some(entry.error.clone(), |d, error| {
                            d.child(div().px_3().pb_1().text_size(px(metrics::TEXT_BODY)).text_color(p.danger).child(error))
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
                                .pl(px(32.))
                                .py(px(3.))
                                .flex()
                                .flex_col()
                                .text_size(px(metrics::TEXT_BODY))
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap_2()
                                        .child(if item.gone {
                                            // Deleted, it has no place to open.
                                            div().child(name).into_any_element()
                                        } else {
                                            crate::view::keep_focus(crate::view::ring(
                                                div().id(history_item_id(id, k)).track_focus(&stops[&Stop::HistoryItem(id, k)]),
                                                p,
                                                stops[&Stop::HistoryItem(id, k)].is_focused(window),
                                                4.,
                                            ))
                                            // Opened in Colet, not in a browser: a button, its name in the accent color.
                                            .role(Role::Button)
                                            .aria_label(name.clone())
                                            .rounded(px(metrics::RADIUS))
                                            .text_color(p.accent)
                                            .hover(|d| d.bg(p.raised_hover))
                                            .child(name)
                                            .on_click(
                                                cx.listener(move |this, _, window, cx| this.reveal_from_history(path.clone(), window, cx)),
                                            )
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
                            d.child(
                                div()
                                    .pl(px(32.))
                                    .py_1()
                                    .text_size(px(metrics::TEXT_BODY))
                                    .text_color(p.text_faint)
                                    .child(format!("ほか {} 項目", entry.more)),
                            )
                        })
                    })
                    .into_any_element()
            })
            .collect();
        let clearable = history.entries.iter().any(|e| e.status != Status::Running);
        Some(
            deferred(
                div()
                    .id("history")
                    .absolute()
                    .right_3()
                    .bottom(px(32.))
                    .w(px(440.))
                    .max_h(px(520.))
                    .flex()
                    .flex_col()
                    .bg(p.raised)
                    .border_1()
                    .border_color(p.border_strong)
                    .rounded(px(8.))
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| this.show_history(false, None, window, cx)))
                    .child(
                        div()
                            .px_3()
                            .h(px(40.))
                            .flex()
                            .items_center()
                            .border_b_1()
                            .border_color(p.border)
                            .child(div().flex_1().font_weight(FontWeight::SEMIBOLD).child("操作履歴"))
                            .child(
                                crate::view::keep_focus(push("clear-history", "履歴を消去", false, clearable).ghost().small()).on_click(
                                    cx.listener(|this, _, _, cx| this.operations.update(cx, |operations, cx| operations.clear_history(cx))),
                                ),
                            ),
                    )
                    .child(if entries.is_empty() {
                        div()
                            .p_4()
                            .text_size(px(metrics::TEXT_BODY))
                            .text_color(p.text_muted)
                            .child("まだファイル操作はありません")
                            .into_any_element()
                    } else {
                        div().id("history-list").flex_1().overflow_y_scroll().children(entries).into_any_element()
                    }),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
