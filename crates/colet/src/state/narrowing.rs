//! Narrowing a column to the names holding some words, and marking where they matched.
use super::*;

/// The names a column is narrowed to, those holding `text`, and all it lists, kept aside. A folder column
/// that leaves out the hidden items has one too, its `text` empty when nothing else narrows it.
#[derive(Clone, Debug)]
pub struct Narrowing {
    pub(super) text: String,
    /// The words of `text`, read once rather than for each name.
    pub(super) needles: Needles,
    /// Shared with `entries`, which holds those it lets through.
    pub(super) all: Vec<Arc<Entry>>,
    /// The search results, which Everything narrows, keep nothing aside: how many the search finds
    /// without the filter stands for them.
    pub whole: usize,
    /// How many of `all` are hidden items left out.
    pub(super) concealed: usize,
}

impl Narrowing {
    pub(super) fn new(text: impl Into<String>, all: Vec<Arc<Entry>>, whole: usize) -> Self {
        let text = text.into();
        Self { needles: Needles::new(&text), text, all, whole, concealed: 0 }
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    /// The same, narrowed to `text` instead.
    pub(super) fn retext(self, text: impl Into<String>) -> Self {
        let text = text.into();
        Self { needles: Needles::new(&text), text, ..self }
    }
}

/// The words of a filter, each lowered once, to look for in many names.
#[derive(Clone, Debug, Default)]
pub struct Needles(Vec<String>);

impl Needles {
    pub fn new(text: &str) -> Self {
        Self(narrow_words(text).iter().map(|word| word.to_lowercase()).collect())
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Where `name` holds each word, in any order, if it holds them all: the ranges of all of them, those
    /// that meet as one. No words, nothing marked, are held by any name.
    pub fn holds(&self, name: &str) -> Option<Vec<Range<usize>>> {
        let mut ranges = Vec::new();
        for word in &self.0 {
            let start = ranges.len();
            found_lowered(name, word, &mut ranges);
            if ranges.len() == start {
                return None;
            }
        }
        ranges.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::with_capacity(ranges.len());
        for r in ranges {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        Some(merged)
    }
}

/// Where `needle` stands in `name`, case aside, each time it does: byte ranges of `name`.
#[cfg(test)]
pub fn found(name: &str, needle: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    found_lowered(name, &needle.to_lowercase(), &mut ranges);
    ranges
}

/// Where the already lowered `needle` stands in `name`, case aside, each time it does, added to `ranges`. Run over every name in a folder at each
/// key typed, so it allocates nothing of its own.
pub(super) fn found_lowered(name: &str, needle: &str, ranges: &mut Vec<Range<usize>>) {
    if needle.is_empty() {
        return;
    }
    // An ASCII name lowers byte for byte into ASCII, which only an ASCII word can match: the bytes are
    // compared as they are.
    if name.is_ascii() && needle.is_ascii() {
        let (name, needle) = (name.as_bytes(), needle.as_bytes());
        let mut i = 0;
        while i + needle.len() <= name.len() {
            if name[i..i + needle.len()].eq_ignore_ascii_case(needle) {
                ranges.push(i..i + needle.len());
                i += needle.len();
            } else {
                i += 1;
            }
        }
        return;
    }
    let mut at = 0;
    while let Some(c) = name[at..].chars().next() {
        match lowered_match(&name[at..], needle) {
            Some(len) => {
                ranges.push(at..at + len);
                at += len;
            }
            None => at += c.len_utf8(),
        }
    }
}

/// How long the start of `rest` is that lowers, a letter at a time, into exactly `needle`: a letter may
/// lower into more than one, and one that runs past the needle's end does not match.
pub(super) fn lowered_match(rest: &str, needle: &str) -> Option<usize> {
    let mut wanted = needle.chars();
    for (at, c) in rest.char_indices() {
        for lower in c.to_lowercase() {
            if wanted.next() != Some(lower) {
                return None;
            }
        }
        if wanted.as_str().is_empty() {
            return Some(at + c.len_utf8());
        }
    }
    None
}

/// The words a filter's `text` asks a name to hold, apart at any space, half or full width, but in quotes,
/// which are taken out. A quote left open is closed at the end, as Everything closes it.
pub fn narrow_words(text: &str) -> Vec<String> {
    let (mut words, mut word, mut quoted) = (Vec::new(), String::new(), false);
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            c if c.is_whitespace() && !quoted => words.extend((!word.is_empty()).then(|| std::mem::take(&mut word))),
            c => word.push(c),
        }
    }
    words.extend((!word.is_empty()).then_some(word));
    words
}

/// A filter's `text` as a favorite or saved search keeps it: the spaces at its ends gone, and each run of
/// them between its words one; those in quotes, part of a word, as they are. None when it holds no word.
pub fn narrowing(text: &str) -> Option<String> {
    if narrow_words(text).is_empty() {
        return None;
    }
    let (mut kept, mut quoted, mut space) = (String::new(), false, false);
    for c in text.trim().chars() {
        if c.is_whitespace() && !quoted {
            space = true;
            continue;
        }
        if std::mem::take(&mut space) {
            kept.push(' ');
        }
        if c == '"' {
            quoted = !quoted;
        }
        kept.push(c);
    }
    Some(kept)
}

/// What a filter's field holds, made ready for a word typed after it: a space put between, and a quote
/// left open closed first, so that the word is one of its own.
pub fn room_for_a_word(held: &str) -> String {
    let open = held.matches('"').count() % 2 == 1;
    if held.is_empty() || !open && held.ends_with(char::is_whitespace) {
        return held.into();
    }
    format!("{held}{} ", if open { "\"" } else { "" })
}

/// Where `name` holds each of the words of `text`, in any order, if it holds them all: the ranges of all
/// of them, those that meet as one. No words, nothing marked, are held by any name.
/// For one name; many are asked through [`Needles`].
pub fn holds(name: &str, text: &str) -> Option<Vec<Range<usize>>> {
    Needles::new(text).holds(name)
}

impl State {
    /// Puts all the folder column at `index` lists, in its `entries` with no filter, under one of `text`,
    /// when that or the hidden items left out narrow it.
    pub(super) fn renarrow(&mut self, index: usize, text: String) {
        if text.is_empty() && !self.conceals(index) {
            return;
        }
        let c = &mut self.columns[index];
        c.narrow = Some(Narrowing::new(text, std::mem::take(&mut c.entries), 0));
        self.narrow(index);
    }

    /// Narrows the column at `index` to the names holding the words of `text`, or with none, or no words in
    /// it, lists them all again. What it hides stays picked, and what that opened open, while another is looked for. The
    /// search results are narrowed by Everything, asked again: they show as they are until it answers.
    pub fn set_narrowing(&mut self, index: usize, text: Option<&str>) {
        let Some(c) = self.columns.get_mut(index) else { return };
        let text = text.filter(|text| !narrow_words(text).is_empty());
        if c.kind == Kind::Search {
            c.narrow = match (text, c.narrow.take()) {
                (None, _) => None,
                (Some(text), Some(filter)) => Some(filter.retext(text)),
                (Some(text), None) => Some(Narrowing::new(text, Vec::new(), c.total)),
            };
            return;
        }
        if let Some(filter) = c.narrow.take() {
            c.entries = filter.all;
            // All show again: the keys go back to the row the filter took them off, unless it hides it
            // still.
            if text.is_none() {
                let hid = c.hid.take();
                if c.focused.is_none() {
                    c.focused = hid;
                }
            }
        }
        self.renarrow(index, text.unwrap_or_default().into());
    }

    /// Lists what the column's filter lets through; the keys let go of a row it hides, kept to go back
    /// to once it shows again, while they have gone to no other.
    pub(super) fn narrow(&mut self, index: usize) {
        let (conceal, showing) = (self.conceals(index), self.showing);
        // The hidden item on the way to where the user is shows still: the folder open beside, or the item
        // pointed at.
        let beside = self.columns.get(index + 1).filter(|next| next.kind == Kind::Folder).map(|next| next.path.clone());
        let c = &mut self.columns[index];
        if c.kind == Kind::Search {
            return;
        }
        let Some(filter) = c.narrow.as_mut() else { return };
        let on_the_way = |path: &PathBuf| beside.as_ref() == Some(path) || c.highlight.as_ref() == Some(path);
        let mut concealed = 0;
        c.entries = filter
            .all
            .iter()
            .filter_map(|e| {
                if conceal && !showing.shows(e.hidden) && !on_the_way(&e.path) {
                    concealed += 1;
                    return None;
                }
                filter.needles.holds(&e.name).is_some().then(|| e.clone())
            })
            .collect();
        filter.concealed = concealed;
        let shown: HashSet<&Path> = c.entries.iter().map(|e| e.path.as_path()).collect();
        match &c.focused {
            Some(f) if !shown.contains(f.as_path()) => c.hid = c.focused.take(),
            Some(_) => c.hid = None,
            None if c.hid.as_ref().is_some_and(|h| shown.contains(h.as_path())) => c.focused = c.hid.take(),
            None => {}
        }
    }
}
