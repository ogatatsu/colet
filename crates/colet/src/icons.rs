//! The shell's icons for rows, asked off the UI thread and kept: files are asked by extension, so a
//! folder of thousands costs a handful of shell calls.
use std::{collections::HashMap, sync::Arc};

use colet_core::{entry::Entry, shell::{self, IconSource}};
use gpui_kit::{Context, RenderImage};

use crate::app::Colet;

/// Each icon asked for: `None` while it is on its way, or when the shell has none.
#[derive(Default)]
pub struct Icons(HashMap<IconSource, Option<Arc<RenderImage>>>);

/// How many it keeps before starting over.
const LIMIT: usize = 4096;

/// Files that carry their own icon are asked for by path; everything else by extension.
pub fn source(entry: &Entry) -> IconSource {
    let drive = entry.path.len() == 3 && entry.path.ends_with(":\\");
    if entry.directory {
        return if drive { IconSource::Item(entry.path.clone()) } else { IconSource::Folder };
    }
    let lower = entry.name.to_lowercase();
    if [".exe", ".lnk", ".ico", ".url"].iter().any(|ext| lower.ends_with(ext)) {
        return IconSource::Item(entry.path.clone());
    }
    match lower.rfind('.') {
        Some(dot) if dot > 0 => IconSource::Extension(lower[dot..].into()),
        _ => IconSource::Extension(String::new()),
    }
}

impl Icons {
    /// The icon for `entry`, or `None` while it is being asked for or when the shell has none; the view
    /// redraws once it arrives.
    pub fn get(&mut self, entry: &Entry, cx: &mut Context<Colet>) -> Option<Arc<RenderImage>> {
        let source = source(entry);
        if let Some(image) = self.0.get(&source) {
            return image.clone();
        }
        if self.0.len() >= LIMIT {
            self.0.clear();
        }
        self.0.insert(source.clone(), None);
        let key = source.clone();
        let task = cx.background_executor().spawn(async move {
            let icon = shell::icon(&source)?;
            let buffer = image::RgbaImage::from_raw(icon.width, icon.height, icon.bgra)?;
            Some(Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(buffer)])))
        });
        cx.spawn(async move |view, cx| {
            let image = task.await;
            view.update(cx, |view, cx| {
                view.icons.0.insert(key, image);
                cx.notify();
            })
        })
        .detach();
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asks_by_extension_but_by_path_for_self_iconed_files_and_drives() {
        let file = |name: &str| Entry { name: name.into(), path: format!(r"D:\{name}"), ..Entry::folder("", "") };
        let file = |name: &str| Entry { directory: false, ..file(name) };
        assert_eq!(source(&file("a.TXT")), IconSource::Extension(".txt".into()));
        assert_eq!(source(&file("README")), IconSource::Extension(String::new()));
        assert_eq!(source(&file(".gitignore")), IconSource::Extension(String::new()));
        assert_eq!(source(&file("app.exe")), IconSource::Item(r"D:\app.exe".into()));
        assert_eq!(source(&Entry::folder(r"D:\", "D:")), IconSource::Item(r"D:\".into()));
        assert_eq!(source(&Entry::folder(r"D:\a", "a")), IconSource::Folder);
    }
}
