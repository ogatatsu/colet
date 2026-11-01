//! The file menu and the fields' menus.
use super::*;

#[gpui_kit::test]
fn offers_the_explorer_menu_only_for_items_in_one_folder(cx: &mut TestAppContext) {
    let results = vec![file(r"D:\a\x.txt"), file(r"D:\a\y.txt"), file(r"D:\b\z.txt"), file(r"\\nas\share\w.txt")];
    let fake = Arc::new(Fake::default().with_results(results));
    let ui = open(cx, fake);
    ui.input(cx, "x");
    ui.press(cx, "enter");
    let reasons = |cx: &mut TestAppContext| ui.read(cx, |v| (v.reasons().shell, v.reasons().copy));
    assert_eq!(ui.selected(cx, 1), [r"D:\a\x.txt"]);
    assert_eq!(reasons(cx), (None, None));
    ui.press(cx, "shift-down");
    assert_eq!(ui.selected(cx, 1), [r"D:\a\x.txt", r"D:\a\y.txt"]);
    assert_eq!(reasons(cx), (None, None));
    ui.press(cx, "ctrl-a");
    assert_eq!(reasons(cx), (Some("同じフォルダーの項目を選んだときに使えます"), None));
}

#[gpui_kit::test]
fn acts_from_the_menu_on_the_row_right_clicked_leaving_the_selection(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.act(cx, |window, cx| window.right_click(row(r"D:\work\c.txt"), cx));
    settle(cx);
    assert!(ui.read(cx, |v| v.menu().is_some()));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    ui.click(cx, "menu-copy");
    assert_eq!(texts(&fake.clipboard.lock().unwrap().paths), [r"D:\work\c.txt"]);
    assert!(ui.read(cx, |v| v.menu().is_none()));
    // The Windows menu for the folder itself, from its empty space.
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.act(cx, |window, cx| window.right_click(("column", work), cx));
    settle(cx);
    ui.click(cx, "menu-windows");
    assert_eq!(*fake.menus.lock().unwrap(), [(vec![], r"D:\work".to_owned())]);
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn acts_from_the_menu_on_the_row_right_clicked_over_the_selection_or_none(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let dialog_paths = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read(cx, |v| match &v.dialog {
            Some(crate::places::Dialog::Name { request: Request { paths, .. }, .. }) => texts(paths),
            _ => Vec::new(),
        })
    };
    let menu = |ui: &Ui, cx: &mut TestAppContext, path: &str, id: &str| {
        ui.act(cx, |window, cx| window.right_click(row(path), cx));
        settle(cx);
        ui.click(cx, id.to_owned());
    };
    // Another row than the selection: the rename is the one right-clicked's.
    menu(&ui, cx, r"D:\work\c.txt", "menu-rename");
    assert_eq!(dialog_paths(&ui, cx), [r"D:\work\c.txt"]);
    ui.press(cx, "escape");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    // Nothing selected in the column: the commands are on all the same.
    ui.view.update(cx, |v, cx| {
        v.state.columns[2].selected.clear();
        cx.notify();
    });
    settle(cx);
    assert!(ui.selected(cx, 2).is_empty());
    menu(&ui, cx, r"D:\work\b.txt", "menu-copy");
    assert_eq!(texts(&fake.clipboard.lock().unwrap().paths), [r"D:\work\b.txt"]);
    menu(&ui, cx, r"D:\work\b.txt", "menu-recycle");
    let asked = fake.operations.lock().unwrap().last().unwrap().clone();
    assert_eq!((asked.kind, asked.paths), (Operation::Recycle, vec![PathBuf::from(r"D:\work\b.txt")]));
}

#[gpui_kit::test]
fn walks_the_file_menu_by_keys_and_closes_it_on_escape(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    assert!(ui.read(cx, |v| v.menu().is_some()));
    // ↓ lights the first line, then passes over those that are off.
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(texts(&fake.clipboard.lock().unwrap().paths), [r"D:\work\a.txt"]);
    assert!(ui.read(cx, |v| v.menu().is_none()));
    assert!(ui.columns_focused(cx));
    ui.press(cx, "shift-f10");
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.menu().is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn opens_the_file_menu_on_the_menu_key_and_closes_it_on_either_menu_key(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "menu");
    assert!(ui.read(cx, |v| v.menu().is_some()));
    ui.press(cx, "menu");
    assert!(ui.read(cx, |v| v.menu().is_none()));
    assert!(ui.columns_focused(cx));
    ui.press(cx, "shift-f10");
    ui.press(cx, "shift-f10");
    assert!(ui.read(cx, |v| v.menu().is_none()));
    assert!(ui.columns_focused(cx));
    // A field's menu closes on it too.
    ui.press(cx, "ctrl-f");
    ui.input(cx, "a");
    ui.press(cx, "menu");
    assert!(ui.read(cx, |v| v.text_menu().is_some()));
    ui.press(cx, "menu");
    assert!(ui.read(cx, |v| v.popup.is_none()));
    assert!(ui.search_field_focused(cx));
}

#[gpui_kit::test]
fn closes_the_menu_and_returns_the_keys_when_its_column_vanishes(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    fake.remove(Path::new(r"D:\work"));
    let changed = ui.read(cx, |v| v.changed.clone());
    *changed.lock().unwrap() = Changes::Paths(HashSet::from([r"D:\work".into()]));
    settle(cx);
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    assert!(ui.read(cx, |v| v.menu().is_none()));
    assert!(ui.columns_focused(cx));
    assert!(fake.menus.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn dismisses_a_removed_columns_menu_before_another_frame(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.view.update(cx, |v, cx| {
        v.state.select(1, vec![r"D:\other".into()]);
        v.changed(cx);
        assert!(v.menu().is_none());
    });
    assert!(cx.update_window(ui.window, |_, window, cx| ui.view.read(cx).columns_focus.is_focused(window)).unwrap());
}

#[gpui_kit::test]
fn dismissing_a_removed_columns_menu_keeps_focus_given_to_a_field(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| {
        v.state.select(1, vec![r"D:\other".into()]);
        v.changed(cx);
        v.search.update(cx, |input, cx| input.focus(window, cx));
    }));
    assert!(cx.update_window(ui.window, |_, window, cx| {
        ui.view.read(cx).search.read(cx).focus_handle(cx).is_focused(window)
    }).unwrap());
    assert!(ui.read(cx, |v| v.menu().is_none()));
}

#[gpui_kit::test]
fn refuses_a_stale_menu_even_when_another_column_takes_its_index(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| {
        let column = v.menu().unwrap().column;
        v.state.select(1, vec![r"D:\other".into()]);
        assert_ne!(v.state.columns[2].id, column);
        v.show_shell_menu(window, cx);
    }));
    assert!(fake.menus.lock().unwrap().is_empty());
    assert!(ui.read(cx, |v| v.menu().is_none() && v.menu_view.is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn showing_the_shell_menu_directly_returns_the_keys_to_its_column(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-f10");
    ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| v.show_shell_menu(window, cx)));
    settle(cx);
    assert_eq!(fake.menus.lock().unwrap().len(), 1);
    assert!(ui.read(cx, |v| v.menu().is_none() && v.menu_view.is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn edits_the_search_field_from_its_menu(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default().with_results(vec![])));
    let search = ui.read(cx, |v| v.search.clone());
    // Empty, and nothing on the clipboard: only undoing is on. Esc gives the keys back to the field.
    ui.right_click_field(cx, &search);
    assert!(ui.read(cx, |v| v.text_menu().is_some()));
    assert_eq!(ui.lines_on(cx), ["元に戻す"]);
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.popup.is_none()));
    assert!(ui.has_focus(cx, &search));
    ui.input(cx, "cat");
    ui.press(cx, "ctrl-a");
    ui.right_click_field(cx, &search);
    assert_eq!(ui.lines_on(cx), ["元に戻す", "切り取り", "コピー", "削除", "すべて選択"]);
    // The field still counts as having the keys, its text picked, while its menu has them.
    assert!(ui.read(cx, |v| v.search_focused));
    assert_eq!(search.read_with(cx, |input, _| input.selected_range()), 0..3);
    ui.click(cx, "text-cut");
    assert_eq!(ui.field(cx), "");
    assert_eq!(clipboard_text(cx).as_deref(), Some("cat"));
    assert!(ui.has_focus(cx, &search));
    // What is on the clipboard now pastes.
    ui.right_click_field(cx, &search);
    assert!(ui.lines_on(cx).contains(&"貼り付け".to_owned()));
    ui.click(cx, "text-paste");
    assert_eq!(ui.field(cx), "cat");
    ui.right_click_field(cx, &search);
    ui.click(cx, "text-undo");
    assert_eq!(ui.field(cx), "");
}

#[gpui_kit::test]
fn opens_a_fields_menu_from_the_keys(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default().with_results(vec![])));
    let search = ui.read(cx, |v| v.search.clone());
    ui.input(cx, "cat");
    ui.press(cx, "shift-f10");
    assert!(ui.read(cx, |v| v.text_menu().is_some()));
    ui.press(cx, "down");
    // Down passes over what is off, to picking all.
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| v.popup.is_none()));
    assert!(ui.has_focus(cx, &search));
    assert_eq!(search.read_with(cx, |input, _| input.selected_range()), 0..3);
    // Deleting from it takes the text picked.
    ui.press(cx, "menu");
    assert!(ui.read(cx, |v| v.text_menu().is_some()));
    ui.click(cx, "text-delete");
    assert_eq!(ui.field(cx), "");
}

#[gpui_kit::test]
fn copies_from_a_dialogs_field_with_its_menu_over_the_dialog(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match &v.dialog {
        Some(Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.press(cx, "ctrl-a");
    ui.right_click_field(cx, &input);
    assert!(ui.read(cx, |v| v.text_menu().is_some_and(|menu| menu.input == input)));
    ui.click(cx, "text-copy");
    assert_eq!(clipboard_text(cx).as_deref(), Some("a.txt"));
    assert!(ui.read(cx, |v| v.dialog.is_some()));
    assert!(ui.has_focus(cx, &input));
    // The menu key in the field opens it too, and Esc closes only the menu.
    ui.press(cx, "shift-f10");
    assert!(ui.read(cx, |v| v.text_menu().is_some()));
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.popup.is_none() && v.dialog.is_some()));
    assert!(ui.has_focus(cx, &input));
}

#[gpui_kit::test]
fn keeps_what_is_picked_in_a_field_and_its_caret_on_a_right_click_anywhere_in_it(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default().with_results(vec![])));
    let search = ui.read(cx, |v| v.search.clone());
    ui.input(cx, "cat");
    ui.press(cx, "home");
    ui.press(cx, "shift-right");
    ui.press(cx, "shift-right");
    // Past the text picked, which a field of Windows' own leaves as it is.
    ui.right_click_field(cx, &search);
    assert!(ui.read(cx, |v| v.text_menu().is_some()));
    assert_eq!(search.read_with(cx, |input, _| input.selected_range()), 0..2);
    ui.click(cx, "text-copy");
    assert_eq!(clipboard_text(cx).as_deref(), Some("ca"));
    // With nothing picked, the caret stays where it was.
    ui.press(cx, "right");
    ui.press(cx, "left");
    ui.right_click_field(cx, &search);
    ui.press(cx, "escape");
    assert_eq!(search.read_with(cx, |input, _| input.selected_range()), 1..1);
}

#[gpui_kit::test]
fn takes_the_keys_into_a_field_right_clicked_from_elsewhere_with_what_it_had_picked(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let search = ui.read(cx, |v| v.search.clone());
    ui.press(cx, "ctrl-f");
    ui.input(cx, "cat");
    ui.act(cx, |window, cx| search.update(cx, |input, cx| input.select_all(window, cx)));
    ui.press(cx, "down");
    assert!(!ui.has_focus(cx, &search));
    ui.right_click_field(cx, &search);
    assert!(ui.read(cx, |v| v.text_menu().is_some_and(|menu| menu.input == search)));
    assert_eq!(search.read_with(cx, |input, _| input.selected_range()), 0..3);
    ui.press(cx, "escape");
    assert!(ui.has_focus(cx, &search));
}
