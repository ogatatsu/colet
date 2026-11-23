//! Every word the window says, in one place: the labels, the tips, the dialogs, the notices and the
//! reasons an action is off. The words said in many places sit at the top; the rest under the part of the
//! window that says them. Each is a function, of the value it takes if any, so a word is looked up as it is
//! said and not fixed when Colet is built.
//!
//! colet-core says nothing to the user: what it reports, its failures among them, is said here.
use std::fmt::Display;

use colet_core::entry::{Order, Sort};
use windows_core::HRESULT;

// ---- Said in many places ----

pub fn cancel() -> &'static str {
    "キャンセル"
}

pub fn save() -> &'static str {
    "保存"
}

pub fn close() -> &'static str {
    "閉じる"
}

pub fn open() -> &'static str {
    "開く"
}

pub fn open_in_new_tab() -> &'static str {
    "新しいタブで開く"
}

pub fn edit() -> &'static str {
    "編集"
}

pub fn delete() -> &'static str {
    "削除"
}

pub fn select() -> &'static str {
    "選択"
}

pub fn select_all() -> &'static str {
    "すべて選択"
}

pub fn none() -> &'static str {
    "なし"
}

pub fn default() -> &'static str {
    "既定"
}

pub fn name() -> &'static str {
    "名前"
}

pub fn query() -> &'static str {
    "検索文字列"
}

pub fn search_filter() -> &'static str {
    "検索フィルター"
}

pub fn narrowing() -> &'static str {
    "絞り込み"
}

pub fn sort_order() -> &'static str {
    "並び順"
}

pub fn no_match() -> &'static str {
    "一致する項目はありません"
}

pub fn home() -> &'static str {
    "ホーム"
}

pub fn search_results() -> &'static str {
    "検索結果"
}

pub fn folder() -> &'static str {
    "フォルダー"
}

pub fn levels_above() -> &'static str {
    "上の階層"
}

pub fn settings() -> &'static str {
    "設定"
}

pub fn refresh() -> &'static str {
    "更新"
}

pub fn tab() -> &'static str {
    "タブ"
}

pub fn new_tab() -> &'static str {
    "新しいタブ"
}

pub fn close_tab() -> &'static str {
    "タブを閉じる"
}

pub fn reopen_tab() -> &'static str {
    "閉じたタブを開き直す"
}

pub fn clear_search() -> &'static str {
    "検索をクリア"
}

pub fn theme() -> &'static str {
    "テーマ"
}

pub fn display_size() -> &'static str {
    "表示サイズ"
}

// The file actions, as the menus, the keys' list and the notices name them.
pub fn copy() -> &'static str {
    "コピー"
}

pub fn cut() -> &'static str {
    "切り取り"
}

pub fn paste() -> &'static str {
    "貼り付け"
}

pub fn copy_path() -> &'static str {
    "パスをコピー"
}

pub fn rename() -> &'static str {
    "名前を変更"
}

pub fn new_folder() -> &'static str {
    "新規フォルダー"
}

pub fn recycle() -> &'static str {
    "ごみ箱へ移動"
}

pub fn properties() -> &'static str {
    "プロパティ"
}

pub fn send_to() -> &'static str {
    "送る"
}

pub fn show_in_explorer() -> &'static str {
    "エクスプローラーで表示"
}

pub fn more_options() -> &'static str {
    "その他のオプションを確認"
}

pub fn add_favorite() -> &'static str {
    "お気に入りに追加"
}

pub fn remove_favorite() -> &'static str {
    "お気に入りを解除"
}

pub fn unsave_search() -> &'static str {
    "保存した検索を解除"
}

pub fn pin() -> &'static str {
    "ピン留め"
}

pub fn unpin() -> &'static str {
    "ピン留めを外す"
}

/// What joins the parts of a list said in one line.
pub fn separator() -> &'static str {
    "・"
}

/// A name as a notice quotes it.
pub fn quoted(name: impl Display) -> String {
    format!("「{name}」")
}

/// The items a list leaves unnamed.
pub fn more_items(count: usize) -> String {
    format!("ほか {count} 項目")
}

/// What a sort goes by.
pub fn sort_name(by: Sort) -> &'static str {
    match by {
        Sort::Name => name(),
        Sort::Location => "場所",
        Sort::Size => "サイズ",
        Sort::Modified => "更新日時",
    }
}

pub fn direction(descending: bool) -> &'static str {
    if descending { "降順" } else { "昇順" }
}

/// `名前・昇順`, with `・混在` when folders mix in with files.
pub fn order(order: Order) -> String {
    let mixed = if order.folders_first { "" } else { "・混在" };
    format!("{}{}{}{mixed}", sort_name(order.by), separator(), direction(order.descending))
}

/// Windows's own words for what it refused, or the code when it has none.
pub fn windows_failure(code: HRESULT) -> String {
    colet_core::operations::system_message(code).unwrap_or_else(|| format!("Windowsの操作に失敗しました (0x{:08X})", code.0 as u32))
}

// ---- The window: its buttons, and what it says as it closes ----
pub mod window {
    use super::Display;

    pub fn minimize() -> &'static str {
        "最小化"
    }

    pub fn maximize() -> &'static str {
        "最大化"
    }

    pub fn restore() -> &'static str {
        "元に戻す（縮小）"
    }

    pub fn back() -> &'static str {
        "戻る"
    }

    pub fn back_key() -> &'static str {
        "戻る（Alt+←）"
    }

    pub fn forward() -> &'static str {
        "進む"
    }

    pub fn forward_key() -> &'static str {
        "進む（Alt+→）"
    }

    pub fn refresh_key() -> &'static str {
        "更新（F5）"
    }

    /// A notice's button that opens its operation in the history.
    pub fn details() -> &'static str {
        "詳細"
    }

    pub fn busy_operating() -> &'static str {
        "ファイル操作中です。完了を待つか、進捗ダイアログでキャンセルしてから閉じてください。"
    }

    pub fn busy_sending() -> &'static str {
        "「送る」の処理中です。完了を待ってから閉じてください。"
    }

    pub fn quick_look_not_running() -> &'static str {
        "QuickLook が起動していません"
    }

    pub fn quick_look_unnamable() -> &'static str {
        "この名前は QuickLook に渡せないため表示できません"
    }

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

    /// The watch on the folders could not start at all.
    pub fn cannot_start_watching(error: &colet_core::watch::WatchError) -> String {
        format!("フォルダーの変更を監視できません: {error}。F5で更新できます。")
    }
}

// ---- The address bar ----
pub mod address {
    use super::Display;

    /// What the empty address says before and while editing.
    pub fn placeholder() -> &'static str {
        "パスを入力（Ctrl+L）"
    }

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
    pub fn placeholder() -> &'static str {
        "Everythingで検索（Ctrl+F）"
    }

    pub fn clear_key() -> &'static str {
        "検索をクリア（Esc）"
    }

    /// The filter's button, with none chosen.
    pub fn filter() -> &'static str {
        "フィルター"
    }

    /// A search a newer one overtook before it ran.
    pub fn stale() -> &'static str {
        "古い検索要求です"
    }

    pub fn hint() -> &'static str {
        "名前やEverythingの検索式で探せます"
    }

    /// What the empty search shows to try, each with what it finds.
    pub fn examples() -> &'static [(&'static str, &'static str); 3] {
        &[("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")]
    }

    pub fn filters_about() -> &'static str {
        "検索に加えるEverythingの検索式を登録し、検索欄の左のボタン（Ctrl+Shift+F）で1つ選んで使います。"
    }

    pub fn no_filters() -> &'static str {
        "フィルターはまだありません"
    }

    pub fn add() -> &'static str {
        "追加"
    }

    pub fn add_filter() -> &'static str {
        "フィルターを追加"
    }

    pub fn edit_filter() -> &'static str {
        "フィルターを編集"
    }

    pub fn edit_filters() -> &'static str {
        "フィルターを編集…"
    }

    pub fn delete_filter() -> &'static str {
        "フィルターを削除"
    }

    pub fn filter_name_placeholder() -> &'static str {
        "画像"
    }

    pub fn filter_exists() -> &'static str {
        "この名前のフィルターは既にあります"
    }

    pub fn filter_query_hint() -> &'static str {
        "Everythingの検索式で書きます（例：ext:jpg;png、dm:thisweek）。検索欄に打った語と両方に合うものを探します"
    }

    /// The filter's button, saying which is chosen.
    pub fn filter_label(chosen: Option<&str>) -> String {
        format!("検索フィルター: {}", chosen.unwrap_or(super::none()))
    }

    pub fn filter_tip(label: impl Display) -> String {
        format!("{label}（Ctrl+Shift+F）")
    }

    pub fn ask_delete_filter(name: impl Display) -> String {
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

    /// Why a search, or the watch on it, failed.
    pub fn error(error: &colet_core::everything::SearchError) -> String {
        use colet_core::everything::SearchError;
        match error {
            SearchError::NulInQuery => "入力に使用できない文字が含まれています".into(),
            SearchError::CannotConnect => "Everything 1.5に接続できません。本体が起動しているか確認して、再試行してください。".into(),
            SearchError::TooOld { needed, found } => {
                format!("Everything {needed}以降が必要です（接続先: {found}）。更新してから再試行してください。")
            }
            SearchError::NoSearchState => "検索状態を作成できません".into(),
            SearchError::Indexing => "Everythingの索引を準備中です。しばらく待って再試行してください。".into(),
            SearchError::Disconnected => "Everythingとの接続が切れました。本体が起動しているか確認して、再試行してください。".into(),
            SearchError::Failed(code) => format!("Everything検索に失敗しました (0x{code:08X})"),
            SearchError::WatchDisconnected(code) => format!("Everythingとの接続が切れました。次の検索で再接続します (0x{code:08X})"),
            SearchError::WatchStopped(code) => format!("Everythingの自動更新を停止しました。F5で一覧を更新できます (0x{code:08X})"),
        }
    }

    /// The warning of what of the size and date modified Everything leaves unindexed or not fast sorted.
    pub fn slow(slow: &colet_core::everything::SlowProperties) -> String {
        let names = |sorts: &[super::Sort], end: &str| {
            sorts.iter().map(|by| format!("{}{end}", super::sort_name(*by))).collect::<Vec<_>>().join("と")
        };
        let mut warning = String::new();
        if !slow.unindexed.is_empty() {
            warning += &format!("Everything が{}を索引していないため、検索が遅くなることがあります。", names(&slow.unindexed, ""));
        }
        if !slow.unsorted.is_empty() {
            let lead = if slow.unindexed.is_empty() { "Everything で" } else { "" };
            warning += &format!(
                "{lead}{}の高速ソートが無効なため、{}の並べ替えが遅くなることがあります。",
                names(&slow.unsorted, ""),
                names(&slow.unsorted, "順")
            );
        }
        warning + "Everything のオプションの「インデックス」で有効にしてください。"
    }
}

// ---- A column: its header, its rows and its order ----
pub mod column {
    use super::Display;

    pub fn loading() -> &'static str {
        "読み込み中…"
    }

    pub fn only_hidden() -> &'static str {
        "隠しファイルのみがあります"
    }

    pub fn empty_folder() -> &'static str {
        "このフォルダーは空です"
    }

    pub fn retry() -> &'static str {
        "再試行"
    }

    pub fn narrow_placeholder() -> &'static str {
        "絞り込む"
    }

    pub fn clear_filter() -> &'static str {
        "フィルタをクリア"
    }

    pub fn clear_filter_key() -> &'static str {
        "フィルタをクリア（Esc）"
    }

    pub fn save_search_key() -> &'static str {
        "検索を保存（Ctrl+D）"
    }

    pub fn unsave_search_key() -> &'static str {
        "保存した検索を解除（Ctrl+D）"
    }

    pub fn add_favorite_key() -> &'static str {
        "お気に入りに追加（Ctrl+D）"
    }

    pub fn remove_favorite_key() -> &'static str {
        "お気に入りを解除（Ctrl+D）"
    }

    pub fn being_cut() -> &'static str {
        "切り取り中"
    }

    pub fn being_copied() -> &'static str {
        "コピー中"
    }

    pub fn folders_first() -> &'static str {
        "フォルダを先に表示"
    }

    pub fn unsort() -> &'static str {
        "既定に戻す"
    }

    /// The strip of the folders folded away above a search result, a letter to a line.
    pub fn folded() -> &'static [&'static str; 2] {
        &["階", "層"]
    }

    pub fn closed_right() -> &'static str {
        "選択していたフォルダーがなくなったため、右側のカラムを閉じました。"
    }

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

    pub fn show_levels_above_key(away: usize) -> String {
        format!("上の階層を表示（{away}階層、Ctrl+U）")
    }

    pub fn show_levels_above(away: usize) -> String {
        format!("上の階層を表示（{away}階層）")
    }

    /// Why a folder could not be listed.
    pub fn list_error(error: &colet_core::folders::ListError) -> String {
        use colet_core::folders::ListError;
        match error {
            ListError::Stale => "古いフォルダー要求です".into(),
            ListError::NotAbsolute => super::ops::not_absolute().into(),
            ListError::Shares(error) => format!("共有フォルダーを一覧できません: {error}"),
            ListError::Open(error) => format!("フォルダーを開けません: {error}"),
            ListError::BrokenOff(error) => format!("フォルダーを最後まで読めません: {error}"),
        }
    }
}

// ---- Home ----
pub mod home {
    use super::Display;

    /// Home's strip, folded, a letter to a line.
    pub fn strip() -> &'static [&'static str; 3] {
        &["ホ", "ー", "ム"]
    }

    pub fn show() -> &'static str {
        "ホームを表示（Ctrl+0）"
    }

    pub fn empty() -> &'static str {
        "場所もお気に入りもありません"
    }

    pub fn pinned() -> &'static str {
        "ピン留め"
    }

    pub fn favorites() -> &'static str {
        "お気に入り"
    }

    pub fn saved_searches() -> &'static str {
        "保存した検索"
    }

    pub fn places() -> &'static str {
        "場所"
    }

    pub fn copy_query() -> &'static str {
        "検索文字列をコピー"
    }

    pub fn query_needed() -> &'static str {
        "検索語を入力すると保存できます"
    }

    pub fn save_where() -> &'static str {
        "「保存」は検索結果かフォルダーのカラムで使えます"
    }

    pub fn search_saved_already() -> &'static str {
        "この検索は既に保存しています"
    }

    pub fn favorite_exists() -> &'static str {
        "この絞り込みのお気に入りは既にあります"
    }

    pub fn edit_favorite() -> &'static str {
        "お気に入りを編集"
    }

    pub fn edit_search() -> &'static str {
        "保存した検索を編集"
    }

    pub fn narrow_hint() -> &'static str {
        "開いたとき、この文字で名前を絞り込みます"
    }

    pub fn favorite_name_hint() -> &'static str {
        "空欄ならフォルダー名を表示します"
    }

    pub fn search_name_hint() -> &'static str {
        "空欄なら検索文字列をそのまま表示します"
    }

    pub fn sort_by() -> &'static str {
        "並べ替え"
    }

    pub fn direction() -> &'static str {
        "向き"
    }

    pub fn folders() -> &'static str {
        "フォルダ"
    }

    pub fn folders_first() -> &'static str {
        "フォルダを先に"
    }

    pub fn mixed() -> &'static str {
        "混在"
    }

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
    pub fn file() -> &'static str {
        "ファイル操作"
    }

    pub fn undo() -> &'static str {
        "元に戻す"
    }
}

// ---- The tabs ----
pub mod tabs {
    pub fn new_tab_key() -> &'static str {
        "新しいタブ（Ctrl+T）"
    }

    pub fn close_tab_key() -> &'static str {
        "タブを閉じる（Ctrl+W）"
    }

    pub fn close_others() -> &'static str {
        "他のタブを閉じる"
    }

    pub fn close_right() -> &'static str {
        "右のタブを閉じる"
    }

    pub fn duplicate() -> &'static str {
        "複製"
    }
}

// ---- The status bar ----
pub mod status {
    use super::Display;

    pub fn cannot_search() -> &'static str {
        "検索できません"
    }

    pub fn searching() -> &'static str {
        "検索中…"
    }

    pub fn engine_unknown() -> &'static str {
        "Everything · 検索時に接続"
    }

    pub fn engine_unknown_tip() -> &'static str {
        "検索エンジン: Everything（検索すると接続します）"
    }

    pub fn engine_connecting() -> &'static str {
        "Everything · 接続中…"
    }

    pub fn engine_connecting_tip() -> &'static str {
        "検索エンジン: Everythingに接続しています"
    }

    pub fn engine_failed() -> &'static str {
        "Everything · 要確認"
    }

    pub fn engine_connected(name: impl Display) -> String {
        format!("検索エンジン: {name}（接続済み）")
    }

    /// The Everything connected to, by its version; read again on F5 only while its results go unwatched.
    pub fn connection(connection: &colet_core::everything::Connection) -> String {
        let manual = if connection.watched { "" } else { " · 手動更新（F5）" };
        format!("Everything {}{manual}", connection.version)
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

    pub fn new_folder_name() -> &'static str {
        "新しいフォルダー"
    }

    pub fn name_taken() -> &'static str {
        "同じ名前の項目があります"
    }

    pub fn create() -> &'static str {
        "作成"
    }

    pub fn checking() -> &'static str {
        "確認中…"
    }

    pub fn open_many_hint() -> &'static str {
        "それぞれ関連付けられたアプリで開きます。"
    }

    pub fn cannot_write_clipboard() -> &'static str {
        "クリップボードに書き込めません"
    }

    pub fn menu_busy() -> &'static str {
        "その他のオプションで開いた操作が終わってから使えます"
    }

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
        format!("{count} 項目を{}しました", if cut { super::cut() } else { super::copy() })
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

    pub fn not_absolute() -> &'static str {
        "絶対パスを指定してください"
    }

    /// What the history and the notices call an operation.
    pub fn title(kind: colet_core::operations::Kind) -> &'static str {
        use colet_core::operations::Kind;
        match kind {
            Kind::Paste => super::paste(),
            Kind::Recycle => super::recycle(),
            Kind::Delete => "完全に削除",
            Kind::Rename => super::rename(),
            Kind::NewFolder => "フォルダーを作成",
        }
    }

    /// Why an operation was not carried out, or not all of it.
    pub fn error(error: &colet_core::operations::OperationError) -> String {
        use colet_core::operations::OperationError;
        match error {
            OperationError::Cancelled => "操作をキャンセルしました".into(),
            OperationError::NotAbsolute => not_absolute().into(),
            OperationError::NotAFolder => "貼り付け先がフォルダーではありません".into(),
            OperationError::IntoItself => "フォルダー自身やその子孫にはコピー・移動できません".into(),
            OperationError::NoFilesOnClipboard => "クリップボードにファイルがありません".into(),
            OperationError::NothingSelected => "項目が選択されていません".into(),
            OperationError::InvalidName => "使用できない名前です".into(),
            OperationError::RenameOneAtATime => "名前の変更は1項目ずつ行ってください".into(),
            OperationError::DriveRoot => "ドライブのルートは操作できません".into(),
            OperationError::NoDestination => "作成先が見つかりません".into(),
            OperationError::Refused => "貼り付け先がクリップボードの内容を受け付けませんでした".into(),
            OperationError::ClipboardChanged => "貼り付けるまでにクリップボードの内容が変わりました。もう一度貼り付けてください。".into(),
            OperationError::PartlyFailed => "一部の項目を操作できませんでした。結果を確認してください。".into(),
            OperationError::Io(error) => error.to_string(),
            OperationError::Windows(code) => super::windows_failure(*code),
        }
    }

    /// What Windows refuses in a name, said as it is typed.
    pub fn name_problem(problem: colet_core::operations::NameProblem) -> &'static str {
        use colet_core::operations::NameProblem;
        match problem {
            NameProblem::ForbiddenCharacter => "次の文字は使えません：\\ / : * ? \" < > |",
            NameProblem::TrailingDotOrSpace => "末尾にピリオドや空白は使えません",
            NameProblem::Reserved => "Windowsで予約された名前は使えません",
        }
    }
}

// ---- Why an action is off, as its button and menu item say ----
pub mod reasons {
    use super::Display;

    pub fn needs_selection() -> &'static str {
        "項目を選択すると使えます"
    }

    pub fn needs_folder() -> &'static str {
        "フォルダーのカラムで使えます"
    }

    pub fn busy() -> &'static str {
        "ファイル操作の完了後に使えます"
    }

    pub fn not_at_home() -> &'static str {
        "ホームでは使えません"
    }

    pub fn not_at_server() -> &'static str {
        "サーバーの共有一覧では使えません"
    }

    pub fn one_item() -> &'static str {
        "1項目を選択したときに使えます"
    }

    pub fn needs_file() -> &'static str {
        "ファイルを選択したときに使えます"
    }

    pub fn same_folder() -> &'static str {
        "同じフォルダーの項目を選んだときに使えます"
    }

    pub fn no_send_targets() -> &'static str {
        "送るフォルダーに送り先がありません"
    }

    pub fn no_send_drive() -> &'static str {
        "ドライブは送れません"
    }

    pub fn no_copy_saved() -> &'static str {
        "保存した検索はコピーできません"
    }

    pub fn no_copy_drive() -> &'static str {
        "ドライブはコピーできません"
    }

    pub fn not_for_saved() -> &'static str {
        "保存した検索には使えません"
    }

    /// What a delete refused at home says to do instead.
    pub fn unsave_hint() -> &'static str {
        "保存した検索は右クリックメニューの「保存した検索を解除」で外せます。"
    }

    pub fn unfavorite_hint() -> &'static str {
        "お気に入りは右クリックメニューの「お気に入りを解除」で外せます。"
    }

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

    pub fn title() -> &'static str {
        "操作履歴"
    }

    pub fn clear() -> &'static str {
        "履歴を消去"
    }

    pub fn empty() -> &'static str {
        "まだファイル操作はありません"
    }

    pub fn running() -> &'static str {
        "処理中"
    }

    // How an item, or an operation, ended.
    pub fn done() -> &'static str {
        "完了"
    }

    pub fn destroyed() -> &'static str {
        "完全に削除"
    }

    pub fn failed() -> &'static str {
        "失敗"
    }

    pub fn skipped() -> &'static str {
        "スキップ"
    }

    pub fn cancelled() -> &'static str {
        "キャンセル"
    }

    pub fn handed() -> &'static str {
        "シェルに渡した"
    }

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
    pub fn show() -> &'static str {
        "表示"
    }

    pub fn show_hidden() -> &'static str {
        "隠しファイルを表示する"
    }

    pub fn show_protected() -> &'static str {
        "保護されたシステムファイルも表示する"
    }

    pub fn theme_system() -> &'static str {
        "システム"
    }

    pub fn theme_light() -> &'static str {
        "ライト"
    }

    pub fn theme_dark() -> &'static str {
        "ダーク"
    }
}

// ---- The keys' list ----
pub mod shortcuts {
    pub fn keys() -> &'static str {
        "キーボード操作"
    }

    pub fn keys_key() -> &'static str {
        "キーボード操作（Ctrl+/）"
    }

    /// The keys the list shows, under a heading for each kind of work.
    pub fn shortcuts() -> &'static [(&'static str, &'static [(&'static str, &'static str)]); 6] {
        &[
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
                    ("Ctrl+/", "キーボード操作を表示"),
                ],
            ),
        ]
    }
}

// ---- The files Colet keeps, and what it says when one cannot be read or written ----
pub mod kept {
    use super::Display;

    pub fn settings() -> &'static str {
        "設定"
    }

    pub fn settings_file() -> &'static str {
        "設定ファイル"
    }

    pub fn settings_fallback() -> &'static str {
        "既定の設定"
    }

    pub fn no_settings_place() -> &'static str {
        "設定の保存先がわかりません"
    }

    pub fn sorts() -> &'static str {
        "カラムの並び順"
    }

    pub fn sorts_file() -> &'static str {
        "カラムの並び順のファイル"
    }

    pub fn sorts_fallback() -> &'static str {
        "既定の並び順"
    }

    pub fn no_sorts_place() -> &'static str {
        "カラムの並び順の保存先がわかりません"
    }

    pub fn not_an_object() -> &'static str {
        "オブジェクトではありません"
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use colet_core::everything::SlowProperties;

    #[test]
    fn names_an_order_and_says_when_folders_mix_in() {
        assert_eq!(order(Order { by: Sort::Location, ..Order::default() }), "場所・昇順");
        let first = Order { by: Sort::Size, descending: true, ..Order::default() };
        assert_eq!(order(first), "サイズ・降順");
        assert_eq!(order(Order { folders_first: false, ..first }), "サイズ・降順・混在");
    }

    #[test]
    fn warns_of_what_everything_does_not_index_or_fast_sort() {
        let slow = |unindexed: &[Sort], unsorted: &[Sort]| {
            search::slow(&SlowProperties { unindexed: unindexed.to_vec(), unsorted: unsorted.to_vec() })
        };
        assert_eq!(
            slow(&[Sort::Size], &[Sort::Modified]),
            "Everything がサイズを索引していないため、検索が遅くなることがあります。更新日時の高速ソートが無効なため、更新日時順の並べ替えが遅くなることがあります。Everything のオプションの「インデックス」で有効にしてください。"
        );
        assert_eq!(
            slow(&[Sort::Size, Sort::Modified], &[]),
            "Everything がサイズと更新日時を索引していないため、検索が遅くなることがあります。Everything のオプションの「インデックス」で有効にしてください。"
        );
        assert_eq!(
            slow(&[], &[Sort::Size, Sort::Modified]),
            "Everything でサイズと更新日時の高速ソートが無効なため、サイズ順と更新日時順の並べ替えが遅くなることがあります。Everything のオプションの「インデックス」で有効にしてください。"
        );
    }

    #[test]
    fn names_the_connection_and_says_when_it_is_read_again_by_hand() {
        use colet_core::everything::Connection;
        assert_eq!(status::connection(&Connection { version: "1.5.0.1424".into(), watched: true }), "Everything 1.5.0.1424");
        assert_eq!(
            status::connection(&Connection { version: "1.5.0.1424".into(), watched: false }),
            "Everything 1.5.0.1424 · 手動更新（F5）"
        );
    }
}
