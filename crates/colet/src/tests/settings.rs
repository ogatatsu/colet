//! The settings kept between starts, and the preferences.
use super::*;

#[gpui_kit::test]
fn never_writes_over_settings_it_could_not_read(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    *fake.unreadable.lock().unwrap() = Some(LoadError { message: "設定を読み込めません".into(), kept: true });
    let ui = open(cx, fake.clone());
    assert!(ui.toasts(cx).iter().any(|t| t == "設定を読み込めません"), "{:?}", ui.toasts(cx));
    ui.input(cx, "ext:pdf");
    ui.press(cx, "ctrl-d");
    // Kept for the session, but the file stays as it was.
    assert_eq!(ui.read(cx, |v| v.settings.saved_searches.len()), 1);
    // Nor as the window closes.
    assert!(ui.view.update(cx, |v, _| v.flush_settings()).is_ok());
    assert_eq!(fake.saves.load(Ordering::Relaxed), 0);
}

#[gpui_kit::test]
fn saves_afresh_once_a_broken_file_is_put_aside(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    *fake.unreadable.lock().unwrap() = Some(LoadError { message: "壊れていました".into(), kept: false });
    let ui = open(cx, fake.clone());
    ui.input(cx, "ext:pdf");
    ui.press(cx, "ctrl-d");
    assert_eq!(fake.settings.lock().unwrap().saved_searches.len(), 1);
}

#[gpui_kit::test]
fn keeps_the_widths_it_is_left_with_and_starts_with_them(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().sidebar_width = 300.;
    let ui = open(cx, fake.clone());
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 300.);
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(20.), edge.y), cx));
    settle(cx);
    assert_eq!(fake.settings.lock().unwrap().sidebar_width, 320.);
}

#[gpui_kit::test]
fn opens_the_preferences_with_the_theme_taking_the_keys(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    ui.click(cx, "preferences");
    assert!(ui.read(cx, |v| matches!(v.dialog, Some(crate::places::Dialog::Preferences))));
    // The theme chosen takes the keys, there being no field to type in.
    assert!(ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| v.stop("theme:System", cx).is_focused(window))));
    ui.click(cx, "close-preferences");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
}

#[gpui_kit::test]
fn fixes_the_theme_from_the_preferences_at_once_and_keeps_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    // A test window's appearance is light, as Windows set light.
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::light().bg);
    ui.click(cx, "preferences");
    ui.click(cx, ("theme", Theme::Dark as usize));
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::dark().bg);
    // The components draw in it too.
    assert!(cx.read(|cx| gpui_kit::component::Theme::global(cx).is_dark()));
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::Dark);
    ui.click(cx, ("theme", Theme::Light as usize));
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::light().bg);
    assert!(!cx.read(|cx| gpui_kit::component::Theme::global(cx).is_dark()));
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::Light);
    // Back to following Windows, light here.
    ui.click(cx, ("theme", Theme::Dark as usize));
    ui.click(cx, ("theme", Theme::System as usize));
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::light().bg);
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::System);
    // From the keys, as a stop for Tab.
    let dark = ui.read(cx, |v| v.stops.borrow()[&format!("theme:{:?}", Theme::Dark)].clone());
    ui.act(cx, |window, cx| dark.focus(window, cx));
    ui.press(cx, "space");
    assert_eq!(ui.read(cx, |v| v.settings.theme), Theme::Dark);
    let light = ui.read(cx, |v| v.stops.borrow()[&format!("theme:{:?}", Theme::Light)].clone());
    ui.act(cx, |window, cx| light.focus(window, cx));
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.settings.theme), Theme::Light);
}

#[gpui_kit::test]
fn opens_in_the_theme_kept_whatever_windows_is_set_to(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().theme = Theme::Dark;
    let ui = open(cx, fake);
    assert_eq!(ui.read(cx, |v| v.palette.bg), Palette::dark().bg);
}

#[gpui_kit::test]
fn the_final_settings_save_supersedes_debounced_saves(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    let before = fake.saves.load(Ordering::Relaxed);
    ui.view.update(cx, |v, cx| {
        v.update_settings(|s| s.favorites = vec!["older".into()], cx);
        v.update_settings(|s| s.favorites = vec!["latest".into()], cx);
        v.flush_settings().unwrap();
    });
    settle(cx);
    assert_eq!(favorite_texts(&fake.settings.lock().unwrap().favorites), ["latest"]);
    assert_eq!(fake.saves.load(Ordering::Relaxed), before + 1);
}
