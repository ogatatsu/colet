//! The preferences: the theme, the display size, which hidden items show, and the keys, in a dialog of
//! their own. Each is taken at once as it is picked, and kept.
use std::rc::Rc;

use gpui_kit::component::{Disableable, checkbox::Checkbox};
use gpui_kit::*;

use crate::{
    app::Colet,
    dialogs::{Choice, Dialog, DialogParts, Pick, label, push},
    metrics::{self, scaled},
    settings::{Theme, ZOOMS},
    stops::Stop,
    theme::ActivePalette,
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

impl Colet {
    pub fn open_preferences(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.begin_dialog(window, cx);
        // The theme now chosen takes the keys, as a radio group's checked one does.
        self.stop(Stop::Theme(self.stored.settings().theme), cx).focus(window, cx);
        self.overlays.set_dialog(Dialog::Preferences);
        cx.notify();
    }

    /// Takes `theme`'s colors at once, and keeps it.
    pub fn set_theme(&mut self, theme: Theme, window: &mut Window, cx: &mut Context<Self>) {
        self.update_settings(|s| s.theme = theme, cx);
        self.apply_theme(window, cx);
    }

    /// Draws everything at `zoom` percent at once, and keeps it. The columns are brought to rest again
    /// at their new widths as the window is next laid out.
    pub fn set_zoom(&mut self, zoom: u16, cx: &mut Context<Self>) {
        self.update_settings(|s| s.zoom = zoom, cx);
        crate::theme::set_zoom(zoom, cx);
        cx.notify();
    }

    /// The theme, the display, and the keys.
    pub(crate) fn preferences_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) -> DialogParts {
        let p = cx.palette();
        let themes = Theme::ALL
            .into_iter()
            .map(|theme| {
                let pick: Pick = Rc::new(move |this, window, cx| this.set_theme(theme, window, cx));
                let chosen = self.stored.settings().theme == theme;
                Choice::new(("theme", theme as usize), theme.label(), Stop::Theme(theme), chosen, pick)
            })
            .collect();
        let themes = self.radio_group("themes", "テーマ", themes, window, cx);
        let zooms = ZOOMS
            .into_iter()
            .map(|zoom| {
                let pick: Pick = Rc::new(move |this, _, cx| this.set_zoom(zoom, cx));
                let chosen = self.stored.settings().zoom == zoom;
                Choice::new(("zoom", zoom as usize), format!("{zoom}%"), Stop::Zoom(zoom), chosen, pick)
            })
            .collect();
        let zooms = self.radio_group("zooms", "表示サイズ", zooms, window, cx);
        let content = div()
            .flex()
            .flex_col()
            .gap_3()
            // Taken at once as one is picked, and kept.
            .child(div().flex().flex_col().gap_1().child(label(p, "テーマ")).child(themes))
            .child(div().flex().flex_col().gap_1().child(label(p, "表示サイズ")).child(zooms))
            // Taken at once, as the theme is. The protected items hang off the hidden ones: they count only
            // while those show, so they are set only then, and keep what was chosen meanwhile.
            .child({
                let (hidden, protected) = (cx.entity().downgrade(), cx.entity().downgrade());
                let settings = self.stored.settings();
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(p, "表示"))
                    .child(Checkbox::new("show-hidden").label("隠しファイルを表示する").checked(settings.show_hidden).on_click(
                        move |_, _, cx| {
                            hidden.update(cx, |this, cx| this.toggle_hidden(cx)).ok();
                        },
                    ))
                    .child(
                        div().pl_6().child(
                            Checkbox::new("show-protected")
                                .label("保護されたシステムファイルも表示する")
                                .checked(settings.show_protected)
                                .disabled(!settings.show_hidden)
                                .on_click(move |_, _, cx| {
                                    protected.update(cx, |this, cx| this.toggle_protected(cx)).ok();
                                }),
                        ),
                    )
            })
            .child(div().pt_1().text_size(scaled(metrics::TEXT_LARGE)).font_weight(FontWeight::SEMIBOLD).child("キーボード操作"))
            // One by one: an array of them all would lie whole on the stack, past what a debug build has.
            .child(div().flex().flex_col().gap_3().text_size(scaled(metrics::TEXT_BODY)).children(SHORTCUTS.iter().map(
                |&(heading, keys)| {
                    div().flex().flex_col().gap_1().child(label(p, heading)).children(keys.iter().map(|&(key, what)| {
                        div()
                            .flex()
                            .gap_3()
                            // As wide as the longest key, so none wraps.
                            .child(div().w(scaled(KEYS_WIDTH)).flex_none().text_color(p.text).child(key))
                            // Wrapped in the room left, rather than cut at the dialog's edge.
                            .child(div().flex_1().min_w_0().text_color(p.text_muted).child(what))
                    }))
                },
            )));
        let buttons = vec![(
            "close-preferences",
            push("close-preferences", "閉じる", true, true).on_click(cx.listener(|this, _, window, cx| this.close_dialog(window, cx))),
        )];
        ("設定", content, buttons)
    }
}
