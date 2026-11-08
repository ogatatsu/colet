//! Where the keys go as a menu, a dialog or the history closes, and as a tab comes back to the front.
use super::*;

impl Ui {
    /// What the address field holds while it is open.
    fn typed_address(&self, cx: &mut TestAppContext) -> Option<String> {
        let (open, field) = self.view.read_with(cx, |v, _| (v.address.is_open(), v.address.field.clone()));
        open.then(|| field.read_with(cx, |input, _| input.value().to_string()))
    }

    fn address_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).address.field.read(cx).focus_handle(cx).is_focused(window))
    }
}

#[gpui_kit::test]
fn a_dialog_gives_the_keys_back_to_where_they_were_as_it_opened(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-f");
    ui.click(cx, "preferences");
    assert!(ui.read(cx, |v| v.dialog.is_some()));
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert!(ui.search_field_focused(cx));
    // From the column, back to it.
    ui.click(cx, row(r"D:\work\b.txt"));
    ui.click(cx, "preferences");
    ui.press(cx, "escape");
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn the_history_gives_the_keys_back_to_where_they_were_as_it_opened(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-f");
    ui.press_and_release(cx, "history-button");
    assert!(ui.read(cx, |v| v.history.open));
    ui.press_and_release(cx, "history-button");
    assert!(ui.read(cx, |v| !v.history.open));
    assert!(ui.search_field_focused(cx));
}

#[gpui_kit::test]
fn a_dialog_opened_from_a_menu_gives_the_keys_back_to_where_the_menu_opened(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-shift-f");
    ui.click(cx, "edit-filters");
    assert!(ui.read(cx, |v| matches!(v.dialog, Some(Dialog::Filters { .. }))));
    ui.click(cx, "close-filters");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert!(ui.search_field_focused(cx));
}

#[gpui_kit::test]
fn the_keys_go_to_the_column_worked_in_when_where_they_were_is_gone(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let gone = ui.act(cx, |window, cx| {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        focus
    });
    ui.click(cx, "preferences");
    drop(gone);
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn a_tab_left_while_typing_an_address_takes_it_up_again(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\wo");
    ui.press(cx, "ctrl-t");
    assert_eq!(ui.typed_address(cx), None);
    assert!(ui.read(cx, |v| v.search_focused));
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    assert!(ui.address_focused(cx));
    // The caret where it was: typed on, it goes on from there.
    ui.input(cx, "rk");
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\work"));
}
