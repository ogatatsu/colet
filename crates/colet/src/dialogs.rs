//! What opens over the window: the dialogs, the file menu, and the history of file operations.
use colet_core::{
    operations::{Kind as Operation, Request},
    paths::{ancestors, folder_name, shown},
};
use std::path::PathBuf;

use gpui_kit::base::input::{Input, InputState};
use gpui_kit::component::{
    Disableable, Sizable,
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
    theme::Palette,
    view::{RING_ROOM, RingAt, TipExt},
};

/// The keys the preferences list, as the old app's did.
const SHORTCUTS: [(&str, &str); 29] = [
    ("↑ ↓", "項目を選択（Shift で範囲選択）"),
    ("← →", "カラムを移動"),
    ("Enter", "開く"),
    ("Space", "QuickLook でプレビュー"),
    ("Alt+← / Alt+→", "戻る / 進む"),
    ("マウスの戻る / 進むボタン", "戻る / 進む"),
    ("Ctrl+0", "ホームへ移動"),
    ("Ctrl+H", "隠しファイルの表示を切り替え"),
    ("Ctrl+U", "上の階層を表示"),
    ("Shift+F10", "メニューを開く"),
    ("Ctrl+F", "検索"),
    ("Ctrl+L", "パスを入力して移動（Alt+Enter で新しいタブに開く）"),
    ("↓ / Enter", "検索欄から結果へ移動"),
    ("Esc", "検索をクリア（空ならフィルターを解除）"),
    ("Ctrl+Shift+F", "検索フィルターを選ぶ"),
    ("Ctrl+クリック / Shift+クリック", "複数選択"),
    ("Ctrl+↑↓ / Ctrl+Space", "離れた項目を選択"),
    ("Ctrl+Shift+↑↓ / Ctrl+Shift+クリック", "範囲を選択に足す"),
    ("Ctrl+A", "読み込み済み項目を全選択"),
    ("Ctrl+C / X / V", "コピー / 切り取り / 貼り付け"),
    ("Ctrl+Shift+C", "パスをコピー"),
    ("Ctrl+Shift+N", "新規フォルダー"),
    ("Ctrl+D", "検索を保存 / フォルダーをお気に入りに追加"),
    ("F2", "名前を変更（保存した検索は編集）"),
    ("Delete", "ごみ箱へ移動"),
    ("Shift+Delete", "完全に削除"),
    ("Alt+Enter", "プロパティ"),
    ("F5", "更新"),
    ("文字キー", "カラムを名前で絞り込む（Backspace で 1 文字消す、Esc で解除）"),
];

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
    // The ring, two pixels in the focus color a pixel off the field, as the old app's outline.
    crate::view::frame(p, focused, 6.).child(
        div()
            .h(px(metrics::CONTROL_HEIGHT))
            .px(px(10.))
            .flex()
            .items_center()
            .bg(p.surface)
            .border_1()
            .border_color(p.border_strong)
            .rounded(px(6.))
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

/// A choice among the filters, checked as a radio button is.
fn filter_choice(id: ElementId, text: &str, chosen: bool) -> Button {
    push(id, text.to_owned(), chosen, true).role(Role::RadioButton).toggled(chosen)
}

impl Colet {
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

    fn dialog_parts(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<(&'static str, Div, Buttons)> {
        let title = match self.dialog.as_ref()? {
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
        let (content, buttons) = match self.dialog.as_ref()? {
            Dialog::Name { request, input, checking, .. } => self.name_dialog(request, input, checking.is_some(), window, cx),
            Dialog::EditFavorite { original, name, narrow } => self.favorite_dialog(original, name, narrow, window, cx),
            Dialog::EditSearch { name, query, filter, narrow, .. } => self.search_dialog(name, query, filter, narrow, window, cx),
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

    /// A saved search's name, query, filter and narrowing.
    fn search_dialog(
        &self,
        name: &Entity<InputState>,
        query: &Entity<InputState>,
        filter: &Option<String>,
        narrow: &Entity<InputState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> (Div, Buttons) {
        let p = self.palette;
        // The saved search's choice of filter, none first.
        let choices: Vec<Option<String>> =
            std::iter::once(None).chain(self.settings.filters.iter().map(|f| Some(f.name.clone()))).collect();
        let problem = self.edit_problem(cx);
        let chosen = filter.clone();
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
            .child(
                div().flex().flex_col().gap_1().child(label(p, "検索フィルター")).child(
                    div()
                        .id("edit-search-filters")
                        .flex()
                        .flex_wrap()
                        .gap(px(2.))
                        .role(Role::RadioGroup)
                        .aria_label("検索フィルター")
                        // Ringed as the fields are; the rings' room makes up most of the gaps.
                        .children(choices.into_iter().enumerate().map(|(i, choice)| {
                            let text = choice.clone().unwrap_or_else(|| "なし".into());
                            let button = filter_choice(("edit-search-filter", i).into(), &text, choice == chosen)
                                .on_click(cx.listener(move |this, _, _, cx| this.pick_edit_filter(choice.clone(), cx)));
                            self.ringed(button, format!("edit-search-filter:{i}"), RingAt::Round, window, cx)
                        })),
                ),
            )
            .child(self.narrow_field(narrow, None, window, cx));
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
                push("delete-filter", "削除", true, true).on_click(cx.listener(|this, _, window, cx| this.delete_filter(window, cx))),
            ),
        ];
        (content, buttons)
    }

    /// The search filters, in their order.
    fn filters_dialog(&self, window: &Window, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
        let filters = self.settings.filters.clone();
        let last = filters.len().saturating_sub(1);
        let rows: Vec<AnyElement> = filters
            .into_iter()
            .enumerate()
            .map(|(i, f)| {
                let name = f.name.clone();
                let (up_name, down_name, edit_name) = (name.clone(), name.clone(), name.clone());
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
                            .child(div().truncate().text_size(px(metrics::TEXT_LARGE)).child(f.name.clone()))
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(metrics::TEXT_BODY))
                                    .text_color(p.text_muted)
                                    .font_family("Consolas")
                                    .child(f.query.clone()),
                            ),
                    )
                    // Their stops named after the filter, to go with it.
                    .child(
                        self.ringed(
                            tool(("filter-up", i).into(), "arrow-up", format!("「{}」を上へ", f.name), i > 0)
                                .when(i > 0, |d| d.on_click(cx.listener(move |this, _, _, cx| this.move_filter(&up_name, -1, cx)))),
                            format!("filter-up:{}", f.name),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(("filter-down", i).into(), "arrow-down", format!("「{}」を下へ", f.name), i < last)
                                .when(i < last, |d| d.on_click(cx.listener(move |this, _, _, cx| this.move_filter(&down_name, 1, cx)))),
                            format!("filter-down:{}", f.name),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(("filter-edit", i).into(), "pencil-line", format!("「{}」を編集", f.name), true)
                                .on_click(cx.listener(move |this, _, window, cx| this.edit_filter(Some(edit_name.clone()), window, cx))),
                            format!("filter-edit:{}", f.name),
                            RingAt::Edge,
                            window,
                            cx,
                        ),
                    )
                    .child(
                        self.ringed(
                            tool(("filter-delete", i).into(), "trash", format!("「{}」を削除", f.name), true)
                                .on_click(cx.listener(move |this, _, window, cx| this.ask_delete_filter(name.clone(), window, cx))),
                            format!("filter-delete:{}", f.name),
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
                    .child("検索に加えるEverythingの検索式を登録し、検索欄の右のボタン（Ctrl+Shift+F）で1つ選んで使います。"),
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

    /// The theme, the connection to Everything, and the keys.
    fn preferences_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> (Div, Buttons) {
        let p = self.palette;
        // Each theme a stop for Tab, kept across frames.
        let themes = Theme::ALL;
        let (engine, said) = self.engine.describe();
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            // Taken at once as one is picked, and kept.
            .child(div().flex().flex_col().gap_1().child(label(p, "テーマ")).child(
                // Ringed as the fields are; the rings' room makes up most of the gaps.
                div().id("themes").flex().gap(px(2.)).role(Role::RadioGroup).aria_label("テーマ").children(themes.into_iter().map(
                    |theme| {
                        let chosen = self.settings.theme == theme;
                        // The mouse leaves the keys where they are, but within the group they go with the
                        // pick, as with a radio group's checked one, so the ring does not stay on the last.
                        let button = push(("theme", theme as usize), theme.label(), chosen, true)
                            .role(Role::RadioButton)
                            .toggled(chosen)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let in_group = Theme::ALL.iter().any(|t| this.stop(format!("theme:{t:?}"), cx).is_focused(window));
                                if in_group {
                                    this.stop(format!("theme:{theme:?}"), cx).focus(window, cx);
                                }
                                this.set_theme(theme, window, cx)
                            }));
                        self.ringed(button, format!("theme:{theme:?}"), RingAt::Round, window, cx)
                    },
                )),
            ))
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
                            .checked(self.settings.show_protected)
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
            .child(
                div().flex().flex_col().gap_1().child(label(p, "Everything")).child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .id("engine-state")
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_size(px(metrics::TEXT_LARGE))
                                .text_color(p.text)
                                .role(Role::Status)
                                .aria_label(said.clone())
                                .tip(said)
                                .child(format!("接続先：{engine}")),
                        )
                        .child(
                            self.ringed(
                                push("reconnect", "再接続", false, true).on_click(cx.listener(|this, _, _, cx| this.reconnect(cx))),
                                "reconnect",
                                RingAt::Round,
                                window,
                                cx,
                            )
                            .m(px(-RING_ROOM)),
                        ),
                ),
            )
            .child(
                div()
                    .text_size(px(metrics::TEXT_BODY))
                    .text_color(p.text_muted)
                    .child("Everything 1.5.0.1424以降を起動してお使いください。つながらないときは「再接続」を押してください。"),
            )
            .child(div().pt_1().text_size(px(metrics::TEXT_LARGE)).font_weight(FontWeight::SEMIBOLD).child("キーボード操作"))
            // One by one: an array of them all would lie whole on the stack, past what a debug build has.
            .child(div().flex().flex_col().gap_1().text_size(px(metrics::TEXT_BODY)).children(SHORTCUTS.iter().map(|&(key, what)| {
                div()
                    .flex()
                    .gap_3()
                    .child(div().w(px(180.)).flex_none().text_color(p.text).child(key))
                    // Wrapped in the room left, rather than cut at the dialog's edge.
                    .child(div().flex_1().min_w_0().text_color(p.text_muted).child(what))
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
    /// [`Colet::dialog`] goes.
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
                    .track_focus(&self.dialog_focus)
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
        if self.dialog.is_some() {
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
        self.popup = Some(Popup::File(Menu { column, paths, at, screen: (point.x, point.y) }));
        self.show_menu(window, cx);
        // Read afresh for each menu, so one added since is there.
        self.load_send_targets(window, cx);
    }

    /// The file menu, or a text field's, where it was opened; over a dialog too, for its fields'.
    pub fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (at, id, label, width) = match self.popup.as_ref()? {
            Popup::File(menu) => (menu.at, "menu", "ファイル操作", 240.),
            Popup::Text(menu) => (menu.at, "text-menu", "編集", 200.),
            Popup::Tab { at, .. } => (*at, "tab-menu", "タブ", 220.),
            _ => return None,
        };
        let menu = self.menu_element(id, label, cx)?;
        Some(
            deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(div().min_w(px(width)).child(menu)))
                .with_priority(2)
                .into_any_element(),
        )
    }

    /// The file actions under the toolbar.
    pub fn render_actions(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let reasons = self.reasons();
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
        let unseen = self.history.unseen;
        let label = if unseen > 0 { format!("操作履歴（問題のある操作 {unseen} 件）") } else { "操作履歴".to_owned() };
        let button = Button::new("history-button")
            .ghost()
            .xsmall()
            .map(|b| if self.running.is_some() { b.icon(Spinner::new()) } else { b.icon(crate::view::glyph("clock")) })
            .accessibility_label(label.clone())
            .tooltip(label)
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
                if this.history.open {
                    this.show_history(false, None, window, cx);
                } else if !this.pressed_shut(event) {
                    this.show_history(true, None, window, cx);
                }
            }));
        // The button takes no listener for the press, so the box round it notes it.
        let button = self.ringed(button, "history-button", RingAt::Edge, window, cx);
        Self::opener(div().flex(), |this| this.history.open, cx).child(button).into_any_element()
    }

    /// The file operations run since Colet opened, newest first.
    pub fn render_history(&mut self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.history.open {
            return None;
        }
        let p = self.palette;
        // Each row and each item that opens a stop for Tab; the mouse leaves the keys
        // where they are, as before.
        let mut keys = Vec::new();
        for entry in &self.history.entries {
            keys.push(format!("history-entry:{}", entry.id));
            if self.history.expanded.contains(&entry.id) {
                keys.extend(
                    entry.items.iter().enumerate().filter(|(_, item)| !item.gone).map(|(k, _)| format!("history-item:{}:{k}", entry.id)),
                );
            }
        }
        let stops: std::collections::HashMap<String, FocusHandle> = keys.into_iter().map(|key| (key.clone(), self.stop(key, cx))).collect();
        let entries: Vec<AnyElement> = self
            .history
            .entries
            .iter()
            .map(|entry| {
                let id = entry.id;
                let open = self.history.expanded.contains(&id);
                let problems = entry.has_problems();
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        crate::view::keep_focus(crate::view::ring(
                            div().id(("history-entry", id)).track_focus(&stops[&format!("history-entry:{id}")]),
                            p,
                            stops[&format!("history-entry:{id}")].is_focused(window),
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
                            if !this.history.expanded.remove(&id) {
                                this.history.expanded.insert(id);
                            }
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
                                                div()
                                                    .id(("history-item", (id << 16) | k as u64))
                                                    .track_focus(&stops[&format!("history-item:{id}:{k}")]),
                                                p,
                                                stops[&format!("history-item:{id}:{k}")].is_focused(window),
                                                4.,
                                            ))
                                            // Opened in Colet, not in a browser: a button, its name in the accent color.
                                            .role(Role::Button)
                                            .aria_label(name.clone())
                                            .rounded(px(4.))
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
        let clearable = self.history.entries.iter().any(|e| e.status != Status::Running);
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
                                    cx.listener(|this, _, _, cx| {
                                        this.history.entries.retain(|e| e.status == Status::Running);
                                        // The rows' stops go with them; a running one's comes back as it is drawn.
                                        this.stops.get_mut().retain(|key, _| !key.starts_with("history-"));
                                        cx.notify();
                                    }),
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
