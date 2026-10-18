#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod crumbs;
mod dialogs;
mod icons;
mod menus;
mod ops;
mod places;
mod reveal;
mod settings;
mod state;
#[cfg(test)]
mod tests;
mod theme;
mod view;

use std::borrow::Cow;

use gpui_kit::*;

gpui_kit::assets::icon_assets!(ExtraIcons, [
        X,
        ChevronsLeft,
        Check,
        SlidersHorizontal,
        StarFill,
        Scissors,
        ClipboardPaste,
        Link,
        PencilLine,
        FolderPlus,
        Trash,
        Clock
    ]);

/// GPUI Kit's icons, and the few more Colet draws.
struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = gpui_kit::assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

fn main() {
    application().with_assets(AppAssets).run(|cx| {
        init(cx);
        app::bind_keys(cx);
        let live: backend::Shared = std::sync::Arc::new(backend::Live::default());
        let settings = live.load_settings();
        // Where it was left, or centered on the screen the first time.
        let (display_id, window_bounds) = match settings.as_ref().ok().and_then(|s| s.window) {
            Some(w) => {
                let displays: Vec<_> = cx.displays().iter().map(|d| (d.id(), d.bounds())).collect();
                let primary = cx.primary_display().map(|d| d.id());
                let (display, bounds) = places::restore_bounds(w, &displays, primary);
                (display, if w.maximized { WindowBounds::Maximized(bounds) } else { WindowBounds::Windowed(bounds) })
            }
            None => (None, WindowBounds::Windowed(Bounds::centered(None, size(px(1200.), px(780.)), cx))),
        };
        open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                display_id,
                titlebar: Some(TitlebarOptions { title: Some("Colet".into()), ..Default::default() }),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| app::Colet::new(live, settings, window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
