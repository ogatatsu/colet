use super::*;
use colet_core::paths::ancestors;

fn entry(path: &str, directory: bool) -> Entry {
    let chain = ancestors(Path::new(path));
    Entry {
        path: path.into(),
        name: path.into(),
        parent: chain.len().checked_sub(2).map(|i| shown(&chain[i])).unwrap_or_default(),
        directory,
        size: None,
        modified: None,
        hidden: Hiding::Shown,
        marks: Marks::default(),
    }
}

fn folder(path: &str) -> Entry {
    entry(path, true)
}

fn file(path: &str) -> Entry {
    entry(path, false)
}

fn with_entries(s: &mut State, index: usize, entries: Vec<Entry>) {
    s.columns[index].entries = entries.into_iter().map(Arc::new).collect();
}

fn paths(s: &State) -> Vec<&str> {
    s.columns.iter().map(|c| c.path.to_str().unwrap()).collect()
}

/// Each folder column after home: its path, selection and highlight.
fn chain(s: &State) -> Vec<(PathBuf, Vec<PathBuf>, Option<PathBuf>)> {
    s.columns[1..].iter().map(|c| (c.path.clone(), c.selected.clone(), c.highlight.clone())).collect()
}

fn link(path: &str, selected: &[&str], highlight: Option<&Path>) -> (PathBuf, Vec<PathBuf>, Option<PathBuf>) {
    (path.into(), selected.iter().map(PathBuf::from).collect(), highlight.map(Into::into))
}

fn all(paths: &[&str]) -> Vec<PathBuf> {
    paths.iter().map(PathBuf::from).collect()
}

fn one(path: impl AsRef<Path>) -> Vec<PathBuf> {
    vec![path.as_ref().to_owned()]
}

#[test]
fn removes_vanished_selection_and_focus_after_a_refresh() {
    for kind in [Kind::Search, Kind::Home, Kind::Folder] {
        let mut s = State::new();
        let (kept, gone) = (file(r"D:\kept"), file(r"D:\gone"));
        let mut c = s.column(kind, Path::new(""), None);
        c.entries = vec![Arc::new(kept.clone()), Arc::new(gone.clone())];
        c.selected = vec![kept.path.clone(), gone.path.clone()];
        c.focused = Some(gone.path.clone());
        c.request = 2;
        let id = c.id;
        s.columns = vec![c];
        s.page(id, 2, vec![kept.clone()], 1, 0, &HashSet::new());
        assert_eq!(s.columns[0].selected, std::slice::from_ref(&kept.path));
        assert_eq!(s.columns[0].focused, None);
        s.page(id, 2, vec![], 0, 0, &HashSet::new());
        assert!(s.columns[0].selected.is_empty());
    }
}

#[test]
fn a_refresh_with_thousands_picked_hands_what_colet_sent_away_to_the_next_row() {
    let entries: Vec<Entry> = (0..20_000).map(|i| file(&format!(r"D:\work\{i:05}"))).collect();
    let mut s = State::new();
    let mut c = s.column(Kind::Folder, Path::new(r"D:\work"), None);
    c.entries = entries.iter().cloned().map(Arc::new).collect();
    c.selected = entries[..10_000].iter().map(|e| e.path.clone()).collect();
    c.request = 2;
    let id = c.id;
    s.columns = vec![c];
    // Pair by pair, these would be 200 million comparisons on the UI thread.
    let departed: HashSet<OsString> = entries[..10_000].iter().map(|e| path_key(&e.path)).collect();
    s.page(id, 2, entries[10_000..].to_vec(), 10_000, 0, &departed);
    assert_eq!(s.columns[0].selected, one(&entries[10_000].path));
}

#[test]
fn keeps_selections_on_appended_pages_but_drops_unloaded_selections_on_refresh() {
    let entries: Vec<Entry> = (0..501).map(|i| file(&format!(r"D:\file{i}"))).collect();
    let (first, last) = (entries[0].path.clone(), entries[500].path.clone());
    let mut s = State::new();
    s.set_query("file");
    let id = s.columns[1].id;
    s.load(id, 2, false);
    s.page(id, 2, entries[..500].to_vec(), 501, 0, &HashSet::new());
    s.columns[1].selected = one(&first);
    s.columns[1].focused = Some(first.clone());
    s.page(id, 2, entries[500..].to_vec(), 501, 500, &HashSet::new());
    assert_eq!(s.columns[1].entries.len(), 501);
    assert_eq!(s.columns[1].selected, one(&first));
    assert_eq!(s.columns[1].focused.as_deref(), Some(first.as_path()));
    s.select(1, vec![first.clone(), last.clone()]);
    // A stale listing changes nothing.
    s.page(id, 1, entries[..500].to_vec(), 501, 0, &HashSet::new());
    assert_eq!(s.columns[1].selected, [first.clone(), last.clone()]);
    s.page(id, 2, entries[..500].to_vec(), 501, 0, &HashSet::new());
    assert_eq!(s.columns[1].selected, one(&first));
    assert_eq!(s.columns[1].focused, None);
}

#[test]
fn discards_stale_and_removed_column_responses() {
    let mut s = State::new();
    let home = s.columns[0].id;
    s.load(home, 2, false);
    s.page(home, 1, vec![folder("old")], 1, 0, &HashSet::new());
    assert!(s.columns[0].entries.is_empty());
    s.set_query("old");
    let search = s.columns[1].id;
    s.load(search, 3, false);
    s.set_query("new");
    s.error(search, 3, "old".into());
    assert!(s.columns[1].error.is_none());
}

#[test]
fn replaces_descendants_and_opens_a_search_file_parent_exactly_once() {
    let target = file(r"D:\parent\file");
    let mut s = State::new();
    s.set_query("test");
    with_entries(&mut s, 1, vec![target.clone(), folder(r"D:\other")]);
    s.select(1, one(&target.path));
    assert_eq!(paths(&s), ["", "", r"D:\", r"D:\parent"]);
    assert_eq!(s.columns[3].highlight.as_deref(), Some(target.path.as_path()));
    with_entries(&mut s, 3, vec![target.clone()]);
    s.select(3, one(&target.path));
    assert_eq!(s.columns.len(), 4);
    assert_eq!(s.columns[3].highlight, None);
    s.select(1, one(r"D:\other"));
    assert_eq!(paths(&s), ["", "", r"D:\", r"D:\other"]);
}

#[test]
fn reveals_a_search_file_under_all_of_its_ancestors() {
    let target = file(r"D:\a\b\x.txt");
    let mut s = State::new();
    s.set_query("x");
    with_entries(&mut s, 1, vec![target.clone()]);
    s.select(1, one(&target.path));
    assert_eq!(
        chain(&s)[1..],
        [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], None), link(r"D:\a\b", &[], Some(&target.path))]
    );
    assert_eq!(s.active, 1);
}

#[test]
fn reveals_a_search_folder_and_opens_its_contents() {
    let target = folder(r"D:\a\b");
    let mut s = State::new();
    s.set_query("b");
    with_entries(&mut s, 1, vec![target.clone()]);
    s.select(1, one(&target.path));
    assert_eq!(
        chain(&s)[1..],
        [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], Some(Path::new(r"D:\a\b"))), link(r"D:\a\b", &[], None)]
    );
    // Picking in its parent takes the pointer away, as for a file.
    with_entries(&mut s, 3, vec![target.clone(), folder(r"D:\a\c")]);
    s.select(3, one(r"D:\a\c"));
    assert_eq!(chain(&s)[2..], [link(r"D:\a", &[r"D:\a\c"], None), link(r"D:\a\c", &[], None)]);
}

#[test]
fn keeps_shared_ancestor_columns_when_moving_between_search_results() {
    let (a, b) = (file(r"D:\a\b\one.txt"), file(r"D:\a\c\two.txt"));
    let mut s = State::new();
    s.set_query("t");
    with_entries(&mut s, 1, vec![a.clone(), b.clone()]);
    s.select(1, one(&a.path));
    let ids: Vec<_> = s.columns.iter().map(|c| c.id).collect();
    let past = s.past.len();
    s.select(1, one(&a.path));
    assert_eq!(s.columns.iter().map(|c| c.id).collect::<Vec<_>>(), ids);
    assert_eq!(s.past.len(), past);
    s.select(1, one(&b.path));
    assert_eq!(s.columns[..4].iter().map(|c| c.id).collect::<Vec<_>>(), ids[..4]);
    assert_eq!(chain(&s)[1..], [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\c"], None), link(r"D:\a\c", &[], Some(&b.path))]);
    s.back();
    assert_eq!(chain(&s)[1..], [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[r"D:\a\b"], None), link(r"D:\a\b", &[], Some(&a.path))]);
}

#[test]
fn drops_a_deeper_results_columns_when_revealing_its_ancestor_folder() {
    let (deep, top) = (file(r"D:\a\b\x.txt"), folder(r"D:\a"));
    let mut s = State::new();
    s.set_query("x");
    with_entries(&mut s, 1, vec![deep.clone(), top.clone()]);
    s.select(1, one(&deep.path));
    s.select(1, one(&top.path));
    assert_eq!(chain(&s)[1..], [link(r"D:\", &[r"D:\a"], Some(Path::new(r"D:\a"))), link(r"D:\a", &[], None)]);
}

#[test]
fn folds_away_the_folders_above_a_result_until_shown() {
    let (a, c) = (file(r"D:\a\b\one.txt"), folder(r"D:\a\c"));
    let mut s = State::new();
    s.set_query("o");
    with_entries(&mut s, 1, vec![a.clone(), c.clone()]);
    s.select(1, one(&a.path));
    // D: and a, above b.
    assert_eq!(s.folded(), Some(Folded { from: 2, to: 4, open: false }));
    assert_eq!(s.folded_away(), 2);
    // Taking the keys to a folded column brings the folders out, for good.
    s.set_active(3);
    assert_eq!(s.folded_away(), 0);
    s.set_active(4);
    assert_eq!(s.folded_away(), 0);
    let past = s.past.len();
    s.select(1, one(&c.path));
    assert_eq!(s.past.len(), past + 1);
    // A folder opens on its contents alone; picked afresh, folded again.
    assert_eq!(s.folded(), Some(Folded { from: 2, to: 4, open: false }));
    assert_eq!(s.columns[4].path, Path::new(r"D:\a\c"));
    // Gone back to, a place is folded as it was left; going forward too.
    s.back();
    assert_eq!(s.columns[4].path, Path::new(r"D:\a\b"));
    assert!(s.folded().unwrap().open);
    s.forward();
    assert!(!s.folded().unwrap().open);
    // Picking in a folder above it, once out, closes the anchor and the fold.
    s.unfold();
    with_entries(&mut s, 2, vec![folder(r"D:\a"), folder(r"D:\z")]);
    s.select(2, one(r"D:\z"));
    assert_eq!(s.folded(), None);
    // Nothing above a file at a drive's top: nothing to fold.
    let top = file(r"D:\top.txt");
    with_entries(&mut s, 1, vec![top.clone()]);
    s.select(1, one(&top.path));
    assert_eq!(s.folded(), None);
}

#[test]
fn reveals_a_drive_from_home_on_its_own() {
    let drive = Entry::folder(r"D:\", "データ (D:)");
    let mut s = State::new();
    with_entries(&mut s, 0, vec![drive.clone()]);
    s.select(0, one(&drive.path));
    assert_eq!(chain(&s), [link(r"D:\", &[], None)]);
}

#[test]
fn closes_descendants_without_history_on_multiple_or_empty_selection() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\a"), folder(r"D:\b")]);
    s.select(0, one(r"D:\a"));
    assert_eq!(s.columns.len(), 3);
    let past = s.past.len();
    s.select(0, vec![r"D:\a".into(), r"D:\b".into()]);
    assert_eq!(s.columns.len(), 1);
    assert_eq!(s.columns[0].selected.len(), 2);
    assert_eq!(s.columns[0].focused.as_deref(), Some(Path::new(r"D:\b")));
    assert_eq!(s.past.len(), past);
    s.select(0, vec![]);
    assert!(s.columns[0].selected.is_empty());
    assert_eq!(s.past.len(), past);
}

#[test]
fn restores_selections_when_going_back_and_forward() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\a"), folder(r"D:\b")]);
    s.select(0, one(r"D:\a"));
    s.select(0, one(r"D:\b"));
    s.back();
    assert_eq!(s.columns[0].selected, one(r"D:\a"));
    assert_eq!(s.columns[0].focused.as_deref(), Some(Path::new(r"D:\a")));
    assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\a"));
    s.forward();
    assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\b"));
}

#[test]
fn does_not_add_each_query_keystroke_to_history() {
    let mut s = State::new();
    for query in ["a", "ab", "abc"] {
        s.set_query(query);
    }
    assert!(s.past.is_empty());
    with_entries(&mut s, 1, vec![folder(r"D:\a")]);
    s.select(1, one(r"D:\a"));
    s.back();
    assert_eq!(s.query, "abc");
    assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
    s.forward();
    assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\a"));
}

#[test]
fn closes_a_vanished_folder_and_what_it_opened() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\a")]);
    s.select(0, one(r"D:\a"));
    let parent = s.columns[1].id;
    s.load(parent, 4, false);
    s.page(parent, 4, vec![folder(r"D:\a\b")], 1, 0, &HashSet::new());
    s.select(1, one(r"D:\a\b"));
    s.set_active(2);
    s.page(parent, 4, vec![], 0, 0, &HashSet::new());
    assert_eq!(s.columns.len(), 2);
    assert_eq!(s.active, 1);
    assert!(s.notice.is_some());
}

#[test]
fn keeps_home_beside_the_search_letting_go_of_its_pick() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\")]);
    s.select(0, one(r"D:\"));
    s.set_query("x");
    assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
    assert!(s.columns[0].selected.is_empty());
    assert_eq!(s.active, 1);
}

#[test]
fn opens_a_deep_search_file_on_its_own_folder_and_right_jumps_to_it() {
    let deep = file(r"D:\a\b\zeta.txt");
    let mut s = State::new();
    s.set_query("zeta");
    with_entries(&mut s, 1, vec![deep.clone()]);
    s.select(1, one(&deep.path));
    with_entries(&mut s, 4, vec![deep.clone()]);
    // → passes over the folded folders to the one that shows it, and picks it there.
    let next = s.neighbour(1, true).unwrap();
    assert_eq!(next, 4);
    s.set_active(next);
    s.enter();
    assert_eq!(s.columns[4].selected, one(&deep.path));
    assert_eq!(s.folded_away(), 2);
    // ← passes over them, back to the results.
    assert_eq!(s.neighbour(4, false), Some(1));
    assert_eq!(s.neighbour(0, false), None);
    assert_eq!(s.neighbour(4, true), None);
}

#[test]
fn right_from_a_folder_result_lands_in_its_contents_folded_above_or_not() {
    let deep = folder(r"D:\a\b");
    let mut s = State::new();
    s.set_query("b");
    with_entries(&mut s, 1, vec![deep.clone()]);
    s.select(1, one(&deep.path));
    assert_eq!(paths(&s), ["", "", r"D:\", r"D:\a", r"D:\a\b"]);
    assert_eq!(s.neighbour(1, true), Some(4));
    // Brought out, its parent pointing at it is passed over all the same.
    s.unfold();
    assert_eq!(s.neighbour(1, true), Some(4));
    // ← steps back one column at a time once they are out.
    assert_eq!(s.neighbour(4, false), Some(3));
}

#[test]
fn right_goes_back_to_the_folder_column_left_until_another_pick() {
    let (deep, other) = (file(r"D:\a\b\zeta.txt"), file(r"D:\a\b\eta.txt"));
    let mut s = State::new();
    s.set_query("ta");
    with_entries(&mut s, 1, vec![deep.clone(), other.clone()]);
    s.select(1, one(&deep.path));
    s.unfold();
    s.set_active(4);
    // ← back up the folders brought out, then to the results.
    for at in [3, 2, 1] {
        let next = s.neighbour(s.active, false).unwrap();
        assert_eq!(next, at);
        s.set_active(next);
    }
    assert_eq!(s.neighbour(1, true), Some(2));
    // Home beside the results leads to them.
    assert_eq!(s.neighbour(0, true), Some(1));
    // Another result shows another file, in the same folders: → lands where it is shown.
    s.step(1);
    assert_eq!(s.columns[1].selected, one(&other.path));
    assert_eq!(s.neighbour(1, true), Some(4));
}

#[test]
fn right_lands_where_the_pick_is_shown_with_the_keys_moved_apart_from_it() {
    let (deep, other) = (file(r"D:\a\b\zeta.txt"), file(r"E:\eta.txt"));
    let mut s = State::new();
    s.set_query("ta");
    with_entries(&mut s, 1, vec![deep.clone(), other.clone()]);
    s.select(1, one(&deep.path));
    with_entries(&mut s, 4, vec![deep.clone()]);
    s.active = 1;
    s.focus_step(1);
    assert_eq!(s.columns[1].focused, Some(other.path.clone()));
    assert_eq!(s.neighbour(1, true), Some(4));
}

#[test]
fn steps_through_rows_picking_each() {
    let mut s = State::new();
    s.set_query("a");
    with_entries(&mut s, 1, vec![folder(r"D:\a"), folder(r"D:\b"), file(r"D:\c")]);
    s.step(1);
    assert_eq!(s.columns[1].selected, one(r"D:\a"));
    s.step(5);
    assert_eq!(s.columns[1].selected, one(r"D:\c"));
    s.step(-1);
    assert_eq!(s.columns[1].selected, one(r"D:\b"));
    assert_eq!(s.columns.last().unwrap().path, Path::new(r"D:\b"));
}

#[test]
fn enters_home_without_picking_and_results_picking_the_first() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"C:\"), folder(r"D:\")]);
    s.enter();
    assert_eq!(s.columns[0].focused.as_deref(), Some(Path::new(r"C:\")));
    assert!(s.columns[0].selected.is_empty());
    s.set_query("x");
    with_entries(&mut s, 1, vec![file(r"D:\x.txt")]);
    s.enter();
    assert_eq!(s.columns[1].selected, one(r"D:\x.txt"));
}

#[test]
fn runs_a_saved_search_picked_in_home_keeping_homes_pick() {
    let mut s = State::new();
    let pdf = HomeKey::search("ext:pdf", None, None, Order::default()).key();
    with_entries(&mut s, 0, vec![Entry::folder(&pdf, "PDF"), folder(r"D:\")]);
    s.select(0, one(&pdf));
    assert_eq!(s.query, "ext:pdf");
    assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
    assert_eq!(s.columns[0].selected, one(&pdf));
    assert_eq!(s.active, 0);
    // Picked again, nothing changes; a place picked leaves the search, one step back.
    let search = s.columns[1].id;
    s.select(0, one(&pdf));
    assert_eq!(s.columns[1].id, search);
    s.select(0, one(r"D:\"));
    assert_eq!(s.query, "");
    s.back();
    assert_eq!(s.query, "ext:pdf");
}

#[test]
fn makes_the_search_afresh_on_reset_and_leaves_home_be() {
    let mut s = State::new();
    s.set_query("x");
    s.columns[1].selected = one(r"D:\x");
    let (home, search) = (s.columns[0].id, s.columns[1].id);
    s.reset_search();
    assert_eq!(s.columns[0].id, home);
    assert_ne!(s.columns[1].id, search);
    assert_eq!(s.columns[1].selected, one(r"D:\x"));
}

/// D:\ listed in its column, beside home.
fn drive(entries: Vec<Entry>) -> State {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\")]);
    s.select(0, one(r"D:\"));
    with_entries(&mut s, 1, entries);
    s.set_active(1);
    s
}

#[test]
fn picks_ranges_and_single_rows_by_ctrl_and_shift() {
    let mut s = drive(vec![file(r"D:\a"), file(r"D:\b"), file(r"D:\c"), file(r"D:\d")]);
    s.select(1, one(r"D:\b"));
    s.extend(1, Path::new(r"D:\d"));
    assert_eq!(s.columns[1].selected, all(&[r"D:\b", r"D:\c", r"D:\d"]));
    s.extend(1, Path::new(r"D:\a"));
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\b"]));
    s.toggle(1, Path::new(r"D:\d"));
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\b", r"D:\d"]));
    s.toggle(1, Path::new(r"D:\a"));
    assert_eq!(s.columns[1].selected, all(&[r"D:\b", r"D:\d"]));
    s.extend_step(-1, false);
    assert_eq!(s.columns[1].selected, all(&[r"D:\a"]));
    s.select_all(1);
    assert_eq!(s.columns[1].selected.len(), 4);
}

#[test]
fn picks_rows_apart_from_the_keys_alone() {
    let entries = ["a", "b", "c", "d", "e", "f"].map(|n| file(&format!(r"D:\{n}")));
    let mut s = drive(entries.to_vec());
    s.select(1, one(r"D:\a"));
    // Ctrl+↓ moves the keys and nothing else; Ctrl+Space picks where they are.
    s.focus_step(2);
    assert_eq!(s.columns[1].selected, one(r"D:\a"));
    assert_eq!(s.columns[1].focused.as_deref(), Some(Path::new(r"D:\c")));
    s.toggle_focused();
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c"]));
    s.focus_step(isize::MAX / 2);
    assert_eq!(s.columns[1].focused.as_deref(), Some(Path::new(r"D:\f")));
    s.focus_step(-1);
    s.toggle_focused();
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\e"]));
    // Again, it lets go.
    s.toggle_focused();
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c"]));
}

#[test]
fn adds_a_range_drawn_back_from_the_anchor_to_what_was_picked() {
    let entries = ["a", "b", "c", "d", "e", "f"].map(|n| file(&format!(r"D:\{n}")));
    let mut s = drive(entries.to_vec());
    s.select(1, one(r"D:\a"));
    s.focus_step(2);
    s.toggle_focused();
    // From c, Ctrl+Shift+↓ twice adds d and e beside a.
    s.extend_step(1, true);
    s.extend_step(1, true);
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\d", r"D:\e"]));
    // Drawn back, e goes; a and c, picked before, stay.
    s.extend_step(-1, true);
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\d"]));
    // Moving the keys alone keeps the anchor, and the next range starts from c again.
    s.focus_step(2);
    s.extend_step(1, true);
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\d", r"D:\e", r"D:\f"]));
    s.extend_step(-3, true);
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c"]));
    // Upward from c takes b in, and a, already picked, once only.
    s.extend_step(-1, true);
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\b"]));
    s.add_range(1, Path::new(r"D:\a"));
    assert_eq!(s.columns[1].selected, all(&[r"D:\a", r"D:\c", r"D:\b"]));
    // Shift alone puts the range in the selection's place.
    s.extend_step(1, false);
    assert_eq!(s.columns[1].selected, all(&[r"D:\b", r"D:\c"]));
}

#[test]
fn carries_a_renamed_folders_selection_and_open_columns_to_its_new_name() {
    let mut s = drive(vec![folder(r"D:\work"), file(r"D:\notes.txt")]);
    s.select(1, one(r"D:\work"));
    with_entries(&mut s, 2, vec![file(r"D:\work\a.txt")]);
    s.select(2, one(r"D:\work\a.txt"));
    s.set_active(1);
    s.renamed(Path::new(r"D:\work"), Path::new(r"D:\projects"));
    assert_eq!(s.columns[1].selected, one(r"D:\projects"));
    assert_eq!(s.columns[1].entries[0].name, "projects");
    assert_eq!(s.columns[2].path, Path::new(r"D:\projects"));
    assert_eq!(s.columns[2].selected, one(r"D:\projects\a.txt"));
    assert_eq!(s.columns[2].entries[0].parent, r"D:\projects");
    assert_eq!(s.past.last().unwrap().folders.iter().map(|f| f.0.to_str().unwrap()).collect::<Vec<_>>(), [r"D:\", r"D:\projects"]);
    // The listing finds it under its new name, so nothing closes.
    let parent = s.columns[1].id;
    s.load(parent, 3, true);
    s.page(parent, 3, vec![folder(r"D:\projects"), file(r"D:\notes.txt")], 2, 0, &HashSet::new());
    assert_eq!(s.columns.len(), 3);
    assert!(s.notice.is_none());
}

#[test]
fn picks_a_new_folder_and_opens_it() {
    let mut s = drive(vec![file(r"D:\notes.txt")]);
    s.created(Path::new(r"D:\"), Path::new(r"D:\新しいフォルダー (2)"));
    let made = s.columns[1].entries.last().unwrap();
    assert_eq!((made.name.as_str(), made.directory), ("新しいフォルダー (2)", true));
    assert_eq!(s.columns[1].selected, one(r"D:\新しいフォルダー (2)"));
    assert_eq!(s.columns[2].path, Path::new(r"D:\新しいフォルダー (2)"));
    assert_eq!(s.active, 1);
}

#[test]
fn picks_what_was_pasted_once_listed() {
    let mut s = drive(vec![file(r"D:\a")]);
    let id = s.columns[1].id;
    s.pasted(Path::new(r"D:\"), &[r"D:\a - コピー".into(), r"D:\b".into()]);
    assert!(s.columns[1].selected.is_empty());
    s.load(id, 5, true);
    s.page(id, 5, vec![file(r"D:\a"), file(r"D:\a - コピー"), file(r"D:\b")], 3, 0, &HashSet::new());
    assert_eq!(s.columns[1].selected, all(&[r"D:\a - コピー", r"D:\b"]));
    assert!(s.columns[1].pick.is_none());
}

#[test]
fn picks_the_item_that_takes_a_deleted_ones_place_saying_nothing() {
    let mut s = drive(vec![folder(r"D:\a"), folder(r"D:\b"), file(r"D:\c")]);
    s.select(1, one(r"D:\b"));
    s.set_active(1);
    let id = s.columns[1].id;
    s.load(id, 6, true);
    let departed = HashSet::from([path_key(Path::new(r"D:\b"))]);
    s.page(id, 6, vec![folder(r"D:\a"), file(r"D:\c")], 2, 0, &departed);
    assert_eq!(s.columns[1].selected, one(r"D:\c"));
    assert!(s.notice.is_none());
    // Gone some other way, its column closes with a word, and nothing is picked unasked.
    let mut s = drive(vec![folder(r"D:\a"), folder(r"D:\b")]);
    s.select(1, one(r"D:\b"));
    s.set_active(1);
    let id = s.columns[1].id;
    s.load(id, 7, true);
    s.page(id, 7, vec![folder(r"D:\a")], 1, 0, &HashSet::new());
    assert!(s.columns[1].selected.is_empty());
    assert_eq!(s.columns.len(), 2);
    assert!(s.notice.is_some());
}

#[test]
fn reveals_an_item_where_it_lives_leaving_the_search() {
    let mut s = State::new();
    s.set_query("x");
    s.reveal(Path::new(r"D:\a\b.txt"), false);
    assert_eq!(s.query, "");
    assert_eq!(chain(&s), [link(r"D:\", &[r"D:\a"], None), link(r"D:\a", &[], Some(Path::new(r"D:\a\b.txt")))]);
    assert_eq!(s.active, 2);
    s.back();
    assert_eq!(s.query, "x");
}

#[test]
fn goes_to_a_folder_open_as_its_crumb_does_keeping_the_columns_below() {
    let mut s = State::new();
    s.reveal(Path::new(r"D:\a\b"), true);
    let before = chain(&s);
    s.go_to_path(Path::new(r"d:\A"), true);
    assert_eq!(chain(&s), before);
    assert_eq!(s.active, 2);
    assert_eq!(s.fold, None);
}

#[test]
fn opens_on_from_the_deepest_folder_to_what_lies_under_it_all_else_open() {
    let mut s = State::new();
    s.reveal(Path::new(r"D:\a"), true);
    s.go_to_path(Path::new(r"D:\a\b\c.txt"), false);
    assert_eq!(
        chain(&s),
        [
            link(r"D:\", &[r"D:\a"], Some(Path::new(r"D:\a"))),
            link(r"D:\a", &[r"D:\a\b"], None),
            link(r"D:\a\b", &[], Some(Path::new(r"D:\a\b\c.txt")))
        ]
    );
    assert_eq!((s.active, s.folded_away()), (3, 0));
    s.go_to_path(Path::new(r"D:\a\b\d"), true);
    assert_eq!(chain(&s)[2..], [link(r"D:\a\b", &[r"D:\a\b\d"], Some(Path::new(r"D:\a\b\d"))), link(r"D:\a\b\d", &[], None)]);
    assert_eq!((s.active, s.folded_away()), (4, 0));
    s.back();
    assert_eq!(s.columns.len(), 4);
}

#[test]
fn opens_anything_else_afresh_with_the_folders_above_folded_away() {
    let mut s = State::new();
    s.reveal(Path::new(r"D:\a\b"), true);
    s.go_to_path(Path::new(r"D:\a\x\y.txt"), false);
    assert_eq!(chain(&s).last(), Some(&link(r"D:\a\x", &[], Some(Path::new(r"D:\a\x\y.txt")))));
    assert_eq!((s.active, s.folded_away()), (3, 2));
    s.go_to_path(Path::new(r"E:\"), true);
    assert_eq!(chain(&s), [link(r"E:\", &[], None)]);
    assert_eq!((s.active, s.fold), (1, None));
    s.back();
    assert_eq!(s.folded_away(), 2);
}

fn names(s: &State, index: usize) -> Vec<&str> {
    s.columns[index].entries.iter().map(|e| e.path.to_str().unwrap()).collect()
}

#[test]
fn finds_every_place_a_name_holds_the_text_case_aside() {
    assert_eq!(found("ReadMe readme.TXT", "README"), [0..6, 7..13]);
    assert_eq!(found("資料メモ.txt", "メモ"), [6..12]);
    // A letter lowering into two is matched whole.
    assert_eq!(found("\u{130}x", "\u{69}\u{307}x"), [0..3]);
    assert!(found("abc", "").is_empty());
    assert!(found("abc", "abcd").is_empty());
}

#[test]
fn takes_the_words_apart_at_any_space_but_in_quotes() {
    assert_eq!(narrow_words(" rep\u{3000}2024  x"), ["rep", "2024", "x"]);
    assert_eq!(narrow_words("\"my doc\" a\"b c\"d"), ["my doc", "ab cd"]);
    // A quote left open is closed at the end.
    assert_eq!(narrow_words("x \"my do"), ["x", "my do"]);
    assert!(narrow_words(" \u{3000}\"\" ").is_empty());
    // Nothing else is read: a minus or a bar is a letter as any.
    assert_eq!(narrow_words("-a|b"), ["-a|b"]);
}

#[test]
fn holds_a_name_with_all_the_words_in_any_order_marked_as_one_where_they_meet() {
    assert_eq!(holds("2024_Report.txt", "rep 2024"), Some(vec![0..4, 5..8]));
    assert_eq!(holds("abcd", "ab bc"), Some(vec![0..3]));
    assert_eq!(holds("abcd", "ab cd"), Some(vec![0..4]));
    assert_eq!(holds("my doc.txt", "\"my doc\""), Some(vec![0..6]));
    assert_eq!(holds("2024_Report.txt", "rep 2025"), None);
    assert_eq!(holds("abc", "  "), Some(vec![]));
}

#[test]
fn narrows_a_folder_to_the_names_holding_the_text_keeping_what_it_hides_picked() {
    let mut s = drive(vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), folder(r"D:\gamma")]);
    s.select(1, one(r"D:\alpha"));
    assert_eq!(s.columns.len(), 3);
    s.set_narrowing(1, Some("TA"));
    assert_eq!(names(&s, 1), [r"D:\beta.txt"]);
    assert_eq!(marks(&s, 1, 0), [5..7]);
    // The folder it had open beside it is hidden: it stays picked, and its column open, but is not
    // what is acted on.
    assert_eq!(s.columns[1].selected, one(r"D:\alpha"));
    assert_eq!(s.columns.len(), 3);
    assert!(s.columns[1].shown_selected().is_empty());
    // Of several picked, what shows is acted on.
    s.select(1, vec![PathBuf::from(r"D:\alpha"), PathBuf::from(r"D:\beta.txt")]);
    s.set_narrowing(1, Some("t"));
    assert_eq!(s.columns[1].selected.len(), 2);
    assert_eq!(*s.columns[1].shown_selected(), one(r"D:\beta.txt"));
    // Ctrl+A picks only what shows.
    s.set_narrowing(1, Some("a"));
    s.select_all(1);
    assert_eq!(s.columns[1].selected.len(), 3);
    s.set_narrowing(1, Some("mm"));
    s.select_all(1);
    assert_eq!(s.columns[1].selected, one(r"D:\gamma"));
    // Lifted, it lists all again, unmarked.
    s.set_narrowing(1, None);
    assert_eq!(names(&s, 1).len(), 3);
    assert!((0..3).all(|row| marks(&s, 1, row).is_empty()));
    assert!(s.columns[1].narrow.is_none());
}

#[test]
fn narrows_home_keeping_its_pick_and_what_it_opened() {
    let mut s = drive(vec![folder(r"D:\alpha")]);
    with_entries(&mut s, 0, vec![folder(r"C:\"), folder(r"D:\")]);
    s.select(0, one(r"D:\"));
    s.select(1, one(r"D:\alpha"));
    assert_eq!(s.columns.len(), 3);
    s.set_narrowing(0, Some("c:"));
    assert_eq!(names(&s, 0), [r"C:\"]);
    assert_eq!(s.columns[0].selected, one(r"D:\"));
    assert_eq!(s.columns.len(), 3);
    // A listing that comes after is narrowed too.
    let id = s.columns[0].id;
    s.load(id, 9, true);
    s.page(id, 9, vec![folder(r"C:\"), folder(r"D:\"), folder(r"E:\")], 3, 0, &HashSet::new());
    assert_eq!(names(&s, 0), [r"C:\"]);
    s.set_narrowing(0, None);
    assert_eq!(names(&s, 0).len(), 3);
}

#[test]
fn narrows_each_listing_that_comes_after() {
    let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
    let id = s.columns[1].id;
    s.set_narrowing(1, Some("ta"));
    s.load(id, 3, true);
    s.page(id, 3, vec![file(r"D:\beta.txt"), file(r"D:\delta"), file(r"D:\gamma")], 3, 0, &HashSet::new());
    assert_eq!(names(&s, 1), [r"D:\beta.txt", r"D:\delta"]);
    assert_eq!(s.columns[1].total, 3);
    // Emptied, it narrows no more.
    s.set_narrowing(1, Some(""));
    assert_eq!(names(&s, 1).len(), 3);
    assert!(s.columns[1].narrow.is_none());
}

#[test]
fn narrows_the_results_as_everything_answers_keeping_what_it_hides_picked() {
    let results = vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), file(r"E:\alphabet.txt")];
    let mut s = State::new();
    s.set_query("a");
    let id = s.columns[1].id;
    s.load(id, 1, false);
    s.page(id, 1, results.clone(), 3, 0, &HashSet::new());
    s.select(1, one(r"D:\alpha"));
    let open = s.columns.len();
    assert!(open > 2);
    // The results show as they are until Everything answers, counted from all the search finds.
    s.set_narrowing(1, Some("bet"));
    assert_eq!(names(&s, 1).len(), 3);
    assert_eq!(s.columns[1].narrow.as_ref().map(|f| f.whole), Some(3));
    s.load(id, 2, true);
    s.counted(id, 2, 4);
    s.page(id, 2, results[1..].to_vec(), 2, 0, &HashSet::new());
    assert_eq!(names(&s, 1), [r"D:\beta.txt", r"E:\alphabet.txt"]);
    assert_eq!((s.columns[1].total, s.columns[1].narrow.as_ref().map(|f| f.whole)), (2, Some(4)));
    // The one picked, hidden, stays picked, the folder it opened open.
    assert_eq!(s.columns[1].selected, one(r"D:\alpha"));
    assert_eq!(s.columns.len(), open);
    // Of several picked, those hidden stay picked too, but only what shows is acted on; a rename, which
    // narrows the others again, leaves the results be.
    s.select(1, vec![PathBuf::from(r"D:\beta.txt"), PathBuf::from(r"E:\alphabet.txt")]);
    s.set_narrowing(1, Some("beta."));
    s.load(id, 3, true);
    s.page(id, 3, results[1..2].to_vec(), 1, 0, &HashSet::new());
    assert_eq!(s.columns[1].selected, [PathBuf::from(r"D:\beta.txt"), PathBuf::from(r"E:\alphabet.txt")]);
    assert_eq!(*s.columns[1].shown_selected(), one(r"D:\beta.txt"));
    s.renamed(Path::new(r"D:\gamma"), Path::new(r"D:\delta"));
    assert_eq!(names(&s, 1), [r"D:\beta.txt"]);
    // Emptied, it narrows no more; another search starts with none.
    s.set_narrowing(1, Some(""));
    assert!(s.columns[1].narrow.is_none());
    s.set_narrowing(1, Some("x"));
    s.set_query("b");
    assert!(s.columns[1].narrow.is_none());
}

#[test]
fn keeps_the_result_a_filter_takes_the_keys_off_to_go_back_to_once_it_shows() {
    let results = vec![folder(r"D:\alpha"), file(r"D:\beta.txt"), file(r"E:\alphabet.txt")];
    let mut s = State::new();
    s.set_query("a");
    let id = s.columns[1].id;
    s.load(id, 1, false);
    s.page(id, 1, results.clone(), 3, 0, &HashSet::new());
    s.select(1, one(r"D:\alpha"));
    let answer = |s: &mut State, request: u64, entries: &[Entry]| {
        s.load(id, request, true);
        s.page(id, request, entries.to_vec(), entries.len(), 0, &HashSet::new());
    };
    s.set_narrowing(1, Some("bet"));
    answer(&mut s, 2, &results[1..]);
    assert_eq!(s.columns[1].focused, None);
    // Shown again, loosened or lifted, the keys are back on it.
    s.set_narrowing(1, Some("al"));
    answer(&mut s, 3, &[results[0].clone(), results[2].clone()]);
    assert_eq!(s.columns[1].focused, Some(PathBuf::from(r"D:\alpha")));
    s.set_narrowing(1, Some("bet"));
    answer(&mut s, 4, &results[1..]);
    s.set_narrowing(1, None);
    answer(&mut s, 5, &results);
    assert_eq!(s.columns[1].focused, Some(PathBuf::from(r"D:\alpha")));
    // Gone to another meanwhile, they stay there.
    s.set_narrowing(1, Some("bet"));
    answer(&mut s, 6, &results[1..]);
    s.select(1, one(r"D:\beta.txt"));
    s.set_narrowing(1, None);
    answer(&mut s, 7, &results);
    assert_eq!(s.columns[1].focused, Some(PathBuf::from(r"D:\beta.txt")));
}

#[test]
fn lifts_the_filter_for_what_is_made_or_renamed_here_that_it_would_hide() {
    let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
    s.set_narrowing(1, Some("ta"));
    s.created(Path::new(r"D:\"), Path::new(r"D:\data"));
    assert_eq!(names(&s, 1), [r"D:\beta.txt", r"D:\data"]);
    assert_eq!(s.columns[1].selected, one(r"D:\data"));
    s.created(Path::new(r"D:\"), Path::new(r"D:\new"));
    assert!(s.columns[1].narrow.is_none());
    assert_eq!(s.columns[1].selected, one(r"D:\new"));

    let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
    s.set_narrowing(1, Some("ta"));
    s.renamed(Path::new(r"D:\beta.txt"), Path::new(r"D:\beta2.txt"));
    assert_eq!(marks(&s, 1, 0), [2..4]);
    s.renamed(Path::new(r"D:\beta2.txt"), Path::new(r"D:\b.txt"));
    assert!(s.columns[1].narrow.is_none());

    // Pasted where it hides them, already listed or listed after.
    let mut s = drive(vec![file(r"D:\beta.txt"), file(r"D:\gamma")]);
    s.set_narrowing(1, Some("ta"));
    s.pasted(Path::new(r"D:\"), &[r"D:\gamma".into()]);
    assert!(s.columns[1].narrow.is_none());
    assert_eq!(s.columns[1].selected, one(r"D:\gamma"));
    let mut s = drive(vec![file(r"D:\beta.txt")]);
    s.set_narrowing(1, Some("ta"));
    s.pasted(Path::new(r"D:\"), &[r"D:\x".into()]);
    let id = s.columns[1].id;
    s.load(id, 4, true);
    s.page(id, 4, vec![file(r"D:\beta.txt"), file(r"D:\x")], 2, 0, &HashSet::new());
    assert!(s.columns[1].narrow.is_none());
    assert_eq!(s.columns[1].selected, one(r"D:\x"));
}

#[test]
fn leaves_the_search_for_a_place_picked_in_home_one_step_back() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\")]);
    s.set_query("x");
    s.select(0, one(r"D:\"));
    assert_eq!(s.query, "");
    assert_eq!(paths(&s), ["", r"D:\"]);
    s.back();
    assert_eq!(s.query, "x");
    assert_eq!(s.columns.iter().map(|c| c.kind).collect::<Vec<_>>(), [Kind::Home, Kind::Search]);
}

fn kinds(s: &State) -> Vec<Kind> {
    s.columns.iter().map(|c| c.kind).collect()
}

#[test]
fn searches_within_a_filter_only_what_is_typed_and_steps_back_to_the_filter_before() {
    let mut s = State::new();
    // A filter with nothing typed is no search, and no step.
    s.set_search("", Some("画像".into()));
    assert_eq!(kinds(&s), [Kind::Home]);
    assert_eq!(s.search_filter, None);
    assert!(!s.can_go_back());
    // Typing searches within it, and is no step of its own.
    s.set_search("cat", Some("画像".into()));
    assert_eq!(kinds(&s), [Kind::Home, Kind::Search]);
    assert_eq!(s.search_filter.as_deref(), Some("画像"));
    assert!(!s.can_go_back());
    s.set_search("cat", Some("動画".into()));
    s.set_search("cat", None);
    s.back();
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("動画")));
    s.back();
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("画像")));
    s.forward();
    assert_eq!(s.search_filter.as_deref(), Some("動画"));
    // Nothing typed, the results go, though a filter is still chosen.
    s.set_search("", Some("動画".into()));
    assert_eq!(kinds(&s), [Kind::Home]);
    assert_eq!(s.search_filter, None);
    s.back();
    assert_eq!(kinds(&s), [Kind::Home, Kind::Search]);
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("動画")));
}

#[test]
fn runs_a_saved_search_within_its_filter() {
    let mut s = State::new();
    let images = HomeKey::search("cat", Some("画像"), None, Order::default()).key();
    let plain = HomeKey::search("cat", None, None, Order::default()).key();
    assert_ne!(images, plain);
    assert_eq!(HomeKey::of(&images), HomeKey::search("cat", Some("画像"), None, Order::default()));
    assert_eq!(HomeKey::of(&plain), HomeKey::search("cat", None, None, Order::default()));
    with_entries(&mut s, 0, vec![Entry::folder(&images, "cat"), Entry::folder(&plain, "cat"), folder(r"D:\")]);
    s.select(0, one(&images));
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", Some("画像")));
    s.select(0, one(&plain));
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", None));
    // A place picked leaves the search and its filter.
    s.select(0, one(&images));
    s.select(0, one(r"D:\"));
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("", None));
    s.back();
    assert_eq!(s.search_filter.as_deref(), Some("画像"));
}

#[test]
fn keys_a_favorite_and_a_saved_search_by_what_they_are_narrowed_to() {
    let narrowed = HomeKey::favorite(Path::new(r"D:\Work"), Some("a b")).key();
    assert_eq!(HomeKey::favorite(Path::new(r"D:\Work"), None).key(), PathBuf::from(r"D:\Work"));
    assert_eq!(HomeKey::of(&narrowed), HomeKey::favorite(Path::new(r"D:\Work"), Some("a b")));
    assert_eq!(HomeKey::on_disk(&narrowed), Path::new(r"D:\Work"));
    assert_eq!(HomeKey::of(Path::new(r"D:\Work")), HomeKey::favorite(Path::new(r"D:\Work"), None));
    let search = HomeKey::search("cat", None, Some("a"), Order::default()).key();
    assert_eq!(HomeKey::of(&search), HomeKey::search("cat", None, Some("a"), Order::default()));
    assert_eq!(
        HomeKey::of(&HomeKey::search("cat", Some("画像"), Some("a"), Order::default()).key()),
        HomeKey::search("cat", Some("画像"), Some("a"), Order::default())
    );
    assert_eq!(HomeKey::on_disk(&search), search.as_path());
}

#[test]
fn keeps_a_narrowing_as_it_would_be_typed_again() {
    assert_eq!(narrowing("  a \u{3000} b  "), Some("a b".into()));
    assert_eq!(narrowing("\"x  y\"  z"), Some("\"x  y\" z".into()));
    assert_eq!(narrowing("A"), Some("A".into()));
    assert_eq!(narrowing("   "), None);
    assert_eq!(narrowing(""), None);
}

#[test]
fn opens_a_favorite_from_home_narrowed_as_it_was_kept() {
    let mut s = State::new();
    let narrowed = HomeKey::favorite(Path::new(r"D:\a"), Some("x")).key();
    with_entries(&mut s, 0, vec![Entry::folder(&narrowed, "a"), folder(r"D:\a")]);
    s.select(0, one(&narrowed));
    let last = s.columns.last().unwrap();
    assert_eq!((last.path.as_path(), last.narrowing().as_deref()), (Path::new(r"D:\a"), Some("x")));
    // The same folder from its favorite not narrowed lists it all.
    s.select(0, one(r"D:\a"));
    let last = s.columns.last().unwrap();
    assert_eq!((last.path.as_path(), last.narrow.is_none()), (Path::new(r"D:\a"), true));
    // A saved search runs narrowed as it was kept.
    let search = HomeKey::search("cat", None, Some("y"), Order::default()).key();
    with_entries(&mut s, 0, vec![Entry { directory: false, ..Entry::folder(&search, "cat") }]);
    s.select(0, one(&search));
    assert_eq!((s.query.as_str(), s.search_narrowing().as_deref()), ("cat", Some("y")));
}

#[test]
fn follows_a_filter_renamed_or_gone_back_and_forth() {
    let mut s = State::new();
    s.set_search("cat", Some("画像".into()));
    s.set_search("cat", Some("動画".into()));
    s.rename_search_filter("画像", Some("写真"));
    s.rename_search_filter("動画", None);
    // Gone from the results, they show without it.
    assert_eq!((s.query.as_str(), s.search_filter.as_deref()), ("cat", None));
    assert_eq!(kinds(&s), [Kind::Home, Kind::Search]);
    s.back();
    assert_eq!(s.search_filter.as_deref(), Some("写真"));
}

#[test]
fn opens_a_tab_kept_searching_with_a_filter_alone_on_home() {
    let mut kept = Kept {
        search_filter: Some("画像".into()),
        folders: vec![(r"D:\a".into(), None)],
        picks: vec![None, Some(r"D:\a".into()), None],
        active: 2,
        ..Kept::default()
    };
    kept.without_filter_alone();
    assert_eq!(kept.search_filter, None);
    let s = State::from_kept(kept);
    assert_eq!(kinds(&s), [Kind::Home, Kind::Folder]);
    // The results' pick goes with them, and the folder takes back its own, none.
    assert!(s.columns[1].selected.is_empty());
    assert_eq!(s.active, 1);
}

fn hidden(entry: Entry) -> Entry {
    Entry { hidden: Hiding::Hidden, ..entry }
}

fn protected(entry: Entry) -> Entry {
    Entry { hidden: Hiding::Protected, ..entry }
}

const NONE: Showing = Showing { hidden: false, protected: false };
const ALL: Showing = Showing { hidden: true, protected: true };

#[test]
fn shows_the_protected_files_only_along_with_the_hidden_items() {
    let mut s = drive(vec![folder(r"D:\a"), hidden(folder(r"D:\.git")), protected(file(r"D:\pagefile.sys"))]);
    s.columns[1].total = 3;
    let shown = |s: &mut State, showing| {
        s.set_showing(showing);
        names(s, 1).into_iter().map(str::to_owned).collect::<Vec<_>>()
    };
    assert_eq!(shown(&mut s, Showing { hidden: true, protected: false }), [r"D:\a", r"D:\.git"]);
    assert_eq!(s.columns[1].shown_total(), 2);
    // Asked for alone, they stay out with the rest.
    assert_eq!(shown(&mut s, Showing { hidden: false, protected: true }), [r"D:\a"]);
    assert_eq!(shown(&mut s, ALL).len(), 3);
    assert!(s.columns[1].narrow.is_none());
    assert_eq!(shown(&mut s, NONE), [r"D:\a"]);
}

/// The listing of the column at `index` arrives, as from a refresh.
fn listed(s: &mut State, index: usize, entries: Vec<Entry>) {
    let (id, request, total) = (s.columns[index].id, s.columns[index].request, entries.len());
    s.page(id, request, entries, total, 0, &HashSet::new());
}

#[test]
fn leaves_out_the_hidden_items_keeping_them_picked_as_a_filter_does() {
    let mut s = State::new();
    with_entries(&mut s, 0, vec![folder(r"D:\")]);
    s.select(0, one(r"D:\"));
    let listing = vec![folder(r"D:\a"), hidden(folder(r"D:\.git")), hidden(file(r"D:\desktop.ini"))];
    listed(&mut s, 1, listing.clone());
    s.set_active(1);
    s.select(1, one(r"D:\desktop.ini"));
    s.set_showing(NONE);
    assert_eq!(names(&s, 1), [r"D:\a"]);
    assert_eq!(s.columns[1].shown_total(), 1);
    assert!(!s.columns[1].is_narrowed());
    // Out of sight, it stays picked, but is not what is acted on.
    assert_eq!(s.columns[1].selected, one(r"D:\desktop.ini"));
    assert!(s.columns[1].shown_selected().is_empty());
    // A refresh leaves them out again; narrowing by the field, and lifting it, does too.
    listed(&mut s, 1, listing.clone());
    assert_eq!(names(&s, 1), [r"D:\a"]);
    s.set_narrowing(1, Some("i"));
    assert!(s.columns[1].entries.is_empty());
    assert!(s.columns[1].is_narrowed());
    s.set_narrowing(1, None);
    assert_eq!(names(&s, 1), [r"D:\a"]);
    // Shown again, all are listed as before, the pick with them.
    s.set_showing(ALL);
    assert_eq!(names(&s, 1).len(), 3);
    assert!(s.columns[1].narrow.is_none());
    assert_eq!(*s.columns[1].shown_selected(), one(r"D:\desktop.ini"));
    assert_eq!(s.columns[1].shown_total(), 3);
}

#[test]
fn a_hidden_folder_left_open_beside_keeps_its_column_and_its_row() {
    let mut s = drive(vec![folder(r"D:\a"), hidden(folder(r"D:\.git"))]);
    s.select(1, one(r"D:\.git"));
    with_entries(&mut s, 2, vec![hidden(file(r"D:\.git\HEAD")), file(r"D:\.git\config")]);
    s.set_showing(NONE);
    assert_eq!(s.columns.len(), 3);
    assert_eq!(names(&s, 1), [r"D:\a", r"D:\.git"]);
    assert_eq!(names(&s, 2), [r"D:\.git\config"]);
}

#[test]
fn going_into_a_hidden_folder_shows_the_way_there_and_not_what_else_is_hidden() {
    let mut s = State::new();
    s.set_showing(NONE);
    s.reveal(Path::new(r"D:\.git\HEAD"), false);
    listed(&mut s, 1, vec![folder(r"D:\a"), hidden(folder(r"D:\.git")), hidden(folder(r"D:\$RECYCLE.BIN"))]);
    listed(&mut s, 2, vec![hidden(file(r"D:\.git\HEAD")), hidden(file(r"D:\.git\index")), file(r"D:\.git\config")]);
    assert_eq!(names(&s, 1), [r"D:\a", r"D:\.git"]);
    assert_eq!(names(&s, 2), [r"D:\.git\HEAD", r"D:\.git\config"]);
    assert_eq!(s.columns[2].focused.as_deref(), Some(Path::new(r"D:\.git\HEAD")));
}

#[test]
fn keys_a_saved_search_by_its_order_the_default_as_before() {
    let size = Order { by: colet_core::entry::Sort::Size, descending: true, ..Order::default() };
    assert_eq!(HomeKey::search("cat", None, None, Order::default()).key(), PathBuf::from("search:cat"));
    for (filter, narrow) in [(None, None), (Some("画像"), None), (None, Some("a")), (Some("画像"), Some("a"))] {
        let key = HomeKey::search("cat", filter, narrow, size).key();
        assert_eq!(HomeKey::of(&key), HomeKey::search("cat", filter, narrow, size));
        assert_ne!(key, HomeKey::search("cat", filter, narrow, Order::default()).key());
    }
}

#[test]
fn runs_a_saved_search_in_its_order_and_goes_back_to_the_order_before() {
    let mut s = State::new();
    let size = Order { by: colet_core::entry::Sort::Size, descending: true, ..Order::default() };
    let sized = HomeKey::search("cat", None, None, size).key();
    with_entries(&mut s, 0, vec![Entry::folder(&sized, "cat"), folder(r"D:\")]);
    s.select(0, one(&sized));
    assert_eq!((s.query.as_str(), s.search_order), ("cat", size));
    // Searching again in the tab keeps it.
    s.set_query("dog");
    assert_eq!(s.search_order, size);
    s.search_order = Order::default();
    s.select(0, one(r"D:\"));
    s.back();
    assert_eq!(s.search_order, Order::default());
}

/// Where the row at `row` of the column at `index` is marked, as it is drawn.
fn marks(s: &State, index: usize, row: usize) -> Vec<Range<usize>> {
    let c = &s.columns[index];
    let entry = &c.entries[row];
    c.narrow_marks(&entry.name).unwrap_or_else(|| entry.marks.name.clone())
}

#[test]
fn finds_a_word_in_any_case_without_lowering_names_whole() {
    let needles = Needles::new("re \"Ü\"");
    assert_eq!(needles.holds("Read_über.txt"), Some(vec![0..2, 5..7]));
    assert_eq!(needles.holds("Read.txt"), None);
    // ASCII names are compared byte by byte, and still find each time, without overlap.
    assert_eq!(found("aAaA", "aa"), [0..2, 2..4]);
    // A letter that lowers past the word's end does not match.
    assert!(found("\u{130}", "i").is_empty());
}
