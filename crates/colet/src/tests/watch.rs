//! Reloading what changes on disk or in Everything, and letting go of what an operation moves.
use super::*;
use colet_core::everything::PAGE;

#[gpui_kit::test]
fn reloads_only_the_folders_a_change_names(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake {
        roots: vec![Entry::folder(r"D:\", "D:")],
        ..Fake::default().with_folders(&[(r"D:\", vec![folder(r"D:\a")]), (r"D:\a", vec![file(r"D:\a\x.txt")])])
    });
    let ui = open(cx, fake.clone());
    ui.click(cx, row(r"D:\"));
    ui.press(cx, "right");
    assert_eq!(ui.read(cx, |v| v.state.columns.len()), 3);
    fake.lists.lock().unwrap().clear();
    fake.folders.lock().unwrap().get_mut(r"D:\a").unwrap().push(file(r"D:\a\new.txt"));
    let changed = ui.read(cx, |v| v.loading.watching().changed.clone());
    *changed.lock().unwrap() = Changes::Paths(HashSet::from([PathBuf::from(r"D:\a")]));
    settle(cx);
    assert_eq!(*fake.lists.lock().unwrap(), [r"D:\a"]);
    assert_eq!(ui.read(cx, |v| v.state.columns[2].entries.len()), 2);
    // Any change, unnamed, reloads them all.
    *changed.lock().unwrap() = Changes::All;
    settle(cx);
    let lists: HashSet<String> = fake.lists.lock().unwrap().iter().cloned().collect();
    assert!(lists.contains(r"D:\") && lists.contains(r"D:\a"), "{lists:?}");
}

#[gpui_kit::test]
fn reloads_the_results_when_everything_says_they_changed(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results(vec![file(r"D:\x.txt")]));
    let ui = open(cx, fake.clone());
    ui.input(cx, "x");
    let before = fake.queries().len();
    fake.results.lock().unwrap().push(file(r"D:\x2.txt"));
    fake.changed.store(true, Ordering::Relaxed);
    for _ in 0..4 {
        settle(cx);
    }
    assert_eq!(fake.queries().len(), before + 1);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), 2);
}

#[test]
fn lets_go_of_the_folders_at_or_inside_what_an_operation_moves() {
    let folders: Vec<PathBuf> = [r"D:\", r"D:\work", r"D:\work\a", r"D:\work\a\b", r"D:\work\ab"].into_iter().map(PathBuf::from).collect();
    let released = HashSet::from([colet_core::paths::path_key(Path::new(r"d:\WORK\a"))]);
    assert_eq!(texts(&crate::watching::kept_watching(folders.clone(), &released)), [r"D:\", r"D:\work", r"D:\work\ab"]);
    assert_eq!(crate::watching::kept_watching(folders.clone(), &HashSet::new()), folders);
}

#[gpui_kit::test]
fn watches_again_once_an_operation_ends(cx: &mut TestAppContext) {
    let (_, ui) = in_work(cx);
    ui.press(cx, "delete");
    assert!(ui.read_app(cx, |v, app| !v.operations.read(app).is_running() && v.loading.watching().released.is_empty()));
}

#[gpui_kit::test]
fn refreshes_the_pages_in_sight_and_picked_after_a_change_overtaken_by_pagination(cx: &mut TestAppContext) {
    let fake = Arc::new(Fake::default().with_results((0..1211).map(|i| file(&format!(r"D:\{i:04}.txt"))).collect()));
    let ui = open(cx, fake.clone());
    ui.input(cx, "fixture");
    let id = ui.read(cx, |v| v.state.columns[1].id);
    ui.view.update(cx, |v, _| {
        let c = &mut v.state.columns[1];
        c.selected = vec![r"D:\0400.txt".into()];
        c.focused = c.selected.first().cloned();
    });
    fake.results.lock().unwrap().remove(0);
    fake.changed.store(true, Ordering::Relaxed);
    ui.view.update(cx, |v, cx| v.more(id, cx));
    for _ in 0..4 {
        settle(cx);
    }
    let paths = ui.read(cx, |v| v.state.columns[1].entries.iter().map(|e| text(&e.path)).collect::<Vec<_>>());
    assert!(!paths.contains(&r"D:\0000.txt".into()));
    assert_eq!(ui.selected(cx, 1), [r"D:\0400.txt"]);
    // The page the pick is on is read again, not the one loaded past it, which loads as the list nears it.
    assert_eq!(paths.len(), PAGE);
    assert_eq!(ui.read(cx, |v| v.state.columns[1].total), 1210);
    // A pick further down keeps every page up to it.
    ui.view.update(cx, |v, cx| {
        v.more(id, cx);
    });
    settle(cx);
    ui.view.update(cx, |v, _| {
        let c = &mut v.state.columns[1];
        c.selected = vec![r"D:\0700.txt".into()];
        c.focused = c.selected.first().cloned();
    });
    fake.changed.store(true, Ordering::Relaxed);
    for _ in 0..4 {
        settle(cx);
    }
    assert_eq!(ui.read(cx, |v| v.state.columns[1].entries.len()), PAGE * 2);
    assert_eq!(ui.selected(cx, 1), [r"D:\0700.txt"]);
}
