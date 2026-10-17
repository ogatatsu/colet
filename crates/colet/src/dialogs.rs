//! What opens over the window: the dialogs, the file menu, and the history of file operations.
use colet_core::{
    operations::Kind as Operation,
    paths::{ancestors, folder_name},
};
use gpui_kit::base::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::{CloseDialog, Colet, ConfirmDialog, DIALOG},
    ops::{ItemStatus, Menu, Status},
    places::Dialog,
    theme::Palette,
};

/// The keys the preferences list, as the old app's did.
const SHORTCUTS: [(&str, &str); 22] = [
    ("↑ ↓", "項目を選択（Shift で範囲選択）"),
    ("← →", "カラムを移動"),
    ("Enter", "開く"),
    ("Alt+← / Alt+→", "戻る / 進む"),
    ("マウスの戻る / 進むボタン", "戻る / 進む"),
    ("Shift+F10", "メニューを開く"),
    ("Ctrl+F / Ctrl+L", "検索"),
    ("↓ / Enter", "検索欄から結果へ移動"),
    ("Esc", "検索をクリア"),
    ("Ctrl+クリック / Shift+クリック", "複数選択"),
    ("Ctrl+A", "読み込み済み項目を全選択"),
    ("Ctrl+C / X / V", "コピー / 切り取り / 貼り付け"),
    ("Ctrl+Shift+C", "パスをコピー"),
    ("Ctrl+Shift+N", "新規フォルダー"),
    ("Ctrl+D", "検索を保存 / フォルダーをお気に入りに追加"),
    ("F2", "名前を変更（保存した検索は編集）"),
    // Shift+Delete asks the same; Colet never deletes for good.
    ("Delete / Shift+Delete", "ごみ箱へ移動"),
    ("F5", "更新"),
    ("文字キー", "その文字で始まる項目へ移動"),
    ("☆", "お気に入り / 検索の保存"),
    ("右クリック", "メニュー"),
    ("ステータスバー右端", "操作履歴"),
];

/// `HH:MM`, local.
fn clock(unix_ms: u64) -> String {
    colet_core::time::local(unix_ms).map_or_else(String::new, |t| format!("{:02}:{:02}", t.wHour, t.wMinute))
}


fn field(p: Palette, input: &Entity<gpui_kit::base::input::InputState>, window: &Window, cx: &App) -> Div {
    // The ring, two pixels in the focus color a pixel off the field, as the old app's outline.
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    div()
        .p(px(1.))
        .rounded(px(8.))
        .border_2()
        .border_color(if focused { p.focus } else { transparent_black() })
        .child(
            div()
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .bg(p.surface)
                .border_1()
                .border_color(p.border_strong)
                .rounded(px(6.))
                .child(div().flex_1().min_w_0().child(Input::new(input))),
        )
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
            Dialog::Name { kind, paths, destination, input, .. } => {
                let problem = self.name_problem(cx);
                let empty = input.read(cx).value().trim().is_empty();
                // Where the name goes: a rename may come from the results, far from its folder.
                let (title, place, submit) = match kind {
                    Operation::NewFolder => ("新規フォルダー", format!("作成先：{destination}"), "作成"),
                    _ => (
                        "名前を変更",
                        format!("場所：{}", paths.first().and_then(|p| ancestors(p).into_iter().rev().nth(1)).unwrap_or_default()),
                        "保存",
                    ),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
                    .child(div().text_size(px(12.)).text_color(p.text_faint).child(place))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(label(p, "名前"))
                            .child(field(p, input, window, cx))
                            .when_some(problem, |d, problem| d.child(div().text_size(px(12.)).text_color(p.danger).child(problem))),
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
                                button(p, "submit-name", submit, true, !empty && problem.is_none())
                                    .on_click(cx.listener(|this, _, window, cx| this.submit_name(window, cx)))
                                    .test_support(),
                            ),
                    )
            }
            Dialog::Recycle { paths, permanent } => div()
                .flex()
                .flex_col()
                .gap_3()
                .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child("ごみ箱へ移動しますか？"))
                .when(*permanent, |d| d.child(div().text_size(px(12.)).child("Coletは完全な削除（Shift+Delete）に対応していません。")))
                .child(div().text_size(px(12.)).child(format!(
                    "{} 項目をごみ箱へ移動します。ごみ箱を使用できない場所では操作を中止します。",
                    paths.len()
                )))
                .child(
                    div()
                        .p_2()
                        .flex()
                        .flex_col()
                        .text_size(px(12.))
                        .text_color(p.text_muted)
                        .bg(p.surface)
                        .rounded(px(6.))
                        .children(paths.iter().take(8).map(|path| div().truncate().child(path.clone())))
                        .when(paths.len() > 8, |d| d.child(format!("ほか {} 項目", paths.len() - 8))),
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
                            // Delete → Enter confirms, as in Explorer: ringed as the one the keys are on.
                            div()
                                .p(px(1.))
                                .rounded(px(8.))
                                .border_2()
                                .border_color(if self.dialog_focus.is_focused(window) { p.focus } else { transparent_black() })
                                .child(
                                    div()
                                        .id("confirm-recycle")
                                        .h(px(30.))
                                        .px_3()
                                        .flex()
                                        .items_center()
                                        .rounded(px(6.))
                                        .bg(p.danger_fill)
                                        .text_color(p.on_accent)
                                        .cursor_pointer()
                                        .hover(|b| b.opacity(0.9))
                                        .child("ごみ箱へ移動")
                                        .on_click(cx.listener(|this, _, window, cx| this.confirm_recycle(window, cx)))
                                        .test_support(),
                                ),
                        ),
                ),
            Dialog::EditSearch { name, query, .. } => {
                let problem = self.edit_problem(cx);
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
                            .child(field(p, name, window, cx))
                            .child(div().text_size(px(12.)).text_color(p.text_faint).child("空欄なら検索文字列をそのまま表示します")),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(label(p, "検索文字列"))
                            .child(field(p, query, window, cx))
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
                                .child(div().flex_1().child(field(p, instance, window, cx)))
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
                .on_action(cx.listener(|this, _: &ConfirmDialog, window, cx| {
                    if matches!(this.dialog, Some(Dialog::Recycle { .. })) {
                        this.confirm_recycle(window, cx);
                    }
                }))
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
                        .track_focus(&self.dialog_focus)
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

    /// Opens the file menu on `paths` (`None` for the selection, empty for the folder itself) in the
    /// column at `index`, at `at` in the window. The keys stay where they were, as the selection does.
    pub fn open_menu(&mut self, index: usize, paths: Option<Vec<String>>, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.is_some() {
            return;
        }
        self.state.set_active(index);
        let handle = if index == 0 { &self.home_focus } else { &self.columns_focus };
        handle.focus(window, cx);
        let scale = window.scale_factor();
        let mut point = windows::Win32::Foundation::POINT {
            x: (f32::from(at.x) * scale) as i32,
            y: (f32::from(at.y) * scale) as i32,
        };
        let _ = unsafe {
            windows::Win32::Graphics::Gdi::ClientToScreen(windows::Win32::Foundation::HWND(self.owner as *mut _), &mut point)
        };
        self.menu = Some(Menu { index, paths, at, screen: (point.x, point.y) });
        self.sort_open = false;
        self.menu_cursor = None;
        self.menu_focus.focus(window, cx);
        cx.notify();
    }

    pub fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let at = self.menu.as_ref()?.at;
        let lines = self.render_lines(cx);
        Some(
            deferred(
                anchored().position(at).snap_to_window_with_margin(px(8.)).child(
                    div()
                        .id("menu")
                        .track_focus(&self.menu_focus)
                        .key_context(crate::app::MENU)
                        .on_action(cx.listener(|this, _: &crate::app::MenuUp, _, cx| this.move_cursor(-1, cx)))
                        .on_action(cx.listener(|this, _: &crate::app::MenuDown, _, cx| this.move_cursor(1, cx)))
                        .on_action(cx.listener(|this, _: &crate::app::MenuConfirm, window, cx| this.confirm_cursor(window, cx)))
                        .on_action(cx.listener(|this, _: &crate::app::CloseMenu, window, cx| this.close_menus(true, window, cx)))
                        .min_w(px(240.))
                        .p_1()
                        .bg(p.raised)
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(8.))
                        .shadow_lg()
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _, window, cx| this.close_menus(true, window, cx)))
                        .children(lines),
                ),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }

    /// The file actions beside the trail, and the room they take.
    pub fn render_actions(&mut self, cx: &mut Context<Self>) -> (f32, AnyElement) {
        let p = self.palette;
        let reasons = self.reasons();
        let action = |id: &'static str, glyph: &'static str, label: &'static str, key: &'static str, off: Option<&'static str>| {
            let tip = match off {
                Some(why) => format!("{label}（{why}）"),
                None => format!("{label}（{key}）"),
            };
            div()
                .id(id)
                .size(px(28.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .child(svg().path(format!("icons/{glyph}.svg")).size(px(15.)).text_color(p.text_muted))
                .tooltip(move |_, cx| cx.new(|_| crate::view::Tip { text: tip.clone().into(), palette: p }).into())
                .when(off.is_none(), |d| d.cursor_pointer().hover(|d| d.bg(p.hover)))
                .when(off.is_some(), |d| d.opacity(0.35))
        };
        let row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .child(action("copy", "copy", "コピー", "Ctrl+C", reasons.copy).on_click(cx.listener(|this, _, _, cx| this.copy(false, cx))).test_support())
            .child(action("cut", "scissors", "切り取り", "Ctrl+X", reasons.cut).on_click(cx.listener(|this, _, _, cx| this.copy(true, cx))).test_support())
            .child(action("paste", "clipboard-paste", "貼り付け", "Ctrl+V", reasons.paste).on_click(cx.listener(|this, _, _, cx| this.paste(cx))).test_support())
            .child(action("copy-path", "link", "パスをコピー", "Ctrl+Shift+C", reasons.copy_path).on_click(cx.listener(|this, _, _, cx| this.copy_paths(cx))).test_support())
            .child(div().mx(px(6.)).w(px(1.)).h(px(16.)).bg(p.border))
            .child(action("rename", "pencil-line", "名前を変更", "F2", reasons.rename).on_click(cx.listener(|this, _, window, cx| this.ask_rename(window, cx))).test_support())
            .child(action("new-folder", "folder-plus", "新規フォルダー", "Ctrl+Shift+N", reasons.paste).on_click(cx.listener(|this, _, window, cx| this.ask_new_folder(window, cx))).test_support())
            .child(action("recycle", "trash", "ごみ箱へ移動", "Delete", reasons.delete).on_click(cx.listener(|this, _, window, cx| this.ask_recycle(false, window, cx))).test_support());
        (7. * 30. + 13. + 16., row.into_any_element())
    }

    /// The history's button in the status bar: turning while an operation runs, with the count of those
    /// with problems not seen yet.
    pub fn history_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let unseen = self.unseen;
        let label = if unseen > 0 { format!("操作履歴（問題のある操作 {unseen} 件）") } else { "操作履歴".to_owned() };
        div()
            .id("history-button")
            .h(px(22.))
            .px_1()
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|d| d.bg(p.hover))
            .tooltip(move |_, cx| cx.new(|_| crate::view::Tip { text: label.clone().into(), palette: p }).into())
            .child(if self.running {
                crate::view::spinner("history-running", 14., p.text_muted).into_any_element()
            } else {
                svg().path("icons/clock.svg").size(px(14.)).text_color(p.text_muted).into_any_element()
            })
            .when(unseen > 0, |d| {
                d.child(
                    div()
                        .min_w(px(14.))
                        .h(px(14.))
                        .px(px(3.))
                        .rounded(px(7.))
                        .bg(p.danger)
                        .text_color(hsla(0., 0., 0.08, 1.))
                        .text_size(px(10.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(unseen.to_string()),
                )
            })
            .on_click(cx.listener(|this, _, window, cx| {
                let open = !this.history_open;
                this.show_history(open, None, window, cx);
            }))
            .test_support()
            .into_any_element()
    }

    /// The file operations run since Colet opened, newest first.
    pub fn render_history(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.history_open {
            return None;
        }
        let p = self.palette;
        let entries: Vec<AnyElement> = self
            .history
            .iter()
            .map(|entry| {
                let id = entry.id;
                let open = self.expanded.contains(&id);
                let problems = entry.has_problems();
                div()
                    .flex()
                    .flex_col()
                    .border_b_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .id(("history-entry", id))
                            .px_3()
                            .py_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.hover))
                            .child(svg().path(if open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" }).size(px(12.)).text_color(p.text_faint))
                            .child(div().text_color(p.text_faint).child(clock(entry.started)))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(entry.kind.title()))
                            .child(div().flex_1().min_w_0().truncate().text_color(p.text_muted).child(entry.target()))
                            .child(div().text_color(if problems { p.danger } else { p.text_muted }).child(entry.outcome()))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if !this.expanded.remove(&id) {
                                    this.expanded.insert(id);
                                }
                                cx.notify();
                            }))
                            .test_support(),
                    )
                    .when(open, |d| {
                        d.when_some(entry.error.clone(), |d, error| d.child(div().px_3().pb_1().text_size(px(12.)).text_color(p.danger).child(error)))
                            .children(entry.items.iter().enumerate().map(|(k, item)| {
                                let path = item.path.clone();
                                let name = folder_name(&item.path).to_owned();
                                let color = match item.status {
                                    ItemStatus::Done => p.text_muted,
                                    ItemStatus::Failed => p.danger,
                                    _ => p.text_faint,
                                };
                                div()
                                    .px_3()
                                    .pl(px(32.))
                                    .py(px(3.))
                                    .flex()
                                    .flex_col()
                                    .text_size(px(12.))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(if item.gone {
                                                // Gone to the Recycle Bin, it has no place to open.
                                                div().child(name).into_any_element()
                                            } else {
                                                div()
                                                    .id(("history-item", (id << 16) | k as u64))
                                                    .text_color(p.accent)
                                                    .cursor_pointer()
                                                    .hover(|d| d.underline())
                                                    .child(name)
                                                    .on_click(cx.listener(move |this, _, window, cx| this.reveal_from_history(path.clone(), window, cx)))
                                                    .test_support()
                                                    .into_any_element()
                                            })
                                            .child(div().text_color(color).child(item.status.label())),
                                    )
                                    .child(div().truncate().text_color(p.text_faint).child(match &item.from {
                                        Some(from) => format!("← {from}"),
                                        None => item.path.clone(),
                                    }))
                                    .when_some(item.error.clone(), |d, error| d.child(div().text_color(p.danger).child(error)))
                                    .into_any_element()
                            }))
                            .when(entry.more > 0, |d| d.child(div().pl(px(32.)).py_1().text_size(px(12.)).text_color(p.text_faint).child(format!("ほか {} 項目", entry.more))))
                    })
                    .into_any_element()
            })
            .collect();
        let clearable = self.history.iter().any(|e| e.status != Status::Running);
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
                                div()
                                    .id("clear-history")
                                    .px_2()
                                    .h(px(26.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.))
                                    .text_size(px(12.))
                                    .when(clearable, |d| d.cursor_pointer().hover(|d| d.bg(p.hover)))
                                    .when(!clearable, |d| d.opacity(0.35))
                                    .child("履歴を消去")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.history.retain(|e| e.status == Status::Running);
                                        cx.notify();
                                    }))
                                    .test_support(),
                            ),
                    )
                    .child(if entries.is_empty() {
                        div().p_4().text_size(px(12.)).text_color(p.text_muted).child("まだファイル操作はありません").into_any_element()
                    } else {
                        div().id("history-list").flex_1().overflow_y_scroll().children(entries).into_any_element()
                    }),
            )
            .with_priority(2)
            .into_any_element(),
        )
    }
}

