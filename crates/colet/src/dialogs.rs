//! The dialogs over the window, and the menu a favorite or saved search in home opens.
use gpui_kit::base::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::{CloseDialog, Colet, DIALOG},
    places::Dialog,
    state::saved_query,
    theme::Palette,
};

/// The keys the preferences list, as the old app's did.
const SHORTCUTS: [(&str, &str); 12] = [
    ("↑ ↓", "項目を選択"),
    ("← →", "カラムを移動"),
    ("Enter", "開く"),
    ("Alt+← / Alt+→", "戻る / 進む"),
    ("マウスの戻る / 進むボタン", "戻る / 進む"),
    ("Ctrl+F / Ctrl+L", "検索"),
    ("↓ / Enter", "検索欄から結果へ移動"),
    ("Esc", "検索をクリア"),
    ("Ctrl+D", "検索を保存 / フォルダーをお気に入りに追加"),
    ("F2", "保存した検索を編集"),
    ("F5", "更新"),
    ("文字キー", "その文字で始まる項目へ移動"),
];

fn field(p: Palette, input: &Entity<gpui_kit::base::input::InputState>, focused: bool) -> Div {
    div()
        .h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .bg(p.surface)
        .border_1()
        .border_color(if focused { p.accent } else { p.border_strong })
        .rounded(px(6.))
        .child(div().flex_1().min_w_0().child(Input::new(input)))
}

fn label(p: Palette, text: &'static str) -> Div {
    div().text_size(px(12.)).text_color(p.text_muted).child(text)
}

fn button(p: Palette, id: &'static str, text: &'static str, primary: bool, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.))
        .px_3()
        .flex()
        .items_center()
        .rounded(px(6.))
        .border_1()
        .when(primary, |b| b.bg(p.accent_fill).border_color(p.accent_fill).text_color(p.on_accent))
        .when(!primary, |b| b.bg(p.raised).border_color(p.border_strong))
        .when(enabled, |b| b.cursor_pointer().hover(|b| b.opacity(0.9)))
        .when(!enabled, |b| b.opacity(0.35))
        .child(text)
}

impl Colet {
    pub fn render_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let body = match self.dialog.as_ref()? {
            Dialog::EditSearch { name, query, .. } => {
                let problem = self.edit_problem(cx);
                let (name_focused, query_focused) =
                    (name.read(cx).focus_handle(cx).is_focused(window), query.read(cx).focus_handle(cx).is_focused(window));
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("保存した検索を編集"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(label(p, "名前"))
                            .child(field(p, name, name_focused))
                            .child(div().text_size(px(12.)).text_color(p.text_faint).child("空欄なら検索文字列をそのまま表示します")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(label(p, "検索文字列"))
                            .child(field(p, query, query_focused))
                            .when_some(problem.filter(|p| !p.is_empty()), |d, problem| {
                                d.child(div().text_size(px(12.)).text_color(p.danger).child(problem))
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .gap_2()
                            .child(
                                button(p, "cancel", "キャンセル", false, true)
                                    .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                                    .test_support(),
                            )
                            .child(
                                button(p, "save-search", "保存", true, problem.is_none())
                                    .on_click(cx.listener(|this, _, window, cx| this.submit_edit(window, cx)))
                                    .test_support(),
                            ),
                    )
            }
            Dialog::Preferences { instance } => {
                let focused = instance.read(cx).focus_handle(cx).is_focused(window);
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("設定"))
                    .child(
                        div().flex().flex_col().gap_1().child(label(p, "Everything インスタンス名")).child(
                            div()
                                .flex()
                                .gap_2()
                                .child(div().flex_1().child(field(p, instance, focused)))
                                .child(
                                    button(p, "reconnect", "再接続", false, true)
                                        .on_click(cx.listener(|this, _, _, cx| this.reconnect(cx)))
                                        .test_support(),
                                ),
                        ),
                    )
                    .child(div().text_size(px(12.)).text_color(p.text_muted).child(
                        "Everything 1.5を起動してお使いください。空欄の場合は自動接続します。名前を変更したら「再接続」を押してください。",
                    ))
                    .child(div().pt_1().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child("キーボード操作"))
                    .child(div().flex().flex_col().gap_1().text_size(px(12.)).children(SHORTCUTS.map(|(key, what)| {
                        div()
                            .flex()
                            .gap_3()
                            .child(div().w(px(180.)).flex_none().text_color(p.text).child(key))
                            .child(div().text_color(p.text_muted).child(what))
                    })))
                    .child(
                        div().flex().justify_end().child(
                            button(p, "close-preferences", "閉じる", true, true)
                                .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                                .test_support(),
                        ),
                    )
            }
        };
        Some(
            div()
                .id("dialog-overlay")
                .key_context(DIALOG)
                .on_action(cx.listener(|this, _: &CloseDialog, window, cx| this.close_dialog(window, cx)))
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(hsla(0., 0., 0., 0.45))
                .occlude()
                .child(
                    div()
                        .id("dialog")
                        .w(px(440.))
                        .p_5()
                        .bg(p.raised)
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(8.))
                        .shadow_lg()
                        .child(body),
                )
                .into_any_element(),
        )
    }

    pub fn render_home_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let menu = self.home_menu.as_ref()?;
        let path = menu.path.clone();
        let query = saved_query(&path).map(str::to_owned);
        let item = |id: &'static str, text: &'static str| {
            div()
                .id(id)
                .h(px(28.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(4.))
                .text_size(px(12.))
                .cursor_pointer()
                .hover(|d| d.bg(p.accent_fill).text_color(p.on_accent))
                .child(text)
        };
        let items: Vec<AnyElement> = match query {
            Some(query) => {
                let edit = query.clone();
                vec![
                    item("edit-search", "編集（F2）")
                        .on_click(cx.listener(move |this, _, window, cx| this.open_edit(&edit, window, cx)))
                        .test_support()
                        .into_any_element(),
                    item("unsave-search", "保存した検索を解除")
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.home_menu = None;
                            this.toggle_saved(&query, cx);
                        }))
                        .test_support()
                        .into_any_element(),
                ]
            }
            None => vec![
                item("unfavorite", "お気に入りを解除")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.home_menu = None;
                        this.toggle_favorite(&path, cx);
                    }))
                    .test_support()
                    .into_any_element(),
            ],
        };
        Some(
            deferred(
                div()
                    .id("home-menu")
                    .absolute()
                    .left(menu.at.x)
                    .top(menu.at.y)
                    .min_w(px(180.))
                    .p_1()
                    .bg(p.raised)
                    .border_1()
                    .border_color(p.border_strong)
                    .rounded(px(8.))
                    .shadow_lg()
                    .occlude()
                    .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                        this.home_menu = None;
                        cx.notify();
                    }))
                    .children(items),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}
