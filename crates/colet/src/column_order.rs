//! The order each column is listed in: the default, or one of its own, a folder's kept for every tab and
//! the search results' kept with the tab; set from the column's heading, its order's menu, or the
//! headings of its rows.
use colet_core::entry::{Order, Sort, direction_label};
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    menus::{Line, Popup, choice},
    state::{Column, ColumnId, Kind},
};

/// What a line of a column's order's menu does.
#[derive(Clone, Debug, PartialEq)]
pub enum OrderCommand {
    /// A sort for the column.
    Sort(ColumnId, Sort),
    Direction(ColumnId, bool),
    /// Folders before files, or mixed in with them.
    FoldersFirst(ColumnId, bool),
    /// The column goes back to the default order.
    Unsort(ColumnId),
}

impl Colet {
    /// The order the column `c` is listed in: its own, or the default.
    pub fn order_of(&self, c: &Column) -> Order {
        self.own_order(c).unwrap_or_default()
    }

    /// The order the column `c` is given apart from the default: its folder's, kept for every tab, or the
    /// search results', this tab's own.
    pub fn own_order(&self, c: &Column) -> Option<Order> {
        match c.kind {
            Kind::Home => None,
            Kind::Search => (self.state.search_order != Order::default()).then_some(self.state.search_order),
            Kind::Folder => self.stored.sorts().folder(&c.path),
        }
    }

    /// The order of the column `id`, or the default when it is gone.
    fn order_for(&self, id: ColumnId) -> Order {
        self.state.index_of(id).map(|index| self.order_of(&self.state.columns[index])).unwrap_or_default()
    }

    /// Sorts the column `id` by `by`, keeping the way it runs.
    pub fn set_sort(&mut self, id: ColumnId, by: Sort, cx: &mut Context<Self>) {
        self.set_order(id, Order { by, ..self.order_for(id) }, cx);
    }

    pub fn set_direction(&mut self, id: ColumnId, descending: bool, cx: &mut Context<Self>) {
        self.set_order(id, Order { descending, ..self.order_for(id) }, cx);
    }

    /// Puts folders before files in the column `id`, or mixes them in.
    pub fn set_folders_first(&mut self, id: ColumnId, folders_first: bool, cx: &mut Context<Self>) {
        self.set_order(id, Order { folders_first, ..self.order_for(id) }, cx);
    }

    /// Sorts the column `id` by the heading `by` pressed: the way it runs turned round when it already
    /// goes by it, or else as that heading starts, names and locations from A and sizes and times from
    /// the largest.
    pub fn sort_by_heading(&mut self, id: ColumnId, by: Sort, cx: &mut Context<Self>) {
        let order = self.order_for(id);
        let descending = if order.by == by { !order.descending } else { !matches!(by, Sort::Name | Sort::Location) };
        self.set_order(id, Order { by, descending, ..order }, cx);
    }

    /// Gives the column `id` the order; a column given the default keeps no order of its own. The columns
    /// whose order this changes are listed again.
    pub fn set_order(&mut self, id: ColumnId, order: Order, cx: &mut Context<Self>) {
        if matches!(self.overlays.popup(), Some(Popup::ColumnSort(_))) {
            self.overlays.forget_popup();
        }
        let orders = |this: &Self| -> Vec<(ColumnId, Order)> {
            this.state.columns.iter().filter(|c| c.kind != Kind::Home).map(|c| (c.id, this.order_of(c))).collect()
        };
        let before = orders(self);
        if let Some(index) = self.state.index_of(id) {
            let c = &self.state.columns[index];
            let own = (order != Order::default()).then_some(order);
            if c.kind == Kind::Search {
                self.state.search_order = order;
                // The tab keeps it between starts.
                self.keep_tabs(cx);
            } else if own != self.own_order(c) {
                let path = c.path.clone();
                self.update_sorts(|sorts| sorts.set_folder(&path, own), cx);
            }
        }
        for ((id, was), (_, is)) in before.into_iter().zip(orders(self)) {
            if was != is {
                self.load(id, 0, true, cx);
            }
        }
        cx.notify();
    }
    // ---- The order's menu ----

    /// The column whose own order's menu is open, if one is.
    pub fn column_sort_open(&self) -> Option<ColumnId> {
        match self.overlays.popup() {
            Some(Popup::ColumnSort(id)) => Some(*id),
            _ => None,
        }
    }

    /// Opens the menu of the column `id`'s own order.
    pub fn open_column_sort(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        self.overlays.set_popup(Popup::ColumnSort(id));
        self.show_menu(window, cx);
    }

    /// The lines of the column `id`'s order's menu: what it sorts by, the way it runs, whether folders come
    /// first, and going back to the default; none once the column is gone.
    pub(crate) fn order_lines(&self, id: ColumnId) -> Vec<Line> {
        let Some(column) = self.state.index_of(id).map(|i| &self.state.columns[i]) else { return Vec::new() };
        let target = column.id;
        let sort = self.order_of(column);
        // By location only the search results, whose items are in all sorts of folders.
        let search = column.kind == Kind::Search;
        let mut lines: Vec<Line> = Sort::ALL
            .into_iter()
            .filter(|by| search || by.for_folders())
            .map(|by| choice(by.label(), by.label(), OrderCommand::Sort(target, by), by == sort.by))
            .collect();
        let sorts = lines.len();
        lines.extend([false, true].map(|descending| {
            let label = direction_label(descending);
            choice(label, label, OrderCommand::Direction(target, descending), descending == sort.descending)
        }));
        lines[sorts].group = true;
        let first = sort.folders_first;
        lines.push(choice("folders-first", words::column::FOLDERS_FIRST, OrderCommand::FoldersFirst(target, !first), first).grouped());
        // A column given its own order goes back to the default from here.
        let own = self.own_order(column).is_some();
        lines.push(
            Line { off: (!own).then_some(""), ..choice("unsort", words::column::UNSORT, OrderCommand::Unsort(target), false) }.grouped(),
        );
        lines
    }

    /// Does what the line picked in a column's order's menu says.
    pub(crate) fn run_order_command(&mut self, command: OrderCommand, cx: &mut Context<Self>) {
        match command {
            OrderCommand::Sort(target, by) => self.set_sort(target, by, cx),
            OrderCommand::Direction(target, descending) => self.set_direction(target, descending, cx),
            OrderCommand::FoldersFirst(target, first) => self.set_folders_first(target, first, cx),
            OrderCommand::Unsort(id) => self.set_order(id, Order::default(), cx),
        }
    }
}
