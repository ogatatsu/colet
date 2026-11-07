//! A column's field, narrowing it by name.
use super::*;

#[gpui_kit::test]
fn goes_back_to_the_row_a_filter_hid_once_it_shows_again(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), folder(r"D:\apple"), file(r"D:\beta.txt")])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "down");
    ui.press(cx, "down");
    let focused = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[1].focused.as_deref().map(text));
    // Hidden, then shown again by Esc in the field: ↓ goes to it, still picked on its own.
    ui.input(cx, "ap");
    assert_eq!(focused(&ui, cx), None);
    ui.press(cx, "escape");
    ui.press(cx, "down");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    assert_eq!(focused(&ui, cx).as_deref(), Some(r"D:\beta.txt"));
    // Shown again by a filter loosened, too.
    ui.input(cx, "al");
    ui.press(cx, "backspace");
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    assert_eq!(focused(&ui, cx).as_deref(), Some(r"D:\beta.txt"));
    // Gone to another meanwhile, the keys stay there.
    ui.press(cx, "escape");
    ui.input(cx, "ap");
    ui.press(cx, "down");
    ui.press(cx, "escape");
    assert_eq!(ui.selected(cx, 1), [r"D:\apple"]);
    assert_eq!(focused(&ui, cx).as_deref(), Some(r"D:\apple"));
}

#[gpui_kit::test]
fn types_from_the_rows_into_their_field_each_time_as_another_word(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), folder(r"D:\apple"), file(r"D:\beta.txt")])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    let id = ui.read(cx, |v| v.state.columns[1].id);
    let typed = |ui: &Ui, cx: &mut TestAppContext| ui.view.read_with(cx, |v, cx| v.filters[&id].input.read(cx).value().to_string());
    let shown =
        |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>());
    // A key in the rows goes on in the field, which takes the keys.
    ui.input(cx, "ap");
    assert_eq!(typed(&ui, cx), "ap");
    assert!(!ui.columns_focused(cx));
    assert_eq!(shown(&ui, cx), [r"D:\apple"]);
    ui.press(cx, "down");
    assert!(ui.columns_focused(cx));
    // Typed again from the rows, it is another word.
    ui.input(cx, "e");
    assert_eq!(typed(&ui, cx), "ap e");
    assert_eq!(shown(&ui, cx), [r"D:\apple"]);
    ui.press(cx, "down");
    // A space alone starts nothing.
    ui.input(cx, " ");
    assert_eq!(typed(&ui, cx), "ap e");
    assert!(ui.columns_focused(cx));
    // Backspace takes the last char off, the keys going to the field; there, it is the field's own.
    ui.press(cx, "backspace");
    assert_eq!(typed(&ui, cx), "ap ");
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "backspace");
    assert_eq!(typed(&ui, cx), "ap");
    // Ending in a space, the field takes the word after it; a quote left open is closed first.
    ui.input(cx, " ");
    ui.press(cx, "down");
    ui.input(cx, "p");
    assert_eq!(typed(&ui, cx), "ap p");
    ui.press(cx, "escape");
    ui.press(cx, "escape");
    assert!(ui.columns_focused(cx));
    ui.input(cx, "\"a");
    ui.press(cx, "down");
    ui.input(cx, "h");
    assert_eq!(typed(&ui, cx), "\"a\" h");
    assert_eq!(shown(&ui, cx), [r"D:\alpha"]);
    // With nothing in the field, Backspace in the rows leaves the keys there.
    ui.press(cx, "escape");
    ui.press(cx, "escape");
    ui.press(cx, "backspace");
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn hands_what_an_ime_composes_in_the_rows_on_to_their_field(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "データ (D:)")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\写真"), folder(r"D:\動画")])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    let id = ui.read(cx, |v| v.state.columns[1].id);
    let view = ui.view.clone();
    // The IME shows its panel at the column's own field, not home's beside it.
    let at = ui.act(cx, |window, cx| view.update(cx, |v, cx| v.bounds_for_range(0..0, Bounds::default(), window, cx)));
    let sidebar = ui.act(cx, |window, cx| view.read(cx).sidebar_width(window));
    assert!(at.expect("the field's caret").origin.x > px(sidebar));
    ui.act(cx, |window, cx| view.update(cx, |v, cx| v.replace_and_mark_text_in_range(None, "しゃ", Some(2..2), window, cx)));
    settle(cx);
    assert!(!ui.columns_focused(cx));
    let input = ui.view.read_with(cx, |v, _| v.filters[&id].input.clone());
    // The field goes on composing it, and takes what the IME settles on in its place.
    let marked = ui.act(cx, |window, cx| input.update(cx, |input, cx| input.marked_text_range(window, cx)));
    assert_eq!(marked, Some(0..2));
    ui.act(cx, |window, cx| input.update(cx, |input, cx| input.replace_text_in_range(None, "写", window, cx)));
    settle(cx);
    assert_eq!(ui.view.read_with(cx, |_, cx| input.read(cx).value().to_string()), "写");
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>()), [r"D:\写真"]);
}

#[gpui_kit::test]
fn keeps_what_a_filter_hides_picked_acting_only_on_what_shows(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.press(cx, "ctrl-a");
    ui.input(cx, "b");
    assert_eq!(ui.selected(cx, 2).len(), 3);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].shown_selected().len()), 1);
    // ↓ lands on the row it lets through, picked already: the others stay picked.
    ui.press(cx, "down");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 2).len(), 3);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].focused.clone()), Some(PathBuf::from(r"D:\work\b.txt")));
    // What is acted on is only what shows.
    ui.press(cx, "delete");
    let asked = fake.operations.lock().unwrap()[0].clone();
    assert_eq!((asked.kind, asked.paths), (Operation::Recycle, vec![PathBuf::from(r"D:\work\b.txt")]));
}

#[gpui_kit::test]
fn moves_a_filter_up_from_the_keys(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(with_filters(Fake::default())));
    ui.click(cx, "search-filter");
    ui.click(cx, "edit-filters");
    // The second's way up; the first's is off, and Tab passes it by.
    ui.tab_to(cx, ("filter-up", 1usize));
    ui.press(cx, "enter");
    assert_eq!(ui.read(cx, |v| v.settings.filters.iter().map(|f| f.name.clone()).collect::<Vec<_>>()), ["動画", "画像"]);
}

#[gpui_kit::test]
fn narrows_a_folder_by_name_typed_in_its_rows_and_lists_all_again_on_escape(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default()
            .with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), folder(r"D:\gamma")]), (r"D:\alpha", vec![])])
    });
    let ui = open(cx, fake);
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    let (id, alpha) = ui.read(cx, |v| (v.state.columns[1].id, v.state.columns[2].id));
    // Each folder has its field from the start, an empty one too.
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    assert!(ui.act(cx, |window, _| window.try_find(("filter", alpha)).is_some()));
    let typed = |ui: &Ui, cx: &mut TestAppContext| ui.view.read_with(cx, |v, cx| v.filters[&id].input.read(cx).value().to_string());
    // What is typed narrows the rows rather than jumping through them; the folder it hides stays picked,
    // and open, as home's place does.
    ui.input(cx, "TA");
    let shown =
        |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>());
    assert_eq!(shown(&ui, cx), [r"D:\beta.txt"]);
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    // ↓ picks the first it lets through, as a click does, the keys on it.
    ui.press(cx, "down");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    // Esc in the rows lists all again, and what is typed there starts afresh.
    ui.press(cx, "escape");
    ui.input(cx, "mm");
    assert_eq!(shown(&ui, cx), [r"D:\gamma"]);
    ui.press(cx, "enter");
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma"]);
    // Esc in the rows lists all again and empties the field, the pick kept.
    ui.press(cx, "escape");
    assert_eq!(shown(&ui, cx).len(), 3);
    assert_eq!(typed(&ui, cx), "");
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\gamma"]);
    // In the field, Esc clears it, then goes back to the rows.
    ui.input(cx, "zz");
    assert!(shown(&ui, cx).is_empty());
    // Nothing let through, ↓ and Enter keep the keys in the field.
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "escape");
    assert_eq!(shown(&ui, cx).len(), 3);
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "escape");
    assert!(ui.columns_focused(cx));
    assert!(ui.act(cx, |window, _| window.try_find(("filter", id)).is_some()));
    // Its x clears it.
    ui.input(cx, "be");
    assert_eq!(shown(&ui, cx), [r"D:\beta.txt"]);
    ui.click(cx, ("clear-filter", id));
    assert_eq!(shown(&ui, cx).len(), 3);
    assert_eq!(typed(&ui, cx), "");
    // Lifted for what was made or pasted, the field empties too.
    ui.input(cx, "be");
    ui.view.update(cx, |v, cx| {
        v.state.set_filter(1, None);
        cx.notify();
    });
    settle(cx);
    ui.act(cx, |_, _| ());
    assert_eq!(typed(&ui, cx), "");
    assert_eq!(shown(&ui, cx).len(), 3);
}

#[gpui_kit::test]
fn narrows_home_from_its_field_keeping_the_place_open(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"C:\", "OS (C:)"), Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"C:\", vec![]), (r"D:\", vec![folder(r"D:\alpha")])])
    });
    let ui = open(cx, fake);
    let home = ui.read(cx, |v| v.state.columns[0].id);
    // Home has its field from the start, where back and forward were: those are beside the search.
    assert!(ui.act(cx, |window, _| window.try_find(("filter", home)).is_some()));
    let (back, field) = ui.act(cx, |window, _| (window.find("back").bounds(), window.find("search-box").bounds()));
    assert!(back.right() <= field.left(), "{back:?} {field:?}");
    ui.click(cx, row(r"D:\"));
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    // / in home goes to its field; what is typed narrows its rows, and the place hidden stays open.
    assert!(ui.toasts(cx).is_empty());
    ui.input(cx, "os");
    let shown =
        |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.columns[0].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>());
    assert_eq!(shown(&ui, cx), [r"C:\"]);
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 2);
    // ↓ opens the first it lets through, the keys on it in home's rows.
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 0), [r"C:\"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].path.clone()), PathBuf::from(r"C:\"));
    assert!(ui.home_focused(cx));
    assert_eq!(ui.read(cx, |v| v.state.columns[0].focused.clone()), Some(PathBuf::from(r"C:\")));
    // Enter opens the row the keys were on while it shows, rather than the first.
    ui.press(cx, "escape");
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    ui.input(cx, ":");
    assert_eq!(shown(&ui, cx).len(), 2);
    ui.press(cx, "enter");
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    assert!(ui.home_focused(cx));
    // Nothing let through says so, and ↓ and Enter keep the keys in the field.
    ui.input(cx, "zz");
    assert!(shown(&ui, cx).is_empty());
    ui.press(cx, "down");
    ui.press(cx, "enter");
    assert!(!ui.home_focused(cx));
    assert_eq!(ui.selected(cx, 0), [r"D:\"]);
    // Esc clears it, then goes back to the rows.
    ui.press(cx, "escape");
    assert_eq!(shown(&ui, cx).len(), 2);
    ui.press(cx, "escape");
    assert!(ui.home_focused(cx));
    assert_eq!(ui.active(cx), 0);
}

#[gpui_kit::test]
fn keeps_the_arrows_and_enter_in_a_filter_field_to_itself(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\alpha"), file(r"D:\beta.txt")]), (r"D:\alpha", vec![])])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    assert_eq!(ui.selected(cx, 1), [r"D:\alpha"]);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    let active = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.state.active);
    ui.input(cx, "al");
    // Past either end of the text, ← and → stay in the field rather than going to the next column.
    ui.press(cx, "right");
    assert_eq!(active(&ui, cx), 1);
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "left");
    ui.press(cx, "left");
    ui.press(cx, "left");
    assert_eq!(active(&ui, cx), 1);
    assert!(!ui.columns_focused(cx));
    ui.press(cx, "escape");
    ui.press(cx, "escape");
    ui.press(cx, "down");
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    // Enter goes into the rows and opens nothing there.
    ui.input(cx, "ta");
    ui.press(cx, "enter");
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.selected(cx, 1), [r"D:\beta.txt"]);
    assert!(fake.opened.lock().unwrap().is_empty());
}
