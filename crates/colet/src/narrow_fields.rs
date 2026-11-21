//! The field of each column, narrowing it to some names: the fields themselves, kept in step with the
//! columns, and what Colet does as they are typed in, left, cleared and escaped.
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    time::Duration,
};

use gpui_kit::base::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::words;
use crate::{
    app::Colet,
    state::{Column, ColumnId, Kind, narrow_words, room_for_a_word},
    theme::ActivePalette,
};

/// The field narrowing a column, and what it last narrowed it to: the column lifting the filter on
/// its own, for what was made or pasted there, tells from that, not from text typed and yet to be told.
pub struct NarrowField {
    pub input: Entity<InputState>,
    applied: String,
    _changes: Subscription,
}

/// The columns' fields, by column.
#[derive(Default)]
pub struct NarrowFields {
    fields: HashMap<ColumnId, NarrowField>,
}

impl NarrowFields {
    pub fn get(&self, id: &ColumnId) -> Option<&NarrowField> {
        self.fields.get(id)
    }

    /// The field of the column `id`, its text field alone.
    pub fn input(&self, id: ColumnId) -> Option<Entity<InputState>> {
        self.fields.get(&id).map(|field| field.input.clone())
    }

    pub fn contains_key(&self, id: &ColumnId) -> bool {
        self.fields.contains_key(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&ColumnId, &NarrowField)> {
        self.fields.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &NarrowField> {
        self.fields.values()
    }

    /// The column `id` is narrowed to `text` now, as it was typed in its field.
    fn applied(&mut self, id: ColumnId, text: &str) {
        if let Some(field) = self.fields.get_mut(&id) {
            field.applied = text.into();
        }
    }

    /// The column `id` lists all again: its field empties.
    fn clear(&mut self, id: ColumnId, window: &mut Window, cx: &mut App) {
        if let Some(field) = self.fields.get_mut(&id) {
            field.applied.clear();
            field.input.update(cx, |input, cx| input.set_value("", window, cx));
        }
    }

    /// Each column has its field, holding what it is narrowed to: a field is made for a column new here,
    /// goes with one gone, empties where a new folder or a paste lifted the filter, and takes the narrowing
    /// a favorite or saved search opened with.
    fn sync(&mut self, columns: &[Column], window: &mut Window, cx: &mut Context<Colet>) {
        let kept: HashSet<ColumnId> = columns.iter().map(|c| c.id).collect();
        self.fields.retain(|id, _| kept.contains(id));
        for c in columns {
            let id = c.id;
            match self.fields.get_mut(&id) {
                Some(field) => match &c.narrow {
                    None if !field.applied.is_empty() => {
                        field.applied.clear();
                        field.input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                    Some(filter) if filter.text() != field.applied => {
                        field.applied = filter.text().to_owned();
                        field.input.update(cx, |input, cx| input.set_value(filter.text().to_owned(), window, cx));
                    }
                    _ => {}
                },
                None => {
                    let text = c.narrow.as_ref().map(|f| f.text().to_owned()).unwrap_or_default();
                    let input =
                        cx.new(|cx| InputState::new(window, cx).placeholder(words::column::NARROW_PLACEHOLDER).default_value(text.clone()));
                    cx.palette().style_input(&input, cx);
                    let subscription = cx.subscribe_in(&input, window, move |this: &mut Colet, input, event, window, cx| match event {
                        InputEvent::Change => {
                            let text = input.read(cx).value().to_string();
                            this.narrow(id, &text, cx);
                        }
                        InputEvent::PressEnter { .. } => this.open_from_narrow_field(id, window, cx),
                        InputEvent::Focus | InputEvent::Blur => cx.notify(),
                    });
                    self.fields.insert(id, NarrowField { input, applied: text, _changes: subscription });
                }
            }
        }
    }
}

impl Colet {
    /// Keeps the columns' fields in step with the columns; see [`NarrowFields::sync`].
    pub fn sync_narrow_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.narrow_fields.sync(&self.state.columns, window, cx);
    }

    /// What is typed in the rows goes on in their column's field, which takes the keys: after what it
    /// holds, as a word of its own. What an IME is composing, `marked` with its caret, is handed on as it
    /// is, so the field takes the rest of it. A space alone starts nothing.
    pub fn type_into_narrowing(&mut self, text: &str, marked: Option<Option<Range<usize>>>, window: &mut Window, cx: &mut Context<Self>) {
        if text.trim().is_empty() {
            return;
        }
        let id = self.state.columns[self.state.active].id;
        self.sync_narrow_fields(window, cx);
        let Some(input) = self.narrow_fields.input(id) else { return };
        input.update(cx, |input, cx| {
            let held = room_for_a_word(&input.value());
            input.set_value(held, window, cx);
            input.focus(window, cx);
            match marked {
                Some(caret) => input.replace_and_mark_text_in_range(None, text, caret, window, cx),
                None => input.replace_text_in_range(None, text, window, cx),
            }
        });
        // The IME's panel goes on under the field's caret, now the field's to tell.
        window.invalidate_character_coordinates();
        self.changed(cx);
    }

    /// Backspace in the rows takes the last char off their column's field, which takes the keys; false
    /// while it holds none.
    pub fn trim_narrowing(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let id = self.state.columns[self.state.active].id;
        let Some(input) = self.narrow_fields.input(id) else { return false };
        let mut held = input.read(cx).value().to_string();
        if held.pop().is_none() {
            return false;
        }
        input.update(cx, |input, cx| {
            input.set_value(held.clone(), window, cx);
            input.focus(window, cx);
        });
        self.narrow(id, &held, cx);
        true
    }

    /// Narrows the column `id` to the names holding `text`, as it is typed.
    pub fn narrow(&mut self, id: ColumnId, text: &str, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        self.state.set_narrowing(index, Some(text));
        self.narrow_fields.applied(id, text);
        // Everything narrows the results, asked as the search field asks it: once typing pauses, for the
        // text still there.
        if self.state.columns[index].kind == Kind::Search {
            let text = text.to_owned();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(150)).await;
                this.update(cx, |this, cx| {
                    let Some(index) = this.state.index_of(id) else { return };
                    // Spaces alone, no words, ask for all as an empty field does.
                    let applied = this.state.columns[index].narrow.as_ref().map_or("", |f| f.text());
                    if applied == text || applied.is_empty() && narrow_words(&text).is_empty() {
                        this.load(id, 0, true, cx);
                    }
                })
                .ok();
            })
            .detach();
        }
        // The rows move: the focused one is brought in sight again.
        self.scrolls.rows.moved(id);
        self.changed(cx);
    }

    /// From the field into the rows it lets through, landing on the first while none is picked.
    pub fn leave_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        self.state.set_active(index);
        self.focus_active(window, cx);
    }

    /// ↓ or Enter in a field picks the row the keys were on, or the first it lets through, as a click does,
    /// and the keys go there; a row picked already, among others too, is not picked again. With none let
    /// through, the field keeps the keys.
    pub fn open_from_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        let c = &self.state.columns[index];
        let shown = c.focused.clone().filter(|path| c.entry(path).is_some());
        let Some(path) = shown.or_else(|| c.entries.first().map(|e| e.path.clone())) else { return };
        if c.selected.contains(&path) {
            self.state.columns[index].focused = Some(path);
            self.leave_narrow_field(id, window, cx);
        } else {
            self.pick(index, path, window, cx);
        }
    }

    /// Lists all again and empties the field; the keys go back to the rows.
    pub fn clear_narrowing(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.state.index_of(id) else { return };
        self.state.set_narrowing(index, None);
        if self.state.columns[index].kind == Kind::Search {
            self.load(id, 0, true, cx);
        }
        self.narrow_fields.clear(id, window, cx);
        self.scrolls.rows.moved(id);
        self.leave_narrow_field(id, window, cx);
    }

    /// Esc in the field: clears what it holds, or, empty, goes back to the rows.
    pub fn escape_narrow_field(&mut self, id: ColumnId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(input) = self.narrow_fields.input(id) else { return };
        if input.read(cx).value().is_empty() {
            // Back to the rows picking nothing new, as Esc from the address field does.
            let Some(index) = self.state.index_of(id) else { return };
            self.state.set_active(index);
            self.come_back_to_active(window, cx);
        } else {
            input.update(cx, |input, cx| input.set_value("", window, cx));
            self.narrow(id, "", cx);
        }
    }
}

impl std::ops::Index<&ColumnId> for NarrowFields {
    type Output = NarrowField;

    fn index(&self, id: &ColumnId) -> &NarrowField {
        &self.fields[id]
    }
}
