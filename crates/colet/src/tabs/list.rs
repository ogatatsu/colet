//! The tabs on the strip, in order: which is in front, the others parked as they were, and the ones
//! closed to bring back. Colet swaps a tab's state in and out of its own fields; this keeps the list, the
//! front and the closed ones in step as it does.
use crate::{
    narrow_fields::NarrowFields,
    scrolls::Scrolls,
    state::{ColumnId, State},
};

pub type TabId = u64;

/// How many closed tabs Ctrl+Shift+T can bring back.
const CLOSED: usize = 10;

/// A tab out of sight, kept as it was.
pub struct Parked {
    pub front: Front,
    /// Where the keys were as it went, and go again as it comes back.
    pub keys: Keys,
}

/// Where a tab's keys are.
#[derive(Clone, PartialEq, Debug)]
pub enum Keys {
    Search,
    Column,
    /// The field narrowing the column.
    NarrowField(ColumnId),
    /// The address being typed, as it was left: what it holds and the text picked in it.
    Address {
        text: String,
        picked: std::ops::Range<usize>,
        reversed: bool,
    },
}

/// What a tab in front holds in the window's own fields, handed over as it is parked or taken back as it
/// comes to the front.
pub struct Front {
    pub state: State,
    /// The filter its next search is narrowed by.
    pub filter: Option<String>,
    pub narrow_fields: NarrowFields,
    pub scrolls: Scrolls,
}

impl Parked {
    /// A tab never in front yet: its fields and scrolls are made as it first comes there.
    pub fn new(state: State, filter: Option<String>, keys: Keys) -> Self {
        Self { front: Front { state, filter, narrow_fields: NarrowFields::default(), scrolls: Scrolls::default() }, keys }
    }

    /// The search filter `from` is named `to` now, or is gone.
    fn rename_filter(&mut self, from: &str, to: Option<&str>) {
        let front = &mut self.front;
        front.state.rename_search_filter(from, to);
        if front.filter.as_deref() == Some(from) {
            front.filter = to.map(Into::into);
        }
    }
}

pub struct Tab {
    id: TabId,
    /// None for the tab in front.
    parked: Option<Parked>,
}

impl Tab {
    pub fn id(&self) -> TabId {
        self.id
    }

    /// As it was left; none for the tab in front, whose state is the window's.
    pub fn parked(&self) -> Option<&Parked> {
        self.parked.as_ref()
    }
}

#[derive(Default)]
pub struct Tabs {
    list: Vec<Tab>,
    /// The tab in front.
    current: usize,
    next_id: TabId,
    /// The tabs closed, the latest last, each with where it stood.
    closed: Vec<(usize, Tab)>,
}

impl Tabs {
    fn tab(&mut self, parked: Option<Parked>) -> Tab {
        self.next_id += 1;
        Tab { id: self.next_id, parked }
    }

    // ---- The list ----

    pub fn len(&self) -> usize {
        self.list.len()
    }

    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.list.iter()
    }

    pub fn get(&self, index: usize) -> Option<&Tab> {
        self.list.get(index)
    }

    /// Where the tab `id` stands; none once it is closed.
    pub fn position(&self, id: TabId) -> Option<usize> {
        self.list.iter().position(|tab| tab.id == id)
    }

    /// Where the tab in front stands.
    pub fn current(&self) -> usize {
        self.current
    }

    pub fn current_id(&self) -> TabId {
        self.list[self.current].id
    }

    /// The tab `id` is the one in front.
    pub fn is_front(&self, id: TabId) -> bool {
        self.list.get(self.current).is_some_and(|tab| tab.id == id)
    }

    /// The tab `id` as it was left; none for the tab in front or one closed.
    pub fn parked_mut(&mut self, id: TabId) -> Option<&mut Parked> {
        self.list.iter_mut().find(|tab| tab.id == id).and_then(|tab| tab.parked.as_mut())
    }

    /// A tab at the end: `parked`, or none for the one in front.
    pub fn push(&mut self, parked: Option<Parked>) {
        let tab = self.tab(parked);
        self.list.push(tab);
    }

    /// A parked tab at `at`, the one in front staying in front; where it went.
    pub fn insert(&mut self, at: usize, parked: Parked) -> usize {
        let at = at.min(self.list.len());
        let tab = self.tab(Some(parked));
        self.insert_tab(at, tab);
        at
    }

    fn insert_tab(&mut self, at: usize, tab: Tab) {
        self.list.insert(at, tab);
        if at <= self.current && self.list.len() > 1 {
            self.current += 1;
        }
    }

    /// The tab at `index` is in front, as the tabs were kept; the strip brings it in sight.
    pub fn set_front(&mut self, index: usize) {
        self.current = index;
    }

    /// Takes the tab at `index` to bring it to the front; none for the one in front already, or none there.
    pub fn take_parked(&mut self, index: usize) -> Option<Parked> {
        self.list.get_mut(index).and_then(|tab| tab.parked.take())
    }

    /// The tab at `index`, taken with [`Self::take_parked`], is in front now, the one that was parked as
    /// `outgoing`, and is brought in sight.
    pub fn bring_to_front(&mut self, index: usize, outgoing: Parked) {
        self.list[self.current].parked = Some(outgoing);
        self.set_front(index);
    }

    /// Takes the tab at `index`, out of sight, off the strip, to be brought back.
    pub fn close(&mut self, index: usize) {
        let tab = self.list.remove(index);
        if index < self.current {
            self.current -= 1;
        }
        self.closed.push((index, tab));
        if self.closed.len() > CLOSED {
            self.closed.remove(0);
        }
    }

    pub fn can_reopen(&self) -> bool {
        !self.closed.is_empty()
    }

    /// The tab closed last, back where it stood: where that is now.
    pub fn reopen(&mut self) -> Option<usize> {
        let (at, tab) = self.closed.pop()?;
        let at = at.min(self.list.len());
        self.insert_tab(at, tab);
        Some(at)
    }

    /// The tab `id` dropped in the gap `gap`: 0 before the first tab, up to the count after the last. The
    /// tab in front stays in front, wherever it now stands. False when nothing moved.
    pub fn move_tab(&mut self, id: TabId, gap: usize) -> bool {
        let Some(from) = self.position(id) else { return false };
        // Taken out first, the tabs past it close up by one.
        let to = if gap > from { gap - 1 } else { gap };
        if from == to {
            return false;
        }
        let front = self.current_id();
        let tab = self.list.remove(from);
        self.list.insert(to.min(self.list.len()), tab);
        self.current = self.position(front).unwrap_or_default();
        true
    }

    /// The search filter `from` is named `to` now, or is gone: every tab parked follows it, closed ones too.
    pub fn rename_filter(&mut self, from: &str, to: Option<&str>) {
        let closed = self.closed.iter_mut().map(|(_, tab)| tab);
        for parked in self.list.iter_mut().chain(closed).filter_map(|tab| tab.parked.as_mut()) {
            parked.rename_filter(from, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parked() -> Parked {
        Parked::new(State::new(), None, Keys::Column)
    }

    /// Four tabs, the second in front.
    fn four() -> Tabs {
        let mut tabs = Tabs::default();
        tabs.push(Some(parked()));
        tabs.push(None);
        tabs.push(Some(parked()));
        tabs.push(Some(parked()));
        tabs.set_front(1);
        tabs
    }

    fn ids(tabs: &Tabs) -> Vec<TabId> {
        tabs.iter().map(Tab::id).collect()
    }

    #[test]
    fn keeps_the_tab_in_front_as_tabs_go_in_before_and_after_it() {
        let mut tabs = four();
        let front = tabs.current_id();
        assert_eq!(tabs.insert(0, parked()), 0);
        assert_eq!((tabs.current(), tabs.current_id()), (2, front));
        assert_eq!(tabs.insert(3, parked()), 3);
        assert_eq!((tabs.current(), tabs.current_id()), (2, front));
        // Past the end, at the end.
        assert_eq!(tabs.insert(99, parked()), tabs.len() - 1);
        assert_eq!(tabs.current_id(), front);
    }

    #[test]
    fn brings_back_a_closed_tab_where_it_stood_the_one_in_front_staying() {
        let mut tabs = four();
        let (front, first) = (tabs.current_id(), tabs.get(0).unwrap().id());
        tabs.close(0);
        assert_eq!((tabs.len(), tabs.current_id()), (3, front));
        assert!(tabs.can_reopen());
        assert_eq!(tabs.reopen(), Some(0));
        assert_eq!(tabs.get(0).unwrap().id(), first);
        assert_eq!(tabs.current_id(), front);
        assert!(!tabs.can_reopen());
        assert_eq!(tabs.reopen(), None);
    }

    #[test]
    fn keeps_only_the_latest_closed_tabs() {
        let mut tabs = Tabs::default();
        tabs.push(None);
        for _ in 0..CLOSED + 2 {
            tabs.insert(1, parked());
            tabs.close(1);
        }
        let reopened = std::iter::from_fn(|| tabs.reopen()).count();
        assert_eq!(reopened, CLOSED);
    }

    #[test]
    fn moves_a_tab_into_a_gap_the_one_in_front_staying_in_front() {
        let mut tabs = four();
        let before = ids(&tabs);
        let front = tabs.current_id();
        // The first into the gap after the third.
        assert!(tabs.move_tab(before[0], 3));
        assert_eq!(ids(&tabs), [before[1], before[2], before[0], before[3]]);
        assert_eq!(tabs.current_id(), front);
        // Into either gap beside itself, nothing moves.
        assert!(!tabs.move_tab(before[0], 2));
        assert!(!tabs.move_tab(before[0], 3));
    }

    #[test]
    fn swaps_the_tab_in_front_for_a_parked_one() {
        let mut tabs = four();
        let (was, next) = (tabs.current_id(), tabs.get(2).unwrap().id());
        assert!(tabs.take_parked(1).is_none(), "the tab in front is not parked");
        let incoming = tabs.take_parked(2).expect("parked");
        tabs.bring_to_front(2, Parked { keys: Keys::Search, ..incoming });
        assert_eq!(tabs.current_id(), next);
        assert!(tabs.is_front(next) && !tabs.is_front(was));
        assert_eq!(tabs.get(1).unwrap().parked().map(|p| p.keys.clone()), Some(Keys::Search));
        assert!(tabs.parked_mut(next).is_none());
    }
}
