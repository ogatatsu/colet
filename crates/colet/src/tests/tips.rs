//! The tips that give a row's name, and a search result's folder, whole where the row cuts them short.
use super::*;

const LONG: &str = r"D:\work\a file name far longer than any column is wide enough to show in full, cut short.txt";
const SHORT: &str = r"D:\work\a.txt";

/// D:\work open in its column, listing a file whose name it cuts short and one it shows whole.
fn long_and_short(cx: &mut TestAppContext) -> Ui {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![file(LONG), file(SHORT)])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().path.to_str().unwrap().to_owned()), r"D:\work");
    ui
}

/// Rests the pointer on `id` long enough for a tip to come out; whether one did: one more box drawn in
/// the fill a tip takes than with the pointer on the room past the columns, which has no tip.
fn tip_on(ui: &Ui, cx: &mut TestAppContext, id: impl Into<ElementId>) -> bool {
    let mut rest_on = |id: ElementId| {
        ui.act(cx, |window, cx| window.hover(id, cx));
        cx.executor().advance_clock(Duration::from_secs(1));
        settle(cx);
        ui.act(cx, |window, cx| {
            use gpui_kit::component::ActiveTheme;
            let fill = format!("{:?}", gpui_kit::Background::from(cx.theme().tokens.popover));
            window.painted_quads().iter().filter(|q| format!("{:?}", q.background) == fill).count()
        })
    };
    let away = rest_on("columns-rest".into());
    rest_on(id.into()) > away
}

#[gpui_kit::test]
fn gives_a_name_cut_short_whole_and_nothing_for_one_shown_whole(cx: &mut TestAppContext) {
    let ui = long_and_short(cx);
    assert!(tip_on(&ui, cx, row(LONG)), "the name cut short given whole");
    assert!(!tip_on(&ui, cx, row(SHORT)), "nothing for a name shown whole");
}

#[gpui_kit::test]
fn gives_a_result_whose_folder_is_cut_short_though_its_name_fits(cx: &mut TestAppContext) {
    let deep = file(r"D:\a folder with a long name\and another under it with one longer still\and a third, to be sure it is cut\zeta.txt");
    let short = file(r"D:\b\zeta.c");
    let ui = open(cx, Arc::new(Fake::default().with_results(vec![deep.clone(), short.clone()])));
    ui.input(cx, "zeta");
    assert!(tip_on(&ui, cx, row(&deep.path)), "the folder cut short given whole");
    assert!(!tip_on(&ui, cx, row(&short.path)), "nothing for a result shown whole");
}

#[gpui_kit::test]
fn gives_no_name_while_a_menu_lies_over_the_rows(cx: &mut TestAppContext) {
    let ui = long_and_short(cx);
    ui.act(cx, |window, cx| window.right_click(row(SHORT), cx));
    settle(cx);
    assert!(ui.read(cx, |v| v.menu().is_some()), "the menu open");
    assert!(!tip_on(&ui, cx, row(LONG)));
}

#[gpui_kit::test]
fn gives_the_filter_a_saved_search_runs_within_on_its_badge_only_when_cut_short(cx: &mut TestAppContext) {
    let long = "去年の夏に海辺で撮ったとても大きな写真ばかり";
    let fake = Fake::default();
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.filters = vec![
            SearchFilter { name: long.into(), query: "ext:jpg".into() },
            SearchFilter { name: "画像".into(), query: "ext:png".into() },
        ];
        settings.saved_searches = [("cat", long), ("dog", "画像")]
            .map(|(query, filter)| SavedSearch {
                name: String::new(),
                query: query.into(),
                filter: Some(filter.into()),
                narrow: None,
                order: Order::default(),
            })
            .into();
    }
    let ui = open(cx, Arc::new(fake));
    let badge = |query, filter| row_part(HomeKey::search(query, Some(filter), None, Order::default()).key(), "within");
    assert!(tip_on(&ui, cx, badge("cat", long)), "the filter cut short given whole");
    assert!(!tip_on(&ui, cx, badge("dog", "画像")), "nothing for a filter shown whole");
}
