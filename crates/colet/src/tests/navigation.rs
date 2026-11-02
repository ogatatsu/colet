//! Moving the keys and the picks among home, the results and the columns.
use super::*;

#[gpui_kit::test]
fn moves_from_the_empty_search_field_into_home_with_down_picking_its_row(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.press(cx, "down");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].path.to_str().unwrap().to_owned()), r"D:\");
    // The drive picked again leaves the folders open beside it be.
    ui.press(cx, "right");
    ui.press(cx, "right");
    ui.press(cx, "ctrl-f");
    ui.press(cx, "down");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.read(cx, |v| v.state.columns.last().unwrap().path.to_str().unwrap().to_owned()), r"D:\work");
}

#[gpui_kit::test]
fn opens_a_deep_search_file_on_its_own_folder_and_right_jumps_to_it(cx: &mut TestAppContext) {
    let deep = file(r"D:\a\b\zeta.txt");
    let fake = Arc::new(
        Fake::default()
            .with_results(vec![deep.clone()])
            .with_folders(&[(r"D:\", vec![folder(r"D:\a")]), (r"D:\a", vec![folder(r"D:\a\b")]), (r"D:\a\b", vec![deep.clone()])]),
    );
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    ui.click(cx, row(&deep.path));
    // The folders above it are folded away; their crumbs are still there.
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 2);
    assert!(ui.act(cx, |window, _| window.try_find(("crumb", 2usize)).is_some()));
    ui.press(cx, "right");
    assert_eq!(ui.active(cx), 4);
    assert_eq!(ui.selected(cx, 4), std::slice::from_ref(&deep.path));
    // ← passes over them, back to the results.
    ui.press(cx, "left");
    assert_eq!(ui.active(cx), 1);
    // The strip beside the results says how many it holds, brings them out for good, and goes.
    assert!(ui.act(cx, |window, _| window.try_find("unfold-count").is_some()));
    // Its heading's line runs on from the columns'.
    let deepest = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    let (strip, column) = ui.act(cx, |window, _| (window.find("unfold-head").bounds(), window.find(("head", deepest)).bounds()));
    assert_eq!((strip.top(), strip.bottom()), (column.top(), column.bottom()));
    ui.click(cx, "unfold");
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
    assert!(ui.act(cx, |window, _| window.try_find("unfold").is_none()));
    // Clicked, it leaves the keys in the columns: → still jumps to the file.
    assert!(ui.columns_focused(cx));
    ui.press(cx, "right");
    assert_eq!(ui.active(cx), 4);
    // Picked afresh, folded again: a crumb brings its column out and takes the keys.
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click_in(cx, ("column", results), row(&deep.path));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 2);
    ui.click(cx, ("crumb", 3usize));
    assert_eq!(ui.active(cx), 3);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
}

#[gpui_kit::test]
fn brings_the_folded_folders_out_with_ctrl_u_leaving_the_keys_be(cx: &mut TestAppContext) {
    let (deep, other) = (file(r"D:\a\b\zeta.txt"), file(r"D:\a\b\zeta2.txt"));
    let fake = Arc::new(Fake::default().with_results(vec![deep.clone(), other.clone()]).with_folders(&[
        (r"D:\", vec![folder(r"D:\a")]),
        (r"D:\a", vec![folder(r"D:\a\b")]),
        (r"D:\a\b", vec![deep.clone(), other.clone()]),
    ]));
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    ui.click(cx, row(&deep.path));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 2);
    ui.press(cx, "ctrl-u");
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
    assert!(ui.act(cx, |window, _| window.try_find("unfold").is_none()));
    assert_eq!(ui.active(cx), 1);
    assert!(ui.columns_focused(cx));
    // Nothing folded: nothing to do.
    ui.press(cx, "ctrl-u");
    assert_eq!(ui.active(cx), 1);
    // From the search field too, which keeps the keys.
    let results = ui.read(cx, |v| v.state.columns[1].id);
    ui.click_in(cx, ("column", results), row(&other.path));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 2);
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-u");
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
    assert!(ui.read(cx, |v| v.search_focused));
}

#[gpui_kit::test]
fn takes_the_keys_home_with_ctrl_h_on_the_row_they_were_on(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.press(cx, "down");
    ui.press(cx, "right");
    ui.press(cx, "right");
    assert!(ui.columns_focused(cx));
    let open = ui.read(cx, |v| v.state.columns.len());
    ui.press(cx, "ctrl-h");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.active(cx), 0);
    assert_eq!(ui.read(cx, |v| v.state.columns[0].focused.clone()), Some(PathBuf::from(r"D:\")));
    // It only goes there: the folders beside it stay open.
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), open);
    // From the search field too.
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-h");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.read(cx, |v| v.state.columns[0].focused.clone()), Some(PathBuf::from(r"D:\")));
}

#[gpui_kit::test]
fn walks_folders_by_keys_and_goes_back(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), folder(r"D:\beta"), file(r"D:\gamma.txt")])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    ui.press(cx, "right");
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha"]);
    ui.press(cx, "end");
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma.txt"]);
    ui.press(cx, "up");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta"]);
    ui.press(cx, "alt-left");
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma.txt"]);
    ui.press(cx, "alt-right");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta"]);
}

#[gpui_kit::test]
fn leaves_the_search_for_a_place_picked_in_home_clearing_the_field(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![])]).with_results(vec![file(r"D:\x.txt")])
    });
    let ui = open(cx, fake);
    ui.input(cx, "x");
    ui.click(cx, row(r"D:\"));
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "");
    let field = ui.view.read_with(cx, |v, _| v.search.clone());
    assert_eq!(field.read_with(cx, |input, _| input.value().to_string()), "");
    // One step back returns to the results, and the field says so.
    ui.press(cx, "alt-left");
    assert_eq!(ui.read(cx, |v| v.state.query.clone()), "x");
    assert_eq!(field.read_with(cx, |input, _| input.value().to_string()), "x");
}

#[gpui_kit::test]
fn works_in_a_favorite_by_mouse_and_tab_leaving_the_folders_above_it_folded(cx: &mut TestAppContext) {
    let sub = folder(r"D:\work\sub");
    let fake = Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![sub.clone(), file(r"D:\work\a.txt")]),
            (r"D:\work\sub", vec![]),
        ])
    };
    fake.settings.lock().unwrap().favorites = vec![r"D:\work".into()];
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\work"));
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    // A row in its own column is picked there; the folder above stays folded.
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click_in(cx, ("column", work), row(r"D:\work\a.txt"));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    // A click off the rows takes the keys to the column clicked, folding nothing out.
    ui.click_in(cx, "home", row(r"D:\work"));
    ui.click(cx, ("column", work));
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
    // The keys tabbed over from home land where → would, past the fold.
    ui.click_in(cx, "home", row(r"D:\work"));
    let view = ui.view.clone();
    ui.act(cx, |window, cx| view.read(cx).columns_focus.clone().focus(window, cx));
    settle(cx);
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.active(cx), 2);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 1);
}

#[gpui_kit::test]
fn leaves_the_keys_in_home_when_the_room_past_the_columns_is_pressed(cx: &mut TestAppContext) {
    let fake = Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\work")]), (r"D:\work", vec![folder(r"D:\work\sub")])])
    };
    fake.settings.lock().unwrap().favorites = vec![r"D:\work".into()];
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\work"));
    let columns = ui.read(cx, |v| v.state.columns.len());
    ui.click(cx, "columns-rest");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.active(cx), 0);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), columns);
    assert!(ui.selected(cx, columns - 1).is_empty());
}

#[gpui_kit::test]
fn lists_the_folder_tabbing_into_the_columns_opens(cx: &mut TestAppContext) {
    let fake = Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[
            (r"D:\", vec![folder(r"D:\work")]),
            (r"D:\work", vec![folder(r"D:\work\sub")]),
            (r"D:\work\sub", vec![file(r"D:\work\sub\a.txt")]),
        ])
    };
    fake.settings.lock().unwrap().favorites = vec![r"D:\work".into()];
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\work"));
    let view = ui.view.clone();
    ui.act(cx, |window, cx| view.read(cx).columns_focus.clone().focus(window, cx));
    settle(cx);
    let last = ui.read(cx, |v| v.state.columns.last().map(|c| (text(&c.path), c.loading, c.entries.len())).unwrap());
    assert_eq!(last, (r"D:\work\sub".to_owned(), false, 1));
}

#[gpui_kit::test]
fn goes_to_a_crumb_s_column_picking_nothing_in_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), folder(r"D:\beta")])])
    });
    let ui = open(cx, fake);
    ui.click_in(cx, "home", row(r"D:\"));
    assert_eq!(ui.active(cx), 0);
    ui.click(cx, ("crumb", 1usize));
    assert_eq!(ui.active(cx), 1);
    assert!(ui.columns_focused(cx));
    assert!(ui.selected(cx, 1).is_empty());
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
}

#[gpui_kit::test]
fn picks_several_rows_with_ctrl_shift_and_ctrl_a(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "shift-down");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\b.txt"]);
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.selected(cx, 2).len(), 3);
    ui.click(cx, row(r"D:\work\b.txt"));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\b.txt"]);
    // Shift+↑ runs from the row last picked on its own.
    ui.press(cx, "shift-up");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\b.txt"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
}

#[gpui_kit::test]
fn picks_rows_apart_from_the_keys(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    // Ctrl+↓ passes b by, picking nothing; Ctrl+Space picks c beside a.
    ui.press(cx, "ctrl-down");
    ui.press(cx, "ctrl-down");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt"]);
    ui.press(cx, "ctrl-space");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\c.txt"]);
    // Ctrl+Shift+↑ adds b, from c.
    ui.press(cx, "ctrl-shift-up");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\c.txt", r"D:\work\b.txt"]);
    // Ctrl+Shift+click draws it back to c alone, a staying.
    ui.click_with(cx, row(r"D:\work\c.txt"), Modifiers { control: true, shift: true, ..Default::default() });
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\c.txt"]);
}

#[gpui_kit::test]
fn picks_rows_with_their_checkboxes_as_ctrl_does(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let check = |path: &str| ElementId::Name(format!("check:{}", id_text(Path::new(path))).into());
    // Home picks one row at a time, and has none.
    assert!(ui.act(cx, |window, _| window.try_find(check(r"D:\")).is_none()));
    ui.click(cx, check(r"D:\work\c.txt"));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\c.txt"]);
    ui.click(cx, check(r"D:\work\a.txt"));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\c.txt"]);
    // With Shift, the rows from the one last clicked, a, are added.
    ui.click_with(cx, check(r"D:\work\c.txt"), Modifiers::shift());
    let mut picked = ui.selected(cx, 2);
    picked.sort();
    assert_eq!(picked, [r"D:\work\a.txt", r"D:\work\b.txt", r"D:\work\c.txt"]);
    // Twice in a row it takes the row out and back, opening nothing.
    ui.act(cx, |window, cx| window.double_click(check(r"D:\work\b.txt"), cx));
    settle(cx);
    assert_eq!(ui.selected(cx, 2).len(), 3);
    assert!(fake.opened.lock().unwrap().is_empty());
}

#[gpui_kit::test]
fn picks_every_row_or_none_with_the_checkbox_over_them(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    // Home picks one row at a time, and has none.
    let home = ui.read(cx, |v| v.state.columns[0].id);
    assert!(ui.act(cx, |window, _| window.try_find(("check-all", home)).is_none()));
    ui.click(cx, ("check-all", work));
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\b.txt", r"D:\work\c.txt"]);
    assert_eq!(ui.read(cx, |v| v.state.active), 2);
    // Every row picked, it picks none, the keys staying on their row.
    let focused = ui.read(cx, |v| v.state.columns[2].focused.clone());
    ui.click(cx, ("check-all", work));
    assert!(ui.selected(cx, 2).is_empty());
    assert_eq!(ui.read(cx, |v| v.state.columns[2].focused.clone()), focused);
    // Some picked, it picks them all.
    ui.click(cx, ElementId::Name(format!("check:{}", id_text(Path::new(r"D:\work\b.txt"))).into()));
    ui.click(cx, ("check-all", work));
    assert_eq!(ui.selected(cx, 2).len(), 3);
}

#[gpui_kit::test]
fn the_buttons_done_on_the_spot_leave_the_keys_where_they_were(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    assert!(ui.columns_focused(cx));
    ui.click(cx, "copy");
    assert_eq!(texts(&fake.clipboard.lock().unwrap().paths), [r"D:\work\a.txt"]);
    assert!(ui.columns_focused(cx));
    ui.click(cx, "refresh");
    assert!(ui.columns_focused(cx));
    // ↓ still moves in the column the keys stayed in.
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 2), [r"D:\work\b.txt"]);
    // From the search field too.
    let search = ui.read(cx, |v| v.search.clone());
    ui.act(cx, |window, cx| search.update(cx, |input, cx| input.focus(window, cx)));
    ui.click(cx, "cut");
    assert!(ui.read(cx, |v| v.clipboard.files.cut && texts(&v.clipboard.files.paths) == [r"D:\work\b.txt"]));
    assert!(ui.act(cx, |window, cx| search.read(cx).focus_handle(cx).is_focused(window)));
}

#[gpui_kit::test]
fn tab_passes_the_buttons_that_are_off_and_a_press_on_one_leaves_the_keys(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    // Three picked: one name cannot be changed for them all.
    ui.press(cx, "ctrl-a");
    // Back from the columns, the way that keeps away from home's pane.
    ui.press(cx, "shift-tab");
    ui.press(cx, "shift-tab");
    assert!(ui.has_keys(cx, "new-folder"));
    ui.press(cx, "shift-tab");
    assert!(ui.has_keys(cx, "copy-path"), "Shift+Tab goes past rename to the one before");
    ui.press(cx, "tab");
    assert!(ui.has_keys(cx, "new-folder"), "and Tab back past it");
    // Pressed by the mouse, it does nothing and takes nothing.
    let view = ui.view.clone();
    ui.act(cx, |window, cx| view.read(cx).columns_focus.clone().focus(window, cx));
    ui.click(cx, "rename");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn picks_one_row_at_a_time_in_home(cx: &mut TestAppContext) {
    let (_, ui) = ordered_home(cx);
    ui.click(cx, row(r"D:\a"));
    ui.press(cx, "left");
    // Ctrl+A picks nothing more, and Shift+↓ moves as ↓ does.
    ui.press(cx, "ctrl-a");
    assert_eq!(ui.selected(cx, 0), [r"D:\a"]);
    ui.press(cx, "shift-down");
    assert_eq!(ui.selected(cx, 0), [r"D:\b"]);
    ui.press(cx, "ctrl-shift-down");
    assert_eq!(ui.selected(cx, 0), [r"D:\c"]);
    // A click with Ctrl or Shift held picks the row alone, as a plain one does.
    ui.click_with(cx, row(r"D:\a"), Modifiers::control());
    assert_eq!(ui.selected(cx, 0), [r"D:\a"]);
    ui.click_with(cx, row(r"D:\c"), Modifiers::shift());
    assert_eq!(ui.selected(cx, 0), [r"D:\c"]);
    ui.press(cx, "left");
    // Ctrl+Space picks the row the keys are on alone, or lets go of it.
    ui.press(cx, "ctrl-up");
    ui.press(cx, "ctrl-space");
    assert_eq!(ui.selected(cx, 0), [r"D:\b"]);
    ui.press(cx, "ctrl-space");
    assert!(ui.selected(cx, 0).is_empty());
}

#[gpui_kit::test]
fn marks_under_the_crumbs_what_is_on_screen_while_some_are_folded(cx: &mut TestAppContext) {
    let deep = file(r"D:\a\b\zeta.txt");
    let fake = Arc::new(
        Fake::default()
            .with_results(vec![deep.clone()])
            .with_folders(&[(r"D:\", vec![folder(r"D:\a")]), (r"D:\a", vec![folder(r"D:\a\b")]), (r"D:\a\b", vec![deep.clone()])]),
    );
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    assert!(ui.act(cx, |window, _| window.try_find("crumb-bar").is_none()));
    ui.click(cx, row(&deep.path));
    ui.act(cx, |_, _| ());
    assert!(ui.act(cx, |window, _| window.try_find("crumb-bar").is_some()));
    // Out for good, the bar stays only while some column is out of sight past an edge.
    ui.click(cx, "unfold");
    ui.act(cx, |_, _| ());
    let hidden = ui.read(cx, |v| v.on_screen().hidden_left() || v.on_screen().hidden_right());
    assert_eq!(ui.act(cx, |window, _| window.try_find("crumb-bar").is_some()), hidden);
}

#[gpui_kit::test]
fn opens_the_folders_folded_off_the_trail_from_the_keys_and_goes_to_one(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    cx.simulate_window_resize(ui.window, size(px(800.), px(800.)));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    let fold = ui.read(cx, |v| v.stops["crumb-fold"].clone());
    ui.act(cx, |window, cx| fold.focus(window, cx));
    // Enter opens it; ↓ lights a line, Esc hands the keys back to the button.
    ui.press(cx, "enter");
    assert!(ui.read(cx, |v| v.crumbs_open()));
    ui.press(cx, "down");
    ui.press(cx, "escape");
    assert!(!ui.read(cx, |v| v.crumbs_open()));
    assert!(ui.act(cx, |window, _| fold.is_focused(window)));
    // Space opens it too; ↓ twice lights the second line, and Enter goes to its column, the keys with it.
    ui.press(cx, "space");
    ui.press(cx, "down");
    ui.press(cx, "down");
    // The trail starts at the drive: its second line is the first folder.
    let target = ui.read(cx, |v| v.state.columns.iter().position(|c| c.path == chain[0]).unwrap());
    ui.press(cx, "enter");
    assert!(!ui.read(cx, |v| v.crumbs_open()));
    assert_eq!(ui.active(cx), target);
    assert!(ui.columns_focused(cx));
}
