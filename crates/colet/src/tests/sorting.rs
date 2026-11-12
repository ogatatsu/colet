//! The sort orders: the default, a folder's own, and the results'.
use super::*;

#[gpui_kit::test]
fn sorts_from_the_menu_reloading_what_is_open(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order { by: Sort::Size, descending: false, ..Order::default() });
    assert_eq!(fake.queries().len(), before + 1);
    assert!(!ui.read(cx, |v| v.sort_open()));
}

#[gpui_kit::test]
fn turns_the_sort_round_from_the_menu_and_keeps_it_for_another_sort(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    ui.click(cx, "sort");
    ui.click(cx, "降順");
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order { by: Sort::Name, descending: true, ..Order::default() });
    assert_eq!(fake.queries().len(), before + 1);
    assert!(!ui.read(cx, |v| v.sort_open()));
    ui.click(cx, "sort");
    ui.click(cx, "更新日時");
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order { by: Sort::Modified, descending: true, ..Order::default() });
    // Picking the way it already runs loads nothing again.
    ui.click(cx, "sort");
    ui.click(cx, "降順");
    assert_eq!(fake.queries().len(), before + 2);
}

#[gpui_kit::test]
fn mixes_folders_in_with_files_from_the_menu_and_puts_them_first_again(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    ui.click(cx, "sort");
    ui.click(cx, "folders-first");
    let mixed = Order { folders_first: false, ..Order::default() };
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), mixed);
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&mixed));
    assert_eq!(fake.queries().len(), before + 1);
    assert!(!ui.read(cx, |v| v.sort_open()));
    // Another sort keeps them mixed.
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order { by: Sort::Size, ..mixed });
    ui.click(cx, "sort");
    ui.click(cx, "folders-first");
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order { by: Sort::Size, ..Order::default() });
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
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order::default());
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
fn keeps_the_sort_order_for_the_next_start(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    let saves = fake.saves.load(Ordering::Relaxed);
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().sort, Order { by: Sort::Size, descending: false, ..Order::default() });
    ui.click(cx, "sort");
    ui.click(cx, "降順");
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().sort, Order { by: Sort::Size, descending: true, ..Order::default() });
    assert_eq!(fake.saves.load(Ordering::Relaxed), saves + 2);
    // Picking the order already in use writes nothing.
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    settle(cx);
    assert_eq!(fake.saves.load(Ordering::Relaxed), saves + 2);
}

#[gpui_kit::test]
fn opens_in_the_sort_order_kept(cx: &mut TestAppContext) {
    let kept = Order { by: Sort::Modified, descending: true, ..Order::default() };
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    fake.settings.lock().unwrap().sort = kept;
    let ui = open(cx, fake.clone());
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), kept);
    ui.input(cx, "x");
    let orders = fake.orders.lock().unwrap().clone();
    assert!(!orders.is_empty());
    assert!(orders.iter().all(|o| *o == kept), "{orders:?}");
}

#[gpui_kit::test]
fn picks_the_sort_order_by_keys(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.click(cx, "sort");
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order { by: Sort::Size, descending: false, ..Order::default() });
    assert!(!ui.read(cx, |v| v.sort_open()));
    assert!(ui.columns_focused(cx));
    // From the button itself, Enter opens it.
    let sort = ui.read(cx, |v| v.sort_focus.clone());
    ui.act(cx, |window, cx| sort.focus(window, cx));
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| v.sort_open()));
    ui.press(cx, "escape");
    assert!(!ui.read(cx, |v| v.sort_open()));
    ui.press(cx, "enter");
    ui.press(cx, "menu");
    assert!(!ui.read(cx, |v| v.sort_open()));
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
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order::default());
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
fn keeps_a_folders_own_order_when_the_default_changes(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("sort-by-modified", work));
    let (own, listings) = (Order { by: Sort::Modified, descending: true, ..Order::default() }, listings_of(&fake, r"D:\work"));
    ui.click(cx, "sort");
    ui.click(cx, "サイズ");
    let size = Order { by: Sort::Size, descending: false, ..Order::default() };
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), size);
    // Only what follows the default is listed again.
    assert_eq!(listed_in(&fake, r"D:\"), Some(size));
    assert_eq!(listings_of(&fake, r"D:\work"), listings);
    assert_eq!(fake.sorts.lock().unwrap().folder(Path::new(r"D:\work")), Some(own));
    // The default the folder is given now is its own no longer.
    ui.click(cx, ("column-sort", work));
    ui.click(cx, "サイズ");
    ui.click(cx, ("column-sort", work));
    ui.click(cx, "昇順");
    assert_eq!(listed_in(&fake, r"D:\work"), Some(size));
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
fn gives_the_search_results_one_order_of_their_own_for_every_search(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    let before = fake.queries().len();
    ui.click(cx, ("sort-by-modified", results));
    let modified = Order { by: Sort::Modified, descending: true, ..Order::default() };
    assert_eq!(fake.queries().len(), before + 1);
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&modified));
    assert_eq!(fake.sorts.lock().unwrap().search, Some(modified));
    // Another search goes the same way.
    ui.input(cx, "y");
    assert_eq!(fake.orders.lock().unwrap().last(), Some(&modified));
    assert_eq!(ui.read(cx, |v| v.stored.settings().sort), Order::default());
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
