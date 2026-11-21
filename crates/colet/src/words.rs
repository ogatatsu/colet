//! Every word the window says, in one place: the labels, the tips, the dialogs, the notices and the
//! reasons an action is off. The words said in many places sit at the top; the rest under the part of the
//! window that says them. What takes a value is a function of it.
//!
//! colet-core says its own failures, in its error types; the orders' names are its too.
use std::fmt::Display;

// ---- Said in many places ----

pub const CANCEL: &str = "キャンセル";
pub const SAVE: &str = "保存";
pub const CLOSE: &str = "閉じる";
pub const OPEN: &str = "開く";
pub const OPEN_IN_NEW_TAB: &str = "新しいタブで開く";
pub const EDIT: &str = "編集";
pub const DELETE: &str = "削除";
pub const SELECT: &str = "選択";
pub const SELECT_ALL: &str = "すべて選択";
pub const NONE: &str = "なし";
pub const DEFAULT: &str = "既定";
pub const NAME: &str = "名前";
pub const QUERY: &str = "検索文字列";
pub const SEARCH_FILTER: &str = "検索フィルター";
pub const NARROWING: &str = "絞り込み";
pub const SORT_ORDER: &str = "並び順";
pub const NO_MATCH: &str = "一致する項目はありません";
pub const HOME: &str = "ホーム";
pub const SEARCH_RESULTS: &str = "検索結果";
pub const FOLDER: &str = "フォルダー";
pub const LEVELS_ABOVE: &str = "上の階層";
pub const SETTINGS: &str = "設定";
pub const REFRESH: &str = "更新";
pub const TAB: &str = "タブ";
pub const NEW_TAB: &str = "新しいタブ";
pub const CLOSE_TAB: &str = "タブを閉じる";
pub const REOPEN_TAB: &str = "閉じたタブを開き直す";
pub const CLEAR_SEARCH: &str = "検索をクリア";
pub const THEME: &str = "テーマ";
pub const DISPLAY_SIZE: &str = "表示サイズ";

// The file actions, as the menus, the keys' list and the notices name them.
pub const COPY: &str = "コピー";
pub const CUT: &str = "切り取り";
pub const PASTE: &str = "貼り付け";
pub const COPY_PATH: &str = "パスをコピー";
pub const RENAME: &str = "名前を変更";
pub const NEW_FOLDER: &str = "新規フォルダー";
pub const RECYCLE: &str = "ごみ箱へ移動";
pub const PROPERTIES: &str = "プロパティ";
pub const SEND_TO: &str = "送る";
pub const SHOW_IN_EXPLORER: &str = "エクスプローラーで表示";
pub const MORE_OPTIONS: &str = "その他のオプションを確認";
pub const ADD_FAVORITE: &str = "お気に入りに追加";
pub const REMOVE_FAVORITE: &str = "お気に入りを解除";
pub const UNSAVE_SEARCH: &str = "保存した検索を解除";
pub const PIN: &str = "ピン留め";
pub const UNPIN: &str = "ピン留めを外す";

/// What joins the parts of a list said in one line.
pub const SEPARATOR: &str = "・";

/// A name as a notice quotes it.
pub fn quoted(name: impl Display) -> String {
    format!("「{name}」")
}

/// The items a list leaves unnamed.
pub fn more_items(count: usize) -> String {
    format!("ほか {count} 項目")
}

// ---- The window: its buttons, and what it says as it closes ----
pub mod window {
    use super::Display;

    pub const MINIMIZE: &str = "最小化";
    pub const MAXIMIZE: &str = "最大化";
    pub const RESTORE: &str = "元に戻す（縮小）";
    pub const BACK: &str = "戻る";
    pub const BACK_KEY: &str = "戻る（Alt+←）";
    pub const FORWARD: &str = "進む";
    pub const FORWARD_KEY: &str = "進む（Alt+→）";
    pub const REFRESH_KEY: &str = "更新（F5）";
    /// A notice's button that opens its operation in the history.
    pub const DETAILS: &str = "詳細";
    pub const BUSY_OPERATING: &str = "ファイル操作中です。完了を待つか、進捗ダイアログでキャンセルしてから閉じてください。";
    pub const BUSY_SENDING: &str = "「送る」の処理中です。完了を待ってから閉じてください。";
    pub const QUICK_LOOK_NOT_RUNNING: &str = "QuickLook が起動していません";
    pub const QUICK_LOOK_UNNAMABLE: &str = "この名前は QuickLook に渡せないため表示できません";

    /// The settings could not be written as the window closed.
    pub fn close_without_settings(error: impl Display) -> String {
        format!("{error}。もう一度閉じると、設定を保存せずに終了します。")
    }

    pub fn quick_look_failed(error: impl Display) -> String {
        format!("QuickLook で表示できません: {error}")
    }

    /// `names`, each quoted, are listed but not watched.
    pub fn cannot_watch(names: impl Display) -> String {
        format!("{names}の変更を監視できません。F5で更新できます。")
    }
}

// ---- The address bar ----
pub mod address {
    use super::Display;

    /// What the empty address says before and while editing.
    pub const PLACEHOLDER: &str = "パスを入力（Ctrl+L）";

    pub fn not_a_path(typed: impl Display) -> String {
        format!("「{typed}」は開けません。ドライブ（C:\\…）かサーバー（\\\\server）から始まるパスを入力してください")
    }

    pub fn not_found(typed: impl Display) -> String {
        format!("「{typed}」が見つかりません")
    }
}

// ---- The search field and the search filters ----
pub mod search {
    use super::Display;

    /// What the empty search field says.
    pub const PLACEHOLDER: &str = "Everythingで検索（Ctrl+F）";
    pub const CLEAR_KEY: &str = "検索をクリア（Esc）";
    /// The filter's button, with none chosen.
    pub const FILTER: &str = "フィルター";
    /// A search a newer one overtook before it ran.
    pub const STALE: &str = "古い検索要求です";
    pub const HINT: &str = "名前やEverythingの検索式で探せます";
    /// What the empty search shows to try, each with what it finds.
    pub const EXAMPLES: [(&str, &str); 3] =
        [("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")];

    pub const FILTERS_ABOUT: &str = "検索に加えるEverythingの検索式を登録し、検索欄の左のボタン（Ctrl+Shift+F）で1つ選んで使います。";
    pub const NO_FILTERS: &str = "フィルターはまだありません";
    pub const ADD: &str = "追加";
    pub const ADD_FILTER: &str = "フィルターを追加";
    pub const EDIT_FILTER: &str = "フィルターを編集";
    pub const EDIT_FILTERS: &str = "フィルターを編集…";
    pub const DELETE_FILTER: &str = "フィルターを削除";
    pub const FILTER_NAME_PLACEHOLDER: &str = "画像";
    pub const FILTER_EXISTS: &str = "この名前のフィルターは既にあります";
    pub const FILTER_QUERY_HINT: &str =
        "Everythingの検索式で書きます（例：ext:jpg;png、dm:thisweek）。検索欄に打った語と両方に合うものを探します";

    /// The filter's button, saying which is chosen.
    pub fn filter_label(chosen: Option<&str>) -> String {
        format!("検索フィルター: {}", chosen.unwrap_or(super::NONE))
    }

    pub fn filter_tip(label: impl Display) -> String {
        format!("{label}（Ctrl+Shift+F）")
    }

    pub fn delete_filter(name: impl Display) -> String {
        format!("「{name}」を削除しますか？")
    }

    /// The saved searches that use the filter about to be deleted.
    pub fn filter_users(users: usize) -> String {
        format!("このフィルターを使う保存した検索が{users}件あります。削除すると、それらはフィルターなしで検索します。")
    }

    pub fn make_default(name: impl Display) -> String {
        format!("「{name}」を既定にする")
    }

    pub fn undefault(name: impl Display) -> String {
        format!("「{name}」を既定から外す")
    }

    pub fn move_up(name: impl Display) -> String {
        format!("「{name}」を上へ")
    }

    pub fn move_down(name: impl Display) -> String {
        format!("「{name}」を下へ")
    }

    pub fn edit_named(name: impl Display) -> String {
        format!("「{name}」を編集")
    }

    pub fn delete_named(name: impl Display) -> String {
        format!("「{name}」を削除")
    }
}

// ---- A column: its header, its rows and its order ----
pub mod column {
    use super::Display;

    pub const LOADING: &str = "読み込み中…";
    pub const ONLY_HIDDEN: &str = "隠しファイルのみがあります";
    pub const EMPTY_FOLDER: &str = "このフォルダーは空です";
    pub const RETRY: &str = "再試行";
    pub const NARROW_PLACEHOLDER: &str = "絞り込む";
    pub const CLEAR_FILTER: &str = "フィルタをクリア";
    pub const CLEAR_FILTER_KEY: &str = "フィルタをクリア（Esc）";
    pub const SAVE_SEARCH_KEY: &str = "検索を保存（Ctrl+D）";
    pub const UNSAVE_SEARCH_KEY: &str = "保存した検索を解除（Ctrl+D）";
    pub const ADD_FAVORITE_KEY: &str = "お気に入りに追加（Ctrl+D）";
    pub const REMOVE_FAVORITE_KEY: &str = "お気に入りを解除（Ctrl+D）";
    pub const BEING_CUT: &str = "切り取り中";
    pub const BEING_COPIED: &str = "コピー中";
    pub const FOLDERS_FIRST: &str = "フォルダを先に表示";
    pub const UNSORT: &str = "既定に戻す";
    /// The strip of the folders folded away above a search result, a letter to a line.
    pub const FOLDED: [&str; 2] = ["階", "層"];
    pub const CLOSED_RIGHT: &str = "選択していたフォルダーがなくなったため、右側のカラムを閉じました。";

    pub fn label(title: impl Display) -> String {
        format!("{title} カラム")
    }

    /// The order's button: the column's own order, or the default it follows.
    pub fn sort_order(order: impl Display, own: bool) -> String {
        format!("並び順: {order}{}", if own { "" } else { "（既定）" })
    }

    pub fn sorted_by(by: impl Display) -> String {
        format!("{by}で並べ替え")
    }

    pub fn sorted_by_in(by: impl Display, direction: impl Display) -> String {
        format!("{by}で並べ替え（{direction}）")
    }

    pub fn narrowed_to(narrow: impl Display) -> String {
        format!("絞り込み：{narrow}")
    }

    pub fn show_levels_above(away: usize) -> String {
        format!("上の階層を表示（{away}階層）")
    }
}

// ---- Home ----
pub mod home {
    use super::Display;

    /// Home's strip, folded, a letter to a line.
    pub const STRIP: [&str; 3] = ["ホ", "ー", "ム"];
    pub const SHOW: &str = "ホームを表示（Ctrl+0）";
    pub const EMPTY: &str = "場所もお気に入りもありません";
    pub const PINNED: &str = "ピン留め";
    pub const FAVORITES: &str = "お気に入り";
    pub const SAVED_SEARCHES: &str = "保存した検索";
    pub const PLACES: &str = "場所";
    pub const COPY_QUERY: &str = "検索文字列をコピー";
    pub const QUERY_NEEDED: &str = "検索語を入力すると保存できます";
    pub const SAVE_WHERE: &str = "「保存」は検索結果かフォルダーのカラムで使えます";
    pub const SEARCH_SAVED_ALREADY: &str = "この検索は既に保存しています";
    pub const FAVORITE_EXISTS: &str = "この絞り込みのお気に入りは既にあります";
    pub const EDIT_FAVORITE: &str = "お気に入りを編集";
    pub const EDIT_SEARCH: &str = "保存した検索を編集";
    pub const NARROW_HINT: &str = "開いたとき、この文字で名前を絞り込みます";
    pub const FAVORITE_NAME_HINT: &str = "空欄ならフォルダー名を表示します";
    pub const SEARCH_NAME_HINT: &str = "空欄なら検索文字列をそのまま表示します";
    pub const SORT_BY: &str = "並べ替え";
    pub const DIRECTION: &str = "向き";
    pub const FOLDERS: &str = "フォルダ";
    pub const FOLDERS_FIRST: &str = "フォルダを先に";
    pub const MIXED: &str = "混在";

    pub fn folder_of(path: impl Display) -> String {
        format!("フォルダー：{path}")
    }

    pub fn unfavorited(label: impl Display) -> String {
        format!("「{label}」をお気に入りから外しました")
    }

    pub fn unsaved(label: impl Display) -> String {
        format!("「{label}」を保存した検索から外しました")
    }
}

// ---- The menus ----
pub mod menu {
    pub const FILE: &str = "ファイル操作";
    pub const UNDO: &str = "元に戻す";
}

// ---- The tabs ----
pub mod tabs {
    pub const NEW_TAB_KEY: &str = "新しいタブ（Ctrl+T）";
    pub const CLOSE_TAB_KEY: &str = "タブを閉じる（Ctrl+W）";
    pub const CLOSE_OTHERS: &str = "他のタブを閉じる";
    pub const CLOSE_RIGHT: &str = "右のタブを閉じる";
    pub const DUPLICATE: &str = "複製";
}

// ---- The status bar ----
pub mod status {
    use super::Display;

    pub const CANNOT_SEARCH: &str = "検索できません";
    pub const SEARCHING: &str = "検索中…";
    pub const ENGINE_UNKNOWN: &str = "Everything · 検索時に接続";
    pub const ENGINE_UNKNOWN_TIP: &str = "検索エンジン: Everything（検索すると接続します）";
    pub const ENGINE_CONNECTING: &str = "Everything · 接続中…";
    pub const ENGINE_CONNECTING_TIP: &str = "検索エンジン: Everythingに接続しています";
    pub const ENGINE_FAILED: &str = "Everything · 要確認";

    pub fn engine_connected(name: impl Display) -> String {
        format!("検索エンジン: {name}（接続済み）")
    }

    pub fn selected(count: usize) -> String {
        format!("{count} 項目を選択")
    }

    /// The item the keys are on: what it is, and when it was last changed.
    pub fn with_time(what: impl Display, when: impl Display) -> String {
        format!("{what} · {when}")
    }
}

// ---- File operations, and the dialogs they ask in ----
pub mod ops {
    use super::Display;

    pub const NEW_FOLDER_NAME: &str = "新しいフォルダー";
    pub const NAME_TAKEN: &str = "同じ名前の項目があります";
    pub const CREATE: &str = "作成";
    pub const CHECKING: &str = "確認中…";
    pub const OPEN_MANY_HINT: &str = "それぞれ関連付けられたアプリで開きます。";
    pub const CANNOT_WRITE_CLIPBOARD: &str = "クリップボードに書き込めません";
    pub const MENU_BUSY: &str = "その他のオプションで開いた操作が終わってから使えます";

    pub fn created_in(destination: impl Display) -> String {
        format!("作成先：{destination}")
    }

    pub fn located_in(place: impl Display) -> String {
        format!("場所：{place}")
    }

    pub fn cannot_open(error: impl Display) -> String {
        format!("開けません: {error}")
    }

    pub fn cannot_open_named(name: impl Display, error: impl Display) -> String {
        format!("「{name}」を開けません: {error}")
    }

    /// Several items could not be opened: the first few, `names`, then how many more.
    pub fn cannot_open_many(count: usize, names: impl Display, more: impl Display, error: impl Display) -> String {
        format!("{count} 項目を開けません: {names}{more}（{error}）")
    }

    pub fn open_many(count: usize) -> String {
        format!("{count} 項目を開きますか？")
    }

    pub fn cannot_drag(error: impl Display) -> String {
        format!("ドラッグできません: {error}")
    }

    pub fn cannot_watch_clipboard(error: impl Display) -> String {
        format!("クリップボードを監視できません: {error}")
    }

    /// `count` items were put on the clipboard, `cut` or copied.
    pub fn put_on_clipboard(count: usize, cut: bool) -> String {
        format!("{count} 項目を{}しました", if cut { super::CUT } else { super::COPY })
    }

    pub fn paths_copied(count: usize) -> String {
        format!("{count} 項目のパスをコピーしました")
    }

    /// The Send To target did not take what was dropped on it.
    pub fn refused(name: impl Display) -> String {
        format!("「{name}」は受け付けませんでした")
    }

    pub fn cannot_send(name: impl Display, error: impl Display) -> String {
        format!("「{name}」に送れません: {error}")
    }

    /// What went wrong with an operation, named by what it was.
    pub fn problem_of(title: impl Display, problem: impl Display) -> String {
        format!("「{title}」: {problem}")
    }
}

// ---- Why an action is off, as its button and menu item say ----
pub mod reasons {
    use super::Display;

    pub const NEEDS_SELECTION: &str = "項目を選択すると使えます";
    pub const NEEDS_FOLDER: &str = "フォルダーのカラムで使えます";
    pub const BUSY: &str = "ファイル操作の完了後に使えます";
    pub const NOT_AT_HOME: &str = "ホームでは使えません";
    pub const NOT_AT_SERVER: &str = "サーバーの共有一覧では使えません";
    pub const ONE_ITEM: &str = "1項目を選択したときに使えます";
    pub const NEEDS_FILE: &str = "ファイルを選択したときに使えます";
    pub const SAME_FOLDER: &str = "同じフォルダーの項目を選んだときに使えます";
    pub const NO_SEND_TARGETS: &str = "送るフォルダーに送り先がありません";
    pub const NO_SEND_DRIVE: &str = "ドライブは送れません";
    pub const NO_COPY_SAVED: &str = "保存した検索はコピーできません";
    pub const NO_COPY_DRIVE: &str = "ドライブはコピーできません";
    pub const NOT_FOR_SAVED: &str = "保存した検索には使えません";
    /// What a delete refused at home says to do instead.
    pub const UNSAVE_HINT: &str = "保存した検索は右クリックメニューの「保存した検索を解除」で外せます。";
    pub const UNFAVORITE_HINT: &str = "お気に入りは右クリックメニューの「お気に入りを解除」で外せます。";

    /// The action `title` is off, for `reason`.
    pub fn refused(title: impl Display, reason: impl Display) -> String {
        format!("「{title}」は{reason}")
    }

    pub fn refused_with_hint(title: impl Display, reason: impl Display, hint: impl Display) -> String {
        format!("「{title}」は{reason}。{hint}")
    }
}

// ---- The history of the operations ----
pub mod history {
    use super::Display;

    pub const TITLE: &str = "操作履歴";
    pub const CLEAR: &str = "履歴を消去";
    pub const EMPTY: &str = "まだファイル操作はありません";
    pub const RUNNING: &str = "処理中";
    // How an item, or an operation, ended.
    pub const DONE: &str = "完了";
    pub const DESTROYED: &str = "完全に削除";
    pub const FAILED: &str = "失敗";
    pub const SKIPPED: &str = "スキップ";
    pub const CANCELLED: &str = "キャンセル";
    pub const HANDED: &str = "シェルに渡した";

    /// The history's button, with the operations that had problems not yet looked at.
    pub fn title_with_problems(unseen: usize) -> String {
        format!("操作履歴（問題のある操作 {unseen} 件）")
    }

    pub fn items(count: usize) -> String {
        format!("{count} 項目")
    }

    pub fn pasted(count: usize, into: impl Display) -> String {
        format!("{count} 項目 → {into}")
    }

    pub fn renamed(from: impl Display, to: impl Display) -> String {
        format!("{from} → {to}")
    }

    pub fn new_folder(name: impl Display, parent: impl Display) -> String {
        format!("{name}（{parent}）")
    }

    pub fn failed_count(count: usize) -> String {
        format!("{count} 件失敗")
    }

    pub fn skipped_count(count: usize) -> String {
        format!("{count} 件スキップ")
    }

    pub fn gone(name: impl Display) -> String {
        format!("「{name}」はもうありません")
    }
}

// ---- The preferences ----
pub mod preferences {
    pub const SHOW: &str = "表示";
    pub const SHOW_HIDDEN: &str = "隠しファイルを表示する";
    pub const SHOW_PROTECTED: &str = "保護されたシステムファイルも表示する";
    pub const KEYS: &str = "キーボード操作";
    pub const THEME_SYSTEM: &str = "システム";
    pub const THEME_LIGHT: &str = "ライト";
    pub const THEME_DARK: &str = "ダーク";

    /// The keys the preferences list, under a heading for each kind of work.
    pub const SHORTCUTS: [(&str, &[(&str, &str)]); 6] = [
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
            &[
                ("Space", "QuickLook でプレビュー"),
                ("Ctrl+H", "隠しファイルの表示を切り替え"),
                ("Ctrl+U", "上の階層を表示"),
                ("F5", "更新"),
            ],
        ),
    ];
}

// ---- The files Colet keeps, and what it says when one cannot be read or written ----
pub mod kept {
    use super::Display;

    pub const SETTINGS: &str = "設定";
    pub const SETTINGS_FILE: &str = "設定ファイル";
    pub const SETTINGS_FALLBACK: &str = "既定の設定";
    pub const NO_SETTINGS_PLACE: &str = "設定の保存先がわかりません";
    pub const SORTS: &str = "カラムの並び順";
    pub const SORTS_FILE: &str = "カラムの並び順のファイル";
    pub const SORTS_FALLBACK: &str = "既定の並び順";
    pub const NO_SORTS_PLACE: &str = "カラムの並び順の保存先がわかりません";
    pub const NOT_AN_OBJECT: &str = "オブジェクトではありません";

    /// A save that broke off before it said how it went.
    pub fn cannot_save(what: impl Display) -> String {
        format!("{what}を保存できません")
    }

    pub fn cannot_save_because(what: impl Display, error: impl Display) -> String {
        format!("{what}を保存できません: {error}")
    }

    /// The file could not be read, and is kept as it is: nothing is saved over it.
    pub fn unreadable(what: impl Display, error: impl Display, file: impl Display) -> String {
        format!("{what}を読み込めません: {error}。元の{file}を残すため、変更は保存しません。")
    }

    /// The file was broken and put aside at `aside`; Colet started on `fallback`.
    pub fn put_aside(file: impl Display, fallback: impl Display, error: impl Display, aside: impl Display) -> String {
        format!("{file}が壊れていたため、{fallback}で始めました（{error}）。元のファイルは{aside}に残しています。")
    }

    pub fn skipped(file: impl Display, count: usize) -> String {
        format!("{file}の読めない項目を{count}件飛ばしました")
    }

    /// What was skipped, and where the file was copied before the next save leaves it out.
    pub fn copied_aside(skipped: impl Display, aside: impl Display) -> String {
        format!("{skipped}。元のファイルは{aside}に残しています。")
    }

    /// What was skipped, the file kept as it is since it could not be copied aside.
    pub fn not_saved(skipped: impl Display, file: impl Display) -> String {
        format!("{skipped}。元の{file}を残すため、変更は保存しません。")
    }
}
