//! Favorites and saved searches in home, dragging and pinning them.
use super::*;

#[gpui_kit::test]
fn makes_a_folder_a_favorite_with_its_star_and_ctrl_d_and_lists_it_at_home(cx: &mut TestAppContext) {
    let fake = Arc::new(drive_d());
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("star", work));
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\work"]);
    // Home lists it first, under its own heading.
    assert_eq!(ui.read(cx, |v| v.state.columns[0].entries[0].path.to_str().unwrap().to_owned()), r"D:\work");
    ui.press(cx, "ctrl-d");
    assert!(fake.settings.lock().unwrap().favorites.is_empty());
    ui.press(cx, "ctrl-d");
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\work"]);
    // Taken off from its menu in home.
    ui.show_home(cx);
    ui.act(cx, |window, cx| window.within("home").right_click(row(r"D:\work"), cx));
    settle(cx);
    ui.click(cx, "menu-favorite");
    assert!(fake.settings.lock().unwrap().favorites.is_empty());
    assert!(ui.read(cx, |v| v.menu().is_none()));
}

#[gpui_kit::test]
fn keeps_a_folder_narrowed_as_a_favorite_of_its_own_and_opens_it_narrowed(cx: &mut TestAppContext) {
    use crate::settings::Favorite;
    use crate::state::HomeKey;
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default()
            .with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![file(r"D:\work\a.pdf"), file(r"D:\work\b.txt")])])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, ("star", work));
    // Narrowed, the star is another favorite's, kept with the narrowing as it would be typed again.
    ui.input(cx, "  pdf  ");
    let kept = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.stored.settings().is_favorite(&v.favorite_key_of(2)));
    assert!(!kept(&ui, cx));
    ui.click(cx, ("star", work));
    assert!(kept(&ui, cx));
    let narrowed = Favorite { narrow: Some("pdf".into()), ..Favorite::new(r"D:\work") };
    assert_eq!(fake.settings.lock().unwrap().favorites, [Favorite::new(r"D:\work"), narrowed.clone()]);
    let key = HomeKey::favorite(Path::new(r"D:\work"), Some("pdf")).key();
    let homes = ui.read(cx, |v| v.state.columns[0].entries.iter().map(|e| e.path.clone()).collect::<Vec<_>>());
    assert!(homes.contains(&key) && homes.contains(&PathBuf::from(r"D:\work")), "{homes:?}");
    // Opened from home, it lists what the narrowing lets through, its field showing it; the other, all.
    let opened = |ui: &Ui, cx: &mut TestAppContext| {
        let (id, shown) = ui.read(cx, |v| {
            let c = v.state.columns.last().unwrap();
            (c.id, c.entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>())
        });
        (shown, ui.view.read_with(cx, |v, cx| v.narrow_fields[&id].input.read(cx).value().to_string()))
    };
    ui.show_home(cx);
    ui.click_in(cx, "home", row(r"D:\work"));
    assert_eq!(opened(&ui, cx), (vec![r"D:\work\a.pdf".to_owned(), r"D:\work\b.txt".to_owned()], String::new()));
    ui.click_in(cx, "home", row(&key));
    assert_eq!(opened(&ui, cx), (vec![r"D:\work\a.pdf".to_owned()], "pdf".to_owned()));
    // F2 there edits it, rather than renaming its folder.
    ui.press(cx, "f2");
    let fields = ui.read(cx, |v| match v.overlays.dialog() {
        Some(crate::dialogs::Dialog::EditFavorite { name, narrow, .. }) => Some((name.clone(), narrow.clone())),
        _ => None,
    });
    let (name, narrow) = fields.expect("the favorite's edit");
    let set = |ui: &Ui, cx: &mut TestAppContext, input: &Entity<gpui_kit::base::input::InputState>, text: &str| {
        let (input, text) = (input.clone(), text.to_owned());
        ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value(text, window, cx)));
        settle(cx);
    };
    // Narrowed as the other is, it would be that one again.
    set(&ui, cx, &narrow, " ");
    assert!(ui.view.read_with(cx, |v, cx| v.favorite_edit_problem(cx)).is_some());
    set(&ui, cx, &name, "書類");
    set(&ui, cx, &narrow, "txt");
    ui.click(cx, "save-favorite");
    let edited = Favorite { name: "書類".into(), narrow: Some("txt".into()), ..Favorite::new(r"D:\work") };
    assert_eq!(fake.settings.lock().unwrap().favorites, [Favorite::new(r"D:\work"), edited]);
    // Taken off, its name and narrowing go too, and it says so.
    let key = HomeKey::favorite(Path::new(r"D:\work"), Some("txt")).key();
    ui.show_home(cx);
    ui.act(cx, |window, cx| window.within("home").right_click(row(&key), cx));
    settle(cx);
    ui.click(cx, "menu-favorite");
    assert_eq!(fake.settings.lock().unwrap().favorites, [Favorite::new(r"D:\work")]);
    assert!(ui.toasts(cx).iter().any(|t| t.contains("「書類」をお気に入りから外しました")), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn saves_a_search_narrowed_apart_and_runs_it_narrowed(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf"), file(r"D:\b.pdf")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    rest(&ui, cx);
    ui.press(cx, "down");
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.input(cx, "a");
    ui.click(cx, ("star", results));
    let saved = fake.settings.lock().unwrap().saved_searches.clone();
    assert_eq!(
        saved,
        [SavedSearch { name: String::new(), query: "ext:pdf".into(), filter: None, narrow: Some("a".into()), order: Order::default() }]
    );
    // Run from home, the results are narrowed as they were kept.
    ui.press(cx, "escape");
    ui.show_home(cx);
    ui.click_in(cx, "home", row(HomeKey::search("ext:pdf", None, Some("a"), Order::default()).key()));
    let (narrowing, id) = ui.read(cx, |v| (v.state.search_narrowing(), v.state.columns[1].id));
    assert_eq!(narrowing.as_deref(), Some("a"));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.narrow_fields[&id].input.read(cx).value().to_string()), "a");
}

#[gpui_kit::test]
fn saves_a_search_and_runs_it_from_home_keeping_the_keys_there(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake { roots: vec![Entry::folder(r"D:\", "D:")], ..Fake::default() }.with_results(vec![file(r"D:\a.pdf")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    rest(&ui, cx);
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click(cx, ("star", results));
    assert_eq!(
        fake.settings.lock().unwrap().saved_searches,
        [SavedSearch { name: String::new(), query: "ext:pdf".into(), filter: None, narrow: None, order: Order::default() }]
    );
    ui.press(cx, "ctrl-f");
    ui.press(cx, "escape");
    assert_eq!(ui.field(cx), "");
    ui.show_home(cx);
    ui.click(cx, row(HomeKey::search("ext:pdf", None, None, Order::default()).key()));
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf");
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 1);
    // ↓ in home moves on to the next place, leaving the search.
    ui.press(cx, "down");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "");
    assert_eq!(ui.field(cx), "");
}

#[gpui_kit::test]
fn unsaves_a_search_with_ctrl_d_and_says_so(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches =
        vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None, narrow: None, order: Order::default() }];
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    ui.press(cx, "ctrl-d");
    assert!(fake.settings.lock().unwrap().saved_searches.is_empty());
    assert!(ui.toasts(cx).iter().any(|t| t.contains("「PDF」を保存した検索から外しました")), "{:?}", ui.toasts(cx));
    // Nothing typed: nothing to save, and it says why.
    ui.press(cx, "escape");
    ui.press(cx, "ctrl-d");
    assert!(ui.toasts(cx).iter().any(|t| t.contains("検索語を入力すると保存できます")));
}

#[gpui_kit::test]
fn edits_a_saved_searchs_name_and_query_with_f2(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches = vec![
        SavedSearch { name: String::new(), query: "ext:pdf".into(), filter: None, narrow: None, order: Order::default() },
        SavedSearch { name: String::new(), query: "dm:today".into(), filter: None, narrow: None, order: Order::default() },
    ];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(HomeKey::search("ext:pdf", None, None, Order::default()).key()));
    ui.press(cx, "f2");
    let (name, query) = ui.read(cx, |v| match v.overlays.dialog() {
        Some(crate::dialogs::Dialog::EditSearch(crate::dialogs::SearchEdit { name, query, .. })) => (name.clone(), query.clone()),
        _ => panic!("no dialog"),
    });
    // A query already saved cannot be taken.
    ui.act(cx, |window, cx| query.update(cx, |input, cx| input.set_value("dm:today", window, cx)));
    ui.click(cx, "save-search");
    assert!(ui.read(cx, |v| v.overlays.has_dialog()));
    ui.act(cx, |window, cx| {
        name.update(cx, |input, cx| input.set_value("PDF", window, cx));
        query.update(cx, |input, cx| input.set_value("ext:pdf dm:thisweek", window, cx));
    });
    ui.click(cx, "save-search");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert_eq!(
        fake.settings.lock().unwrap().saved_searches[0],
        SavedSearch { name: "PDF".into(), query: "ext:pdf dm:thisweek".into(), filter: None, narrow: None, order: Order::default() }
    );
    // The search showing stays as it was, and home picks nothing, the row it picked gone.
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf");
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(ui.read(cx, |v| v.state.columns[0].selected.is_empty()));
    // Escape closes the dialog without a change.
    ui.click(cx, row(HomeKey::search("ext:pdf dm:thisweek", None, None, Order::default()).key()));
    ui.press(cx, "f2");
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
}

#[gpui_kit::test]
fn runs_a_saved_search_from_enter_and_double_click_without_sending_it_to_the_shell(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches =
        vec![SavedSearch { name: "PDF".into(), query: "ext:pdf".into(), filter: None, narrow: None, order: Order::default() }];
    let ui = open(cx, fake.clone());
    // ↓ from the field picks it in home standing alone, which runs it.
    ui.press(cx, "down");
    assert_eq!(ui.field(cx), "ext:pdf");
    // ← from other results lands on no row in home, and ↓ from the field back there leaves the results
    // be; ↓ in home picks it, which runs it.
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-a");
    ui.input(cx, "a");
    ui.press(cx, "down");
    ui.press(cx, "left");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "a");
    ui.press(cx, "ctrl-f");
    ui.press(cx, "down");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "a");
    assert_eq!(ui.read(cx, |v| v.state.columns[0].focused.clone()), None);
    ui.press(cx, "down");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "ext:pdf");
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(ui.home_focused(cx));
    assert!(fake.opened.lock().unwrap().is_empty());
    ui.press(cx, "ctrl-f");
    ui.press(cx, "escape");
    ui.act(cx, |window, cx| window.double_click(row(HomeKey::search("ext:pdf", None, None, Order::default()).key()), cx));
    settle(cx);
    assert_eq!(ui.field(cx), "ext:pdf");
    assert!(fake.opened.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn brings_homes_focused_row_into_view_across_all_three_headings(cx: &mut TestAppContext) {
    let fake =
        Arc::new(Fake { roots: vec![folder(r"D:\")], ..Fake::default().with_folders(&[(r"D:\", vec![]), (r"D:\favorite", vec![])]) });
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.favorites = vec![r"D:\favorite".into()];
        settings.saved_searches = (0..50)
            .map(|i| SavedSearch { name: String::new(), query: format!("query{i}"), filter: None, narrow: None, order: Default::default() })
            .collect();
    }
    let ui = open(cx, fake);
    ui.press(cx, "down");
    ui.press(cx, "end");
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    // The scroll asked for as the row was focused is taken at the next frame, which finding it draws.
    assert!(ui.act(cx, |window, _| window.within("home-rows").find(row(r"D:\")).visible()));
    assert!(ui.read(cx, |v| v.scrolls.home.offset().y < px(0.)));
    ui.press(cx, "home");
    assert_eq!(ui.selected(cx, 0), [r"D:\favorite"]);
    assert!(ui.act(cx, |window, _| window.within("home-rows").find(row(r"D:\favorite")).visible()));
    assert!(ui.home_focused(cx));
}

#[gpui_kit::test]
fn opens_a_favorite_renamed_elsewhere_to_another_case_and_keeps_it_in_the_disks_case(cx: &mut TestAppContext) {
    use crate::settings::Favorite;
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![folder(r"D:\work\sub")]),
            (r"D:\work\sub", vec![file(r"D:\work\sub\a.txt")]),
        ])
    });
    let ui = open(cx, fake.clone());
    // Kept as they were named before Explorer lowered them; the second is the first in another case.
    ui.view.update(cx, |v, cx| {
        v.update_settings(
            |s| {
                s.favorites = vec![Favorite::new(r"D:\WORK\Sub"), Favorite::new(r"D:\Work")];
                s.pinned = vec![r"D:\WORK".into()];
            },
            cx,
        )
    });
    settle(cx);
    ui.click(cx, row(r"D:\WORK\Sub"));
    settle(cx);
    // It opens, each folder on the way picked in the disk's case.
    assert!(ui.toasts(cx).is_empty(), "{:?}", ui.toasts(cx));
    let columns: Vec<String> = ui.read(cx, |v| v.state.columns.iter().skip(1).map(|c| text(&c.path)).collect());
    assert_eq!(columns, [r"D:\", r"D:\work", r"D:\work\sub"]);
    assert_eq!(ui.selected(cx, 1), [r"D:\work"]);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\sub"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[3].entries.len()), 1);
    // Kept so from now on, with the other favorite and the pin of the folder above it, whatever their case.
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\work\sub", r"D:\work"]);
    assert_eq!(texts(&fake.settings.lock().unwrap().pinned), [r"D:\work"]);
    assert_eq!(ui.selected(cx, 0), [r"D:\work\sub"]);
}

#[gpui_kit::test]
fn drags_a_favorite_to_a_new_place_among_the_favorites(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    let (a, c) = (ui.home_row(cx, r"D:\a"), ui.home_row(cx, r"D:\c"));
    ui.hold_drag(cx, c.center(), upper(a));
    // The line goes above the row it would go before; the row carried fades.
    assert_eq!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).landing().and_then(|l| l.before.clone())).as_deref(), Some(Path::new(r"D:\a")));
    ui.let_go(cx, upper(a));
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\c", r"D:\a", r"D:\b"]);
    let home: Vec<String> = ui.read(cx, |v| v.state.columns[0].entries.iter().take(3).map(|e| text(&e.path)).collect());
    assert_eq!(home, [r"D:\c", r"D:\a", r"D:\b"]);
    // Neither picked nor opened.
    assert!(ui.selected(cx, 0).is_empty());
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 1);
    assert!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).carried().is_none() && v.home_drag.read(cx).landing().is_none()));
}

#[gpui_kit::test]
fn drags_a_favorite_first_over_its_heading_and_last_below_its_rows(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    // Over the heading above the first favorite, it goes first.
    let (a, c) = (ui.home_row(cx, r"D:\a"), ui.home_row(cx, r"D:\c"));
    let heading = point(a.center().x, a.top() - px(10.));
    ui.hold_drag(cx, c.center(), heading);
    assert_eq!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).landing().and_then(|l| l.before.clone())).as_deref(), Some(Path::new(r"D:\a")));
    ui.let_go(cx, heading);
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\c", r"D:\a", r"D:\b"]);
    // In the gap below the last, before the saved searches' heading, it goes last.
    let (c, b) = (ui.home_row(cx, r"D:\c"), ui.home_row(cx, r"D:\b"));
    let gap = point(b.center().x, b.bottom() + px(4.));
    ui.hold_drag(cx, c.center(), gap);
    assert_eq!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).landing().cloned()), Some(crate::home::Landing { before: None }));
    ui.let_go(cx, gap);
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\a", r"D:\b", r"D:\c"]);
}

#[gpui_kit::test]
fn drags_a_saved_search_below_the_last_without_running_it(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    let (one, two) = (ui.home_row(cx, "search:one"), ui.home_row(cx, "search:two"));
    ui.hold_drag(cx, one.center(), lower(two));
    assert_eq!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).landing().cloned()), Some(crate::home::Landing { before: None }));
    ui.let_go(cx, lower(two));
    let queries: Vec<String> = fake.settings.lock().unwrap().saved_searches.iter().map(|s| s.query.clone()).collect();
    assert_eq!(queries, ["two", "one"]);
    assert!(fake.queries().is_empty());
}

#[gpui_kit::test]
fn pins_rows_of_any_kind_above_the_rest_and_puts_them_in_order(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    let home =
        |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[0].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>());
    let pin = |ui: &Ui, cx: &mut TestAppContext, path: &str, line: &'static str| {
        ui.act(cx, |window, cx| window.within("home").right_click(row(path), cx));
        settle(cx);
        ui.click(cx, line);
    };
    // A place, a saved search and a favorite, each from its menu in home, at the end of those pinned.
    pin(&ui, cx, r"D:\", "menu-pin");
    pin(&ui, cx, "search:two", "menu-pin");
    pin(&ui, cx, r"D:\b", "menu-pin");
    assert_eq!(texts(&fake.settings.lock().unwrap().pinned), [r"D:\", "search:two", r"D:\b"]);
    assert_eq!(home(&ui, cx), [r"D:\", "search:two", r"D:\b", r"D:\a", r"D:\c", "search:one"]);
    // Dragged among themselves, whatever their kinds.
    let (place, favorite) = (ui.home_row(cx, r"D:\"), ui.home_row(cx, r"D:\b"));
    ui.hold_drag(cx, favorite.center(), upper(place));
    ui.let_go(cx, upper(place));
    assert_eq!(texts(&fake.settings.lock().unwrap().pinned), [r"D:\b", r"D:\", "search:two"]);
    // Put back from its menu, it is among its own kind where it was.
    pin(&ui, cx, r"D:\", "menu-unpin");
    assert_eq!(home(&ui, cx), [r"D:\b", "search:two", r"D:\a", r"D:\c", "search:one", r"D:\"]);
    // A favorite or saved search let go of takes its pin with it.
    pin(&ui, cx, r"D:\b", "menu-favorite");
    pin(&ui, cx, "search:two", "menu-unsave");
    assert!(fake.settings.lock().unwrap().pinned.is_empty());
    assert!(!ui.read_app(cx, |v, app| {
        v.menu_lines(app).iter().any(|l| matches!(l.command, crate::menus::Command::Home(crate::home::HomeCommand::Pin(_))))
    }));
}

#[gpui_kit::test]
fn offers_in_home_only_what_its_rows_can_do(cx: &mut TestAppContext) {
    let (_, ui) = ordered_home(cx);
    let lines = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read_app(cx, |v, app| {
            v.menu_lines(app).into_iter().map(|l| format!("{}{}", if l.group { "| " } else { "" }, l.text)).collect::<Vec<_>>()
        })
    };
    let menu = |ui: &Ui, cx: &mut TestAppContext, path: &str| {
        ui.press(cx, "escape");
        ui.act(cx, |window, cx| window.within("home").right_click(row(path), cx));
        settle(cx);
    };
    // Nothing picked, the menu key opens nothing.
    ui.press(cx, "shift-f10");
    assert!(ui.read(cx, |v| v.menu().is_none()));
    menu(&ui, cx, r"D:\a");
    assert_eq!(
        lines(&ui, cx),
        [
            "開く",
            "新しいタブで開く",
            "エクスプローラーで表示",
            "| パスをコピー",
            "| 編集…",
            "ピン留め",
            "お気に入りを解除",
            "| プロパティ",
            "その他のオプションを確認"
        ]
    );
    menu(&ui, cx, r"D:\");
    assert_eq!(
        lines(&ui, cx),
        [
            "開く",
            "新しいタブで開く",
            "エクスプローラーで表示",
            "| パスをコピー",
            "| ピン留め",
            "お気に入りに追加",
            "| プロパティ",
            "その他のオプションを確認"
        ]
    );
    menu(&ui, cx, "search:one");
    assert_eq!(lines(&ui, cx), ["開く", "新しいタブで開く", "| 検索文字列をコピー", "| 編集…", "ピン留め", "保存した検索を解除"]);
    // Nor do the keys copy from home.
    ui.press(cx, "escape");
    ui.click(cx, row(r"D:\a"));
    ui.press(cx, "left");
    ui.press(cx, "ctrl-c");
    assert!(ui.toasts(cx).iter().any(|t| t == "「コピー」はホームでは使えません。"), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn leaves_the_order_as_it_was_when_let_go_outside_its_own_kind(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    let (a, one) = (ui.home_row(cx, r"D:\a"), ui.home_row(cx, "search:one"));
    ui.hold_drag(cx, a.center(), upper(one));
    assert_eq!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).landing().cloned()), None);
    ui.let_go(cx, upper(one));
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\a", r"D:\b", r"D:\c"]);
    // Nor over the columns.
    let (c, columns) = (ui.home_row(cx, r"D:\c"), point(px(900.), px(300.)));
    ui.hold_drag(cx, c.center(), columns);
    ui.let_go(cx, columns);
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\a", r"D:\b", r"D:\c"]);
    assert!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).carried().is_none()));
}

#[gpui_kit::test]
fn puts_down_a_drag_in_home_with_escape(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    let (a, c) = (ui.home_row(cx, r"D:\a"), ui.home_row(cx, r"D:\c"));
    ui.hold_drag(cx, c.center(), upper(a));
    assert!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).landing().is_some()));
    ui.press(cx, "escape");
    assert!(ui.read_app(cx, |v, cx| v.home_drag.read(cx).carried().is_none() && v.home_drag.read(cx).landing().is_none()));
    ui.let_go(cx, upper(a));
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), [r"D:\a", r"D:\b", r"D:\c"]);
}

#[gpui_kit::test]
fn scrolls_home_while_a_drag_is_held_at_its_bottom_edge(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake { roots: vec![folder(r"D:\")], ..Fake::default().with_folders(&[(r"D:\", vec![])]) });
    fake.settings.lock().unwrap().saved_searches = (0..50)
        .map(|i| SavedSearch { name: String::new(), query: format!("query{i}"), filter: None, narrow: None, order: Default::default() })
        .collect();
    let ui = open(cx, fake.clone());
    let list = ui.act(cx, |window, _| window.find("home-rows").bounds());
    let first = ui.home_row(cx, "search:query0");
    let edge = point(first.center().x, list.bottom() - px(4.));
    ui.hold_drag(cx, first.center(), edge);
    cx.executor().advance_clock(Duration::from_millis(500));
    cx.run_until_parked();
    assert!(ui.read(cx, |v| v.scrolls.home.offset().y < px(0.)));
    // Let go there, it lands among the rows scrolled to.
    ui.let_go(cx, edge);
    assert_ne!(fake.settings.lock().unwrap().saved_searches[0].query, "query0");
}

#[gpui_kit::test]
fn saves_a_search_with_its_filter_and_runs_it_within_it(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\cat.jpg")])));
    let ui = open(cx, fake.clone());
    ui.click(cx, "search-filter");
    ui.click(cx, crate::search_filters::filter_tool_id("filter", "画像"));
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    ui.press(cx, "ctrl-d");
    let saved = |filter: Option<&str>| SavedSearch {
        name: String::new(),
        query: "cat".into(),
        filter: filter.map(Into::into),
        narrow: None,
        order: Order::default(),
    };
    assert_eq!(fake.settings.lock().unwrap().saved_searches, [saved(Some("画像"))]);
    // The same query with no filter is another search, saved apart.
    ui.click(cx, "search-filter");
    ui.click(cx, "filter-none");
    assert_eq!(ui.read(cx, |v| (v.state.query.clone(), v.state.search_filter.clone())), ("cat".into(), None));
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-d");
    assert_eq!(fake.settings.lock().unwrap().saved_searches, [saved(Some("画像")), saved(None)]);
    // Run from home, it chooses its filter again.
    ui.show_home(cx);
    ui.click(cx, row(HomeKey::search("cat", Some("画像"), None, Order::default()).key()));
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("画像"));
    assert_eq!(last_query(&fake), "<ext:jpg;png> <cat>");
    // Back, the filter of the search before comes back with it.
    ui.press(cx, "alt-left");
    assert_eq!(ui.read(cx, |v| v.state.search_filter.clone()), None);
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter, None);
    // Its query alone is copied.
    ui.click(cx, row(HomeKey::search("cat", Some("画像"), None, Order::default()).key()));
    ui.press(cx, "ctrl-shift-c");
    assert_eq!(*fake.text.lock().unwrap(), "cat");
}

#[gpui_kit::test]
fn changes_the_filter_a_saved_search_runs_within_from_its_edit(cx: &mut TestAppContext) {
    let fake = Arc::new(with_filters(Fake::default().with_results(vec![file(r"D:\cat.jpg")])));
    fake.settings.lock().unwrap().saved_searches =
        vec![SavedSearch {
            name: String::new(), query: "cat".into(), filter: Some("画像".into()), narrow: None, order: Order::default()
        }];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(HomeKey::search("cat", Some("画像"), None, Order::default()).key()));
    ui.press(cx, "f2");
    ui.click(cx, crate::search_filters::filter_tool_id("edit-search-filter", "動画"));
    ui.click(cx, "save-search");
    assert_eq!(fake.settings.lock().unwrap().saved_searches[0].filter.as_deref(), Some("動画"));
    // The search showing stays within the filter it ran within.
    assert_eq!(last_query(&fake), "<ext:jpg;png> <cat>");
    assert_eq!(fake.settings.lock().unwrap().tabs[0].filter.as_deref(), Some("画像"));
}

/// Home's pane out beside the columns, rather than folded into its strip.
fn home_out(ui: &Ui, cx: &mut TestAppContext) -> bool {
    rest(ui, cx);
    let (pane, strip) = ui.act(cx, |window, _| (window.try_find("sidebar").is_some(), window.try_find("home-strip").is_some()));
    assert_ne!(pane, strip, "home's pane or its strip, one at a time");
    pane
}

#[gpui_kit::test]
fn folds_home_into_its_strip_and_brings_it_out_while_the_keys_are_there(cx: &mut TestAppContext) {
    let fake = Arc::new(drive_d());
    let ui = open(cx, fake.clone());
    ui.click_in(cx, "home", row(r"D:\"));
    // The keys in it: out.
    assert!(ui.home_focused(cx) && home_out(&ui, cx));
    // Gone from it, it folds.
    ui.press(cx, "right");
    assert!(!home_out(&ui, cx));
    // Its strip brings the keys back, and it with them.
    ui.click(cx, "home-strip-button");
    assert!(ui.home_focused(cx) && home_out(&ui, cx));
    ui.press(cx, "right");
    assert!(!home_out(&ui, cx));
    // ← from the first column, and Ctrl+0, as well.
    ui.press(cx, "left");
    assert!(ui.home_focused(cx) && home_out(&ui, cx));
    ui.press(cx, "right");
    assert!(!home_out(&ui, cx));
    ui.press(cx, "ctrl-0");
    assert!(home_out(&ui, cx));
}

/// What the status bar says of where the work is.
fn status_place(ui: &Ui, cx: &mut TestAppContext) -> String {
    ui.act(cx, |window, _| window.try_find("place").and_then(|place| place.label().map(str::to_owned))).unwrap_or_default()
}

#[gpui_kit::test]
fn tells_of_the_columns_in_the_status_bar_while_home_worked_in_is_folded_away(cx: &mut TestAppContext) {
    let fake = Arc::new(drive_d());
    let ui = open(cx, fake.clone());
    ui.click_in(cx, "home", row(r"D:\"));
    // Out with the keys in it: home itself.
    assert!(home_out(&ui, cx));
    assert_eq!(status_place(&ui, cx), crate::words::home());
    // The keys gone to the search field, home is still the column worked in, but folded away out of sight:
    // what the bar tells of is the columns left to see, as the address does.
    ui.press(cx, "ctrl-f");
    assert!(!home_out(&ui, cx) && ui.active(cx) == 0);
    assert_eq!(status_place(&ui, cx), r"D:\");
}

#[gpui_kit::test]
fn keeps_home_out_while_its_menu_is_up_and_while_the_tab_has_nothing_else(cx: &mut TestAppContext) {
    let fake = Arc::new(drive_d().with_results(vec![file(r"D:\abc.txt")]));
    let ui = open(cx, fake.clone());
    // Nothing but home, the keys in the search field: out.
    assert!(ui.search_focused(cx) && home_out(&ui, cx));
    // Results beside it, the keys still in the field: folded.
    ui.input(cx, "abc");
    assert!(!home_out(&ui, cx));
    // A new tab has nothing but home.
    ui.press(cx, "ctrl-t");
    assert!(home_out(&ui, cx));
    ui.press(cx, "ctrl-shift-tab");
    assert!(!home_out(&ui, cx));
    // A menu opened from home gives the keys back there: out while it is up, and once it is gone.
    ui.press(cx, "ctrl-0");
    ui.act(cx, |window, cx| window.within("home").right_click(row(r"D:\"), cx));
    settle(cx);
    assert!(ui.read(cx, |v| v.menu().is_some()) && home_out(&ui, cx));
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.menu().is_none()) && ui.home_focused(cx) && home_out(&ui, cx));
}

#[gpui_kit::test]
fn frames_home_worked_in_whole_at_rest_and_as_it_comes_out(cx: &mut TestAppContext) {
    let fake = Arc::new(drive_d());
    let ui = open(cx, fake.clone());
    allow_motion(cx);
    ui.click_in(cx, "home", row(r"D:\"));
    ui.press(cx, "ctrl-0");
    let frame = |ui: &Ui, cx: &mut TestAppContext| ui.act(cx, |window, _| window.find("home-frame").bounds());
    // Out and at rest: its top on the path bar's line.
    assert!(home_out(&ui, cx));
    let rested = frame(&ui, cx);
    // Coming out of its strip: its right on the edge it has come to.
    ui.press(cx, "right");
    assert!(!home_out(&ui, cx));
    ui.press(cx, "ctrl-0");
    assert!(ui.read(cx, |v| v.home_pane.moving()), "still coming out");
    let (moving, out, pane) =
        ui.act(cx, |window, _| (window.find("home-frame").bounds(), window.find("home-out").bounds(), window.find("sidebar").bounds()));
    assert_eq!((moving.top(), moving.right()), (rested.top(), out.right() - px(1.)));
    assert!(moving.right() < pane.right());
    assert!(home_out(&ui, cx));
    assert_eq!(frame(&ui, cx), rested);
}

#[gpui_kit::test]
fn lets_go_of_the_strip_hovered_when_the_pointer_leaves_the_window_off_it(cx: &mut TestAppContext) {
    use gpui_kit::{MouseExitEvent, MouseMoveEvent, PlatformInput};
    let fake = drive_d();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    ui.press(cx, "right");
    assert!(!home_out(&ui, cx));
    // Off the window on its left, as onto a taskbar there: what Windows' leaving now tells the window.
    let off = point(px(-5.), px(300.));
    let leave = |ui: &Ui, cx: &mut TestAppContext| {
        ui.act(cx, |window, cx| {
            let modifiers = Modifiers::default();
            window.dispatch_event(PlatformInput::MouseMove(MouseMoveEvent { position: off, pressed_button: None, modifiers }), cx);
            window.dispatch_event(PlatformInput::MouseExited(MouseExitEvent { position: off, pressed_button: None, modifiers }), cx);
        });
        settle(cx);
    };
    // Each quad drawn, as where it lies and what fills it.
    let painted = |ui: &Ui, cx: &mut TestAppContext| {
        ui.act(cx, |window, cx| {
            let scale = window.scale_factor();
            let hover = cx.palette().hover;
            let quads = window
                .painted_quads()
                .into_iter()
                .map(|q| (q.bounds.origin.x.0 / scale, q.bounds.origin.y.0 / scale, format!("{:?}", q.background)));
            (quads.collect::<Vec<_>>(), format!("{:?}", gpui_kit::Background::from(hover)))
        })
    };
    leave(&ui, cx);
    let (away, hover) = painted(&ui, cx);
    let strip = ui.act(cx, |window, _| window.find("home-strip").bounds());
    // Under the pointer: the strip lit, and its tip out beside it.
    ui.act(cx, |window, cx| window.hover("home-strip-button", cx));
    cx.executor().advance_clock(Duration::from_secs(1));
    settle(cx);
    let (over, _) = painted(&ui, cx);
    let lit = |quads: &[(f32, f32, String)]| {
        quads.iter().any(|(x, y, fill)| *fill == hover && *x == f32::from(strip.left()) && *y == f32::from(strip.top()))
    };
    // Anything else drawn that was not there away from it: the tip, which starts over the strip's edge.
    let tip = |quads: &[(f32, f32, String)]| {
        quads.iter().filter(|q| !away.contains(q)).any(|(x, y, _)| (*x, *y) != (f32::from(strip.left()), f32::from(strip.top())))
    };
    assert!(!lit(&away) && lit(&over), "the strip lit under the pointer");
    assert!(tip(&over), "its tip out beside it");
    // Left off it to outside the window: neither stays.
    leave(&ui, cx);
    let (left, _) = painted(&ui, cx);
    assert!(!lit(&left), "the strip still lit");
    assert!(!tip(&left), "its tip still out");
}
