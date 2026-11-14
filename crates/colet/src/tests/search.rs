//! The search field, its results and the filters a search runs within.
use super::*;

#[gpui_kit::test]
fn searches_only_for_what_is_typed_once_typing_pauses(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\abc.txt")]));
    let ui = open(cx, fake.clone());
    // Keys a few milliseconds apart: none of the queries on the way runs.
    for key in ["a", "b", "c"] {
        ui.act(cx, |window, cx| window.input(key, cx));
        cx.executor().advance_clock(Duration::from_millis(30));
    }
    settle(cx);
    assert_eq!(fake.queries(), ["abc"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
}

#[gpui_kit::test]
fn moves_from_the_search_field_into_the_results_with_down(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\alpha.txt"), file(r"D:\beta.txt")]));
    let ui = open(cx, fake);
    ui.input(cx, "a");
    ui.press(cx, "down");
    assert_eq!(ui.active(cx), 1);
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha.txt"]);
    // Picked, the file shows in its folder.
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().highlight.clone()).as_deref(), Some(Path::new(r"D:\alpha.txt")));
}

#[gpui_kit::test]
fn the_trail_beside_the_search_starts_at_the_folder_the_results_lead_to(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\alpha.txt")]));
    let ui = open(cx, fake);
    ui.input(cx, "a");
    // The results alone: the search field stands for them and the trail is empty.
    let results = crumb(&ui, cx, 1);
    assert!(ui.act(cx, |window, _| window.try_find(results.clone()).is_none()));
    ui.press(cx, "down");
    let folder = crumb(&ui, cx, 2);
    // Their folder open: its crumb stands first, with none for the results before it.
    let (field, address, crumb) = ui.act(cx, |window, _| {
        assert!(window.try_find(results).is_none());
        (window.find("search-box").bounds(), window.find("address").bounds(), window.find(folder).bounds())
    });
    assert!(field.right() < address.left() && address.left() <= crumb.left(), "{field:?} {address:?} {crumb:?}");
}

#[gpui_kit::test]
fn reports_a_failed_search_as_one_and_retries(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    *fake.failing.lock().unwrap() = Some("Everything 1.5に接続できません。".into());
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    assert!(ui.read(cx, |v| v.state.columns[1].error.is_some()));
    assert!(ui.read(cx, |v| matches!(v.loading.engine(), Engine::Failed(_))));
    *fake.failing.lock().unwrap() = None;
    let id = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("retry", id));
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
    assert!(ui.read(cx, |v| matches!(v.loading.engine(), Engine::Connected(_))));
}

#[gpui_kit::test]
fn warns_once_a_connection_of_what_everything_does_not_index_and_keeps_the_warning_up(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt"), file(r"D:\y.txt")]));
    *fake.warning.lock().unwrap() = Some("Everything がサイズを索引していないため、検索が遅くなることがあります。".into());
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    // The results come all the same, the size and date asked for as ever.
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 2);
    let warned = ["Everything がサイズを索引していないため、検索が遅くなることがあります。"];
    assert_eq!(ui.toasts(cx), warned);
    // Said once for the connection, and up until closed.
    ui.input(cx, "y");
    cx.executor().advance_clock(Duration::from_secs(10));
    settle(cx);
    assert_eq!(ui.toasts(cx), warned);
}

#[gpui_kit::test]
fn loads_the_next_page_of_results_as_the_list_nears_its_end(cx: &mut TestAppContext) {
    let results: Vec<Entry> = (0..1200).map(|i| file(&format!(r"D:\file{i:04}.txt"))).collect();
    let fake = Arc::new(Fake::default().with_results(results));
    let ui = open(cx, fake.clone());
    ui.input(cx, "file");
    assert_eq!(ui.read(cx, |v| (v.state.columns[1].entries.len(), v.state.columns[1].total)), (500, 1200));
    let id = ui.read(cx, |v| v.state.columns[1].id);
    let pages = |offset: usize| fake.searches.lock().unwrap().iter().filter(|(_, o)| *o == offset).count();
    // Drawn with its first rows in sight, far from the end: nothing more is asked for.
    ui.act(cx, |_, _| ());
    settle(cx);
    assert_eq!(pages(500), 0);
    let handle = ui.view.update(cx, |v, _| v.list_handle(id));
    handle.scroll_to_item(499, ScrollStrategy::Bottom);
    settle(cx);
    ui.act(cx, |_, _| ());
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1000);
    // Asked for once, though every frame drawn near the end asks.
    assert_eq!(pages(500), 1);
}

#[gpui_kit::test]
fn narrows_the_results_by_name_through_everything_keeping_the_one_picked(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\alpha.txt"), file(r"D:\beta.txt"), file(r"D:\alphabet.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "a");
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha.txt"]);
    let id = ui.read(cx, |v| v.state.columns[1].id);
    // The results have their field, as a folder has; a key in its rows goes there.
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    assert!(ui.toasts(cx).is_empty());
    // Everything is asked once typing pauses, for the names holding the text as it is.
    for key in ["b", "e", "t"] {
        ui.act(cx, |window, cx| window.input(key, cx));
        cx.executor().advance_clock(Duration::from_millis(30));
    }
    settle(cx);
    let narrowed: Vec<String> = fake.queries().into_iter().filter(|q| q.contains("nopath:")).collect();
    assert_eq!(narrowed, ["<a> nopath:nowildcards:\"bet\""]);
    let shown =
        |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>());
    assert_eq!(shown(&ui, cx), [r"D:\beta.txt", r"D:\alphabet.txt"]);
    // Counted against all the search finds; the one picked, hidden, stays picked with its folder open.
    let header = ui.read(cx, |v| (v.state.columns[1].total, v.state.columns[1].narrow.as_ref().map(|f| f.whole)));
    assert_eq!(header, (2, Some(3)));
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha.txt"]);
    assert!(ui.read(cx, |v| v.state.columns.len()) > 2);
    // ↓ goes into the rows; Esc there lists all again and empties the field.
    ui.press(cx, "down");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.active(cx), 1);
    ui.press(cx, "escape");
    assert_eq!(shown(&ui, cx).len(), 3);
    assert_eq!(ui.view.read_with(cx, |v, cx| v.narrow_fields[&id].input.read(cx).value().to_string()), "");
    // Another search starts with nothing narrowed.
    ui.input(cx, "x");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| v.search_for("b", None, window, cx)));
    settle(cx);
    assert!(ui.read(cx, |v| v.state.columns[1].narrow.is_none()));
    assert_eq!(shown(&ui, cx).len(), 3);
}

#[gpui_kit::test]
fn searches_within_the_filter_chosen_only_once_something_is_typed(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\a.jpg")])));
    let ui = open(cx, fake.clone());
    let asked = fake.queries().len();
    ui.click(cx, "search-filter");
    assert!(ui.read(cx, |v| v.filter_open()));
    ui.click(cx, crate::dialogs::filter_tool_id("filter", "画像"));
    assert!(!ui.read(cx, |v| v.filter_open()));
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("画像"));
    // Chosen with nothing typed, it asks nothing, and home stays alone.
    assert_eq!(fake.queries().len(), asked);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 1);
    assert!(!ui.read(cx, |v| v.state.can_go_back()));
    // What is typed is searched within it.
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    assert_eq!(last_query(&fake), "<ext:jpg;png> <cat>");
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
    // Esc empties the field and home shows alone; again, the filter still stays chosen.
    ui.press(cx, "escape");
    assert_eq!(ui.field(cx), "");
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 1);
    ui.press(cx, "escape");
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("画像"));
    ui.input(cx, "dog");
    assert_eq!(last_query(&fake), "<ext:jpg;png> <dog>");
    // Without one, what is typed is searched as it stands.
    ui.press(cx, "escape");
    ui.act(cx, |_, cx| ui.view.update(cx, |v, cx| v.choose_filter(None, cx)));
    ui.input(cx, "cat");
    assert_eq!(last_query(&fake), "cat");
}

#[gpui_kit::test]
fn chooses_a_filter_from_the_keys_and_hands_them_back_to_the_field(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\a.jpg")])));
    let ui = open(cx, fake.clone());
    assert!(ui.search_field_focused(cx));
    ui.press(cx, "ctrl-shift-f");
    assert!(ui.read(cx, |v| v.filter_open()));
    // ↓ lights the first line, none, then goes on down.
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("動画"));
    assert!(ui.search_field_focused(cx));
    ui.press(cx, "ctrl-shift-f");
    ui.press(cx, "escape");
    assert!(!ui.read(cx, |v| v.filter_open()));
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("動画"));
    ui.press(cx, "ctrl-shift-f");
    ui.press(cx, "shift-f10");
    assert!(!ui.read(cx, |v| v.filter_open()));
    assert!(ui.search_field_focused(cx));
}

#[gpui_kit::test]
fn opens_a_tab_afresh_within_the_default_filter_leaving_those_open_as_they_are(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![])])
    }));
    fake.settings.lock().unwrap().default_filter = Some("動画".into());
    let default = |fake: &Fake| fake.settings.lock().unwrap().default_filter.clone();
    let chosen = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.chosen_filter.clone());
    // With no tab kept, the first opens within it.
    let ui = open(cx, fake.clone());
    assert_eq!(chosen(&ui, cx).as_deref(), Some("動画"));
    // Another made the default, then pressed again, none is; the tab open keeps its own.
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    ui.click(cx, filter_tool_id("filter-default", "画像"));
    assert_eq!(default(&fake).as_deref(), Some("画像"));
    ui.click(cx, filter_tool_id("filter-default", "画像"));
    assert_eq!(default(&fake), None);
    ui.click(cx, filter_tool_id("filter-default", "画像"));
    ui.click(cx, "close-filters");
    assert_eq!(chosen(&ui, cx).as_deref(), Some("動画"));
    // A folder opened in a tab, and a tab of home alone, open within the default.
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "ctrl-enter");
    ui.press(cx, "ctrl-tab");
    assert_eq!(chosen(&ui, cx).as_deref(), Some("画像"));
    ui.press(cx, "ctrl-t");
    assert_eq!(chosen(&ui, cx).as_deref(), Some("画像"));
    // Renamed, the default follows; deleted, none is the default.
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    ui.click(cx, filter_tool_id("filter-edit", "画像"));
    ui.type_filter(cx, "写真", "ext:jpg");
    ui.click(cx, "save-filter");
    assert_eq!(default(&fake).as_deref(), Some("写真"));
    ui.click(cx, filter_tool_id("filter-delete", "写真"));
    ui.click(cx, "delete-filter");
    assert_eq!(default(&fake), None);
    ui.click(cx, "close-filters");
    ui.press(cx, "ctrl-t");
    assert_eq!(chosen(&ui, cx), None);
}

#[gpui_kit::test]
fn adds_edits_and_deletes_filters_with_what_names_them_following(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\cat.jpg")]));
    let ui = open(cx, fake.clone());
    let listing = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read(cx, |v| matches!(v.overlays.dialog(), Some(Dialog::Filters { edit: None, deleting: None })))
    };
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    assert!(listing(&ui, cx));
    ui.click(cx, "add-filter");
    ui.type_filter(cx, "画像", "ext:jpg;png");
    ui.click(cx, "save-filter");
    ui.click(cx, "add-filter");
    ui.type_filter(cx, "動画", "ext:mp4");
    ui.press(cx, "enter");
    let names = |fake: &Fake| fake.settings.lock().unwrap().filters.iter().map(|f| f.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&fake), ["画像", "動画"]);
    // A name taken cannot be had; Esc goes back to the list.
    ui.click(cx, filter_tool_id("filter-edit", "動画"));
    ui.type_filter(cx, "画像", "ext:mp4");
    ui.click(cx, "save-filter");
    assert!(!listing(&ui, cx));
    ui.press(cx, "escape");
    assert!(listing(&ui, cx));
    assert_eq!(names(&fake), ["画像", "動画"]);
    ui.click(cx, "close-filters");
    // Chosen, saved with a search, and shown.
    ui.click(cx, "search-filter");
    ui.click(cx, crate::dialogs::filter_tool_id("filter", "画像"));
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    ui.press(cx, "ctrl-d");
    // Renamed and asking another query, what names it follows, and the results ask again.
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    ui.click(cx, filter_tool_id("filter-edit", "画像"));
    ui.type_filter(cx, "写真", "ext:jpg");
    ui.click(cx, "save-filter");
    assert_eq!(names(&fake), ["写真", "動画"]);
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("写真"));
    assert_eq!(fake.settings.lock().unwrap().saved_searches[0].filter.as_deref(), Some("写真"));
    assert_eq!(ui.read(cx, |v| v.state.search_filter.clone()).as_deref(), Some("写真"));
    assert_eq!(last_query(&fake), "<ext:jpg> <cat>");
    // Put in another order.
    ui.click(cx, filter_tool_id("filter-down", "写真"));
    assert_eq!(names(&fake), ["動画", "写真"]);
    // Asked about first, with the saved searches that use it counted; gone, they go without it.
    ui.click(cx, filter_tool_id("filter-delete", "写真"));
    assert_eq!(ui.read(cx, |v| v.filter_users("写真")), 1);
    assert!(ui.read(cx, |v| matches!(v.overlays.dialog(), Some(Dialog::Filters { deleting: Some(name), .. }) if name == "写真")));
    ui.click(cx, "delete-filter");
    assert_eq!(names(&fake), ["動画"]);
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter, None);
    assert_eq!(fake.settings.lock().unwrap().saved_searches[0].filter, None);
    assert_eq!(last_query(&fake), "cat");
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
}
