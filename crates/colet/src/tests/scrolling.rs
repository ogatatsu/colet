//! Widths, the columns sliding and scrolling, and what stays on screen.
use super::*;

#[gpui_kit::test]
fn widens_home_by_its_edge_within_its_bounds(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default()));
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(100.5), edge.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 341.);
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(px(0.), edge.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 160.);
    assert_eq!(ui.read(cx, |v| v.state.columns[0].kind), Kind::Home);
}

#[gpui_kit::test]
fn ends_a_widening_let_go_of_off_the_window_once_the_pointer_comes_back(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent as _, MouseMoveEvent};
    let fake = Arc::new(Fake::default());
    let ui = open(cx, fake.clone());
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.hold_drag(cx, edge, point(edge.x + px(100.), edge.y));
    assert!(ui.read(cx, |v| v.widths.resizing().is_some()));
    // The button came up off the window, unheard; the pointer comes back with it up.
    ui.act(cx, |window, cx| {
        let modifiers = window.modifiers();
        let back = point(edge.x + px(200.), edge.y);
        window.dispatch_event(MouseMoveEvent { position: back, pressed_button: None, modifiers }.to_platform_input(), cx);
        window.render_frame(cx);
    });
    settle(cx);
    assert!(ui.read(cx, |v| v.widths.resizing().is_none()));
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 340.);
    assert_eq!(fake.settings.lock().unwrap().sidebar_width, 340.);
}

#[gpui_kit::test]
fn widens_the_search_field_by_the_chevron_beside_it_within_its_bounds_and_keeps_it(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().search_field_width = 300.;
    let ui = open(cx, fake.clone());
    let field = |ui: &Ui, cx: &mut TestAppContext| ui.act(cx, |window, _| window.find("search-box").bounds().size.width);
    let address = |ui: &Ui, cx: &mut TestAppContext| ui.act(cx, |window, _| window.find("address").bounds().size.width);
    let (field_before, address_before) = (field(&ui, cx), address(&ui, cx));
    let edge = ui.act(cx, |window, _| window.find("search-field-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(100.4), edge.y), cx));
    settle(cx);
    // The field takes what the address gives.
    assert_eq!(ui.read(cx, |v| v.widths.field), 400.);
    assert_eq!(field(&ui, cx), field_before + px(100.));
    assert_eq!(address(&ui, cx), address_before - px(100.));
    assert_eq!(fake.settings.lock().unwrap().search_field_width, 400.);
    let edge = ui.act(cx, |window, _| window.find("search-field-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(px(0.), edge.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.field), crate::app::SEARCH_FIELD_WIDTHS.0);
    let edge = ui.act(cx, |window, _| window.find("search-field-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(2000.), edge.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.field), crate::app::SEARCH_FIELD_WIDTHS.1);
}

#[gpui_kit::test]
fn widens_a_column_by_an_edge_astride_its_border_even_past_it(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    ui.press(cx, "right");
    let drive = ui.read(cx, |v| v.state.columns[1].id);
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3, "a column past the drive's");
    let before = ui.read(cx, |v| v.widths.column);
    let (column, edge) =
        ui.act(cx, |window, _| (window.find(("column", drive)).bounds(), window.within(("column", drive)).find("column-resizer").bounds()));
    // Half in the column, clear of its scrollbar; half over the next, which it is drawn above.
    assert!(edge.left() < column.right() && column.right() < edge.right(), "{edge:?} {column:?}");
    // Above the columns' scrollbar along the bottom, which keeps its presses.
    assert!(edge.bottom() <= column.bottom() - px(8.), "{edge:?} {column:?}");
    let past = point(column.right() + px(1.), edge.center().y);
    ui.act(cx, |window, cx| window.drag(past, point(past.x + px(40.), past.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.column), before + 40.);
}

#[gpui_kit::test]
fn leaves_the_edge_of_a_column_scrolled_out_of_view_unpressable(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    // A width the view holds no whole number of, so an edge falls past its left.
    fake.settings.lock().unwrap().column_width = 288.;
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    let before = ui.read(cx, |v| v.widths.column);
    let ids: Vec<_> = ui.read(cx, |v| v.state.columns.iter().map(|c| c.id).collect());
    // The edge of a column the view has scrolled past, now under what lies left of the columns.
    let hidden = ui.act(cx, |window, _| {
        let view = window.find("columns-scroll").bounds();
        ids.iter().find_map(|&id| {
            window.try_find(("column", id))?;
            let edge = window.within(("column", id)).try_find("column-resizer")?.bounds();
            (edge.right() < view.left() && edge.left() > px(0.)).then(|| edge.center())
        })
    });
    let at = hidden.expect("an edge scrolled out of view");
    ui.act(cx, |window, cx| window.drag(at, point(at.x + px(40.), at.y), cx));
    settle(cx);
    assert_eq!(ui.read(cx, |v| v.widths.column), before);
}

#[gpui_kit::test]
fn leaves_no_room_to_scroll_once_the_columns_slide_back(cx: &mut TestAppContext) {
    // A chain deep enough to scroll the columns, and a favorite of one column beside it.
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0]), folder(r"D:\fav")]), (r"D:\fav", vec![])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    fake.settings.lock().unwrap().favorites = vec![r"D:\fav".into()];
    let ui = open(cx, Arc::new(fake));
    // The slide runs on the clock: frames a screen's refresh apart until it rests.
    let rest = |ui: &Ui, cx: &mut TestAppContext| {
        for _ in 0..120 {
            std::thread::sleep(Duration::from_millis(16));
            ui.act(cx, |_, _| ());
            settle(cx);
            if ui.read(cx, |v| v.scrolls.sideways.slide.is_none()) {
                break;
            }
        }
    };
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    assert!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x) < px(0.), "the chain scrolls the columns");
    ui.click_in(cx, "home", row(r"D:\fav"));
    rest(&ui, cx);
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x), px(0.));
    assert_eq!(ui.read(cx, |v| v.scrolls.sideways.scroll.max_offset().x), px(0.));
    assert!(!ui.read(cx, |v| v.on_screen().hidden_left()));
}

#[gpui_kit::test]
fn turns_the_wheel_down_a_column_without_moving_the_columns_or_shading_their_edges(cx: &mut TestAppContext) {
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0]), folder(r"D:\fav")]), (r"D:\fav", vec![])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    let ui = open(cx, Arc::new(fake));
    let rest = |ui: &Ui, cx: &mut TestAppContext| {
        for _ in 0..120 {
            std::thread::sleep(Duration::from_millis(16));
            ui.act(cx, |_, _| ());
            settle(cx);
            if ui.read(cx, |v| v.scrolls.sideways.slide.is_none()) {
                break;
            }
        }
    };
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    let at = ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x);
    assert!(at < px(0.), "the chain scrolls the columns");
    // Down and up over a column scroll its rows; the columns stay where they rest.
    let column = ui.read(cx, |v| v.state.columns[v.state.active].id);
    for dy in [-120., 120.] {
        ui.act(cx, |window, cx| window.scroll(("column", column), ScrollDelta::Pixels(point(px(0.), px(dy))), cx));
        settle(cx);
        assert_eq!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x), at);
    }
    // An offset past the bounds, as a wheel leaves it until the next layout, reads as where it will be drawn.
    ui.click_in(cx, "home", row(r"D:\"));
    rest(&ui, cx);
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.scrolls.sideways.scroll.max_offset().x), px(0.));
    ui.read(cx, |v| v.scrolls.sideways.scroll.set_offset(point(px(-300.), px(0.))));
    assert!(!ui.read(cx, |v| v.on_screen().hidden_left()));
}

#[gpui_kit::test]
fn leaves_the_columns_where_they_are_once_left_brings_the_keys_back_home(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    for _ in 1..chain.len() {
        ui.press(cx, "left");
    }
    rest(&ui, cx);
    assert_eq!(ui.read(cx, |v| v.state.active), 1, "the keys are on the first column");
    // Scrolled by hand short of the last column: going home would once have slid them to it.
    ui.read(cx, |v| v.scrolls.sideways.scroll.set_offset(point(px(-100.), px(0.))));
    ui.act(cx, |_, _| ());
    let at = ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x);
    assert!(at < px(0.) && ui.read(cx, |v| v.scrolls.sideways.scroll.max_offset().x) > -at, "short of the last column");
    ui.press(cx, "left");
    assert_eq!(ui.read(cx, |v| v.state.active), 0, "the keys are home");
    rest(&ui, cx);
    assert_eq!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x), at);
}

#[gpui_kit::test]
fn slides_a_column_on_screen_once_a_press_in_its_header_or_its_empty_room_is_let_go(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    // A width the view holds no whole number of, so the right edge cuts a column.
    fake.settings.lock().unwrap().column_width = 288.;
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    // Brought back to the start by hand: the column cut by the right edge, and how far it reaches past it.
    let cut = |ui: &Ui, cx: &mut TestAppContext| {
        ui.read(cx, |v| v.scrolls.sideways.scroll.set_offset(point(px(0.), px(0.))));
        ui.act(cx, |_, _| ());
        let ids: Vec<ColumnId> = ui.read(cx, |v| v.state.columns.iter().map(|c| c.id).collect());
        ui.act(cx, |window, _| {
            let edge = window.find("columns-scroll").bounds().right();
            ids.into_iter().find_map(|id| {
                let b = window.try_find(("column", id))?.bounds();
                (b.left() + px(40.) < edge && b.right() > edge).then_some(id)
            })
        })
        .expect("a column cut by the right edge")
    };
    for target in ["head", "column"] {
        let id = cut(&ui, cx);
        let index = ui.read(cx, |v| v.state.index_of(id).unwrap());
        let selected = ui.selected(cx, index);
        let at = if target == "head" { point(px(20.), px(10.)) } else { point(px(20.), px(300.)) };
        ui.act(cx, |window, cx| window.click_at((target, id), at, cx));
        settle(cx);
        rest(&ui, cx);
        assert_eq!(ui.active(cx), index, "{target}");
        let (column, view) = ui.act(cx, |window, _| (window.find(("column", id)).bounds(), window.find("columns-scroll").bounds()));
        assert!(column.right() <= view.right() + px(1.), "{target}: the column slid on screen");
        // Only gone to: what it had picked stays as it was.
        assert_eq!(ui.selected(cx, index), selected, "{target}");
    }
}

#[gpui_kit::test]
fn keeps_the_last_column_at_the_right_edge_as_the_window_is_resized(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    assert!(room_past_last(&ui, cx).abs() < 1.);
    for width in [1500., 1000., 1300.] {
        cx.simulate_window_resize(ui.window, size(px(width), px(800.)));
        // Already in the first frame laid out at the new size.
        let room = room_past_last(&ui, cx);
        assert!(room.abs() < 1., "{width}: {room}");
        settle(cx);
        assert!(room_past_last(&ui, cx).abs() < 1.);
        assert!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x) < px(0.), "{width}: the columns still scroll");
    }
    // Brought short of the end by hand, the left edge stays as the window narrows.
    ui.read(cx, |v| v.scrolls.sideways.scroll.set_offset(point(px(-100.), px(0.))));
    ui.act(cx, |_, _| ());
    cx.simulate_window_resize(ui.window, size(px(1100.), px(800.)));
    ui.act(cx, |_, _| ());
    assert_eq!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x), px(-100.));
}

#[gpui_kit::test]
fn keeps_the_last_column_at_the_right_edge_as_a_width_is_dragged(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    let last = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    // The columns' width, then home's both ways, which changes the columns' view.
    for (resizer, by) in [("column-resizer", 60.), ("sidebar-resizer", 120.), ("sidebar-resizer", -120.)] {
        let edge = ui.act(cx, |window, _| {
            if resizer == "column-resizer" { window.within(("column", last)).find(resizer) } else { window.find(resizer) }.bounds().center()
        });
        ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(by), edge.y), cx));
        let room = room_past_last(&ui, cx);
        assert!(room.abs() < 1., "{resizer} {by}: {room}");
        settle(cx);
    }
}

#[gpui_kit::test]
fn leaves_no_room_past_the_last_column_once_the_folders_folded_away_come_out(cx: &mut TestAppContext) {
    let deep = file(r"D:\a\b\c\d\zeta.txt");
    let fake = Arc::new(Fake::default().with_results(vec![deep.clone()]).with_folders(&[
        (r"D:\", vec![folder(r"D:\a")]),
        (r"D:\a", vec![folder(r"D:\a\b")]),
        (r"D:\a\b", vec![folder(r"D:\a\b\c")]),
        (r"D:\a\b\c", vec![folder(r"D:\a\b\c\d")]),
        (r"D:\a\b\c\d", vec![deep.clone()]),
    ]));
    let ui = open(cx, fake);
    ui.input(cx, "zeta");
    ui.click(cx, row(&deep.path));
    rest(&ui, cx);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 4);
    ui.click(cx, "unfold");
    rest(&ui, cx);
    assert_eq!(ui.read(cx, |v| v.state.folded_away()), 0);
    assert!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x) < px(0.), "the folders that came out scroll the columns");
    let room = room_past_last(&ui, cx);
    assert!(room.abs() < 1., "{room}");
}

#[gpui_kit::test]
fn slides_the_folders_brought_out_by_their_strip_in_from_the_left_as_far_as_the_anchor_allows(cx: &mut TestAppContext) {
    let ui = folded_deep(cx);
    let anchor = ui.read(cx, |v| v.state.columns.len() - 1);
    let (before, _) = column_edges(&ui, cx, anchor);
    ui.click(cx, "unfold");
    // The anchor stays where it was drawn rather than leap aside for the folders: it has moved only as
    // far as the slide has gone.
    let (moved, _) = column_edges(&ui, cx, anchor);
    let gone = ui.read(cx, |v| v.scrolls.sideways.slide.as_ref().map(|s| s.origin - s.position)).expect("they slide");
    assert!((moved - before - gone).abs() < 1., "{before} {moved} {gone}");
    rest(&ui, cx);
    let (after, right) = column_edges(&ui, cx, anchor);
    assert!(after > moved, "the folders push the anchor right as they come in");
    // At rest the anchor ends at the right edge, with the folders above it in the room on its left.
    let edge = ui.act(cx, |window, _| f32::from(window.find("columns-scroll").bounds().right()));
    assert!((edge - right).abs() < 1., "{edge} {right}");
    assert!(ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x) < px(0.), "they do not all fit");
}

#[gpui_kit::test]
fn slides_a_folded_crumb_picked_in_from_the_left_until_it_is_on_screen(cx: &mut TestAppContext) {
    let ui = folded_deep(cx);
    let anchor = ui.read(cx, |v| v.state.columns.len() - 1);
    let (before, _) = column_edges(&ui, cx, anchor);
    let third = crumb(&ui, cx, 3);
    ui.click(cx, third);
    assert_eq!(ui.active(cx), 3);
    let (moved, _) = column_edges(&ui, cx, anchor);
    let gone = ui.read(cx, |v| v.scrolls.sideways.slide.as_ref().map(|s| s.origin - s.position)).expect("they slide");
    assert!((moved - before - gone).abs() < 1., "{before} {moved} {gone}");
    rest(&ui, cx);
    // Only as far as shows the column gone into, whole; the drive's stays out on the left.
    let (left, _) = column_edges(&ui, cx, 3);
    let edge = ui.act(cx, |window, _| f32::from(window.find("columns-scroll").bounds().left()));
    assert!(left >= edge - 1., "{left} {edge}");
    assert!(ui.read(cx, |v| v.on_screen().hidden_left()));
}

#[gpui_kit::test]
fn turns_the_wheel_with_shift_to_send_the_columns_sideways_leaving_the_rows_under_it(cx: &mut TestAppContext) {
    let chain: Vec<String> = (1..=8).map(|n| format!(r"D:\{}", (1..=n).map(|k| format!("d{k}")).collect::<Vec<_>>().join(r"\"))).collect();
    let mut folders: Vec<(&str, Vec<Entry>)> = vec![(r"D:\", vec![folder(&chain[0])])];
    for (k, path) in chain.iter().enumerate() {
        folders.push((path, chain.get(k + 1).map(|next| vec![folder(next)]).unwrap_or_default()));
    }
    // The deepest folder holds more rows than its column shows.
    let files: Vec<Entry> = (0..100).map(|i| file(&format!(r"{}\f{i:03}.txt", chain[7]))).collect();
    folders.last_mut().unwrap().1 = files;
    let fake = Fake { roots: vec![Entry::folder(r"D:\", "データ (D:)")], ..Fake::default().with_folders(&folders) };
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    for _ in 0..120 {
        std::thread::sleep(Duration::from_millis(16));
        ui.act(cx, |_, _| ());
        settle(cx);
        if ui.read(cx, |v| v.scrolls.sideways.slide.is_none()) {
            break;
        }
    }
    let column = ui.read(cx, |v| v.state.columns.iter().find(|c| c.path == chain[7]).unwrap().id);
    let rows = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.scrolls.rows.handles[&column].0.borrow().base_handle.offset().y);
    let columns = |ui: &Ui, cx: &mut TestAppContext| ui.read(cx, |v| v.scrolls.sideways.scroll.offset().x);
    let wheel = |ui: &Ui, cx: &mut TestAppContext, x: f32, y: f32| {
        ui.act(cx, |window, cx| window.scroll(("column", column), ScrollDelta::Lines(point(x, y)), cx));
        settle(cx);
    };
    // The wheel alone scrolls the rows, leaving the columns.
    let (top, at) = (rows(&ui, cx), columns(&ui, cx));
    assert!(at < px(0.), "the chain scrolls the columns");
    wheel(&ui, cx, 0., -3.);
    let down = rows(&ui, cx);
    assert!(down < top, "the rows scroll");
    assert_eq!(columns(&ui, cx), at);
    // With shift, as Windows sends it, the columns go sideways and the rows stay.
    wheel(&ui, cx, 3., 0.);
    assert!(columns(&ui, cx) > at, "the columns scroll");
    assert_eq!(rows(&ui, cx), down);
}

#[gpui_kit::test]
fn scrolls_a_column_s_last_row_clear_of_the_columns_scrollbar(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let deepest = chain.last().unwrap().clone();
    let files: Vec<Entry> = (0..80).map(|n| file(&format!(r"{deepest}\{n:02}.txt"))).collect();
    let last = files.last().unwrap().path.clone();
    for entry in files {
        fake.add(Path::new(&deepest), entry);
    }
    let ui = open(cx, Arc::new(fake));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..=chain.len() {
        ui.press(cx, "right");
    }
    ui.press(cx, "end");
    rest(&ui, cx);
    let active = ui.active(cx);
    assert_eq!(ui.selected(cx, active), std::slice::from_ref(&last));
    let (view, bottom) = ui.act(cx, |window, _| (window.find("columns-scroll").bounds(), window.find(row(&last)).bounds().bottom()));
    // The columns scroll sideways, so their scrollbar lies over the bottom.
    assert!(ui.read(cx, |v| v.scrolls.sideways.scroll.max_offset().x) > px(0.));
    assert!(bottom <= view.bottom() - px(crate::view::SCROLLBAR_WIDTH), "{bottom:?} against {view:?}");
}

#[gpui_kit::test]
fn widens_home_by_as_much_as_the_pointer_moves_at_the_size_chosen(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().zoom = 125;
    let ui = open(cx, fake.clone());
    let edge = ui.act(cx, |window, _| window.find("sidebar-resizer").bounds().center());
    ui.act(cx, |window, cx| window.drag(edge, point(edge.x + px(100.), edge.y), cx));
    settle(cx);
    // Kept as at 100%, so it is as wide against the text whatever the size.
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 320.);
    assert_eq!(fake.settings.lock().unwrap().sidebar_width, 320.);
    assert_eq!(ui.act(cx, |window, _| f32::from(window.find("sidebar").bounds().size.width)), 400.);
}

#[gpui_kit::test]
fn narrows_home_in_a_window_the_size_chosen_leaves_too_narrow(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default());
    fake.settings.lock().unwrap().zoom = 200;
    let ui = open(cx, fake.clone());
    // 1200 pixels hold 600 at 100%: home gives way to its least, so the rest keeps its own.
    assert_eq!(ui.act(cx, |window, _| f32::from(window.find("sidebar").bounds().size.width)), 2. * crate::app::SIDEBAR_WIDTHS.0);
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 240.);
}

#[gpui_kit::test]
fn keeps_the_last_column_at_the_right_edge_as_the_size_changes(cx: &mut TestAppContext) {
    let (fake, chain) = deep_chain();
    let ui = open(cx, Arc::new(fake));
    // Wide enough at 200% for the column worked in and the last beside it: narrower, the one worked in is
    // kept whole rather than the last flush.
    cx.simulate_window_resize(ui.window, size(px(1800.), px(900.)));
    ui.click_in(cx, "home", row(r"D:\"));
    for _ in 0..chain.len() {
        ui.press(cx, "right");
    }
    rest(&ui, cx);
    assert!(room_past_last(&ui, cx).abs() < 1.);
    let last = ui.read(cx, |v| v.state.columns.last().unwrap().id);
    for zoom in [150, 90, 200, 100] {
        ui.view.update(cx, |v, cx| v.set_zoom(zoom, cx));
        rest(&ui, cx);
        let room = room_past_last(&ui, cx);
        assert!(room.abs() < 1., "{zoom}: {room}");
        // Each column as wide as its width at 100% grown by the size.
        let width = ui.act(cx, |window, _| f32::from(window.find(("column", last)).bounds().size.width));
        let expected = ui.read(cx, |v| v.widths.column) * f32::from(zoom) / 100.;
        assert!((width - expected).abs() < 1., "{zoom}: {width} against {expected}");
    }
}
