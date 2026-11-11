//! Windows' properties of what is picked, from the file menu, the button and Alt+Enter.
use super::*;

/// What each properties dialog was asked to show, in order.
fn shown(fake: &Fake) -> Vec<Vec<String>> {
    fake.properties.lock().unwrap().clone()
}

#[gpui_kit::test]
fn shows_the_properties_of_the_row_right_clicked_or_of_the_folder_from_its_empty_space(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.act(cx, |window, cx| window.right_click(row(r"D:\work\c.txt"), cx));
    settle(cx);
    // Last but the Windows menu, the two in a group of their own.
    let tail = ui.read_app(cx, |v, app| {
        v.menu_lines(app).into_iter().rev().take(2).map(|l| (l.text.to_string(), l.group, l.off)).collect::<Vec<_>>()
    });
    assert_eq!(tail, [("その他のオプションを確認".to_owned(), false, None), ("プロパティ".to_owned(), true, None)]);
    ui.click(cx, "menu-properties");
    settle(cx);
    assert_eq!(shown(&fake), [vec![r"D:\work\c.txt".to_owned()]]);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    assert!(ui.columns_focused(cx));
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.act(cx, |window, cx| window.right_click(("column", work), cx));
    settle(cx);
    ui.click(cx, "menu-properties");
    settle(cx);
    assert_eq!(shown(&fake).last().unwrap(), &[r"D:\work".to_owned()]);
}

#[gpui_kit::test]
fn shows_the_properties_of_all_picked_together_by_keys_and_the_button(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-a");
    ui.press(cx, "alt-enter");
    settle(cx);
    let all = vec![r"D:\work\a.txt".to_owned(), r"D:\work\b.txt".into(), r"D:\work\c.txt".into()];
    assert_eq!(shown(&fake), std::slice::from_ref(&all));
    // The button leaves the keys in the column.
    ui.click(cx, "properties");
    settle(cx);
    assert_eq!(shown(&fake), [all.clone(), all]);
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn shows_the_properties_of_results_from_several_folders_together(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_results(vec![file(r"D:\a\x.txt"), file(r"D:\b\y.txt")])
    });
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    ui.press(cx, "down");
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.read_app(cx, |v, app| v.reasons(app).properties), None);
    ui.press(cx, "alt-enter");
    settle(cx);
    assert_eq!(shown(&fake), [vec![r"D:\a\x.txt".to_owned(), r"D:\b\y.txt".into()]]);
    // The results stand in no folder: with nothing picked it is off, and says why.
    ui.view.update(cx, |v, cx| {
        v.state.columns[1].selected.clear();
        cx.notify();
    });
    settle(cx);
    assert_eq!(ui.read_app(cx, |v, app| v.reasons(app).properties), Some(crate::ops::NEEDS_FOLDER));
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.act(cx, |window, cx| window.right_click(("column", results), cx));
    settle(cx);
    let line = ui.read_app(cx, |v, app| v.menu_lines(app).into_iter().find(|l| l.text == "プロパティ").map(|l| l.off));
    assert_eq!(line, Some(Some("")));
    ui.press(cx, "escape");
    ui.press(cx, "alt-enter");
    assert!(ui.toasts(cx).iter().any(|t| t == "「プロパティ」はフォルダーのカラムで使えます"), "{:?}", ui.toasts(cx));
    assert_eq!(shown(&fake).len(), 1);
}

#[gpui_kit::test]
fn shows_the_properties_of_a_drive_or_folder_in_home_but_not_of_a_saved_search(cx: &mut TestAppContext) {
    let (fake, ui) = ordered_home(cx);
    ui.act(cx, |window, cx| window.within("home").right_click(row(r"D:\"), cx));
    settle(cx);
    ui.click(cx, "menu-properties");
    settle(cx);
    ui.act(cx, |window, cx| window.within("home").right_click(row(r"D:\a"), cx));
    settle(cx);
    ui.click(cx, "menu-properties");
    settle(cx);
    assert_eq!(shown(&fake), [vec![r"D:\".to_owned()], vec![r"D:\a".to_owned()]]);
    // A saved search has none: not in its menu, and off by the keys.
    ui.act(cx, |window, cx| window.within("home").right_click(row("search:one"), cx));
    settle(cx);
    assert!(!ui.read_app(cx, |v, app| v.menu_lines(app).iter().any(|l| l.command == crate::menus::Command::Properties)));
    assert_eq!(ui.read_app(cx, |v, app| v.reasons(app).properties), Some("保存した検索には使えません"));
    ui.press(cx, "escape");
    ui.click(cx, row("search:one"));
    ui.press(cx, "left");
    ui.press(cx, "alt-enter");
    assert!(ui.toasts(cx).iter().any(|t| t == "「プロパティ」は保存した検索には使えません"), "{:?}", ui.toasts(cx));
    assert_eq!(shown(&fake).len(), 2);
}
