//! The address bar: a path typed or pasted in place of the trail goes there.
use super::*;

impl Ui {
    fn address(&self, cx: &mut TestAppContext) -> Option<String> {
        let (open, field) = self.view.read_with(cx, |v, _| (v.address_open, v.address.clone()));
        open.then(|| field.read_with(cx, |input, _| input.value().to_string()))
    }

    fn last_folder(&self, cx: &mut TestAppContext) -> String {
        self.read(cx, |v| v.state.columns.last().unwrap().path.to_string_lossy().into_owned())
    }
}

#[gpui_kit::test]
fn ctrl_l_types_a_folder_over_the_deepest_one_and_enter_opens_it_worked_in(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    assert_eq!(ui.address(cx).as_deref(), Some(r"D:\work"));
    // Written as it is not on disk, quoted as Explorer copies it.
    ui.input(cx, "\"d:/OTHER\"");
    ui.press(cx, "enter");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.last_folder(cx), r"D:\other");
    assert_eq!(ui.active(cx), ui.read(cx, |v| v.state.columns.len() - 1));
    assert_eq!(ui.selected(cx, 1), [r"D:\other"]);
    assert!(ui.columns_focused(cx));
    // Back where it came from.
    ui.press(cx, "alt-left");
    assert_eq!(ui.last_folder(cx), r"D:\work");
}

#[gpui_kit::test]
fn a_file_typed_opens_where_it_lives_picked(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\work\C.TXT");
    ui.press(cx, "enter");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.last_folder(cx), r"D:\work");
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].focused.clone()), Some(PathBuf::from(r"D:\work\c.txt")));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn what_cannot_be_gone_to_is_said_and_stays_to_be_put_right(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\nowhere");
    ui.press(cx, "enter");
    assert_eq!(ui.toasts(cx), [r"「D:\nowhere」が見つかりません"]);
    assert_eq!(ui.address(cx).as_deref(), Some(r"D:\nowhere"));
    assert_eq!(ui.last_folder(cx), r"D:\work");
    ui.press(cx, "ctrl-a");
    ui.input(cx, r"work\a.txt");
    ui.press(cx, "enter");
    assert_eq!(ui.toasts(cx).len(), 2);
    assert!(ui.toasts(cx)[1].starts_with(r"「work\a.txt」は開けません"), "{:?}", ui.toasts(cx));
    assert_eq!(ui.address(cx).as_deref(), Some(r"work\a.txt"));
}

#[gpui_kit::test]
fn esc_or_the_keys_going_elsewhere_let_go_of_what_was_typed(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\other");
    ui.press(cx, "escape");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.last_folder(cx), r"D:\work");
    assert!(ui.columns_focused(cx));
    ui.press(cx, "ctrl-l");
    assert_eq!(ui.address(cx).as_deref(), Some(r"D:\work"));
    ui.press(cx, "ctrl-f");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.last_folder(cx), r"D:\work");
}

#[gpui_kit::test]
fn alt_enter_opens_the_path_in_a_new_tab_in_front(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\other");
    ui.press(cx, "alt-enter");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.read(cx, |v| (v.tabs.list.len(), v.tabs.current)), (2, 1));
    assert_eq!(ui.last_folder(cx), r"D:\other");
    assert_eq!(ui.active(cx), ui.read(cx, |v| v.state.columns.len() - 1));
    // The tab it came from is as it was.
    ui.press(cx, "ctrl-shift-tab");
    assert_eq!(ui.last_folder(cx), r"D:\work");
}

#[gpui_kit::test]
fn a_press_past_the_crumbs_types_a_path_and_one_on_a_crumb_goes_there(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.click(cx, ("crumb", 1usize));
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.active(cx), 1);
    ui.click(cx, "address-room");
    assert_eq!(ui.address(cx).as_deref(), Some(r"D:\work"));
}

#[gpui_kit::test]
fn with_no_folder_open_the_field_starts_empty(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.press(cx, "ctrl-l");
    assert_eq!(ui.address(cx).as_deref(), Some(""));
    ui.input(cx, "d:");
    ui.press(cx, "enter");
    assert_eq!(ui.last_folder(cx), r"D:\");
    assert_eq!(ui.active(cx), 1);
    assert!(ui.selected(cx, 1).is_empty());
}
