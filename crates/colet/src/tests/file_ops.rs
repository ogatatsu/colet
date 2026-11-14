//! File operations, the clipboard, Explorer and the history.
use super::*;
use crate::settings::Favorite;

#[gpui_kit::test]
fn copies_and_pastes_picking_what_was_pasted(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-c");
    assert_eq!(texts(&fake.clipboard.lock().unwrap().paths), [r"D:\work\a.txt"]);
    assert!(ui.toasts(cx).iter().any(|t| t == "1 項目をコピーしました"));
    ui.press(cx, "ctrl-v");
    // Into its own folder: a copy named as Explorer names it, picked once listed.
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt - コピー"]);
    let entry = ui.ops(cx, |o| o.history().entries[0].clone());
    assert_eq!(entry.items[0].path, Path::new(r"D:\work\a.txt - コピー"));
    assert_eq!(entry.items[0].from.as_deref(), Some(Path::new(r"D:\work\a.txt")));
    // Paths as text, one a line.
    ui.press(cx, "ctrl-shift-c");
    assert_eq!(*fake.text.lock().unwrap(), r"D:\work\a.txt - コピー");
}

#[gpui_kit::test]
fn picks_what_the_shell_writes_out_by_its_name_once_it_is_listed(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    *fake.clipboard.lock().unwrap() = ClipboardFiles { contents: vec!["添付.txt".into()], sequence: 2, ..Default::default() };
    ui.press(cx, "ctrl-v");
    let entry = ui.ops(cx, |o| o.history().entries[0].clone());
    assert_eq!((entry.items[0].path.to_str().unwrap(), entry.items[0].status), (r"D:\work\添付.txt", ItemStatus::Handed));
    assert!(!entry.has_problems());
    // Written out after the paste returned: picked once the listing shows it.
    fake.add(Path::new(r"D:\work"), file(r"D:\work\添付.txt"));
    ui.click(cx, "refresh");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\添付.txt"]);
}

#[gpui_kit::test]
fn marks_the_rows_the_clipboard_holds(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-x");
    settle(cx);
    assert!(ui.ops(cx, |o| o.clipboard().files.cut && texts(&o.clipboard().files.paths) == [r"D:\work\a.txt"]));
    assert!(ui.act(cx, |window, _| window.try_find(SharedString::from(r"held:D:\work\a.txt")).is_some()));
}

#[gpui_kit::test]
fn marks_a_row_another_app_put_on_the_clipboard_in_another_case(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    *fake.clipboard.lock().unwrap() =
        ClipboardFiles { paths: vec![r"d:\WORK\A.TXT".into()], cut: false, sequence: 2, ..Default::default() };
    (fake.watcher.lock().unwrap().as_ref().unwrap())();
    settle(cx);
    assert!(ui.act(cx, |window, _| window.try_find(SharedString::from(r"held:D:\work\a.txt")).is_some()));
}

#[gpui_kit::test]
fn pastes_a_copy_still_being_written_once_it_is_on_the_clipboard(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    // Asked together: the paste waits for the copy rather than pasting what was there before.
    ui.view.update(cx, |v, cx| {
        v.copy(false, cx);
        v.paste(cx);
    });
    assert!(fake.operations.lock().unwrap().is_empty());
    settle(cx);
    let asked = fake.operations.lock().unwrap().clone();
    assert_eq!(asked.len(), 1);
    assert_eq!((asked[0].kind, asked[0].destination.as_path()), (Operation::Paste, Path::new(r"D:\work")));
    assert!(ui.toasts(cx).iter().any(|t| t == "1 項目をコピーしました"), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn picks_what_a_waiting_paste_pasted_in_the_tab_it_was_asked_in_not_the_one_in_front(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let asked_in = ui.read(cx, |v| v.current_tab());
    ui.view.update(cx, |v, cx| {
        v.copy(false, cx);
        v.paste(cx);
    });
    // Another tab comes to the front while the copy is still being written.
    ui.act(cx, |window, cx| window.press("ctrl-t", cx));
    assert!(fake.operations.lock().unwrap().is_empty());
    settle(cx);
    let asked = fake.operations.lock().unwrap().clone();
    assert_eq!((asked.len(), asked[0].destination.as_path()), (1, Path::new(r"D:\work")));
    // The tab in front is left as it was, and stays in front.
    assert_eq!(ui.read(cx, |v| (v.current_tab() != asked_in, v.state.columns.len())), (true, 1));
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.read(cx, |v| v.current_tab()), asked_in);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt - コピー"]);
}

#[gpui_kit::test]
fn pastes_all_the_same_once_the_tab_it_was_asked_in_is_closed(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let asked_in = ui.read(cx, |v| v.current_tab());
    ui.view.update(cx, |v, cx| {
        v.copy(false, cx);
        v.paste(cx);
    });
    ui.act(cx, |window, cx| window.press("ctrl-t", cx));
    ui.click(cx, ("tab-close", asked_in as usize));
    settle(cx);
    assert!(ui.read(cx, |v| v.tabs.get(1).is_none() && v.current_tab() != asked_in));
    let asked = fake.operations.lock().unwrap().clone();
    assert_eq!((asked.len(), asked[0].kind), (1, Operation::Paste));
    assert!(fake.folders.lock().unwrap()[r"D:\work"].iter().any(|e| e.name == "a.txt - コピー"));
}

#[gpui_kit::test]
fn lets_go_of_what_a_paste_moves_as_the_clipboard_holds_it_not_as_last_seen(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-c");
    assert!(ui.ops(cx, |o| !o.clipboard().files.cut));
    // Cut since, and not yet seen to be: what Colet last saw is the copy.
    *fake.clipboard.lock().unwrap() = ClipboardFiles { paths: vec![r"D:\other".into()], cut: true, sequence: 2, ..Default::default() };
    ui.view.update(cx, |v, cx| v.paste(cx));
    while fake.operations.lock().unwrap().is_empty() {
        assert!(cx.executor().tick(), "the paste never ran");
    }
    let released = ui.read(cx, |v| v.loading.watching().released.clone());
    assert_eq!(released, HashSet::from([colet_core::paths::path_key(Path::new(r"D:\other"))]));
    settle(cx);
    assert!(ui.read_app(cx, |v, app| !v.operations.read(app).is_running() && v.loading.watching().released.is_empty()));
}

#[gpui_kit::test]
fn follows_a_rename_to_the_name_the_shell_gave_not_the_one_asked(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.view.update(cx, |v, cx| v.update_settings(|s| s.favorites = vec![Favorite::new(r"D:\work")], cx));
    *fake.renamed_as.lock().unwrap() = Some("job (2)".into());
    let request = Request { kind: Operation::Rename, paths: vec![r"D:\work".into()], destination: r"D:\".into(), name: "job".into() };
    ui.view.update(cx, |v, cx| v.run(request, cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.stored.settings().favorites.clone()), [Favorite::new(r"D:\job (2)")]);
    // It stays picked, and its column open, under that name.
    assert_eq!(ui.selected(cx, 1), [r"D:\job (2)"]);
    assert_eq!(ui.read(cx, |v| text(&v.state.columns[2].path)), r"D:\job (2)");
    let item = ui.ops(cx, |o| o.history().entries[0].items[0].clone());
    assert_eq!((text(&item.path), item.from.as_deref().map(text)), (r"D:\job (2)".into(), Some(r"D:\work".into())));
}

#[gpui_kit::test]
fn a_favorite_kept_in_another_case_follows_a_rename(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.view.update(cx, |v, cx| v.update_settings(|s| s.favorites = vec![Favorite::new(r"d:\WORK\Sub")], cx));
    let request = Request { kind: Operation::Rename, paths: vec![r"D:\work".into()], destination: r"D:\".into(), name: "job".into() };
    ui.view.update(cx, |v, cx| v.run(request, cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.stored.settings().favorites.clone()), [Favorite::new(r"D:\job\Sub")]);
}

#[gpui_kit::test]
fn renames_from_f2_refusing_a_name_windows_would_reject(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match v.overlays.dialog() {
        Some(crate::places::Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("a:b", window, cx)));
    assert!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)).is_some());
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| v.overlays.has_dialog()));
    assert!(fake.operations.lock().unwrap().is_empty());
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("renamed.txt", window, cx)));
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert_eq!(fake.operations.lock().unwrap()[0].name, "renamed.txt");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\renamed.txt"]);
}

#[gpui_kit::test]
fn refuses_a_rename_to_a_name_its_folder_has_but_not_its_own_in_another_case(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match v.overlays.dialog() {
        Some(crate::places::Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("b.txt", window, cx)));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), None);
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| v.overlays.has_dialog()));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), Some("同じ名前の項目があります"));
    assert!(fake.operations.lock().unwrap().is_empty());
    // Typing again lifts it until the next try.
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("A.txt", window, cx)));
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), None);
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert_eq!(fake.operations.lock().unwrap()[0].name, "A.txt");
}

#[gpui_kit::test]
fn asks_the_folder_about_a_new_name_off_the_ui_thread_and_lets_go_of_an_answer_overtaken(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "f2");
    let input = ui.read(cx, |v| match v.overlays.dialog() {
        Some(Dialog::Name { input, .. }) => input.clone(),
        _ => panic!("no dialog"),
    });
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.set_value("e.txt", window, cx)));
    settle(cx);
    let checking = |v: &Colet| match v.overlays.dialog() {
        Some(Dialog::Name { checking, .. }) => checking.clone(),
        _ => None,
    };
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| v.submit_name(window, cx));
        // Pressed, the dialog waits for the folder to answer rather than the window for the disk.
        assert_eq!(checking(ui.view.read(cx)).as_deref(), Some("e.txt"));
        // Typed on before it answers, as the user types: setting the value says nothing of a change.
        window.input("d", cx);
    });
    settle(cx);
    // The answer, that "e.txt" is free, was about a name no longer there: nothing is sent.
    assert_eq!(ui.view.read_with(cx, |v, cx| v.name_problem(cx)), None);
    assert!(ui.read(cx, |v| v.overlays.has_dialog() && checking(v).is_none()));
    assert!(fake.operations.lock().unwrap().is_empty());
    // Pressed again while the folder is asked, it is sent once, once the folder answers.
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| v.submit_name(window, cx));
        ui.view.update(cx, |v, cx| v.submit_name(window, cx));
    });
    settle(cx);
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    let names: Vec<String> = fake.operations.lock().unwrap().iter().map(|r| r.name.clone()).collect();
    assert_eq!(names, ["e.txtd"]);
}

#[gpui_kit::test]
fn asks_whether_a_history_item_is_still_there_off_the_ui_thread_opening_only_the_last_pressed(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    // In the folder already open, the item is picked where it lives.
    let picked = |cx: &mut TestAppContext| ui.selected(cx, 2);
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| {
            v.show_history(true, None, window, cx);
            v.reveal_from_history(r"D:\work\a.txt".into(), window, cx);
            v.reveal_from_history(r"D:\work\c.txt".into(), window, cx);
            // The history stays up while the disk is asked.
            assert!(v.history_panel.open);
        });
    });
    settle(cx);
    assert!(ui.read(cx, |v| !v.history_panel.open));
    assert_eq!(picked(cx), [r"D:\work\c.txt"]);
    // Closed before the answer came, the history has moved on from it.
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| {
            v.show_history(true, None, window, cx);
            v.reveal_from_history(r"D:\work\b.txt".into(), window, cx);
            v.show_history(false, None, window, cx);
        });
    });
    settle(cx);
    assert_eq!(picked(cx), [r"D:\work\c.txt"]);
    // Gone since, it says so and the history stays open.
    ui.act(cx, |window, cx| {
        ui.view.update(cx, |v, cx| {
            v.show_history(true, None, window, cx);
            v.reveal_from_history(r"D:\work\gone.txt".into(), window, cx);
        });
    });
    settle(cx);
    assert!(ui.read(cx, |v| v.history_panel.open));
    assert!(ui.toasts(cx).iter().any(|t| t == "「gone.txt」はもうありません"), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn makes_a_folder_and_picks_it(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-shift-n");
    ui.click(cx, "submit-name");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\新しいフォルダー"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().path.to_str().unwrap().to_owned()), r"D:\work\新しいフォルダー");
}

#[gpui_kit::test]
fn sends_to_the_bin_the_item_picked_not_another_that_shows_alike(cx: &mut TestAppContext) {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    // A name holding an unpaired surrogate, which Windows allows, shows as one that holds U+FFFD.
    let real = file("D:\\work\\a\u{FFFD}X");
    let odd: PathBuf = {
        let wide: Vec<u16> = r"D:\work\a".encode_utf16().chain([0xD800]).chain("X".encode_utf16()).collect();
        std::ffi::OsString::from_wide(&wide).into()
    };
    assert_eq!(odd.as_os_str().encode_wide().count(), real.path.as_os_str().encode_wide().count());
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![real.clone(), Entry { path: odd.clone(), ..real.clone() }]),
        ])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.click(cx, row(r"D:\work"));
    // Each row is its own, though both show alike.
    ui.click(cx, row(&odd));
    assert_eq!(ui.read(cx, |v| v.state.columns[2].selected.clone()), std::slice::from_ref(&odd));
    // Its name sent back as shown is no change, not a rename to what stood in for it.
    ui.press(cx, "f2");
    ui.click(cx, "submit-name");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert!(fake.operations.lock().unwrap().is_empty());
    ui.press(cx, "delete");
    let asked = fake.operations.lock().unwrap()[0].clone();
    assert_eq!((asked.kind, asked.paths), (Operation::Recycle, vec![odd]));
}

#[gpui_kit::test]
fn sends_to_the_bin_leaving_the_asking_to_the_shell_and_picks_the_next_row(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "down");
    ui.press(cx, "delete");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    let asked = fake.operations.lock().unwrap()[0].clone();
    assert_eq!((asked.kind, asked.paths), (Operation::Recycle, vec![PathBuf::from(r"D:\work\b.txt")]));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\c.txt"]);
    let entry = ui.ops(cx, |o| o.history().entries[0].clone());
    assert!(entry.items[0].gone);
    assert_eq!(entry.items[0].status, crate::ops::ItemStatus::Done);
}

#[gpui_kit::test]
fn deletes_outright_on_shift_delete_and_says_so_of_what_the_bin_could_not_take(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "shift-delete");
    assert_eq!(fake.operations.lock().unwrap()[0].kind, Operation::Delete);
    let entry = ui.ops(cx, |o| o.history().entries[0].clone());
    assert_eq!(entry.request.kind.title(), "完全に削除");
    assert_eq!(entry.items[0].status.label(), "完全に削除");
    // Sent to the bin, one it could not take is told apart from the rest.
    fake.unbinnable.lock().unwrap().push(r"D:\work\c.txt".into());
    ui.press(cx, "ctrl-a");
    ui.press(cx, "delete");
    let entry = ui.ops(cx, |o| o.history().entries[0].clone());
    assert_eq!(entry.request.kind, Operation::Recycle);
    let statuses: Vec<_> = entry.items.iter().map(|i| (colet_core::paths::shown(folder_name(&i.path)), i.status.label())).collect();
    assert_eq!(statuses, [("b.txt".to_owned(), "完了"), ("c.txt".to_owned(), "完全に削除")]);
    assert_eq!(entry.outcome(), "完了");
}

#[gpui_kit::test]
fn leaves_no_trace_of_a_deletion_turned_down_at_the_shells_question(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    fake.declined.store(true, Ordering::Relaxed);
    ui.press(cx, "delete");
    ui.press(cx, "shift-delete");
    assert_eq!(fake.operations.lock().unwrap().len(), 2);
    assert!(ui.ops(cx, |o| o.history().entries.is_empty() && o.history().unseen == 0));
    assert!(ui.toasts(cx).is_empty(), "{:?}", ui.toasts(cx));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
}

#[gpui_kit::test]
fn never_sends_a_drive_at_home_to_the_bin(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake { roots: vec![Entry::folder(r"D:\", "D:")], ..Fake::default().with_folders(&[(r"D:\", vec![])]) });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "delete");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert!(ui.toasts(cx).iter().any(|t| t.starts_with("「ごみ箱へ移動」はホームでは使えません")), "{:?}", ui.toasts(cx));
    ui.press(cx, "ctrl-c");
    assert!(ui.toasts(cx).iter().any(|t| t == "「コピー」はホームでは使えません"), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn tells_of_a_failed_operation_and_opens_it_in_the_history(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    fake.stuck.lock().unwrap().push(r"D:\work\a.txt".into());
    ui.press(cx, "delete");
    assert!(ui.toasts(cx).iter().any(|t| t == "「ごみ箱へ移動」: 1 件失敗"), "{:?}", ui.toasts(cx));
    assert_eq!(ui.ops(cx, |o| o.history().unseen), 1);
    let toast = ui.read(cx, |v| v.next_toast);
    ui.click(cx, ("toast-details", toast as usize));
    assert!(ui.read_app(cx, |v, app| v.history_panel.open && v.operations.read(app).history().unseen == 0));
    let entry = ui.ops(cx, |o| o.history().entries[0].clone());
    assert!(ui.read(cx, |v| v.history_panel.expanded.contains(&entry.id)));
    assert_eq!(entry.items[0].error.as_deref(), Some("アクセスが拒否されました。"));
    // Still there: its name opens it in the columns.
    ui.click(cx, crate::dialogs::history_item_id(entry.id, 0));
    assert!(ui.read(cx, |v| !v.history_panel.open));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
}

#[gpui_kit::test]
fn shows_a_folder_columns_own_folder_in_explorer_from_its_empty_space(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.act(cx, |window, cx| window.right_click(("column", work), cx));
    settle(cx);
    let explorer = |ui: &Ui, cx: &mut TestAppContext| ui.read_app(cx, |v, app| v.menu_lines(app)[2].off);
    assert_eq!(explorer(&ui, cx), None);
    ui.click(cx, "menu-explorer");
    assert_eq!(*fake.opened.lock().unwrap(), [r"D:\work"]);
    // The results stand in no folder: it is off, and says why.
    ui.press(cx, "ctrl-f");
    ui.input(cx, "a");
    assert_eq!(ui.read(cx, |v| v.state.columns[1].kind), crate::state::Kind::Search);
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.act(cx, |window, cx| window.right_click(("column", results), cx));
    settle(cx);
    assert_eq!(explorer(&ui, cx), Some(crate::ops::NEEDS_FOLDER));
}

#[gpui_kit::test]
fn shows_several_picked_in_one_folder_together_in_explorer(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-a");
    ui.press(cx, "shift-f10");
    let explorer = |ui: &Ui, cx: &mut TestAppContext| ui.read_app(cx, |v, app| v.menu_lines(app)[2].off);
    assert_eq!(explorer(&ui, cx), None);
    ui.click(cx, "menu-explorer");
    assert_eq!(*fake.revealed.lock().unwrap(), [vec![r"D:\work\a.txt".to_owned(), r"D:\work\b.txt".into(), r"D:\work\c.txt".into()]]);
}

#[gpui_kit::test]
fn shows_several_in_explorer_only_from_one_folder(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:"), Entry::folder(r"E:\", "E:")],
        ..Fake::default().with_results(vec![file(r"D:\a\x.txt"), file(r"D:\b\y.txt")])
    });
    let ui = open(cx, fake.clone());
    let explorer = |ui: &Ui, cx: &mut TestAppContext| ui.read_app(cx, |v, app| v.menu_lines(app)[2].off);
    // Results from two folders.
    ui.input(cx, "x");
    ui.press(cx, "down");
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.selected(cx, 1).len(), 2);
    ui.press(cx, "shift-f10");
    assert_eq!(explorer(&ui, cx), Some(crate::ops::SAME_FOLDER));
    ui.press(cx, "escape");
    assert!(fake.revealed.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn opens_the_picked_files_together_from_enter_and_the_menu_passing_over_folders(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\other"), folder(r"D:\work")]),
            (r"D:\work", vec![folder(r"D:\work\sub"), file(r"D:\work\a.txt"), file(r"D:\work\b.txt")]),
            (r"D:\work\sub", vec![]),
            (r"D:\other", vec![]),
        ])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "down");
    ui.press(cx, "right");
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.selected(cx, 2).len(), 3);
    // Enter opens each file in its app, in the order listed; the folder among them is passed over.
    ui.press(cx, "enter");
    assert_eq!(*fake.opened.lock().unwrap(), [r"D:\work\a.txt", r"D:\work\b.txt"]);
    assert_eq!(ui.selected(cx, 2).len(), 3);
    // The menu's 開く does the same.
    ui.press(cx, "shift-f10");
    assert_eq!(ui.read_app(cx, |v, app| v.menu_lines(app)[0].off), None);
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert_eq!(fake.opened.lock().unwrap().len(), 4);
    // Those that fail leave the rest to open, and are told of together.
    *fake.unopenable.lock().unwrap() = vec![r"D:\work\a.txt".into()];
    ui.press(cx, "enter");
    assert_eq!(fake.opened.lock().unwrap().last().map(String::as_str), Some(r"D:\work\b.txt"));
    assert!(ui.toasts(cx).iter().any(|t| t.contains("「a.txt」を開けません")));
    // Folders alone have nothing to open: 開く is off, and Enter does nothing.
    ui.press(cx, "left");
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.selected(cx, 1).len(), 2);
    let opened = fake.opened.lock().unwrap().len();
    ui.press(cx, "enter");
    assert_eq!(fake.opened.lock().unwrap().len(), opened);
    assert_eq!(ui.selected(cx, 1).len(), 2);
    ui.press(cx, "shift-f10");
    assert_eq!(ui.read_app(cx, |v, app| v.menu_lines(app)[0].off), Some(crate::ops::NEEDS_FILE));
}

#[gpui_kit::test]
fn asks_before_opening_many_files_together(cx: &mut TestAppContext) {
    let files: Vec<Entry> = (0..crate::ops::ASK_TO_OPEN).map(|i| file(&format!(r"D:\work\{i:02}.txt"))).collect();
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", files)])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    ui.press(cx, "ctrl-a");
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| matches!(v.overlays.dialog(), Some(Dialog::Open { paths }) if paths.len() == crate::ops::ASK_TO_OPEN)));
    assert!(fake.opened.lock().unwrap().is_empty());
    // Turned down, nothing opens and the keys go back to the column.
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert!(ui.columns_focused(cx));
    assert!(fake.opened.lock().unwrap().is_empty());
    // The dialog's 開く takes the keys, so Enter again opens them all.
    ui.press(cx, "enter");
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert_eq!(fake.opened.lock().unwrap().len(), crate::ops::ASK_TO_OPEN);
}
