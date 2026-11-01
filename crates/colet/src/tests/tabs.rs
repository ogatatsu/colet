//! The tabs: opening, switching, closing and bringing back, and what they keep.
use super::*;

impl Ui {
    fn tabs(&self, cx: &mut TestAppContext) -> (usize, usize) {
        self.read(cx, |v| (v.tabs.list.len(), v.tabs.current))
    }

    fn paths(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |v| v.state.columns.iter().skip(1).map(|c| text(&c.path)).collect())
    }
}

#[gpui_kit::test]
fn opens_a_tab_of_home_alone_and_goes_back_to_the_one_before_as_it_was(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-t");
    assert_eq!(ui.tabs(cx), (2, 1));
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 1);
    assert!(ui.read(cx, |v| v.search_focused));
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
    let (first, opened) = ui.read(cx, |v| (v.tabs.list[0].id, v.tabs.list[1].id));
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
    let middle = ui.read(cx, |v| v.tabs.list[1].id);
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
    assert_eq!(tabs[0].place.picks.iter().map(|p| p.as_deref().map(text)).collect::<Vec<_>>(), [Some(r"D:\".into()), Some(r"D:\work".into()), None]);
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
        let input = self.view.read_with(cx, |v, _| v.filters.get(&id).map(|field| field.input.clone()));
        input.is_some_and(|input| self.has_focus(cx, &input))
    }

    fn typed(&self, cx: &mut TestAppContext, index: usize) -> String {
        let id = self.read(cx, |v| v.state.columns[index].id);
        self.view.read_with(cx, |v, cx| v.filters[&id].input.read(cx).value().to_string())
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
    use crate::state::favorite_key;
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![file(r"D:\work\a.pdf"), file(r"D:\work\b.txt")]),
        ])
    });
    fake.settings.lock().unwrap().favorites = vec![Favorite { narrow: Some("pdf".into()), ..Favorite::new(r"D:\work") }];
    let ui = open(cx, fake);
    let key = favorite_key(Path::new(r"D:\work"), Some("pdf"));
    ui.view.update(cx, |v, cx| v.open_in_tab(0, &key, cx));
    settle(cx);
    ui.press(cx, "ctrl-tab");
    let last = ui.read(cx, |v| v.state.columns.len() - 1);
    assert!(ui.filter_focused(cx, last));
    ui.input(cx, " a");
    assert_eq!(ui.typed(cx, last), "pdf a");
}
