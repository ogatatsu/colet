//! Notices, dialogs and the narrowest window.
use super::*;

#[gpui_kit::test]
fn keeps_an_error_up_until_it_is_closed_and_lets_a_notice_go_on_its_own(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(drive_d()));
    ui.view.update(cx, |v, cx| v.notify("失敗しました", crate::app::Tone::Error, cx));
    ui.view.update(cx, |v, cx| v.notify("コピーしました", crate::app::Tone::Info, cx));
    settle(cx);
    assert_eq!(ui.toasts(cx), ["失敗しました", "コピーしました"]);
    cx.executor().advance_clock(Duration::from_secs(10));
    settle(cx);
    assert_eq!(ui.toasts(cx), ["失敗しました"]);
    // Its close button shows while the pointer is on it.
    ui.act(cx, |window, cx| window.hover(("toast", 1usize), cx));
    ui.click(cx, "close");
    settle(cx);
    assert!(ui.toasts(cx).is_empty(), "{:?}", ui.toasts(cx));
}

#[gpui_kit::test]
fn keeps_a_dialog_taller_than_the_window_within_it_its_buttons_in_reach(cx: &mut TestAppContext) {
    let ui = open(cx, Arc::new(Fake::default()));
    cx.simulate_window_resize(ui.window, size(px(1200.), px(500.)));
    ui.click(cx, "preferences");
    let (content, close) = ui.act(cx, |window, _| (window.find("dialog-content").bounds(), window.find("close-preferences").bounds()));
    assert!(close.top() >= px(0.) && close.bottom() <= px(500.), "{close:?}");
    // What does not fit scrolls between the heading and the buttons: more than the room it has.
    assert!(content.size.height > close.top() - content.top(), "{content:?} {close:?}");
    ui.click(cx, "close-preferences");
    assert!(ui.read(cx, |v| v.dialog.is_none()));
}

#[gpui_kit::test]
fn keeps_room_to_type_a_search_in_the_narrowest_window(cx: &mut TestAppContext) {
    let fake = drive_d();
    {
        let mut settings = fake.settings.lock().unwrap();
        settings.filters = vec![SearchFilter { name: "去年の夏に撮ったとても大きな写真".into(), query: "ext:jpg".into() }];
        settings.tabs = vec![crate::settings::KeptTab { filter: Some("去年の夏に撮ったとても大きな写真".into()), ..Default::default() }];
        // Home's pane dragged as wide as it goes.
        settings.sidebar_width = 480.;
    }
    let ui = open(cx, Arc::new(fake));
    let (width, height) = crate::app::MIN_WINDOW;
    cx.simulate_window_resize(ui.window, size(px(width), px(height)));
    let (sidebar, field, filter, address, last) = ui.act(cx, |window, _| {
        let find = |id: &str| window.find(SharedString::from(id.to_owned())).bounds();
        (find("sidebar"), find("search-box"), find("search-filter"), find("address"), find("preferences"))
    });
    // The filter, the search and the address in a row, the address keeping room to press to type.
    assert!(filter.right() <= field.left() && field.right() < address.left(), "{filter:?} {field:?} {address:?}");
    assert!(address.size.width >= px(crate::app::ADDRESS_MIN - 6.), "{address:?}");
    // The sort order and the hidden items' button end the file actions' row, under the toolbar.
    let (recycle, sort, hidden) = ui.act(cx, |window, _| {
        let find = |id: &str| window.find(SharedString::from(id.to_owned())).bounds();
        (find("recycle"), find("sort"), find("show-hidden"))
    });
    assert!(sort.top() > field.bottom() && recycle.right() < sort.left() && sort.right() <= hidden.left(), "{recycle:?} {sort:?} {hidden:?}");
    assert!(hidden.right() <= px(width), "{hidden:?}");
    // Home's pane gave way to its usual width, keeping the width it was dragged to.
    assert_eq!(sidebar.size.width, px(width - crate::app::MAIN_MIN));
    assert_eq!(ui.read(cx, |v| v.widths.sidebar), 480.);
    assert!(field.size.width >= px(crate::app::SEARCH_FIELD_MIN - 6.), "{field:?}");
    // The filter's name gave way, down to the funnel and the chevron; nothing is pushed past the edge.
    assert!(filter.size.width < px(200.) && filter.size.width >= px(60.), "{filter:?}");
    assert!(last.right() <= px(width), "{last:?}");
    // The hints where to start stay within the side beside home, clear of its padding.
    let (main, hints) = ui.act(cx, |window, _| {
        let hints: Vec<_> = ["ext:pdf", "dm:today", "size:>1gb"].map(|id| window.find(id).bounds()).into();
        (window.find("columns-scroll").bounds(), hints)
    });
    for hint in hints {
        assert!(hint.left() >= main.left() + px(24.) && hint.right() <= main.right() - px(24.), "{hint:?} in {main:?}");
    }
    // Wide again, the pane is as wide as it was dragged.
    cx.simulate_window_resize(ui.window, size(px(1200.), px(800.)));
    assert_eq!(ui.act(cx, |window, _| window.find("sidebar").bounds().size.width), px(480.));
}
