//! Every word the window says, in one place: the labels, the tips, the dialogs, the notices and the
//! reasons an action is off. The words said in many places sit at the top; the rest under the part of the
//! window that says them. Each is a function, of the value it takes if any, that says it in the language
//! spoken as it is asked: Japanese, with its English beside it.
//!
//! colet-core says nothing to the user: what it reports, its failures among them, is said here.
#[cfg(test)]
use std::cell::Cell;
use std::fmt::Display;
#[cfg(not(test))]
use std::sync::atomic::{AtomicU8, Ordering};

use colet_core::entry::{Order, Sort};
use windows_core::HRESULT;

use Spoken::{English, Japanese};

/// The languages Colet speaks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Spoken {
    #[default]
    Japanese,
    English,
}

impl Spoken {
    /// gpui-kit's name for it.
    #[cfg(not(test))]
    fn locale(self) -> &'static str {
        match self {
            Japanese => "ja",
            English => "en",
        }
    }
}

#[cfg(not(test))]
/// What is spoken, for every thread: the words of a search or a save made off the window's are said in it too.
static SPOKEN: AtomicU8 = AtomicU8::new(Japanese as u8);

#[cfg(test)]
thread_local! {
    /// What a test speaks, on its own thread.
    static SPOKEN_HERE: Cell<Spoken> = const { Cell::new(Japanese) };
}

/// Says everything from now on in `spoken`, gpui-kit's own words with Colet's. A test speaks it on its own
/// thread instead, so tests run side by side never change each other's words, and leaves gpui-kit's, which
/// the whole process shares, as they are.
pub fn speak(spoken: Spoken) {
    #[cfg(not(test))]
    {
        // Only the language itself goes across: nothing written before it is read after it.
        SPOKEN.store(spoken as u8, Ordering::Relaxed);
        gpui_kit::component::set_locale(spoken.locale());
    }
    #[cfg(test)]
    SPOKEN_HERE.with(|here| here.set(spoken));
}

/// The language spoken now.
pub fn spoken() -> Spoken {
    #[cfg(test)]
    return SPOKEN_HERE.with(Cell::get);
    #[cfg(not(test))]
    if SPOKEN.load(Ordering::Relaxed) == English as u8 { English } else { Japanese }
}

/// `japanese`, or `english`, as spoken now.
fn said<T: ?Sized>(japanese: &'static T, english: &'static T) -> &'static T {
    match spoken() {
        Japanese => japanese,
        English => english,
    }
}

// ---- Said in many places ----

pub fn cancel() -> &'static str {
    said("キャンセル", "Cancel")
}

pub fn save() -> &'static str {
    said("保存", "Save")
}

pub fn close() -> &'static str {
    said("閉じる", "Close")
}

pub fn open() -> &'static str {
    said("開く", "Open")
}

pub fn open_in_new_tab() -> &'static str {
    said("新しいタブで開く", "Open in new tab")
}

pub fn edit() -> &'static str {
    said("編集", "Edit")
}

// A menu's line or a button whose command opens a dialog, to ask more before it is done, ends with an
// ellipsis; the dialog's title, and a notice of why it cannot be done, name the command without one.

/// Editing a favorite or a saved search, as the menu's line that opens its dialog.
pub fn edit_command() -> &'static str {
    said("編集…", "Edit…")
}

pub fn delete() -> &'static str {
    said("削除", "Delete")
}

pub fn select() -> &'static str {
    said("選択", "Select")
}

pub fn select_all() -> &'static str {
    said("すべて選択", "Select all")
}

pub fn none() -> &'static str {
    said("なし", "None")
}

pub fn default() -> &'static str {
    said("既定", "Default")
}

pub fn name() -> &'static str {
    said("名前", "Name")
}

pub fn query() -> &'static str {
    said("検索文字列", "Search text")
}

pub fn search_filter() -> &'static str {
    said("検索フィルター", "Search filter")
}

pub fn narrowing() -> &'static str {
    said("絞り込み", "Name filter")
}

pub fn sort_order() -> &'static str {
    said("並び順", "Sort order")
}

pub fn no_match() -> &'static str {
    said("一致する項目はありません", "No items match")
}

pub fn home() -> &'static str {
    said("ホーム", "Home")
}

pub fn search_results() -> &'static str {
    said("検索結果", "Search results")
}

pub fn folder() -> &'static str {
    said("フォルダー", "Folder")
}

pub fn levels_above() -> &'static str {
    said("上の階層", "Levels above")
}

pub fn settings() -> &'static str {
    said("設定", "Settings")
}

pub fn refresh() -> &'static str {
    said("更新", "Refresh")
}

pub fn tab() -> &'static str {
    said("タブ", "Tab")
}

pub fn new_tab() -> &'static str {
    said("新しいタブ", "New tab")
}

pub fn close_tab() -> &'static str {
    said("タブを閉じる", "Close tab")
}

pub fn reopen_tab() -> &'static str {
    said("閉じたタブを開き直す", "Reopen closed tab")
}

pub fn clear_search() -> &'static str {
    said("検索をクリア", "Clear search")
}

pub fn theme() -> &'static str {
    said("テーマ", "Theme")
}

pub fn display_size() -> &'static str {
    said("表示サイズ", "Display size")
}

pub fn language() -> &'static str {
    said("言語", "Language")
}

// The file actions, as the menus, the keys' list and the notices name them, in English as File Explorer
// names them.
pub fn copy() -> &'static str {
    said("コピー", "Copy")
}

pub fn cut() -> &'static str {
    said("切り取り", "Cut")
}

pub fn paste() -> &'static str {
    said("貼り付け", "Paste")
}

pub fn copy_path() -> &'static str {
    said("パスをコピー", "Copy as path")
}

pub fn rename() -> &'static str {
    said("名前を変更", "Rename")
}

/// As the menu's line, which opens the dialog that asks the new name.
pub fn rename_command() -> &'static str {
    said("名前を変更…", "Rename…")
}

pub fn new_folder() -> &'static str {
    said("新規フォルダー", "New folder")
}

/// As the menu's line, which opens the dialog that asks the folder's name.
pub fn new_folder_command() -> &'static str {
    said("新規フォルダー…", "New folder…")
}

pub fn recycle() -> &'static str {
    said("ごみ箱へ移動", "Move to Recycle Bin")
}

pub fn properties() -> &'static str {
    said("プロパティ", "Properties")
}

pub fn send_to() -> &'static str {
    said("送る", "Send to")
}

pub fn show_in_explorer() -> &'static str {
    said("エクスプローラーで表示", "Show in File Explorer")
}

pub fn more_options() -> &'static str {
    said("その他のオプションを確認", "Show more options")
}

pub fn add_favorite() -> &'static str {
    said("お気に入りに追加", "Add to Favorites")
}

pub fn remove_favorite() -> &'static str {
    said("お気に入りを解除", "Remove from Favorites")
}

pub fn unsave_search() -> &'static str {
    said("保存した検索を解除", "Remove saved search")
}

pub fn pin() -> &'static str {
    said("ピン留め", "Pin")
}

pub fn unpin() -> &'static str {
    said("ピン留めを外す", "Unpin")
}

/// What joins the parts of a list said in one line.
pub fn separator() -> &'static str {
    said("・", " · ")
}

/// A name as a notice quotes it.
pub fn quoted(name: impl Display) -> String {
    match spoken() {
        Japanese => format!("「{name}」"),
        English => format!("“{name}”"),
    }
}

/// `names`, each quoted, in a row.
pub fn quoted_list<T: Display>(names: impl IntoIterator<Item = T>) -> String {
    let quoted = names.into_iter().map(quoted);
    match spoken() {
        Japanese => quoted.collect(),
        English => quoted.collect::<Vec<_>>().join(", "),
    }
}

/// The items a list leaves unnamed.
pub fn more_items(count: usize) -> String {
    match spoken() {
        Japanese => format!("ほか {count} 項目"),
        English => format!("and {count} more"),
    }
}

/// What a sort goes by.
pub fn sort_name(by: Sort) -> &'static str {
    match by {
        Sort::Name => name(),
        Sort::Location => said("場所", "Location"),
        Sort::Size => said("サイズ", "Size"),
        Sort::Modified => said("更新日時", "Date modified"),
    }
}

pub fn direction(descending: bool) -> &'static str {
    if descending { said("降順", "Descending") } else { said("昇順", "Ascending") }
}

/// `名前・昇順`, with `・混在` when folders mix in with files.
pub fn order(order: Order) -> String {
    let mixed = if order.folders_first { "" } else { said("・混在", " · Mixed") };
    format!("{}{}{}{mixed}", sort_name(order.by), separator(), direction(order.descending))
}

/// `count` items, as English counts them.
fn items(count: usize) -> String {
    if count == 1 { "1 item".into() } else { format!("{count} items") }
}

/// The marks a sentence may end with: 。 or a period, and a question or an exclamation in either script.
const SENTENCE_ENDS: [char; 7] = ['。', '．', '.', '？', '?', '！', '!'];

/// `text`, a sentence said on its own, as a notice, an error under a field or a column's reason for listing
/// nothing is: ended with 。, or in English a period, unless it ends so already, as Windows's own words do
/// and a notice of two sentences does. A label, a heading, a state or a reason on a menu's line is not
/// a sentence said on its own, and stays as it is.
pub fn sentence(text: impl Into<String>) -> String {
    let mut text = text.into();
    text.truncate(text.trim_end().len());
    if !text.is_empty() && !text.ends_with(SENTENCE_ENDS) {
        text.push_str(said("。", "."));
    }
    text
}

/// `text`, a sentence set inside another, before more is said after it: without the 。 or period it may
/// end with, as Windows's own words do, so the two never stand doubled (`。。`, or `。）`).
pub fn clause(text: impl Display) -> String {
    let text = text.to_string();
    text.trim_end().trim_end_matches(['。', '．', '.']).trim_end().to_owned()
}

/// Windows's own words for what it refused, or the code when it has none.
pub fn windows_failure(code: HRESULT) -> String {
    colet_core::operations::system_message(code).unwrap_or_else(|| {
        let code = code.0 as u32;
        match spoken() {
            Japanese => format!("Windowsの操作に失敗しました (0x{code:08X})"),
            English => format!("Windows could not do it (0x{code:08X})"),
        }
    })
}

// ---- The window: its buttons, and what it says as it closes ----
pub mod window {
    use super::{Display, English, Japanese, said, spoken};

    pub fn minimize() -> &'static str {
        said("最小化", "Minimize")
    }

    pub fn maximize() -> &'static str {
        said("最大化", "Maximize")
    }

    pub fn restore() -> &'static str {
        said("元に戻す（縮小）", "Restore down")
    }

    pub fn back() -> &'static str {
        said("戻る", "Back")
    }

    pub fn back_key() -> &'static str {
        said("戻る（Alt+←）", "Back (Alt+←)")
    }

    pub fn forward() -> &'static str {
        said("進む", "Forward")
    }

    pub fn forward_key() -> &'static str {
        said("進む（Alt+→）", "Forward (Alt+→)")
    }

    pub fn refresh_key() -> &'static str {
        said("更新（F5）", "Refresh (F5)")
    }

    /// A notice's button that opens its operation in the history.
    pub fn details() -> &'static str {
        said("詳細", "Details")
    }

    pub fn busy_operating() -> &'static str {
        said(
            "ファイル操作中です。完了を待つか、進捗ダイアログでキャンセルしてから閉じてください。",
            "A file operation is running. Wait for it to finish, or cancel it in its progress dialog, before closing.",
        )
    }

    pub fn busy_sending() -> &'static str {
        said("「送る」の処理中です。完了を待ってから閉じてください。", "“Send to” is still running. Wait for it to finish before closing.")
    }

    pub fn quick_look_not_running() -> &'static str {
        said("QuickLook が起動していません", "QuickLook is not running")
    }

    pub fn quick_look_unnamable() -> &'static str {
        said("この名前は QuickLook に渡せないため表示できません", "QuickLook cannot be given this name, so it cannot show it")
    }

    /// The settings could not be written as the window closed.
    pub fn close_without_settings(error: impl Display) -> String {
        let error = super::clause(error);
        match spoken() {
            Japanese => format!("{error}。もう一度閉じると、設定を保存せずに終了します。"),
            English => format!("{error}. Close again to quit without saving the settings."),
        }
    }

    pub fn quick_look_failed(error: impl Display) -> String {
        match spoken() {
            Japanese => format!("QuickLook で表示できません: {error}"),
            English => format!("QuickLook cannot show it: {error}"),
        }
    }

    /// `names`, each quoted, are listed but not watched.
    pub fn cannot_watch(names: impl Display) -> String {
        match spoken() {
            Japanese => format!("{names}の変更を監視できません。F5で更新できます。"),
            English => format!("Cannot watch {names} for changes. Press F5 to refresh."),
        }
    }

    /// The watch on the folders could not start at all.
    pub fn cannot_start_watching(error: &colet_core::watch::WatchError) -> String {
        let error = super::clause(error);
        match spoken() {
            Japanese => format!("フォルダーの変更を監視できません: {error}。F5で更新できます。"),
            English => format!("Cannot watch the folders for changes: {error}. Press F5 to refresh."),
        }
    }
}

// ---- The address bar ----
pub mod address {
    use super::{Display, English, Japanese, said, spoken};

    /// What the empty address says before and while editing.
    pub fn placeholder() -> &'static str {
        said("パスを入力（Ctrl+L）", "Enter a path (Ctrl+L)")
    }

    pub fn not_a_path(typed: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{typed}」は開けません。ドライブ（C:\\…）かサーバー（\\\\server）から始まるパスを入力してください"),
            English => format!("Cannot open “{typed}”. Enter a path that starts with a drive (C:\\…) or a server (\\\\server)"),
        }
    }

    pub fn not_found(typed: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{typed}」が見つかりません"),
            English => format!("Cannot find “{typed}”"),
        }
    }
}

// ---- The search field and the search filters ----
pub mod search {
    use super::{Display, English, Japanese, said, spoken};

    /// What the empty search field says.
    pub fn placeholder() -> &'static str {
        said("Everythingで検索（Ctrl+F）", "Search with Everything (Ctrl+F)")
    }

    pub fn clear_key() -> &'static str {
        said("検索をクリア（Esc）", "Clear search (Esc)")
    }

    /// The filter's button, with none chosen.
    pub fn filter() -> &'static str {
        said("フィルター", "Filter")
    }

    /// A search a newer one overtook before it ran.
    pub fn stale() -> &'static str {
        said("古い検索要求です", "A newer search took its place")
    }

    pub fn hint() -> &'static str {
        said("名前やEverythingの検索式で探せます", "Search by name, or with Everything's search syntax")
    }

    /// What the empty search shows to try, each with what it finds.
    pub fn examples() -> &'static [(&'static str, &'static str)] {
        said(
            &[("ext:pdf", "PDFファイル"), ("dm:today", "今日更新した項目"), ("size:>1gb", "1 GBを超えるファイル")],
            &[("ext:pdf", "PDF files"), ("dm:today", "Items modified today"), ("size:>1gb", "Files over 1 GB")],
        )
    }

    pub fn filters_about() -> &'static str {
        said(
            "検索に加えるEverythingの検索式を登録し、検索欄の左のボタン（Ctrl+Shift+F）で1つ選んで使います。",
            "Keep Everything searches to add to a search, and pick one with the button left of the search field (Ctrl+Shift+F).",
        )
    }

    pub fn no_filters() -> &'static str {
        said("フィルターはまだありません", "No filters yet")
    }

    /// The filters dialog's button that opens the form a new filter is written in.
    pub fn add() -> &'static str {
        said("追加…", "Add…")
    }

    pub fn add_filter() -> &'static str {
        said("フィルターを追加", "Add filter")
    }

    pub fn edit_filter() -> &'static str {
        said("フィルターを編集", "Edit filter")
    }

    pub fn edit_filters() -> &'static str {
        said("フィルターを編集…", "Edit filters…")
    }

    pub fn delete_filter() -> &'static str {
        said("フィルターを削除", "Delete filter")
    }

    pub fn filter_name_placeholder() -> &'static str {
        said("画像", "Pictures")
    }

    pub fn filter_exists() -> &'static str {
        said("この名前のフィルターは既にあります", "A filter with this name already exists")
    }

    pub fn filter_query_hint() -> &'static str {
        said(
            "Everythingの検索式で書きます（例：ext:jpg;png、dm:thisweek）。検索欄に打った語と両方に合うものを探します",
            "Written in Everything's search syntax (e.g. ext:jpg;png, dm:thisweek). Finds what matches both it and what is typed in the search field",
        )
    }

    /// The filter's button, saying which is chosen.
    pub fn filter_label(chosen: Option<&str>) -> String {
        let chosen = chosen.unwrap_or(super::none());
        match spoken() {
            Japanese => format!("検索フィルター: {chosen}"),
            English => format!("Search filter: {chosen}"),
        }
    }

    pub fn filter_tip(label: impl Display) -> String {
        match spoken() {
            Japanese => format!("{label}（Ctrl+Shift+F）"),
            English => format!("{label} (Ctrl+Shift+F)"),
        }
    }

    pub fn ask_delete_filter(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を削除しますか？"),
            English => format!("Delete “{name}”?"),
        }
    }

    /// The saved searches that use the filter about to be deleted.
    pub fn filter_users(users: usize) -> String {
        match spoken() {
            Japanese => format!("このフィルターを使う保存した検索が{users}件あります。削除すると、それらはフィルターなしで検索します。"),
            English if users == 1 => "1 saved search uses this filter. Once it is deleted, that search goes without a filter.".into(),
            English => format!("{users} saved searches use this filter. Once it is deleted, they search without a filter."),
        }
    }

    pub fn make_default(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を既定にする"),
            English => format!("Make “{name}” the default"),
        }
    }

    pub fn undefault(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を既定から外す"),
            English => format!("Stop making “{name}” the default"),
        }
    }

    pub fn move_up(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を上へ"),
            English => format!("Move “{name}” up"),
        }
    }

    pub fn move_down(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を下へ"),
            English => format!("Move “{name}” down"),
        }
    }

    pub fn edit_named(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を編集"),
            English => format!("Edit “{name}”"),
        }
    }

    pub fn delete_named(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を削除"),
            English => format!("Delete “{name}”"),
        }
    }

    /// Why a search, or the watch on it, failed.
    pub fn error(error: &colet_core::everything::SearchError) -> String {
        use colet_core::everything::SearchError;
        match error {
            SearchError::NulInQuery => said("入力に使用できない文字が含まれています", "The search has a character it cannot use").into(),
            SearchError::CannotConnect => said(
                "Everything 1.5に接続できません。本体が起動しているか確認して、再試行してください。",
                "Cannot connect to Everything 1.5. Check that it is running, and try again.",
            )
            .into(),
            SearchError::TooOld { needed, found } => match spoken() {
                Japanese => format!("Everything {needed}以降が必要です（接続先: {found}）。更新してから再試行してください。"),
                English => format!("Everything {needed} or later is needed (connected to {found}). Update it, and try again."),
            },
            SearchError::NoSearchState => said("検索状態を作成できません", "Cannot set up a search").into(),
            SearchError::Indexing => said(
                "Everythingの索引を準備中です。しばらく待って再試行してください。",
                "Everything is still building its index. Wait a moment, and try again.",
            )
            .into(),
            SearchError::Disconnected => said(
                "Everythingとの接続が切れました。本体が起動しているか確認して、再試行してください。",
                "Lost the connection to Everything. Check that it is running, and try again.",
            )
            .into(),
            SearchError::Failed(code) => match spoken() {
                Japanese => format!("Everything検索に失敗しました (0x{code:08X})"),
                English => format!("The Everything search failed (0x{code:08X})"),
            },
            SearchError::WatchDisconnected(code) => match spoken() {
                Japanese => format!("Everythingとの接続が切れました。次の検索で再接続します (0x{code:08X})"),
                English => format!("Lost the connection to Everything. The next search connects again (0x{code:08X})"),
            },
            SearchError::WatchStopped(code) => match spoken() {
                Japanese => format!("Everythingの自動更新を停止しました。F5で一覧を更新できます (0x{code:08X})"),
                English => format!("Stopped following Everything's changes. Press F5 to refresh the list (0x{code:08X})"),
            },
        }
    }

    /// The warning of what of the size and date modified Everything leaves unindexed or not fast sorted.
    pub fn slow(slow: &colet_core::everything::SlowProperties) -> String {
        match spoken() {
            Japanese => {
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
            English => {
                let names =
                    |sorts: &[super::Sort]| sorts.iter().map(|by| super::sort_name(*by).to_lowercase()).collect::<Vec<_>>().join(" and ");
                let mut warning = String::new();
                if !slow.unindexed.is_empty() {
                    warning += &format!("Everything does not index {}, so searches may be slow. ", names(&slow.unindexed));
                }
                if !slow.unsorted.is_empty() {
                    let names = names(&slow.unsorted);
                    warning += &format!("Fast sorting by {names} is off in Everything, so sorting by {names} may be slow. ");
                }
                warning + "Turn them on under Indexes in Everything's options."
            }
        }
    }
}

// ---- A column: its header, its rows and its order ----
pub mod column {
    use super::{Display, English, Japanese, said, spoken};

    pub fn loading() -> &'static str {
        said("読み込み中…", "Loading…")
    }

    pub fn only_hidden() -> &'static str {
        said("隠しファイルのみがあります", "Only hidden items are here")
    }

    pub fn empty_folder() -> &'static str {
        said("このフォルダーは空です", "This folder is empty")
    }

    pub fn retry() -> &'static str {
        said("再試行", "Try again")
    }

    pub fn narrow_placeholder() -> &'static str {
        said("絞り込む", "Filter")
    }

    pub fn clear_filter() -> &'static str {
        said("フィルタをクリア", "Clear filter")
    }

    pub fn clear_filter_key() -> &'static str {
        said("フィルタをクリア（Esc）", "Clear filter (Esc)")
    }

    pub fn save_search_key() -> &'static str {
        said("検索を保存（Ctrl+D）", "Save search (Ctrl+D)")
    }

    pub fn unsave_search_key() -> &'static str {
        said("保存した検索を解除（Ctrl+D）", "Remove saved search (Ctrl+D)")
    }

    pub fn add_favorite_key() -> &'static str {
        said("お気に入りに追加（Ctrl+D）", "Add to Favorites (Ctrl+D)")
    }

    pub fn remove_favorite_key() -> &'static str {
        said("お気に入りを解除（Ctrl+D）", "Remove from Favorites (Ctrl+D)")
    }

    pub fn being_cut() -> &'static str {
        said("切り取り中", "Cut")
    }

    pub fn being_copied() -> &'static str {
        said("コピー中", "Copied")
    }

    pub fn folders_first() -> &'static str {
        said("フォルダーを先に表示", "Show folders first")
    }

    pub fn unsort() -> &'static str {
        said("既定に戻す", "Back to default")
    }

    /// The strip of the folders folded away above a search result, after how many: a letter to a line.
    pub fn folded() -> &'static [&'static str] {
        said(&["階", "層"], &["U", "P"])
    }

    pub fn closed_right() -> &'static str {
        said(
            "選択していたフォルダーがなくなったため、右側のカラムを閉じました。",
            "The folder selected is gone, so the columns to its right were closed.",
        )
    }

    pub fn label(title: impl Display) -> String {
        match spoken() {
            Japanese => format!("{title} カラム"),
            English => format!("{title} column"),
        }
    }

    /// The order's button: the column's own order, or the default it follows.
    pub fn sort_order(order: impl Display, own: bool) -> String {
        match spoken() {
            Japanese => format!("並び順: {order}{}", if own { "" } else { "（既定）" }),
            English => format!("Sort order: {order}{}", if own { "" } else { " (default)" }),
        }
    }

    pub fn sorted_by(by: impl Display) -> String {
        match spoken() {
            Japanese => format!("{by}で並べ替え"),
            English => format!("Sort by {by}"),
        }
    }

    pub fn sorted_by_in(by: impl Display, direction: impl Display) -> String {
        match spoken() {
            Japanese => format!("{by}で並べ替え（{direction}）"),
            English => format!("Sort by {by} ({direction})"),
        }
    }

    pub fn narrowed_to(narrow: impl Display) -> String {
        match spoken() {
            Japanese => format!("絞り込み：{narrow}"),
            English => format!("Filtered: {narrow}"),
        }
    }

    pub fn show_levels_above_key(away: usize) -> String {
        match spoken() {
            Japanese => format!("上の階層を表示（{away}階層、Ctrl+U）"),
            English => format!("Show the levels above ({away} up, Ctrl+U)"),
        }
    }

    pub fn show_levels_above(away: usize) -> String {
        match spoken() {
            Japanese => format!("上の階層を表示（{away}階層）"),
            English => format!("Show the levels above ({away} up)"),
        }
    }

    /// Why a folder could not be listed.
    pub fn list_error(error: &colet_core::folders::ListError) -> String {
        use colet_core::folders::ListError;
        match error {
            ListError::Stale => said("古いフォルダー要求です", "A newer listing took its place").into(),
            ListError::NotAbsolute => super::ops::not_absolute().into(),
            ListError::Shares(error) => match spoken() {
                Japanese => format!("共有フォルダーを一覧できません: {error}"),
                English => format!("Cannot list the shared folders: {error}"),
            },
            ListError::Open(error) => match spoken() {
                Japanese => format!("フォルダーを開けません: {error}"),
                English => format!("Cannot open the folder: {error}"),
            },
            ListError::BrokenOff(error) => match spoken() {
                Japanese => format!("フォルダーを最後まで読めません: {error}"),
                English => format!("Cannot read the folder to its end: {error}"),
            },
        }
    }
}

// ---- Home ----
pub mod home {
    use super::{Display, English, Japanese, said, spoken};

    /// Home's strip, folded, a letter to a line.
    pub fn strip() -> &'static [&'static str] {
        said(&["ホ", "ー", "ム"], &["H", "O", "M", "E"])
    }

    pub fn show() -> &'static str {
        said("ホームを表示（Ctrl+0）", "Show Home (Ctrl+0)")
    }

    pub fn empty() -> &'static str {
        said("場所もお気に入りもありません", "No places or favorites")
    }

    pub fn pinned() -> &'static str {
        said("ピン留め", "Pinned")
    }

    pub fn favorites() -> &'static str {
        said("お気に入り", "Favorites")
    }

    pub fn saved_searches() -> &'static str {
        said("保存した検索", "Saved searches")
    }

    pub fn places() -> &'static str {
        said("場所", "Places")
    }

    pub fn copy_query() -> &'static str {
        said("検索文字列をコピー", "Copy search text")
    }

    pub fn query_needed() -> &'static str {
        said("検索語を入力すると保存できます", "Type a search to save it")
    }

    pub fn save_where() -> &'static str {
        said("「保存」は検索結果かフォルダーのカラムで使えます", "“Save” works in the search results or a folder's column")
    }

    pub fn search_saved_already() -> &'static str {
        said("この検索は既に保存しています", "This search is already saved")
    }

    pub fn favorite_exists() -> &'static str {
        said("この絞り込みのお気に入りは既にあります", "A favorite with this filter already exists")
    }

    pub fn edit_favorite() -> &'static str {
        said("お気に入りを編集", "Edit favorite")
    }

    pub fn edit_search() -> &'static str {
        said("保存した検索を編集", "Edit saved search")
    }

    pub fn narrow_hint() -> &'static str {
        said("開いたとき、この文字で名前を絞り込みます", "Opened, it shows only the names with this text")
    }

    pub fn favorite_name_hint() -> &'static str {
        said("空欄ならフォルダー名を表示します", "Left empty, it shows the folder's name")
    }

    pub fn search_name_hint() -> &'static str {
        said("空欄なら検索文字列をそのまま表示します", "Left empty, it shows the search text")
    }

    pub fn sort_by() -> &'static str {
        said("並べ替え", "Sort by")
    }

    pub fn direction() -> &'static str {
        said("向き", "Direction")
    }

    pub fn folders() -> &'static str {
        said("フォルダー", "Folders")
    }

    pub fn folders_first() -> &'static str {
        said("フォルダーを先に", "Folders first")
    }

    pub fn mixed() -> &'static str {
        said("混在", "Mixed")
    }

    pub fn folder_of(path: impl Display) -> String {
        match spoken() {
            Japanese => format!("フォルダー：{path}"),
            English => format!("Folder: {path}"),
        }
    }

    pub fn unfavorited(label: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{label}」をお気に入りから外しました"),
            English => format!("Removed “{label}” from Favorites"),
        }
    }

    pub fn unsaved(label: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{label}」を保存した検索から外しました"),
            English => format!("Removed “{label}” from the saved searches"),
        }
    }
}

// ---- The menus ----
pub mod menu {
    use super::said;

    pub fn file() -> &'static str {
        said("ファイル操作", "File")
    }

    pub fn undo() -> &'static str {
        said("元に戻す", "Undo")
    }
}

// ---- The tabs ----
pub mod tabs {
    use super::said;

    pub fn new_tab_key() -> &'static str {
        said("新しいタブ（Ctrl+T）", "New tab (Ctrl+T)")
    }

    pub fn close_tab_key() -> &'static str {
        said("タブを閉じる（Ctrl+W）", "Close tab (Ctrl+W)")
    }

    pub fn close_others() -> &'static str {
        said("他のタブを閉じる", "Close other tabs")
    }

    pub fn close_right() -> &'static str {
        said("右のタブを閉じる", "Close tabs to the right")
    }

    pub fn duplicate() -> &'static str {
        said("複製", "Duplicate")
    }
}

// ---- The status bar ----
pub mod status {
    use super::{Display, English, Japanese, said, spoken};

    pub fn cannot_search() -> &'static str {
        said("検索できません", "Cannot search")
    }

    pub fn searching() -> &'static str {
        said("検索中…", "Searching…")
    }

    pub fn engine_unknown() -> &'static str {
        said("Everything · 検索時に接続", "Everything · Connects on search")
    }

    pub fn engine_unknown_tip() -> &'static str {
        said("検索エンジン: Everything（検索すると接続します）", "Search engine: Everything (connects when you search)")
    }

    pub fn engine_connecting() -> &'static str {
        said("Everything · 接続中…", "Everything · Connecting…")
    }

    pub fn engine_connecting_tip() -> &'static str {
        said("検索エンジン: Everythingに接続しています", "Search engine: connecting to Everything")
    }

    pub fn engine_failed() -> &'static str {
        said("Everything · 要確認", "Everything · Needs a look")
    }

    pub fn engine_connected(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("検索エンジン: {name}（接続済み）"),
            English => format!("Search engine: {name} (connected)"),
        }
    }

    /// The Everything connected to, by its version; read again on F5 only while its results go unwatched.
    pub fn connection(connection: &colet_core::everything::Connection) -> String {
        let manual = if connection.watched { "" } else { said(" · 手動更新（F5）", " · Refresh by hand (F5)") };
        format!("Everything {}{manual}", connection.version)
    }

    pub fn selected(count: usize) -> String {
        match spoken() {
            Japanese => format!("{count} 項目を選択"),
            English if count == 1 => "1 item selected".into(),
            English => format!("{count} items selected"),
        }
    }

    /// The item the keys are on: what it is, and when it was last changed.
    pub fn with_time(what: impl Display, when: impl Display) -> String {
        format!("{what} · {when}")
    }
}

// ---- File operations, and the dialogs they ask in ----
pub mod ops {
    use super::{Display, English, Japanese, said, spoken};

    pub fn new_folder_name() -> &'static str {
        said("新しいフォルダー", "New folder")
    }

    pub fn name_taken() -> &'static str {
        said("同じ名前の項目があります", "An item with this name already exists")
    }

    pub fn create() -> &'static str {
        said("作成", "Create")
    }

    pub fn checking() -> &'static str {
        said("確認中…", "Checking…")
    }

    pub fn open_many_hint() -> &'static str {
        said("それぞれ関連付けられたアプリで開きます。", "Each opens in the app it is associated with.")
    }

    pub fn cannot_write_clipboard() -> &'static str {
        said("クリップボードに書き込めません", "Cannot write to the clipboard")
    }

    pub fn menu_busy() -> &'static str {
        said("その他のオプションで開いた操作が終わってから使えます", "Available once what was started from Show more options is done")
    }

    pub fn created_in(destination: impl Display) -> String {
        match spoken() {
            Japanese => format!("作成先：{destination}"),
            English => format!("Created in: {destination}"),
        }
    }

    pub fn located_in(place: impl Display) -> String {
        match spoken() {
            Japanese => format!("場所：{place}"),
            English => format!("Location: {place}"),
        }
    }

    pub fn cannot_open(error: impl Display) -> String {
        match spoken() {
            Japanese => format!("開けません: {error}"),
            English => format!("Cannot open it: {error}"),
        }
    }

    pub fn cannot_open_named(name: impl Display, error: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」を開けません: {error}"),
            English => format!("Cannot open “{name}”: {error}"),
        }
    }

    /// Several items could not be opened: the first few, `names`, then how many more.
    pub fn cannot_open_many(count: usize, names: impl Display, more: impl Display, error: impl Display) -> String {
        let error = super::clause(error);
        match spoken() {
            Japanese => format!("{count} 項目を開けません: {names}{more}（{error}）"),
            English => {
                let more = more.to_string();
                let more = if more.is_empty() { more } else { format!(" {more}") };
                format!("Cannot open {}: {names}{more} ({error})", super::items(count))
            }
        }
    }

    pub fn open_many(count: usize) -> String {
        match spoken() {
            Japanese => format!("{count} 項目を開きますか？"),
            English => format!("Open {}?", super::items(count)),
        }
    }

    pub fn cannot_drag(error: impl Display) -> String {
        match spoken() {
            Japanese => format!("ドラッグできません: {error}"),
            English => format!("Cannot drag: {error}"),
        }
    }

    pub fn cannot_watch_clipboard(error: impl Display) -> String {
        match spoken() {
            Japanese => format!("クリップボードを監視できません: {error}"),
            English => format!("Cannot watch the clipboard: {error}"),
        }
    }

    /// `count` items were put on the clipboard, `cut` or copied.
    pub fn put_on_clipboard(count: usize, cut: bool) -> String {
        match spoken() {
            Japanese => format!("{count} 項目を{}しました", if cut { super::cut() } else { super::copy() }),
            English => format!("{} {}", if cut { "Cut" } else { "Copied" }, super::items(count)),
        }
    }

    pub fn paths_copied(count: usize) -> String {
        match spoken() {
            Japanese => format!("{count} 項目のパスをコピーしました"),
            English if count == 1 => "Copied the path of 1 item".into(),
            English => format!("Copied the paths of {count} items"),
        }
    }

    /// The Send To target did not take what was dropped on it.
    pub fn refused(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」は受け付けませんでした"),
            English => format!("“{name}” did not take it"),
        }
    }

    pub fn cannot_send(name: impl Display, error: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」に送れません: {error}"),
            English => format!("Cannot send to “{name}”: {error}"),
        }
    }

    /// What went wrong with an operation, named by what it was.
    pub fn problem_of(title: impl Display, problem: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{title}」: {problem}"),
            English => format!("“{title}”: {problem}"),
        }
    }

    pub fn not_absolute() -> &'static str {
        said("絶対パスを指定してください", "Give a full path")
    }

    /// What the history and the notices call an operation.
    pub fn title(kind: colet_core::operations::Kind) -> &'static str {
        use colet_core::operations::Kind;
        match kind {
            Kind::Paste => super::paste(),
            Kind::Recycle => super::recycle(),
            Kind::Delete => said("完全に削除", "Delete permanently"),
            Kind::Rename => super::rename(),
            Kind::NewFolder => said("フォルダーを作成", "Create folder"),
        }
    }

    /// Why an operation was not carried out, or not all of it.
    pub fn error(error: &colet_core::operations::OperationError) -> String {
        use colet_core::operations::OperationError;
        match error {
            OperationError::Cancelled => said("操作をキャンセルしました", "Cancelled").into(),
            OperationError::NotAbsolute => not_absolute().into(),
            OperationError::NotAFolder => said("貼り付け先がフォルダーではありません", "Where it was pasted is not a folder").into(),
            OperationError::IntoItself => said(
                "フォルダー自身やその子孫にはコピー・移動できません",
                "A folder cannot be copied or moved into itself or a folder inside it",
            )
            .into(),
            OperationError::NoFilesOnClipboard => {
                said("クリップボードにファイルがありません", "There are no files on the clipboard").into()
            }
            OperationError::NothingSelected => said("項目が選択されていません", "Nothing is selected").into(),
            OperationError::InvalidName => said("使用できない名前です", "This name cannot be used").into(),
            OperationError::RenameOneAtATime => said("名前の変更は1項目ずつ行ってください", "Rename one item at a time").into(),
            OperationError::DriveRoot => said("ドライブのルートは操作できません", "A drive itself cannot be changed").into(),
            OperationError::NoDestination => said("作成先が見つかりません", "Cannot find where to create it").into(),
            OperationError::Refused => {
                said("貼り付け先がクリップボードの内容を受け付けませんでした", "Where it was pasted did not take what is on the clipboard")
                    .into()
            }
            OperationError::ClipboardChanged => said(
                "貼り付けるまでにクリップボードの内容が変わりました。もう一度貼り付けてください。",
                "The clipboard changed before the paste. Paste again.",
            )
            .into(),
            OperationError::PartlyFailed => said(
                "一部の項目を操作できませんでした。結果を確認してください。",
                "Some items could not be done. Look at what happened to each.",
            )
            .into(),
            OperationError::Io(error) => error.to_string(),
            OperationError::Windows(code) => super::windows_failure(*code),
        }
    }

    /// What Windows refuses in a name, said as it is typed.
    pub fn name_problem(problem: colet_core::operations::NameProblem) -> &'static str {
        use colet_core::operations::NameProblem;
        match problem {
            // The characters in brackets, so the 。 or period that ends the sentence is not read as one more.
            NameProblem::ForbiddenCharacter => {
                said("次の文字は使えません（\\ / : * ? \" < > |）", "A name cannot have any of these characters (\\ / : * ? \" < > |)")
            }
            NameProblem::TrailingDotOrSpace => said("末尾にピリオドや空白は使えません", "A name cannot end in a period or a space"),
            NameProblem::Reserved => said("Windowsで予約された名前は使えません", "This name is reserved by Windows"),
        }
    }
}

// ---- Why an action is off, as its button and menu item say ----
pub mod reasons {
    use super::{Display, English, Japanese, said, spoken};

    pub fn needs_selection() -> &'static str {
        said("項目を選択すると使えます", "Available once items are selected")
    }

    pub fn needs_folder() -> &'static str {
        said("フォルダーのカラムで使えます", "Available in a folder's column")
    }

    pub fn busy() -> &'static str {
        said("ファイル操作の完了後に使えます", "Available once the file operation is done")
    }

    pub fn not_at_home() -> &'static str {
        said("ホームでは使えません", "Not available in Home")
    }

    pub fn not_at_server() -> &'static str {
        said("サーバーの共有一覧では使えません", "Not available in a server's shares")
    }

    pub fn one_item() -> &'static str {
        said("1項目を選択したときに使えます", "Available when one item is selected")
    }

    pub fn needs_file() -> &'static str {
        said("ファイルを選択したときに使えます", "Available when a file is selected")
    }

    pub fn same_folder() -> &'static str {
        said("同じフォルダーの項目を選んだときに使えます", "Available when the items are in one folder")
    }

    pub fn no_send_targets() -> &'static str {
        said("送るフォルダーに送り先がありません", "The Send To folder has nothing to send to")
    }

    pub fn no_send_drive() -> &'static str {
        said("ドライブは送れません", "A drive cannot be sent")
    }

    pub fn no_copy_saved() -> &'static str {
        said("保存した検索はコピーできません", "A saved search cannot be copied")
    }

    pub fn no_copy_drive() -> &'static str {
        said("ドライブはコピーできません", "A drive cannot be copied")
    }

    pub fn not_for_saved() -> &'static str {
        said("保存した検索には使えません", "Not available for a saved search")
    }

    /// What a delete refused at home says to do instead.
    pub fn unsave_hint() -> &'static str {
        said(
            "保存した検索は右クリックメニューの「保存した検索を解除」で外せます。",
            "To remove a saved search, use “Remove saved search” in its right-click menu.",
        )
    }

    pub fn unfavorite_hint() -> &'static str {
        said(
            "お気に入りは右クリックメニューの「お気に入りを解除」で外せます。",
            "To remove a favorite, use “Remove from Favorites” in its right-click menu.",
        )
    }

    /// The action `title` is off, for `reason`.
    pub fn refused(title: impl Display, reason: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{title}」は{reason}"),
            English => format!("“{title}”: {reason}"),
        }
    }

    pub fn refused_with_hint(title: impl Display, reason: impl Display, hint: impl Display) -> String {
        let reason = super::clause(reason);
        match spoken() {
            Japanese => format!("「{title}」は{reason}。{hint}"),
            English => format!("“{title}”: {reason}. {hint}"),
        }
    }
}

// ---- The history of the operations ----
pub mod history {
    use super::{Display, English, Japanese, said, spoken};

    pub fn title() -> &'static str {
        said("操作履歴", "History")
    }

    pub fn clear() -> &'static str {
        said("履歴を消去", "Clear history")
    }

    pub fn empty() -> &'static str {
        said("まだファイル操作はありません", "No file operations yet")
    }

    pub fn running() -> &'static str {
        said("処理中", "Running")
    }

    // How an item, or an operation, ended.
    pub fn done() -> &'static str {
        said("完了", "Done")
    }

    pub fn destroyed() -> &'static str {
        said("完全に削除", "Deleted permanently")
    }

    pub fn failed() -> &'static str {
        said("失敗", "Failed")
    }

    pub fn skipped() -> &'static str {
        said("スキップ", "Skipped")
    }

    pub fn cancelled() -> &'static str {
        said("キャンセル", "Cancelled")
    }

    pub fn handed() -> &'static str {
        said("シェルに渡した", "Handed to the shell")
    }

    /// The history's button, with the operations that had problems not yet looked at.
    pub fn title_with_problems(unseen: usize) -> String {
        match spoken() {
            Japanese => format!("操作履歴（問題のある操作 {unseen} 件）"),
            English if unseen == 1 => "History (1 operation with problems)".into(),
            English => format!("History ({unseen} operations with problems)"),
        }
    }

    pub fn items(count: usize) -> String {
        match spoken() {
            Japanese => format!("{count} 項目"),
            English => super::items(count),
        }
    }

    pub fn pasted(count: usize, into: impl Display) -> String {
        format!("{} → {into}", items(count))
    }

    pub fn renamed(from: impl Display, to: impl Display) -> String {
        format!("{from} → {to}")
    }

    pub fn new_folder(name: impl Display, parent: impl Display) -> String {
        match spoken() {
            Japanese => format!("{name}（{parent}）"),
            English => format!("{name} ({parent})"),
        }
    }

    pub fn failed_count(count: usize) -> String {
        match spoken() {
            Japanese => format!("{count} 件失敗"),
            English => format!("{count} failed"),
        }
    }

    pub fn skipped_count(count: usize) -> String {
        match spoken() {
            Japanese => format!("{count} 件スキップ"),
            English => format!("{count} skipped"),
        }
    }

    pub fn gone(name: impl Display) -> String {
        match spoken() {
            Japanese => format!("「{name}」はもうありません"),
            English => format!("“{name}” is no longer there"),
        }
    }
}

// ---- The preferences ----
pub mod preferences {
    use super::said;

    /// The heading over which items show, as File Explorer's options name it: not a bare "show", which
    /// reads as the display size above it.
    pub fn files() -> &'static str {
        said("ファイルとフォルダー", "Files and folders")
    }

    pub fn show_hidden() -> &'static str {
        said("隠しファイルを表示する", "Show hidden items")
    }

    pub fn show_protected() -> &'static str {
        said("保護されたシステムファイルも表示する", "Show protected operating system files too")
    }

    /// The theme or the language Windows has.
    pub fn system() -> &'static str {
        said("システム", "System")
    }

    pub fn theme_light() -> &'static str {
        said("ライト", "Light")
    }

    pub fn theme_dark() -> &'static str {
        said("ダーク", "Dark")
    }
}

// ---- The keys' list ----
pub mod shortcuts {
    use super::said;

    pub fn keys() -> &'static str {
        said("キーボード操作", "Keyboard shortcuts")
    }

    pub fn keys_key() -> &'static str {
        said("キーボード操作（Ctrl+/）", "Keyboard shortcuts (Ctrl+/)")
    }

    /// The keys the list shows, under a heading for each kind of work.
    pub fn shortcuts() -> &'static [(&'static str, &'static [(&'static str, &'static str)])] {
        said(
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
            ],
            &[
                (
                    "Moving",
                    &[
                        ("← →", "Move between columns"),
                        ("PageUp / PageDown", "Move a screen at a time"),
                        ("Home / End", "Move to the first / last item"),
                        ("Enter", "Open"),
                        ("Alt+← / Alt+→", "Back / Forward"),
                        ("Mouse back / forward buttons", "Back / Forward"),
                        ("Ctrl+0", "Go to Home"),
                        ("Ctrl+L", "Type a path to go to (Alt+Enter opens it in a new tab)"),
                    ],
                ),
                (
                    "Selecting",
                    &[
                        ("↑ ↓", "Select an item (Shift selects a range)"),
                        ("Ctrl+click / Shift+click", "Select several"),
                        ("Ctrl+↑↓ / Ctrl+Space", "Select items apart"),
                        ("Ctrl+Shift+↑↓ / Ctrl+Shift+click", "Add a range to the selection"),
                        ("Ctrl+A", "Select all the items loaded"),
                    ],
                ),
                (
                    "Searching and filtering",
                    &[
                        ("Ctrl+F", "Search"),
                        ("↓ / Enter", "Move from the search field to the results"),
                        ("Esc", "Clear search"),
                        ("Ctrl+Shift+F", "Pick a search filter"),
                        ("Ctrl+D", "Save search / Add folder to Favorites"),
                        ("Letter keys", "Filter a column by name (Backspace erases a letter, Esc clears)"),
                    ],
                ),
                (
                    "Tabs",
                    &[
                        ("Ctrl+T", "New tab"),
                        ("Ctrl+Enter", "Open in new tab"),
                        ("Ctrl+W", "Close tab"),
                        ("Ctrl+Tab / Ctrl+Shift+Tab", "Next / Previous tab"),
                        ("Ctrl+Shift+T", "Reopen closed tab"),
                    ],
                ),
                (
                    "File operations",
                    &[
                        ("Ctrl+C / X / V", "Copy / Cut / Paste"),
                        ("Ctrl+Shift+C", "Copy as path"),
                        ("Ctrl+Shift+N", "New folder"),
                        ("F2", "Rename (edit a saved search)"),
                        ("Delete", "Move to Recycle Bin"),
                        ("Shift+Delete", "Delete permanently"),
                        ("Alt+Enter", "Properties"),
                        ("Shift+F10 / Menu key", "Open the menu"),
                    ],
                ),
                (
                    "Preview and view",
                    &[
                        ("Space", "Preview with QuickLook"),
                        ("Ctrl+H", "Show or hide hidden items"),
                        ("Ctrl+U", "Show the levels above"),
                        ("F5", "Refresh"),
                        ("Ctrl+/", "Show keyboard shortcuts"),
                    ],
                ),
            ],
        )
    }
}

// ---- The files Colet keeps, and what it says when one cannot be read or written ----
pub mod kept {
    use super::{Display, English, Japanese, said, spoken};

    pub fn settings() -> &'static str {
        said("設定", "the settings")
    }

    pub fn settings_file() -> &'static str {
        said("設定ファイル", "the settings file")
    }

    pub fn settings_fallback() -> &'static str {
        said("既定の設定", "the default settings")
    }

    pub fn no_settings_place() -> &'static str {
        said("設定の保存先がわかりません", "Cannot tell where to keep the settings")
    }

    pub fn sorts() -> &'static str {
        said("カラムの並び順", "the columns' sort orders")
    }

    pub fn sorts_file() -> &'static str {
        said("カラムの並び順のファイル", "the file of the columns' sort orders")
    }

    pub fn sorts_fallback() -> &'static str {
        said("既定の並び順", "the default sort orders")
    }

    pub fn no_sorts_place() -> &'static str {
        said("カラムの並び順の保存先がわかりません", "Cannot tell where to keep the columns' sort orders")
    }

    pub fn not_an_object() -> &'static str {
        said("オブジェクトではありません", "not an object")
    }

    /// A save that broke off before it said how it went.
    pub fn cannot_save(what: impl Display) -> String {
        match spoken() {
            Japanese => format!("{what}を保存できません"),
            English => format!("Cannot save {what}"),
        }
    }

    pub fn cannot_save_because(what: impl Display, error: impl Display) -> String {
        match spoken() {
            Japanese => format!("{what}を保存できません: {error}"),
            English => format!("Cannot save {what}: {error}"),
        }
    }

    /// The file could not be read, and is kept as it is: nothing is saved over it.
    pub fn unreadable(what: impl Display, error: impl Display, file: impl Display) -> String {
        let error = super::clause(error);
        match spoken() {
            Japanese => format!("{what}を読み込めません: {error}。元の{file}を残すため、変更は保存しません。"),
            English => format!("Cannot read {what}: {error}. Changes are not saved, to keep {file} as it is."),
        }
    }

    /// The file was broken and put aside at `aside`; Colet started on `fallback`.
    pub fn put_aside(file: impl Display, fallback: impl Display, error: impl Display, aside: impl Display) -> String {
        let error = super::clause(error);
        match spoken() {
            Japanese => format!("{file}が壊れていたため、{fallback}で始めました（{error}）。元のファイルは{aside}に残しています。"),
            English => format!("Started with {fallback}, as {file} was broken ({error}). The file is kept at {aside}."),
        }
    }

    pub fn skipped(file: impl Display, count: usize) -> String {
        match spoken() {
            Japanese => format!("{file}の読めない項目を{count}件飛ばしました"),
            English if count == 1 => format!("Skipped 1 entry in {file} that could not be read"),
            English => format!("Skipped {count} entries in {file} that could not be read"),
        }
    }

    /// What was skipped, and where the file was copied before the next save leaves it out.
    pub fn copied_aside(skipped: impl Display, aside: impl Display) -> String {
        let skipped = super::clause(skipped);
        match spoken() {
            Japanese => format!("{skipped}。元のファイルは{aside}に残しています。"),
            English => format!("{skipped}. The file is kept at {aside}."),
        }
    }

    /// What was skipped, the file kept as it is since it could not be copied aside.
    pub fn not_saved(skipped: impl Display, file: impl Display) -> String {
        let skipped = super::clause(skipped);
        match spoken() {
            Japanese => format!("{skipped}。元の{file}を残すため、変更は保存しません。"),
            English => format!("{skipped}. Changes are not saved, to keep {file} as it is."),
        }
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
        speak(English);
        assert_eq!(order(Order { folders_first: false, ..first }), "Size · Descending · Mixed");
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
        speak(English);
        assert_eq!(
            slow(&[Sort::Size], &[Sort::Modified]),
            "Everything does not index size, so searches may be slow. Fast sorting by date modified is off in Everything, so sorting by date modified may be slow. Turn them on under Indexes in Everything's options."
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

    #[test]
    fn quotes_names_in_a_row_as_each_language_lists_them() {
        assert_eq!(quoted_list(["a", "b"]), "「a」「b」");
        speak(English);
        assert_eq!(quoted_list(["a", "b"]), "“a”, “b”");
        assert_eq!(
            ops::cannot_open_many(5, quoted_list(["a", "b", "c"]), more_items(2), "no app"),
            "Cannot open 5 items: “a”, “b”, “c” and 2 more (no app)"
        );
        assert_eq!(ops::cannot_open_many(2, quoted_list(["a", "b"]), "", "no app"), "Cannot open 2 items: “a”, “b” (no app)");
    }

    /// A sentence said on its own ends as one, once, in either language: Windows's own words, which end so
    /// already, and a question gain nothing. Set inside another, Windows's words leave their end behind,
    /// so none stands doubled.
    #[test]
    fn ends_a_sentence_once_and_never_doubles_it() {
        assert_eq!(sentence("クリップボードに書き込めません"), "クリップボードに書き込めません。");
        assert_eq!(sentence("アクセスが拒否されました。\r\n"), "アクセスが拒否されました。");
        assert_eq!(sentence(search::ask_delete_filter("画像")), "「画像」を削除しますか？");
        assert_eq!(
            sentence(address::not_a_path("x")),
            "「x」は開けません。ドライブ（C:\\…）かサーバー（\\\\server）から始まるパスを入力してください。"
        );
        assert_eq!(sentence(""), "");
        // A list of characters ends in brackets, so the 。 is not read as one of them.
        assert_eq!(
            sentence(ops::name_problem(colet_core::operations::NameProblem::ForbiddenCharacter)),
            "次の文字は使えません（\\ / : * ? \" < > |）。"
        );
        let denied = "アクセスが拒否されました。";
        assert_eq!(
            kept::unreadable(kept::settings(), denied, kept::settings_file()),
            "設定を読み込めません: アクセスが拒否されました。元の設定ファイルを残すため、変更は保存しません。"
        );
        assert_eq!(window::close_without_settings(denied), "アクセスが拒否されました。もう一度閉じると、設定を保存せずに終了します。");
        assert_eq!(
            ops::cannot_open_many(2, quoted_list(["a", "b"]), "", denied),
            "2 項目を開けません: 「a」「b」（アクセスが拒否されました）"
        );
        assert_eq!(
            reasons::refused_with_hint(recycle(), reasons::not_at_home(), reasons::unsave_hint()),
            "「ごみ箱へ移動」はホームでは使えません。保存した検索は右クリックメニューの「保存した検索を解除」で外せます。"
        );
        speak(English);
        assert_eq!(sentence("Cannot write to the clipboard"), "Cannot write to the clipboard.");
        assert_eq!(
            sentence(search::error(&colet_core::everything::SearchError::WatchStopped(5))),
            "Stopped following Everything's changes. Press F5 to refresh the list (0x00000005)."
        );
        assert_eq!(sentence(window::busy_sending()), "“Send to” is still running. Wait for it to finish before closing.");
        assert_eq!(
            kept::put_aside(kept::settings_file(), kept::settings_fallback(), "Access is denied.", "a.json"),
            "Started with the default settings, as the settings file was broken (Access is denied). The file is kept at a.json."
        );
    }

    /// A command that opens a dialog to ask more ends with an ellipsis on its menu's line or button, in
    /// either language; the dialog's title names it without one.
    #[test]
    fn ends_a_command_that_asks_more_with_an_ellipsis() {
        for language in [Japanese, English] {
            speak(language);
            for command in [rename_command(), new_folder_command(), edit_command(), search::add(), search::edit_filters()] {
                assert!(command.ends_with('…'), "{command}");
            }
            for title in [rename(), new_folder(), search::add_filter(), search::edit_filter()] {
                assert!(!title.contains('…'), "{title}");
            }
        }
    }

    /// A folder is フォルダー in every Japanese word, as Explorer writes it, never cut short of its long vowel.
    #[test]
    fn spells_folder_as_explorer_does() {
        let source = include_str!("words.rs");
        // Put together here, so this test is not found by itself.
        let short = concat!("フォル", "ダ");
        let lines: Vec<usize> = source
            .match_indices(short)
            .filter(|(at, found)| !source[at + found.len()..].starts_with('ー'))
            .map(|(at, _)| source[..at].lines().count())
            .collect();
        assert!(lines.is_empty(), "cut short after line {lines:?}");
    }

    #[test]
    fn says_the_same_words_in_each_language() {
        let japanese = (shortcuts::shortcuts().len(), shortcuts::shortcuts().iter().map(|(_, keys)| keys.len()).collect::<Vec<_>>());
        speak(English);
        let english = (shortcuts::shortcuts().len(), shortcuts::shortcuts().iter().map(|(_, keys)| keys.len()).collect::<Vec<_>>());
        assert_eq!(japanese, english, "the keys' list holds the same keys in each");
        assert_eq!(search::examples().len(), 3);
    }
}
