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
    assert_eq!(ui.read(cx, |v| v.stored.settings().saved_searches.len()), 1);
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
    assert!(ui.read(cx, |v| matches!(v.overlays.dialog(), Some(crate::places::Dialog::Preferences))));
    // The theme chosen takes the keys, there being no field to type in.
    assert!(ui.act(cx, |window, cx| ui.view.update(cx, |v, cx| v.stop(Stop::Theme(Theme::System), cx).is_focused(window))));
    ui.click(cx, "close-preferences");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
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
    let dark = ui.read(cx, |v| v.stops.get(&Stop::Theme(Theme::Dark)).unwrap());
    ui.act(cx, |window, cx| dark.focus(window, cx));
    ui.press(cx, "space");
    assert_eq!(ui.read(cx, |v| v.stored.settings().theme), Theme::Dark);
    let light = ui.read(cx, |v| v.stops.get(&Stop::Theme(Theme::Light)).unwrap());
    ui.act(cx, |window, cx| light.focus(window, cx));
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.stored.settings().theme), Theme::Light);
}

#[gpui_kit::test]
fn moves_among_the_themes_with_the_arrows_and_tabs_past_them_at_once(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    ui.click(cx, "preferences");
    let theme = |t: Theme| ElementId::from(("theme", t as usize));
    assert!(ui.has_keys(cx, theme(Theme::System)));
    // The arrows pick as they go, round from the last to the first.
    ui.press(cx, "right");
    assert!(ui.has_keys(cx, theme(Theme::Light)));
    assert_eq!(fake.settings.lock().unwrap().theme, Theme::Light);
    ui.press(cx, "down");
    ui.press(cx, "right");
    assert!(ui.has_keys(cx, theme(Theme::System)));
    ui.press(cx, "left");
    assert!(ui.has_keys(cx, theme(Theme::Dark)));
    assert_eq!(ui.read(cx, |v| v.stored.settings().theme), Theme::Dark);
    // Tab leaves the group for what follows it; back, it lands on the theme chosen.
    ui.press(cx, "tab");
    assert!(Theme::ALL.iter().all(|t| !ui.has_keys(cx, theme(*t))));
    ui.press(cx, "shift-tab");
    assert!(ui.has_keys(cx, theme(Theme::Dark)));
    // From the first of the themes too, past those after it, not onto the next.
    ui.press(cx, "right");
    assert!(ui.has_keys(cx, theme(Theme::System)));
    ui.press(cx, "tab");
    assert!(Theme::ALL.iter().all(|t| !ui.has_keys(cx, theme(*t))));
    ui.press(cx, "shift-tab");
    assert!(ui.has_keys(cx, theme(Theme::System)));
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

#[gpui_kit::test]
fn draws_everything_at_the_size_chosen_at_once_and_keeps_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    let drawn = |ui: &Ui, cx: &mut TestAppContext| {
        ui.act(cx, |window, _| (window.rem_size(), f32::from(window.find("sidebar").bounds().size.width)))
    };
    assert_eq!(drawn(&ui, cx), (px(16.), 240.));
    ui.click(cx, "preferences");
    ui.click(cx, ("zoom", 150usize));
    // The text and home's pane grow together; the width kept stays as given at 100%.
    assert_eq!(drawn(&ui, cx), (px(24.), 360.));
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 240.);
    assert_eq!(fake.settings.lock().unwrap().zoom, 150);
    // From the keys, as a stop for Tab.
    let back = ui.read(cx, |v| v.stops.get(&Stop::Zoom(100)).unwrap());
    ui.act(cx, |window, cx| back.focus(window, cx));
    ui.press(cx, "space");
    assert_eq!(drawn(&ui, cx), (px(16.), 240.));
    assert_eq!(fake.settings.lock().unwrap().zoom, 100);
}

#[gpui_kit::test]
fn opens_at_the_size_kept(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().zoom = 125;
    let ui = open(cx, fake);
    assert_eq!(ui.act(cx, |window, _| window.rem_size()), px(20.));
    // The notices keep clear of the status bar as it grows.
    assert_eq!(cx.read(|cx| gpui_kit::component::Theme::global(cx).notification.margins.bottom), px(45.));
}
