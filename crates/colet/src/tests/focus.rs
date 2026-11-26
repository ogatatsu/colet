//! Where the keys go as a menu, a dialog or the history closes, and as a tab comes back to the front; and
//! the ring round what has them.
use super::*;

impl Ui {
    /// What the address field holds while it is open.
    fn typed_address(&self, cx: &mut TestAppContext) -> Option<String> {
        let (open, field) = self.view.read_with(cx, |v, cx| {
            let address = v.address.read(cx);
            (address.is_open(), address.field().clone())
        });
        open.then(|| field.read_with(cx, |input, _| input.value().to_string()))
    }

    fn address_focused(&self, cx: &mut TestAppContext) -> bool {
        let view = self.view.clone();
        self.act(cx, |window, cx| view.read(cx).address.read(cx).field().read(cx).focus_handle(cx).is_focused(window))
    }
}

#[gpui_kit::test]
fn a_dialog_gives_the_keys_back_to_where_they_were_as_it_opened(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-f");
    ui.click(cx, "preferences");
    assert!(ui.read(cx, |v| v.overlays.has_dialog()));
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
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
    assert!(ui.read(cx, |v| v.overlays.is_history_open()));
    ui.press_and_release(cx, "history-button");
    assert!(ui.read(cx, |v| !v.overlays.is_history_open()));
    assert!(ui.search_field_focused(cx));
}

#[gpui_kit::test]
fn a_dialog_opened_from_a_menu_gives_the_keys_back_to_where_the_menu_opened(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-f");
    ui.press(cx, "ctrl-shift-f");
    ui.click(cx, "edit-filters");
    assert!(ui.read(cx, |v| matches!(v.overlays.dialog(), Some(Dialog::Filters { .. }))));
    ui.click(cx, "close-filters");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
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
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn a_tab_left_while_typing_an_address_takes_it_up_again(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\wo");
    ui.press(cx, "ctrl-t");
    assert_eq!(ui.typed_address(cx), None);
    assert!(ui.search_focused(cx));
    ui.press(cx, "ctrl-tab");
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    assert!(ui.address_focused(cx));
    // The caret where it was: typed on, it goes on from there.
    ui.input(cx, "rk");
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\work"));
}

#[gpui_kit::test]
fn a_dialog_opened_while_typing_an_address_gives_it_back_as_typed(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\wo");
    ui.click(cx, "preferences");
    assert!(ui.read(cx, |v| v.overlays.has_dialog()));
    // The field stays open behind it.
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    ui.press(cx, "escape");
    assert!(ui.read(cx, |v| !v.overlays.has_dialog()));
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    assert!(ui.address_focused(cx));
    // The caret where it was: typed on, it goes on from there.
    ui.input(cx, "rk");
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\work"));
}

#[gpui_kit::test]
fn the_history_opened_while_typing_an_address_gives_it_back_as_typed(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\wo");
    ui.press_and_release(cx, "history-button");
    assert!(ui.read(cx, |v| v.overlays.is_history_open()));
    ui.press_and_release(cx, "history-button");
    assert!(ui.read(cx, |v| !v.overlays.is_history_open()));
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    assert!(ui.address_focused(cx));
}

/// Esc closes the history, the topmost thing open, wherever the keys are, and goes no further: what is
/// typed in the field with the keys stays, and the keys stay there.
#[gpui_kit::test]
fn closes_the_history_with_esc_wherever_the_keys_are(cx: &mut TestAppContext) {
    let open_then_esc = |ui: &Ui, cx: &mut TestAppContext| {
        ui.press_and_release(cx, "history-button");
        assert!(ui.read(cx, |v| v.overlays.is_history_open()));
        ui.press(cx, "escape");
        assert!(ui.read(cx, |v| !v.overlays.is_history_open()));
    };
    let (_, ui) = in_work(cx);

    // An address being typed.
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\wo");
    open_then_esc(&ui, cx);
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    assert!(ui.address_focused(cx));
    ui.press(cx, "escape");

    // A column's narrowing.
    let work = ui.read(cx, |v| v.state.columns[2].id);
    let narrowed = |ui: &Ui, cx: &mut TestAppContext| {
        ui.view.read_with(cx, |v, cx| v.narrow_fields.get(&work).map(|f| f.input.read(cx).value().to_string()))
    };
    ui.input(cx, "c");
    open_then_esc(&ui, cx);
    assert_eq!(narrowed(&ui, cx).as_deref(), Some("c"));
    ui.press(cx, "escape");
    assert_eq!(narrowed(&ui, cx).unwrap_or_default(), "");

    // The search field.
    ui.press(cx, "ctrl-f");
    ui.input(cx, "report");
    open_then_esc(&ui, cx);
    assert_eq!(ui.field(cx), "report");
    assert!(ui.search_focused(cx));
}

#[gpui_kit::test]
fn the_address_field_stays_open_over_a_trip_to_another_window_and_a_click_coming_back_closes_it(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "ctrl-l");
    ui.input(cx, r"D:\wo");
    gpui_kit::VisualTestContext::from_window(ui.window, cx).deactivate_window();
    ui.act(cx, |_, _| ());
    // Back with nothing clicked, the field is as it was left.
    ui.act(cx, |window, _| window.activate_window());
    settle(cx);
    // The first frame drawn in front, and what is run after it, as the platform has them.
    ui.act(cx, |window, cx| window.simulate_next_frame(cx));
    settle(cx);
    assert_eq!(ui.typed_address(cx).as_deref(), Some(r"D:\wo"));
    assert!(ui.address_focused(cx));
    // Away again, and back by a click on a row: it lands before the window is drawn active.
    gpui_kit::VisualTestContext::from_window(ui.window, cx).deactivate_window();
    ui.act(cx, |_, _| ());
    cx.update_window(ui.window, |_, window, cx| {
        window.activate_window();
        window.click(row(r"D:\work\b.txt"), cx);
    })
    .unwrap();
    settle(cx);
    ui.act(cx, |window, cx| window.simulate_next_frame(cx));
    settle(cx);
    assert_eq!(ui.typed_address(cx), None);
    assert!(ui.columns_focused(cx));
}

#[gpui_kit::test]
fn the_column_worked_in_is_marked_only_while_the_keys_are_in_the_columns_or_go_back_there(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.click(cx, row(r"D:\work\b.txt"));
    assert!(ui.column_marks(cx) == (true, true));
    // In the address field, it steps back.
    ui.press(cx, "ctrl-l");
    assert!(ui.address_focused(cx));
    assert!(!ui.column_marks(cx).0);
    // A dialog that gives the keys back to the column leaves it marked, but its row unringed.
    ui.click(cx, row(r"D:\work\b.txt"));
    ui.click(cx, "preferences");
    assert!(ui.column_marks(cx) == (true, false));
    ui.press(cx, "escape");
    assert!(ui.column_marks(cx) == (true, true));
    // One that gives them back to the search field does not.
    ui.press(cx, "ctrl-f");
    assert!(!ui.column_marks(cx).0);
    ui.click(cx, "preferences");
    assert!(!ui.column_marks(cx).0);
}

impl Ui {
    /// Where the focus ring is drawn, if anywhere.
    fn ring(&self, cx: &mut TestAppContext) -> Option<Bounds<gpui_kit::Pixels>> {
        self.act(cx, |window, _| window.try_find("focus-ring").map(|ring| ring.bounds()))
    }

    fn bounds_of(&self, cx: &mut TestAppContext, id: impl Into<ElementId>) -> Bounds<gpui_kit::Pixels> {
        let id = id.into();
        self.act(cx, |window, _| window.find(id).bounds())
    }

    /// The pointer moved, as a hand moves it over the window.
    fn move_pointer(&self, cx: &mut TestAppContext) {
        use gpui_kit::test::TestEventExt;
        self.act(cx, |window, cx| window.pointer_move(point(px(600.), px(400.)), None, cx));
        settle(cx);
    }
}

#[gpui_kit::test]
fn a_ring_stays_on_what_has_the_keys_while_the_mouse_moves(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    // One of Colet's own buttons, ringed on its edges.
    ui.tab_to(cx, ("column-sort", work));
    let sort = ui.bounds_of(cx, ("column-sort", work));
    assert_eq!(ui.ring(cx), Some(sort));
    ui.move_pointer(cx);
    assert_eq!(ui.ring(cx), Some(sort));
    // The library's, ringed on its edges as Colet's are.
    ui.tab_to(cx, "refresh");
    let refresh = ui.bounds_of(cx, "refresh");
    assert_eq!(ui.ring(cx), Some(refresh));
    ui.move_pointer(cx);
    assert_eq!(ui.ring(cx), Some(refresh));
    // One with a border of its own, ringed a pixel off it.
    ui.tab_to(cx, "search-filter");
    let filter = ui.bounds_of(cx, "search-filter").dilate(px(crate::view::RING_ROOM));
    assert_eq!(ui.ring(cx), Some(filter));
    ui.move_pointer(cx);
    assert_eq!(ui.ring(cx), Some(filter));
}

#[gpui_kit::test]
fn the_mouse_presses_a_button_leaving_the_keys_and_no_ring_on_it(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    let work = ui.read(cx, |v| v.state.columns[2].id);
    ui.click(cx, row(r"D:\work\b.txt"));
    ui.click(cx, "refresh");
    ui.click(cx, ("star", work));
    assert!(ui.columns_focused(cx));
    assert_eq!(ui.ring(cx), None);
}

#[gpui_kit::test]
fn a_choice_picked_with_the_mouse_takes_the_ring_with_it_from_any_choice(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.click(cx, "preferences");
    // The language chosen has the keys as the preferences open, ringed round.
    let system = ui.bounds_of(cx, ("language", Language::System as usize)).dilate(px(crate::view::RING_ROOM));
    assert_eq!(ui.ring(cx), Some(system));
    ui.click(cx, ("language", Language::Japanese as usize));
    let japanese = ui.bounds_of(cx, ("language", Language::Japanese as usize)).dilate(px(crate::view::RING_ROOM));
    assert_eq!(ui.ring(cx), Some(japanese));
    // A pick in another group takes them too: the ring is never left on one choice while another is
    // picked, which would read as two picked.
    ui.click(cx, ("theme", Theme::Dark as usize));
    let dark = ui.bounds_of(cx, ("theme", Theme::Dark as usize)).dilate(px(crate::view::RING_ROOM));
    assert_eq!(ui.ring(cx), Some(dark));
}

#[gpui_kit::test]
fn a_choice_picked_with_the_mouse_leaves_the_keys_in_a_field(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a.pdf")]));
    fake.settings.lock().unwrap().saved_searches =
        vec![SavedSearch { name: String::new(), query: "ext:pdf".into(), filter: None, narrow: None, order: Order::default() }];
    let ui = open(cx, fake.clone());
    ui.click(cx, row(HomeKey::search("ext:pdf", None, None, Order::default()).key()));
    ui.press(cx, "f2");
    let name = ui.read(cx, |v| match v.overlays.dialog() {
        Some(crate::dialogs::Dialog::EditSearch(edit)) => edit.name.clone(),
        _ => panic!("no dialog"),
    });
    let in_name = |ui: &Ui, cx: &mut TestAppContext| ui.act(cx, |window, cx| name.read(cx).focus_handle(cx).is_focused(window));
    assert!(in_name(&ui, cx));
    // Being typed in, the field keeps them as the order is picked.
    let descending =
        ElementId::NamedChild(std::sync::Arc::new("edit-search-direction".into()), SharedString::from(crate::words::direction(true)));
    ui.click(cx, descending);
    assert!(in_name(&ui, cx));
}
