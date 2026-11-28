//! The address bar: a path typed or pasted in place of the trail goes there.
use super::*;

impl Ui {
    fn address(&self, cx: &mut TestAppContext) -> Option<String> {
        let (open, field) = self.view.read_with(cx, |v, cx| {
            let address = v.address.read(cx);
            (address.is_open(), address.field().clone())
        });
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
fn a_step_back_by_keys_lets_go_of_what_was_typed_for_the_trail_and_the_column(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\b.txt"]);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\somewhere");
    ui.press(cx, "alt-left");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    assert_eq!(ui.address(cx), None);
    assert!(ui.columns_focused(cx));
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
    assert_eq!(ui.toasts(cx), [r"「D:\nowhere」が見つかりません。"]);
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
fn down_lets_go_of_what_was_typed_back_to_the_column_worked_in(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\other");
    ui.press(cx, "down");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.last_folder(cx), r"D:\work");
    assert_eq!(ui.active(cx), 2);
    assert!(ui.columns_focused(cx));
    // The pick there is left as it was.
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
}

#[gpui_kit::test]
fn esc_back_to_rows_the_keys_were_on_none_of_lands_on_none(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\alpha.txt"), file(r"D:\beta.txt")]));
    let ui = open(cx, fake);
    ui.input(cx, "a");
    let focused = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[1].focused.clone());
    ui.press(cx, "ctrl-l");
    ui.press(cx, "escape");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.active(cx), 1);
    assert!(ui.selected(cx, 1).is_empty());
    assert_eq!(focused(&ui, cx), None);
    // Nor from its own field, emptied first.
    ui.input(cx, "x");
    ui.press(cx, "escape");
    ui.press(cx, "escape");
    assert!(ui.columns_focused(cx));
    assert!(ui.selected(cx, 1).is_empty());
    assert_eq!(focused(&ui, cx), None);
    // ↓ picks the first.
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha.txt"]);
}

#[gpui_kit::test]
fn down_into_home_standing_alone_picks_its_row_where_esc_does_not(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.press(cx, "ctrl-l");
    ui.press(cx, "escape");
    assert!(ui.home_focused(cx));
    assert!(ui.selected(cx, 0).is_empty());
    assert_eq!(ui.read(cx, |v| v.state.columns[0].focused.clone()), None);
    ui.press(cx, "ctrl-l");
    ui.press(cx, "down");
    assert_eq!(ui.address(cx), None);
    assert!(ui.home_focused(cx));
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    assert_eq!(ui.last_folder(cx), r"D:\");
}

#[gpui_kit::test]
fn alt_enter_opens_the_path_in_a_new_tab_in_front(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\other");
    ui.press(cx, "alt-enter");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.read(cx, |v| (v.tabs.len(), v.tabs.current())), (2, 1));
    assert_eq!(ui.last_folder(cx), r"D:\other");
    assert_eq!(ui.active(cx), ui.read(cx, |v| v.state.columns.len() - 1));
    // The tab it came from is as it was.
    ui.press(cx, "ctrl-shift-tab");
    assert_eq!(ui.last_folder(cx), r"D:\work");
}

#[gpui_kit::test]
fn a_press_past_the_crumbs_types_a_path_and_one_on_a_crumb_goes_there(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let first = crumb(&ui, cx, 1);
    ui.click(cx, first);
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

/// A server with a share and a hidden one, and a folder in the share.
fn on_a_server(cx: &mut TestAppContext) -> (Arc<Fake>, Ui) {
    let hide = |entry: Entry| Entry { hidden: Hiding::Hidden, ..entry };
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![]),
            (r"\\nas", vec![hide(folder(r"\\nas\C$")), folder(r"\\nas\share")]),
            (r"\\nas\share", vec![folder(r"\\nas\share\a")]),
            (r"\\nas\share\a", vec![file(r"\\nas\share\a\x.txt")]),
        ])
    });
    let ui = open(cx, fake.clone());
    (fake, ui)
}

impl Ui {
    fn folders(&self, cx: &mut TestAppContext) -> Vec<String> {
        self.read(cx, |v| v.state.columns.iter().skip(1).map(|c| text(&c.path)).collect())
    }

    fn names(&self, cx: &mut TestAppContext, index: usize) -> Vec<String> {
        self.read(cx, |v| v.state.columns[index].entries.iter().map(|e| e.name.clone()).collect())
    }
}

#[gpui_kit::test]
fn a_shared_folder_opens_below_its_server_which_lists_its_shares(cx: &mut TestAppContext) {
    let (_, ui) = on_a_server(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"\\nas\share\a");
    ui.press(cx, "enter");
    assert_eq!(ui.folders(cx), [r"\\nas", r"\\nas\share", r"\\nas\share\a"]);
    assert_eq!(ui.selected(cx, 1), [r"\\nas\share"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].title().into_owned()), "nas");
    // The hidden share stays out while the hidden items do.
    assert_eq!(ui.names(cx, 1), ["share"]);
    ui.press(cx, "ctrl-h");
    assert_eq!(ui.names(cx, 1), ["C$", "share"]);
}

#[gpui_kit::test]
fn a_server_typed_alone_opens_on_its_shares(cx: &mut TestAppContext) {
    let (_, ui) = on_a_server(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"\\NAS\");
    ui.press(cx, "enter");
    assert_eq!(ui.address(cx), None);
    assert_eq!(ui.folders(cx), [r"\\nas"]);
    assert_eq!(ui.names(cx, 1), ["share"]);
}

#[gpui_kit::test]
fn a_servers_shares_are_only_looked_at(cx: &mut TestAppContext) {
    let (fake, ui) = on_a_server(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"\\nas");
    ui.press(cx, "enter");
    assert_eq!(ui.active(cx), 1);
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 1), [r"\\nas\share"]);
    let reasons = ui.read_app(cx, |v, app| {
        let r = v.reasons(app);
        [r.copy, r.cut, r.paste, r.rename, r.delete, r.shell, r.send_to]
    });
    assert_eq!(reasons, [Some(crate::ops::not_at_server()); 7]);
    let reasons = ui.read_app(cx, |v, app| [v.reasons(app).copy_path, v.reasons(app).explorer]);
    assert_eq!(reasons, [None, None]);
    // Nor is anything made or deleted there by its keys.
    ui.press(cx, "delete");
    ui.press(cx, "ctrl-shift-n");
    assert!(fake.operations.lock().unwrap().is_empty());
    // A share's own folder takes a new folder as any does.
    ui.press(cx, "right");
    assert_eq!(ui.read_app(cx, |v, app| v.reasons(app).paste), None);
}

#[gpui_kit::test]
fn lets_go_of_the_answer_about_a_path_typed_over_or_closed_while_it_was_asked(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    for after in ["input", "escape"] {
        ui.press(cx, "ctrl-l");
        ui.input(cx, r"D:\other");
        // Asked, then typed over or closed before the answer comes back: each a step of its own, with
        // nothing let run between them.
        ui.act(cx, |window, cx| window.press("enter", cx));
        ui.act(cx, |window, cx| if after == "input" { window.input("x", cx) } else { window.press("escape", cx) });
        settle(cx);
        assert_eq!(ui.last_folder(cx), r"D:\work", "{after}");
        assert!(ui.toasts(cx).is_empty(), "{after}: {:?}", ui.toasts(cx));
    }
    // The field typed over stays as it is; once closed, the trail is back.
    assert_eq!(ui.address(cx), None);
}
