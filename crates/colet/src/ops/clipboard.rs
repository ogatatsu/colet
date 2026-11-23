//! Copy, cut and paste through the clipboard, and copying the items' paths.
use super::*;
use crate::words;

impl Colet {
    pub fn copy(&mut self, cut: bool, cx: &mut Context<Self>) {
        // What the menu was opened on, read before it closes: after, the selection would stand in.
        let (picked, reasons) = (self.picked(), self.reasons(cx));
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        if let Some(reason) = if cut { reasons.cut } else { reasons.copy } {
            self.refuse(if cut { words::cut() } else { words::copy() }, reason, cx);
            return;
        }
        let picked: Vec<PathBuf> = picked.iter().map(|p| HomeKey::on_disk(p).into_owned()).collect();
        self.operations.update(cx, |operations, cx| operations.copy_files(picked, cut, cx));
    }

    /// The paths as text, one a line; a saved search's is the query it runs, a narrowed favorite's its
    /// folder's. With nothing picked, the folder column's own.
    pub fn copy_paths(&mut self, cx: &mut Context<Self>) {
        let mut picked = self.picked();
        if picked.is_empty() {
            picked.extend(self.folder_here());
        }
        self.take_menu();
        if picked.is_empty() {
            return;
        }
        // As the paths are, even where they are not text: a pasted one still names its item.
        let mut text = OsString::new();
        for (i, p) in picked.iter().enumerate() {
            if i > 0 {
                text.push("\r\n");
            }
            match HomeKey::of(p) {
                HomeKey::Search { query, .. } => text.push(query),
                HomeKey::Folder { .. } => text.push(HomeKey::on_disk(p).as_os_str()),
            }
        }
        let count = picked.len();
        self.operations.update(cx, |operations, cx| operations.copy_text(text, count, cx));
    }

    pub fn paste(&mut self, cx: &mut Context<Self>) {
        self.take_menu();
        match self.reasons(cx).paste {
            Some(reason) => self.refuse(words::paste(), reason, cx),
            None => {
                let paste = Paste { destination: self.destination().unwrap_or_default(), tab: self.current_tab() };
                if let Some(paste) = self.operations.update(cx, |operations, _| operations.clipboard.paste(paste)) {
                    self.run_paste(paste, cx);
                }
            }
        }
    }

    /// Pastes as `paste` asks, for the tab it was asked in, whichever is in front now: that tab picks what
    /// it pastes, unless closed since.
    pub(super) fn run_paste(&mut self, paste: Paste, cx: &mut Context<Self>) {
        let request = Request { kind: Kind::Paste, paths: Vec::new(), destination: paste.destination, name: String::new() };
        self.run_in(request, paste.tab, cx);
    }
}
