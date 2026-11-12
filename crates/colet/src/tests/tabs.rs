//! The tabs: opening, switching, closing and bringing back, and what they keep.
use super::*;

impl Ui {
    fn tabs(&self, cx: &mut TestAppContext) -> (usize, usize) {
        self.read(cx, |v| (v.tabs.len(), v.tabs.current()))
    }

    fn paths(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |v| v.state.columns.iter().skip(1).map(|c| text(&c.path)).collect())
    }
}

#[gpui_kit::test]
fn reveals_the_restored_tab_and_its_curves_at_either_end_of_the_strip(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.tabs = vec![crate::settings::KeptTab::default(); 12];
        settings.tab = 11;
    }
    let ui = open(cx, fake);
    let assert_revealed = |cx: &mut TestAppContext| {
        let id = ui.read(cx, |v| v.current_tab());
        let tab = ui.act(cx, |window, _| window.find(("tab", id as usize)).bounds());
        let viewport = ui.read(cx, |v| v.tabs.scroll().bounds());
        assert!(tab.left() - px(8.) >= viewport.left() - px(0.5), "{tab:?} in {viewport:?}");
        assert!(tab.right() + px(8.) <= viewport.right() + px(0.5), "{tab:?} in {viewport:?}");
    };
    assert_revealed(cx);
    cx.simulate_window_resize(ui.window, size(px(720.), px(480.)));
    ui.press(cx, "ctrl-tab");
    assert_revealed(cx);
    ui.press(cx, "ctrl-shift-tab");
    assert_revealed(cx);
    ui.click(cx, "new-tab");
    assert_eq!(ui.tabs(cx), (13, 12));
    assert_revealed(cx);
}

#[gpui_kit::test]
fn hover_decorations_leave_tab_switching_and_both_close_buttons_pressable(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.tabs = vec![crate::settings::KeptTab::default(); 4];
        settings.tab = 1;
    }
    let ui = open(cx, fake);
    let ids = ui.read(cx, |v| v.tabs.iter().map(|tab| tab.id()).collect::<Vec<_>>());
    // The hovered tab shares a separator region with the one to its right, beside the selected tab.
    ui.act(cx, |window, cx| window.hover(("tab", ids[2] as usize), cx));
    ui.click(cx, ("tab-close", ids[2] as usize));
    assert_eq!(ui.tabs(cx), (3, 1));
    assert_eq!(ui.read(cx, |v| v.current_tab()), ids[1]);
    ui.act(cx, |window, cx| window.hover(("tab", ids[3] as usize), cx));
    ui.click(cx, ("tab", ids[3] as usize));
    assert_eq!(ui.read(cx, |v| v.current_tab()), ids[3]);
    // The selected tab is painted in front, and its close button still receives the click once.
    ui.click(cx, ("tab-close", ids[3] as usize));
    assert_eq!(ui.tabs(cx), (2, 1));
    assert_eq!(ui.read(cx, |v| v.current_tab()), ids[1]);
}

#[gpui_kit::test]
fn opens_a_tab_of_home_alone_and_goes_back_to_the_one_before_as_it_was(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    assert_eq!(ui.tabs(cx), (2, 1));
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 1);
    assert!(ui.search_focused(cx));
    ui.input(cx, "cat");
    assert_eq!(ui.field(cx), "cat");
    // Round from the end, back to the first, with its columns, its pick and its keys.
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.tabs(cx), (2, 0));
    assert_eq!(ui.paths(cx), [r"D:\", r"D:\work"]);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    assert_eq!(ui.field(cx), "");
    assert!(ui.columns_focused(cx));
    ui.press(cx, "ctrl-shift-tab");
    assert_eq!(ui.field(cx), "cat");
    assert!(ui.read(cx, |v| v.state.pinned()));
}

#[gpui_kit::test]
fn opens_the_row_the_keys_are_on_in_a_tab_behind_this_one_where_it_lives(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    ui.press(cx, "ctrl-tab");
    ui.press(cx, "ctrl-enter");
    // Right of this one, which stays in front.
    assert_eq!(ui.tabs(cx), (3, 0));
    let (first, opened) = ui.read(cx, |v| (v.tabs.get(0).unwrap().id(), v.tabs.get(1).unwrap().id()));
    assert!(opened > first + 1);
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.paths(cx), [r"D:\", r"D:\work"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].highlight.clone()).as_deref(), Some(Path::new(r"D:\work\a.txt")));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    assert!(!ui.read(cx, |v| v.state.can_go_back()));
}

#[gpui_kit::test]
fn closes_the_tab_in_front_for_the_one_right_of_it_and_brings_it_back(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    ui.input(cx, "cat");
    ui.press(cx, "ctrl-t");
    // Back to the first, then closed: the one right of it comes to the front.
    ui.press(cx, "ctrl-tab");
    ui.press(cx, "ctrl-w");
    assert_eq!(ui.tabs(cx), (2, 0));
    assert_eq!(ui.field(cx), "cat");
    // Brought back where it stood, as it was.
    ui.press(cx, "ctrl-shift-t");
    assert_eq!(ui.tabs(cx), (3, 0));
    assert_eq!(ui.paths(cx), [r"D:\", r"D:\work"]);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    // The last one closed is the one at the end: the one left of it comes to the front.
    ui.press(cx, "ctrl-shift-tab");
    ui.press(cx, "ctrl-w");
    assert_eq!(ui.tabs(cx), (2, 1));
    assert_eq!(ui.field(cx), "cat");
}

#[gpui_kit::test]
fn closes_the_others_from_a_tabs_menu(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    ui.press(cx, "ctrl-t");
    let middle = ui.read(cx, |v| v.tabs.get(1).unwrap().id());
    ui.act(cx, |window, cx| window.right_click(("tab", middle as usize), cx));
    settle(cx);
    ui.click(cx, "tab-close-others");
    assert_eq!(ui.tabs(cx), (1, 0));
    assert_eq!(ui.read(cx, |v| v.current_tab()), middle);
}

#[gpui_kit::test]
fn lists_a_tab_again_only_as_it_comes_to_the_front(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    let before = listings_of(&fake, r"D:\work");
    ui.press(cx, "f5");
    assert_eq!(listings_of(&fake, r"D:\work"), before);
    ui.press(cx, "ctrl-tab");
    assert_eq!(listings_of(&fake, r"D:\work"), before + 1);
}

#[gpui_kit::test]
fn keeps_the_tabs_in_the_settings(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    ui.input(cx, "cat");
    let (tabs, front) = {
        let settings = fake.settings.lock().unwrap();
        (settings.tabs.clone(), settings.tab)
    };
    assert_eq!(front, 1);
    assert_eq!(tabs.len(), 2);
    let folders: Vec<String> = tabs[0].place.folders.iter().map(|(path, _)| text(path)).collect();
    assert_eq!(folders, [r"D:\", r"D:\work"]);
    // Only what leads on is kept: the file picked last is not.
    assert_eq!(
        tabs[0].place.picks.iter().map(|p| p.as_deref().map(text)).collect::<Vec<_>>(),
        [Some(r"D:\".into()), Some(r"D:\work".into()), None]
    );
    assert_eq!(tabs[1].place.query, "cat");
    // Started again from them, the folders open as they were.
    let state = crate::state::State::from_kept(tabs[0].place.clone());
    assert_eq!(state.columns.iter().skip(1).map(|c| text(&c.path)).collect::<Vec<_>>(), [r"D:\", r"D:\work"]);
    assert_eq!(state.active, 2);
    drop(ui);
}

impl Ui {
    /// The field narrowing the column at `index` has the keys.
    fn filter_focused(&self, cx: &mut TestAppContext, index: usize) -> bool {
        let id = self.read(cx, |v| v.state.columns[index].id);
        let input = self.view.read_with(cx, |v, _| v.narrow_fields.get(&id).map(|field| field.input.clone()));
        input.is_some_and(|input| self.has_focus(cx, &input))
    }

    fn typed(&self, cx: &mut TestAppContext, index: usize) -> String {
        let id = self.read(cx, |v| v.state.columns[index].id);
        self.view.read_with(cx, |v, cx| v.narrow_fields[&id].input.read(cx).value().to_string())
    }
}

#[gpui_kit::test]
fn opens_a_folder_in_a_tab_with_the_keys_in_its_field_and_keeps_them_there(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "left");
    ui.press(cx, "ctrl-enter");
    // In front, the folder opened takes the keys in its field.
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.paths(cx), [r"D:\", r"D:\work"]);
    assert_eq!(ui.read(cx, |v| v.state.active), 2);
    assert!(ui.filter_focused(cx, 2));
    ui.input(cx, "b");
    assert_eq!(ui.typed(cx, 2), "b");
    // Away and back, they are there still; in the other tab, on its rows.
    ui.press(cx, "ctrl-tab");
    assert!(ui.columns_focused(cx));
    ui.press(cx, "ctrl-tab");
    assert!(ui.filter_focused(cx, 2));
    assert_eq!(ui.typed(cx, 2), "b");
}

#[gpui_kit::test]
fn opens_a_file_in_a_tab_with_the_keys_on_its_row(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-enter");
    ui.press(cx, "ctrl-tab");
    assert!(ui.columns_focused(cx));
    assert!(!ui.filter_focused(cx, 2));
}

#[gpui_kit::test]
fn opens_a_favorite_narrowed_in_a_tab_with_the_caret_after_its_narrowing(cx: &mut TestAppContext) {
    use crate::settings::Favorite;
    use crate::state::HomeKey;
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default()
            .with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![file(r"D:\work\a.pdf"), file(r"D:\work\b.txt")])])
    });
    fake.settings.lock().unwrap().favorites = vec![Favorite { narrow: Some("pdf".into()), ..Favorite::new(r"D:\work") }];
    let ui = open(cx, fake);
    let key = HomeKey::favorite(Path::new(r"D:\work"), Some("pdf")).key();
    ui.view.update(cx, |v, cx| v.open_in_tab(0, &key, cx));
    settle(cx);
    // Named for the favorite it opened while still behind, not for home.
    let (glyph, name) = ui.read(cx, |v| {
        let title = crate::tabs::title(v.tabs.get(1).unwrap().parked().unwrap().state(), v.stored.settings());
        (title.glyph, title.name)
    });
    assert_eq!((glyph, name.as_str()), ("star-fill", "work"));
    ui.press(cx, "ctrl-tab");
    let last = ui.read(cx, |v| v.state.columns.len() - 1);
    assert!(ui.filter_focused(cx, last));
    ui.input(cx, " a");
    assert_eq!(ui.typed(cx, last), "pdf a");
    // Narrowed otherwise, it is the favorite no more.
    assert_eq!(ui.title(cx).0, "folder");
}

impl Ui {
    /// The front tab's icon, name and tip.
    fn title(&self, cx: &mut TestAppContext) -> (&'static str, String, String) {
        self.read(cx, |v| {
            let title = crate::tabs::title(&v.state, v.stored.settings());
            (title.glyph, title.name, title.tip)
        })
    }
}

#[gpui_kit::test]
fn names_a_tab_for_the_favorite_it_opened_however_deep_it_goes(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![file(r"D:\work\a.txt")])])
    });
    fake.settings.lock().unwrap().favorites =
        vec![crate::settings::Favorite { name: "データ".into(), ..crate::settings::Favorite::new(r"D:\") }];
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    // Picked in home, it is named so already.
    assert_eq!(ui.title(cx), ("star-fill", "データ".into(), r"D:\".into()));
    ui.press(cx, "right");
    ui.press(cx, "right");
    assert_eq!(ui.title(cx), ("star-fill", "データ".into(), r"D:\work".into()));
    // Renamed, it follows; kept no more, the column worked in names it.
    ui.view.update(cx, |v, cx| v.update_settings(|s| s.favorites[0].name = "倉庫".into(), cx));
    assert_eq!(ui.title(cx).1, "倉庫");
    ui.view.update(cx, |v, cx| v.update_settings(|s| s.favorites.clear(), cx));
    assert_eq!(ui.title(cx), ("folder", "work".into(), r"D:\work".into()));
}

#[gpui_kit::test]
fn names_a_tab_for_its_search_or_the_saved_one_it_runs_however_deep_it_goes(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![folder(r"D:\work")]).with_folders(&[(r"D:\work", vec![file(r"D:\work\a.txt")])]));
    fake.settings.lock().unwrap().saved_searches =
        vec![crate::settings::SavedSearch { name: "仕事".into(), query: "wo".into(), filter: None, narrow: None }];
    let ui = open(cx, fake);
    ui.input(cx, "wo");
    assert_eq!(ui.title(cx), ("search", "仕事".into(), "wo".into()));
    // Searched otherwise, it is the search no more.
    ui.input(cx, "r");
    assert_eq!(ui.title(cx), ("search", "wor".into(), "wor".into()));
    ui.click(cx, row(r"D:\work"));
    ui.press(cx, "right");
    assert_eq!(ui.title(cx), ("search", "wor".into(), r"D:\work".into()));
}

#[gpui_kit::test]
fn names_a_tab_for_the_place_picked_in_home_while_home_is_worked_in(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let drive = colet_core::paths::shown(colet_core::paths::folder_name(Path::new(r"D:\")));
    assert_eq!(ui.title(cx), ("folder", "work".into(), r"D:\work".into()));
    // Back in home, it is the place picked there, not the folder gone deepest into.
    ui.press(cx, "left");
    ui.press(cx, "left");
    assert_eq!(ui.active(cx), 0);
    assert_eq!(ui.title(cx), ("folder", drive, r"D:\".into()));
    // Nothing picked in home: it is home.
    ui.press(cx, "ctrl-t");
    assert_eq!(ui.title(cx), ("house", "ホーム".into(), "ホーム".into()));
}

impl Ui {
    fn tab_ids(&self, cx: &mut TestAppContext) -> Vec<crate::tabs::TabId> {
        self.read(cx, |v| v.tabs.iter().map(|tab| tab.id()).collect())
    }

    fn tab_bounds(&self, cx: &mut TestAppContext, id: crate::tabs::TabId) -> Bounds<gpui_kit::Pixels> {
        self.act(cx, |window, _| window.find(("tab", id as usize)).bounds())
    }

    fn tab_drag(&self, cx: &mut TestAppContext) -> (Option<crate::tabs::TabId>, Option<usize>) {
        self.read(cx, |v| (v.tabs.carried(), v.tabs.gap()))
    }
}

fn four_tabs(cx: &mut TestAppContext) -> Ui {
    let fake = Arc::new(Fake::default());
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.tabs = vec![crate::settings::KeptTab::default(); 4];
        settings.tab = 1;
    }
    open(cx, fake)
}

/// A tab's left or right quarter.
fn left_of(b: Bounds<gpui_kit::Pixels>) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(b.left() + b.size.width / 4., b.center().y)
}

fn right_of(b: Bounds<gpui_kit::Pixels>) -> gpui_kit::Point<gpui_kit::Pixels> {
    point(b.right() - b.size.width / 4., b.center().y)
}

#[gpui_kit::test]
fn drags_a_tab_into_the_gap_its_line_shows(cx: &mut TestAppContext) {
    let ui = four_tabs(cx);
    let ids = ui.tab_ids(cx);
    let (from, to) = (ui.tab_bounds(cx, ids[0]), ui.tab_bounds(cx, ids[2]));
    // Over the right half of a tab, the line goes after it; the tab carried fades where it was.
    ui.hold_drag(cx, from.center(), right_of(to));
    assert_eq!(ui.tab_drag(cx), (Some(ids[0]), Some(3)));
    ui.act(cx, |window, _| assert!(window.find("tab-landing").bounds().size.width > px(0.)));
    ui.let_go(cx, right_of(to));
    assert_eq!(ui.tab_ids(cx), [ids[1], ids[2], ids[0], ids[3]]);
    // The tab in front stays in front, wherever it now stands.
    assert_eq!(ui.read(cx, |v| v.current_tab()), ids[1]);
    assert_eq!(ui.tab_drag(cx), (None, None));

    // Over the left half, before it.
    let ids = ui.tab_ids(cx);
    let (from, to) = (ui.tab_bounds(cx, ids[3]), ui.tab_bounds(cx, ids[1]));
    ui.hold_drag(cx, from.center(), left_of(to));
    assert_eq!(ui.tab_drag(cx), (Some(ids[3]), Some(1)));
    ui.let_go(cx, left_of(to));
    assert_eq!(ui.tab_ids(cx), [ids[0], ids[3], ids[1], ids[2]]);
}

#[gpui_kit::test]
fn drags_a_tab_past_the_last_onto_the_room_beside_them(cx: &mut TestAppContext) {
    let ui = four_tabs(cx);
    let ids = ui.tab_ids(cx);
    let from = ui.tab_bounds(cx, ids[0]);
    let room = ui.act(cx, |window, _| window.find("new-tab").bounds().center());
    ui.hold_drag(cx, from.center(), room);
    assert_eq!(ui.tab_drag(cx), (Some(ids[0]), Some(4)));
    ui.let_go(cx, room);
    assert_eq!(ui.tab_ids(cx), [ids[1], ids[2], ids[3], ids[0]]);
}

#[gpui_kit::test]
fn leaves_the_tabs_as_they_were_let_go_of_off_the_bar(cx: &mut TestAppContext) {
    let ui = four_tabs(cx);
    let ids = ui.tab_ids(cx);
    let from = ui.tab_bounds(cx, ids[0]);
    let below = point(from.center().x, from.bottom() + px(200.));
    ui.hold_drag(cx, from.center(), below);
    // Off the bar, no line.
    assert_eq!(ui.tab_drag(cx), (Some(ids[0]), None));
    ui.let_go(cx, below);
    assert_eq!(ui.tab_ids(cx), ids);
    assert_eq!(ui.tab_drag(cx), (None, None));
}

#[gpui_kit::test]
fn leaves_the_tabs_as_they_were_let_go_of_off_the_window(cx: &mut TestAppContext) {
    let ui = four_tabs(cx);
    let ids = ui.tab_ids(cx);
    let (from, to) = (ui.tab_bounds(cx, ids[0]), ui.tab_bounds(cx, ids[2]));
    ui.hold_drag(cx, from.center(), right_of(to));
    assert_eq!(ui.tab_drag(cx), (Some(ids[0]), Some(3)));
    // The window keeps the pointer while a button is held, so it hears it let go of past its edge.
    ui.let_go(cx, point(px(-40.), px(-40.)));
    assert_eq!(ui.tab_ids(cx), ids);
    assert_eq!(ui.tab_drag(cx), (None, None));
}
