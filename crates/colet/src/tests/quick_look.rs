//! QuickLook following the keys, and space.
use super::*;

#[gpui_kit::test]
fn shows_the_row_the_keys_are_on_in_quick_look_following_them(cx: &mut TestAppContext) {
    use QuickLookMessage::{Switch, Toggle};
    let (fake, ui) = in_work(cx);
    fake.quick_looked();
    ui.press(cx, "space");
    assert_eq!(fake.quick_looked(), [(Toggle, r"D:\work\a.txt".into())]);
    // A preview open goes on with the keys, apart from the selection too, and to a folder.
    ui.press(cx, "down");
    ui.press(cx, "ctrl-down");
    assert_eq!(fake.quick_looked(), [(Switch, r"D:\work\b.txt".into()), (Switch, r"D:\work\c.txt".into())]);
    // Of what is picked, only the row the keys are on.
    ui.press(cx, "ctrl-a");
    ui.press(cx, "space");
    assert_eq!(fake.quick_looked(), [(Toggle, r"D:\work\c.txt".into())]);
    ui.press(cx, "left");
    assert_eq!(fake.quick_looked(), [(Switch, r"D:\work".into())]);
    // The search field takes the keys from the rows: nothing to follow, the preview is left as it is.
    ui.press(cx, "ctrl-f");
    ui.press(cx, "space");
    assert!(fake.quick_looked().is_empty());
}

#[gpui_kit::test]
fn shows_homes_places_in_quick_look_but_not_its_saved_searches(cx: &mut TestAppContext) {
    use QuickLookMessage::{Switch, Toggle};
    let (fake, ui) = ordered_home(cx);
    ui.press(cx, "ctrl-0");
    fake.quick_looked();
    ui.press(cx, "space");
    assert_eq!(fake.quick_looked(), [(Toggle, r"D:\a".into())]);
    ui.press(cx, "down");
    ui.press(cx, "down");
    assert_eq!(fake.quick_looked(), [(Switch, r"D:\b".into()), (Switch, r"D:\c".into())]);
    // A saved search is no file: the preview is left as it is.
    ui.press(cx, "down");
    assert!(ui.read(cx, |v| crate::state::saved_query(v.state.columns[0].focused.as_ref().unwrap()).is_some()));
    ui.press(cx, "space");
    assert!(fake.quick_looked().is_empty());
    // Past the saved searches, on to a drive.
    ui.press(cx, "end");
    assert_eq!(fake.quick_looked(), [(Switch, r"D:\".into())]);
}

#[gpui_kit::test]
fn says_quick_look_is_not_running_only_from_space(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    fake.no_quick_look.store(true, Ordering::Relaxed);
    ui.press(cx, "down");
    assert!(ui.toasts(cx).is_empty(), "{:?}", ui.toasts(cx));
    ui.press(cx, "space");
    assert_eq!(ui.toasts(cx), ["QuickLook が起動していません"]);
}

#[gpui_kit::test]
fn shows_quick_look_from_the_space_an_ime_types(cx: &mut TestAppContext) {
    use QuickLookMessage::Toggle;
    let (fake, ui) = in_work(cx);
    let id = ui.read(cx, |v| v.state.columns[2].id);
    fake.quick_looked();
    ui.input(cx, "　");
    assert_eq!(fake.quick_looked(), [(Toggle, r"D:\work\a.txt".into())]);
    ui.input(cx, " ");
    assert_eq!(fake.quick_looked(), [(Toggle, r"D:\work\a.txt".into())]);
    // Nothing goes on in the field, which the keys are left out of.
    assert!(ui.columns_focused(cx));
    assert!(ui.view.read_with(cx, |v, cx| v.filters.get(&id).is_none_or(|f| f.input.read(cx).value().is_empty())));
}

#[gpui_kit::test]
fn leaves_space_to_a_columns_field(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let id = ui.read(cx, |v| v.state.columns[2].id);
    ui.input(cx, "a");
    assert!(!ui.columns_focused(cx));
    fake.quick_looked();
    ui.press(cx, "space");
    assert_eq!(ui.view.read_with(cx, |v, cx| v.filters[&id].input.read(cx).value().to_string()), "a ");
    assert!(fake.quick_looked().iter().all(|(m, _)| *m != QuickLookMessage::Toggle));
}
