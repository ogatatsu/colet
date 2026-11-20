//! The sort orders: the default, a folder's own, and the results'.
use super::*;

#[gpui_kit::test]
fn sorts_the_results_from_their_menu_reloading_them(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    rest(&ui, cx);
    let before = fake.queries().len();
    ui.click(cx, ("column-sort", results));
    ui.click(cx, "サイズ");
    let size = Order { by: Sort::Size, descending: false, ..Order::default() };
    assert_eq!(ui.read(cx, |v| v.state.search_order), size);
    assert_eq!(fake.queries().len(), before + 1);
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), None);
}

#[gpui_kit::test]
fn turns_the_results_round_from_their_menu_and_keeps_it_for_another_sort(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    rest(&ui, cx);
    let results = ui.read(cx, |v| v.state.columns[1].id);
    let before = fake.queries().len();
    ui.click(cx, ("column-sort", results));
    ui.click(cx, "降順");
    assert_eq!(ui.read(cx, |v| v.state.search_order), Order { by: Sort::Name, descending: true, ..Order::default() });
    assert_eq!(fake.queries().len(), before + 1);
    ui.click(cx, ("column-sort", results));
    ui.click(cx, "更新日時");
    assert_eq!(ui.read(cx, |v| v.state.search_order), Order { by: Sort::Modified, descending: true, ..Order::default() });
    // Picking the way it already runs loads nothing again.
    ui.click(cx, ("column-sort", results));
    ui.click(cx, "降順");
    assert_eq!(fake.queries().len(), before + 2);
}

#[gpui_kit::test]
fn mixes_one_folders_items_apart_from_the_default_and_keeps_it_by_its_headings(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("column-sort", work));
    ui.click(cx, "folders-first");
    let mixed = Order { folders_first: false, ..Order::default() };
    assert_eq!(listed_in(&fake, r"D:\work"), Some(mixed));
    assert_eq!(listed_in(&fake, r"D:\"), Some(Order::default()));
    ui.click(cx, ("sort-by-size", work));
    let size = Order { by: Sort::Size, descending: true, folders_first: false };
    assert_eq!(listed_in(&fake, r"D:\work"), Some(size));
    settle(cx);
    assert_eq!(fake.sorts.lock().unwrap().folder(Path::new(r"D:\work")), Some(size));
    // Back to the default, folders come first again.
    ui.click(cx, ("column-sort", work));
    ui.click(cx, "unsort");
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order::default()));
}

#[gpui_kit::test]
fn picks_a_columns_order_by_keys(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("column-sort", work));
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order { by: Sort::Size, descending: false, ..Order::default() }));
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), None);
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn sorts_one_folder_by_its_headings_apart_from_the_default(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    let (root, before) = (listings_of(&fake, r"D:\"), fake.sort_saves.load(Ordering::Relaxed));
    // A size or a time starts from the largest; the same heading again turns it round.
    ui.click(cx, ("sort-by-size", work));
    let size = Order { by: Sort::Size, descending: true, ..Order::default() };
    assert_eq!(listed_in(&fake, r"D:\work"), Some(size));
    assert_eq!(fake.sorts.lock().unwrap().folder(Path::new(r"D:\work")), Some(size));
    assert_eq!(fake.sort_saves.load(Ordering::Relaxed), before + 1);
    ui.click(cx, ("sort-by-size", work));
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order { by: Sort::Size, descending: false, ..Order::default() }));
    ui.click(cx, ("sort-by-modified", work));
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order { by: Sort::Modified, descending: true, ..Order::default() }));
    // Names start from A: the default, so the folder follows it again.
    ui.click(cx, ("sort-by-name", work));
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order::default()));
    assert_eq!(fake.sorts.lock().unwrap().folder(Path::new(r"D:\work")), None);
    // The folder beside it is never listed again for it.
    assert_eq!(listings_of(&fake, r"D:\"), root);
    // The keys stay in the column worked in.
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
}

#[gpui_kit::test]
fn gives_a_folder_its_own_order_from_its_button_and_takes_it_back(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("column-sort", work));
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), Some(work));
    // Back to the default is off while it follows it.
    ui.click(cx, "unsort");
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), Some(work));
    ui.click(cx, "更新日時");
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), None);
    let modified = Order { by: Sort::Modified, descending: false, ..Order::default() };
    assert_eq!(listed_in(&fake, r"D:\work"), Some(modified));
    ui.click(cx, ("column-sort", work));
    ui.click(cx, "降順");
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order { by: Sort::Modified, descending: true, ..Order::default() }));
    let listings = listings_of(&fake, r"D:\work");
    ui.click(cx, ("column-sort", work));
    ui.click(cx, "unsort");
    assert_eq!(listed_in(&fake, r"D:\work"), Some(Order::default()));
    assert_eq!(listings_of(&fake, r"D:\work"), listings + 1);
    assert_eq!(fake.sorts.lock().unwrap().folder(Path::new(r"D:\work")), None);
}

#[gpui_kit::test]
fn opens_a_folder_in_the_order_kept_for_it_whatever_its_case(cx: &mut TestAppContext) {
    let own = Order { by: Sort::Size, descending: true, ..Order::default() };
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![file(r"D:\work\a.txt")])])
    });
    fake.sorts.lock().unwrap().set_folder(Path::new(r"d:\WORK"), Some(own));
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    assert_eq!(listed_in(&fake, r"D:\work"), Some(own));
    assert_eq!(listed_in(&fake, r"D:\"), Some(Order::default()));
}

#[gpui_kit::test]
fn keeps_the_search_results_order_for_the_next_search_in_the_tab(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    let before = fake.queries().len();
    ui.click(cx, ("sort-by-modified", results));
    let modified = Order { by: Sort::Modified, descending: true, ..Order::default() };
    assert_eq!(fake.queries().len(), before + 1);
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&modified));
    assert_eq!(ui.read(cx, |v| v.state.search_order), modified);
    // Another search goes the same way.
    ui.input(cx, "y");
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&modified));
}

#[gpui_kit::test]
fn sorts_only_the_search_results_by_location_from_a_first_then_turned_round(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    rest(&ui, cx);
    let results = ui.read(cx, |v| v.state.columns[1].id);
    // A location starts from A, as a name does; the heading again turns it round.
    ui.click(cx, ("sort-by-location", results));
    let location = Order { by: Sort::Location, ..Order::default() };
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&location));
    assert_eq!(ui.read(cx, |v| v.state.search_order), location);
    ui.click(cx, ("sort-by-location", results));
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&Order { descending: true, ..location }));
    // Their menu offers it.
    ui.click(cx, ("column-sort", results));
    ui.click(cx, "名前");
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&Order { by: Sort::Name, descending: true, ..Order::default() }));
    ui.click(cx, ("column-sort", results));
    ui.click(cx, "場所");
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&Order { descending: true, ..location }));
}

#[gpui_kit::test]
fn leaves_location_out_of_a_folders_headings_and_menu(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    assert!(ui.act(cx, |window, _| window.try_find(("sort-by-location", work)).is_none()));
    ui.click(cx, ("column-sort", work));
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), Some(work));
    assert!(ui.act(cx, |window, _| window.try_find("場所").is_none()));
}

#[gpui_kit::test]
fn opens_a_columns_order_from_its_button_by_keys(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.tab_to(cx, ("column-sort", work));
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), Some(work));
    ui.press(cx, "escape");
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), None);
    ui.tab_to(cx, ("column-sort", work));
    ui.press(cx, "down");
    assert_eq!(ui.read(cx, |v| v.column_sort_open()), Some(work));
}

#[gpui_kit::test]
fn never_writes_over_column_orders_it_could_not_read(cx: &mut TestAppContext) {
    let (fake, ui) = {
        let fake = Arc::new(Fake {
            roots: vec![Entry::folder(r"D:\", "D:")],
            ..Fake::default().with_folders(&[(r"D:\", vec![file(r"D:\a.txt")])])
        });
        *fake.unreadable_sorts.lock().unwrap() =
            Some(LoadError { message: "カラムの並び順を読み込めません".into(), kept: true });
        let ui = open(cx, fake.clone());
        (fake, ui)
    };
    assert!(ui.toasts(cx).iter().any(|t| t.contains("カラムの並び順を読み込めません")), "{:?}", ui.toasts(cx));
    ui.click(cx, row(r"D:\"));
    let root = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("sort-by-size", root));
    settle(cx);
    assert_eq!(listed_in(&fake, r"D:\"), Some(Order { by: Sort::Size, descending: true, ..Order::default() }));
    assert_eq!(fake.sort_saves.load(Ordering::Relaxed), 0);
}

#[gpui_kit::test]
fn keeps_each_tabs_search_results_order_its_own_and_with_the_way_back(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("sort-by-modified", results));
    let modified = Order { by: Sort::Modified, descending: true, ..Order::default() };
    // A new tab searches in the default order, leaving the first tab's be.
    ui.press(cx, "ctrl-t");
    ui.input(cx, "y");
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&Order::default()));
    assert_eq!(ui.read(cx, |v| v.state.search_order), Order::default());
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.read(cx, |v| (v.state.query.clone(), v.state.search_order)), ("x".into(), modified));
    // Kept with the tab between starts.
    settle(cx);
    let kept = fake.settings.lock().unwrap().tabs.iter().map(|t| t.place.search_order).collect::<Vec<_>>();
    assert_eq!(kept, [modified, Order::default()]);
}

#[gpui_kit::test]
fn saves_a_search_with_its_order_and_runs_it_in_that_order(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    rest(&ui, cx);
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("sort-by-size", results));
    let size = Order { by: Sort::Size, descending: true, ..Order::default() };
    ui.click(cx, ("star", results));
    assert_eq!(fake.settings.lock().unwrap().saved_searches.iter().map(|s| s.order).collect::<Vec<_>>(), [size]);
    // Sorted another way, it is a search not saved.
    ui.click(cx, ("sort-by-size", results));
    let kept = ui.read(cx, |v| v.stored.settings().saved("x", None, None, v.state.search_order).is_some());
    assert!(!kept);
    // Run from home, it lists in its own order again.
    ui.press(cx, "escape");
    ui.show_home(cx);
    ui.click(cx, row(HomeKey::search("x", None, None, size).key()));
    assert_eq!(ui.read(cx, |v| v.state.search_order), size);
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&size));
}

#[gpui_kit::test]
fn changes_a_saved_searchs_order_in_its_dialog(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    fake.settings.lock().unwrap().saved_searches =
        vec![SavedSearch { name: String::new(), query: "x".into(), filter: None, narrow: None, order: Order::default() }];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(HomeKey::search("x", None, None, Order::default()).key()));
    ui.press(cx, "f2");
    let choice = |row: &'static str, text: &'static str| ElementId::NamedChild(std::sync::Arc::new(row.into()), SharedString::from(text));
    ui.click(cx, choice("edit-search-sort", "更新日時"));
    ui.click(cx, choice("edit-search-direction", "降順"));
    ui.click(cx, choice("edit-search-folders", "混在"));
    ui.click(cx, "save-search");
    let order = Order { by: Sort::Modified, descending: true, folders_first: false };
    assert_eq!(fake.settings.lock().unwrap().saved_searches.iter().map(|s| s.order).collect::<Vec<_>>(), [order]);
    // The results showing stay as they were.
    assert_eq!(ui.read(cx, |v| v.state.search_order), Order::default());
}
