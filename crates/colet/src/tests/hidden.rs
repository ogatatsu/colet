//! The hidden and system items: left out of the folders until asked for, by Ctrl+H or the preferences.
use super::*;

fn with_hidden() -> Arc<Fake> {
    let hide = |entry: Entry| Entry { hidden: Hiding::Hidden, ..entry };
    Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work"), hide(folder(r"D:\.git")), hide(file(r"D:\desktop.ini"))])])
    })
}

fn listed(ui: &Ui, cx: &mut TestAppContext) -> Vec<String> {
    ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| text(&e.path)).collect())
}

#[gpui_kit::test]
fn leaves_the_hidden_items_out_until_shown_and_keeps_the_choice(cx: &mut TestAppContext) {
    let fake = with_hidden();
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    settle(cx);
    assert_eq!(listed(&ui, cx), [r"D:\work"]);
    ui.press(cx, "ctrl-h");
    assert_eq!(listed(&ui, cx), [r"D:\work", r"D:\.git", r"D:\desktop.ini"]);
    settle(cx);
    assert!(fake.settings.lock().unwrap().show_hidden);
    // The preferences' checkbox puts them away again.
    ui.click(cx, "preferences");
    ui.click(cx, "show-hidden");
    assert_eq!(listed(&ui, cx), [r"D:\work"]);
    settle(cx);
    assert!(!fake.settings.lock().unwrap().show_hidden);
}

#[gpui_kit::test]
fn starts_with_the_hidden_items_shown_when_left_so(cx: &mut TestAppContext) {
    let fake = with_hidden();
    fake.settings.lock().unwrap().show_hidden = true;
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    settle(cx);
    assert_eq!(listed(&ui, cx).len(), 3);
}

#[gpui_kit::test]
fn shows_or_hides_them_in_a_tab_brought_back(cx: &mut TestAppContext) {
    let fake = with_hidden();
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    settle(cx);
    ui.press(cx, "ctrl-t");
    ui.press(cx, "ctrl-h");
    ui.press(cx, "ctrl-shift-tab");
    settle(cx);
    assert_eq!(listed(&ui, cx).len(), 3);
}

#[gpui_kit::test]
fn shows_the_protected_files_once_asked_for_in_the_preferences_along_with_the_hidden_items(cx: &mut TestAppContext) {
    let fake = with_hidden();
    fake.folders.lock().unwrap().get_mut(r"D:\").unwrap().push(Entry { hidden: Hiding::Protected, ..file(r"D:\pagefile.sys") });
    fake.settings.lock().unwrap().show_hidden = true;
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    settle(cx);
    assert!(!listed(&ui, cx).contains(&r"D:\pagefile.sys".to_owned()));
    ui.click(cx, "preferences");
    ui.click(cx, "show-protected");
    settle(cx);
    assert!(fake.settings.lock().unwrap().show_protected);
    assert!(listed(&ui, cx).contains(&r"D:\pagefile.sys".to_owned()));
    // With the hidden items out, they go too.
    ui.press(cx, "escape");
    ui.press(cx, "ctrl-h");
    assert_eq!(listed(&ui, cx), [r"D:\work"]);
    // Their checkbox is off meanwhile, keeping what was chosen.
    ui.click(cx, "preferences");
    ui.click(cx, "show-protected");
    settle(cx);
    assert!(fake.settings.lock().unwrap().show_protected);
}
