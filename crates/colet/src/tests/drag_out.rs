//! Rows dragged out to another app.
use gpui_kit::{MouseButton, Pixels, Point, test::TestEventExt};

use super::*;

impl Ui {
    /// Presses the left button on `id` and moves it `by` with the button held, without letting go: past
    /// the drag distance, the drag out has started. Where the pointer ends.
    fn press_and_move(&self, cx: &mut TestAppContext, id: impl Into<ElementId>, by: Point<Pixels>) -> Point<Pixels> {
        let id = id.into();
        let at = self.act(cx, |window, cx| {
            let at = window.find(id).bounds().center();
            window.pointer_move(at, None, cx);
            window.pointer_down(at, MouseButton::Left, cx);
            at
        });
        cx.run_until_parked();
        self.act(cx, |window, cx| window.pointer_move(at + by, Some(MouseButton::Left), cx));
        settle(cx);
        at + by
    }
}

fn far() -> Point<Pixels> {
    point(px(40.), px(0.))
}

#[gpui_kit::test]
fn drags_out_the_whole_selection_from_a_row_in_it(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.click_with(cx, row(r"D:\work\b.txt"), Modifiers::control());
    ui.press_and_move(cx, row(r"D:\work\b.txt"), far());
    assert_eq!(*fake.dragged.lock().unwrap(), [vec![r"D:\work\a.txt".to_owned(), r"D:\work\b.txt".to_owned()]]);
    // Carried as it is: neither the press nor the drag picks again.
    assert_eq!(ui.selected(cx, 2), [r"D:\work\a.txt", r"D:\work\b.txt"]);
}

#[gpui_kit::test]
fn lets_go_of_what_it_drags_until_the_app_dropped_in_is_done_with_it(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    fake.drag_writing.store(true, Ordering::Relaxed);
    let carried = |cx: &mut TestAppContext| ui.read(cx, |v| v.watching.carried.values().flatten().cloned().collect::<Vec<_>>());
    ui.press_and_move(cx, row(r"D:\work\b.txt"), far());
    assert_eq!(fake.dragged.lock().unwrap().len(), 1);
    // Still written out on the target's own thread, after the drag returned.
    settle(cx);
    assert_eq!(carried(cx), [colet_core::paths::path_key(Path::new(r"D:\work\b.txt"))]);
    fake.drag_writing.store(false, Ordering::Relaxed);
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    settle(cx);
    assert!(carried(cx).is_empty());
}

#[gpui_kit::test]
fn picks_a_row_out_of_the_selection_and_drags_it_alone(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.click_with(cx, row(r"D:\work\b.txt"), Modifiers::control());
    ui.press_and_move(cx, row(r"D:\work\c.txt"), point(px(0.), px(-40.)));
    assert_eq!(*fake.dragged.lock().unwrap(), [vec![r"D:\work\c.txt".to_owned()]]);
    assert_eq!(ui.selected(cx, 2), [r"D:\work\c.txt"]);
}

#[gpui_kit::test]
fn drags_nothing_out_for_a_click_that_barely_moves(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    let at = ui.press_and_move(cx, row(r"D:\work\b.txt"), point(px(1.), px(0.)));
    ui.let_go(cx, at);
    assert!(fake.dragged.lock().unwrap().is_empty());
    // A click, as ever.
    assert_eq!(ui.selected(cx, 2), [r"D:\work\b.txt"]);
    assert!(ui.read(cx, |v| v.press.is_none()));
}

#[gpui_kit::test]
fn drags_out_search_results_from_several_folders(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\a\x.txt"), file(r"D:\b\y.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    ui.press(cx, "enter");
    ui.press(cx, "ctrl-a");
    ui.press_and_move(cx, row(r"D:\b\y.txt"), far());
    assert_eq!(*fake.dragged.lock().unwrap(), [vec![r"D:\a\x.txt".to_owned(), r"D:\b\y.txt".to_owned()]]);
}

#[gpui_kit::test]
fn keeps_home_rows_in_the_window(cx: &mut TestAppContext) {
    let (fake, ui) = in_work(cx);
    ui.show_home(cx);
    let at = ui.press_and_move(cx, row(r"D:\"), far());
    ui.let_go(cx, at);
    assert!(fake.dragged.lock().unwrap().is_empty());
}
