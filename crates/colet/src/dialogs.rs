//! What opens over the window: the dialogs, the file menu, and the history of file operations.
use colet_core::{
    operations::{Kind as Operation, Request},
    paths::{ancestors, folder_name},
};
use gpui_kit::base::input::Input;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use crate::{
    app::{CloseDialog, Colet, ConfirmDialog, DIALOG},
    ops::{ItemStatus, Menu, Status},
    places::Dialog,
    settings::Theme,
    theme::Palette,
    view::TipExt,
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


/// How much of a dialog's right margin is its rows' own: where its scrollbar lies, clear of them.
const MARGIN: f32 = 14.;

fn field(p: Palette, input: &Entity<gpui_kit::base::input::InputState>, window: &Window, cx: &App) -> Div {
    // The ring, two pixels in the focus color a pixel off the field, as the old app's outline.
    let focused = input.read(cx).focus_handle(cx).is_focused(window);
    crate::view::frame(p, focused).child(
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

fn button(p: Palette, id: impl Into<ElementId>, text: &'static str, primary: bool, enabled: bool) -> Stateful<Div> {
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
        // A plain one lies on the dialog's own color, which a fade would leave as it was.
        .when(enabled && primary, |b| b.cursor_pointer().hover(|b| b.opacity(0.9)))
        .when(enabled && !primary, |b| b.cursor_pointer().hover(|b| b.bg(p.raised_hover)))
        .when(!enabled, |b| b.opacity(0.35))
        .child(text)
}

impl Colet {
    pub fn render_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = self.palette;
        let title = match self.dialog.as_ref()? {
            Dialog::Name { request: Request { kind: Operation::NewFolder, .. }, .. } => "新規フォルダー",
            Dialog::Name { .. } => "名前を変更",
            Dialog::Recycle { .. } => "ごみ箱へ移動しますか？",
            Dialog::EditSearch { .. } => "保存した検索を編集",
            Dialog::Preferences { .. } => "設定",
        };
        // Each theme a stop for Tab, kept across frames.
        let themes: Vec<(Theme, FocusHandle)> =
            Theme::ALL.into_iter().map(|theme| (theme, self.stop(format!("theme:{theme:?}"), cx))).collect();
        let (content, buttons) = match self.dialog.as_ref()? {
            Dialog::Name { request: Request { kind, paths, destination, .. }, input, .. } => {
                let problem = self.name_problem(cx);
                let empty = input.read(cx).value().trim().is_empty();
                // Where the name goes: a rename may come from the results, far from its folder.
                let (place, submit) = match kind {
                    Operation::NewFolder => (format!("作成先：{destination}"), "作成"),
                    _ => (format!("場所：{}", paths.first().and_then(|p| ancestors(p).into_iter().rev().nth(1)).unwrap_or_default()), "保存"),
                };
                let content = div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(div().text_size(px(12.)).text_color(p.text_faint).child(place))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(label(p, "名前"))
                            .child(field(p, input, window, cx))
                            .when_some(problem, |d, problem| d.child(div().text_size(px(12.)).text_color(p.danger).child(problem))),
                    );
                let buttons = vec![
                    button(p, "cancel", "キャンセル", false, true)
                        .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                        .test_support()
                        .into_any_element(),
                    button(p, "submit-name", submit, true, !empty && problem.is_none())
                        .on_click(cx.listener(|this, _, window, cx| this.submit_name(window, cx)))
                        .test_support()
                        .into_any_element(),
                ];
                (content, buttons)
            }
            Dialog::Recycle { paths, permanent } => {
                let content = div()
                    .flex()
                    .flex_col()
                    .gap_3()
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
                            // Set into the dialog: in the light colors the dialog and a surface are both white.
                            .bg(p.bg)
                            .border_1()
                            .border_color(p.border)
                            .rounded(px(6.))
                            .children(paths.iter().take(8).map(|path| div().truncate().child(path.clone())))
                            .when(paths.len() > 8, |d| d.child(format!("ほか {} 項目", paths.len() - 8))),
                    );
                let buttons = vec![
                    button(p, "cancel", "キャンセル", false, true)
                        .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                        .test_support()
                        .into_any_element(),
                    // Delete → Enter confirms, as in Explorer: ringed as the one the keys are on.
                    crate::view::frame(p, self.dialog_focus.is_focused(window))
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
                        )
                        .into_any_element(),
                ];
                (content, buttons)
            }
            Dialog::EditSearch { name, query, .. } => {
                let problem = self.edit_problem(cx);
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
                    );
                let buttons = vec![
                    button(p, "cancel", "キャンセル", false, true)
                        .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                        .test_support()
                        .into_any_element(),
                    button(p, "save-search", "保存", true, problem.is_none())
                        .on_click(cx.listener(|this, _, window, cx| this.submit_edit(window, cx)))
                        .test_support()
                        .into_any_element(),
                ];
                (content, buttons)
            }
            Dialog::Preferences { instance } => {
                let content = div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    // Taken at once as one is picked, and kept.
                    .child(
                        div().flex().flex_col().gap_1().child(label(p, "テーマ")).child(
                            // Ringed as the fields are; the rings' room makes up most of the gaps.
                            div().id("themes").flex().gap(px(2.)).role(Role::RadioGroup).aria_label("テーマ").children(themes.into_iter().map(
                                |(theme, focus)| {
                                    let chosen = self.settings.theme == theme;
                                    crate::view::frame(p, crate::view::keyed(&focus, window)).child(
                                        button(p, ("theme", theme as usize), theme.label(), chosen, true)
                                            .track_focus(&focus)
                                            .role(Role::RadioButton)
                                            .aria_label(theme.label())
                                            .aria_toggled(if chosen { Toggled::True } else { Toggled::False })
                                            .on_click(cx.listener(move |this, _, window, cx| this.set_theme(theme, window, cx)))
                                            .test_support(),
                                    )
                                },
                            )),
                        ),
                    )
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
                    })));
                let buttons = vec![
                    button(p, "close-preferences", "閉じる", true, true)
                        .on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx)))
                        .test_support()
                        .into_any_element(),
                ];
                (content, buttons)
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
                    } else {
                        // Nothing to confirm: Enter presses the button that has the keys.
                        cx.propagate();
                    }
                }))
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                // Clear of the window's edges, which a dialog too tall for it stops short of.
                .p_4()
                .bg(hsla(0., 0., 0., 0.45))
                .occlude()
                .child(
                    div()
                        .id("dialog")
                        .role(Role::Dialog)
                        .aria_label(title)
                        .track_focus(&self.dialog_focus)
                        .w(px(440.))
                        .max_h_full()
                        // The right margin is mostly the rows' own, so the scrollbar lies in it.
                        .py_5()
                        .pl_5()
                        .pr(px(20. - MARGIN))
                        .flex()
                        .flex_col()
                        .gap_3()
                        .bg(p.raised)
                        .border_1()
                        .border_color(p.border_strong)
                        .rounded(px(8.))
                        .shadow_lg()
                        // The heading and the buttons stay; what lies between scrolls when the window is too low.
                        .child(div().flex_none().pr(px(MARGIN)).text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).child(title))
                        .child(
                            div()
                                .relative()
                                .flex_1()
                                .min_h_0()
                                .flex()
                                .flex_col()
                                .child(
                                    div()
                                        .id("dialog-body")
                                        .flex_1()
                                        .min_h_0()
                                        .overflow_y_scroll()
                                        .track_scroll(&self.dialog_scroll)
                                        .child(content.pr(px(MARGIN))),
                                )
                                .child(div().absolute().inset_0().child(crate::view::scrollbar(gpui_kit::base::Scrollbar::vertical(&self.dialog_scroll), p))),
                        )
                        .child(div().flex_none().pr(px(MARGIN)).flex().justify_end().gap_2().children(buttons))
                        .test_support(),
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
        let Some(column) = self.state.columns.get(index).map(|c| c.id) else { return };
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
        self.menu = Some(Menu { column, paths, at, screen: (point.x, point.y) });
        self.crumb_menu_open = false;
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
                        .role(Role::Menu)
                        .aria_label("ファイル操作")
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
        let stops: std::collections::HashMap<&'static str, FocusHandle> = ["copy", "cut", "paste", "copy-path", "rename", "new-folder", "recycle"]
            .into_iter()
            .map(|id| (id, self.stop(id, cx)))
            .collect();
        let action = |id: &'static str, glyph: &'static str, label: &'static str, key: &'static str, off: Option<&'static str>| {
            let tip = match off {
                Some(why) => format!("{label}（{why}）"),
                None => format!("{label}（{key}）"),
            };
            crate::view::ring(div().id(id).track_focus(&stops[id]), p)
                .role(Role::Button)
                .aria_label(label)
                .size(px(28.))
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .child(svg().path(format!("icons/{glyph}.svg")).size(px(15.)).text_color(p.text_muted))
                .map(|d| if off.is_some() { d.steady_tip(p, tip) } else { d.tip(p, tip) })
                .when(off.is_none(), |d| d.cursor_pointer().hover(|d| d.bg(p.hover)))
                .when(off.is_some(), |d| d.opacity(0.35))
        };
        let row = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            // Those done on the spot leave the keys where they were; the ones asking in a dialog take them.
            .child(crate::view::keep_focus(action("copy", "copy", "コピー", "Ctrl+C", reasons.copy)).on_click(cx.listener(|this, _, _, cx| this.copy(false, cx))).test_support())
            .child(crate::view::keep_focus(action("cut", "scissors", "切り取り", "Ctrl+X", reasons.cut)).on_click(cx.listener(|this, _, _, cx| this.copy(true, cx))).test_support())
            .child(crate::view::keep_focus(action("paste", "clipboard-paste", "貼り付け", "Ctrl+V", reasons.paste)).on_click(cx.listener(|this, _, _, cx| this.paste(cx))).test_support())
            .child(crate::view::keep_focus(action("copy-path", "link", "パスをコピー", "Ctrl+Shift+C", reasons.copy_path)).on_click(cx.listener(|this, _, _, cx| this.copy_paths(cx))).test_support())
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
        let focus = self.history_focus.clone();
        crate::view::ring(div().id("history-button").track_focus(&focus), p)
            .role(Role::Button)
            .aria_label(label.clone())
            .h(px(22.))
            .px_1()
            .flex()
            .items_center()
            .gap_1()
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|d| d.bg(p.hover))
            .tip(p, label.clone())
            .child(if self.running.is_some() {
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
                        // White on the deep red, as the Recycle Bin's button: past 4.5:1 in either theme.
                        .bg(p.danger_fill)
                        .text_color(p.on_accent)
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
        // Each row, each item that opens and the clear button a stop for Tab; the mouse leaves the keys
        // where they are, as before.
        let mut keys = vec!["clear-history".to_owned()];
        for entry in &self.history {
            keys.push(format!("history-entry:{}", entry.id));
            if self.expanded.contains(&entry.id) {
                keys.extend(entry.items.iter().enumerate().filter(|(_, item)| !item.gone).map(|(k, _)| format!("history-item:{}:{k}", entry.id)));
            }
        }
        let stops: std::collections::HashMap<String, FocusHandle> = keys.into_iter().map(|key| (key.clone(), self.stop(key, cx))).collect();
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
                        crate::view::keep_focus(crate::view::ring(div().id(("history-entry", id)).track_focus(&stops[&format!("history-entry:{id}")]), p))
                            .role(Role::Button)
                            .aria_label(format!("{} {}", entry.request.kind.title(), entry.target()))
                            .aria_expanded(open)
                            .px_3()
                            .py_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_size(px(12.))
                            .cursor_pointer()
                            .hover(|d| d.bg(p.raised_hover))
                            .child(svg().path(if open { "icons/chevron-down.svg" } else { "icons/chevron-right.svg" }).size(px(12.)).text_color(p.text_faint))
                            .child(div().text_color(p.text_faint).child(clock(entry.started)))
                            .child(div().font_weight(FontWeight::SEMIBOLD).child(entry.request.kind.title()))
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
                                    ItemStatus::Done | ItemStatus::Handed => p.text_muted,
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
                                                crate::view::keep_focus(crate::view::ring(
                                                    div().id(("history-item", (id << 16) | k as u64)).track_focus(&stops[&format!("history-item:{id}:{k}")]),
                                                    p,
                                                ))
                                                    .role(Role::Link)
                                                    .aria_label(name.clone())
                                                    .rounded(px(4.))
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
                                crate::view::keep_focus(crate::view::ring(div().id("clear-history").track_focus(&stops["clear-history"]), p))
                                    .role(Role::Button)
                                    .aria_label("履歴を消去")
                                    .px_2()
                                    .h(px(26.))
                                    .flex()
                                    .items_center()
                                    .rounded(px(4.))
                                    .text_size(px(12.))
                                    .when(clearable, |d| d.cursor_pointer().hover(|d| d.bg(p.raised_hover)))
                                    .when(!clearable, |d| d.opacity(0.35))
                                    .child("履歴を消去")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.history.retain(|e| e.status == Status::Running);
                                        // The rows' stops go with them; a running one's comes back as it is drawn.
                                        this.stops.retain(|key, _| !key.starts_with("history-"));
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

