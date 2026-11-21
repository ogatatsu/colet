#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod address;
mod app;
mod backend;
mod caption;
mod clipboard;
mod column_order;
mod columns;
mod crumbs;
mod dialogs;
mod drag_out;
mod file_menu;
mod history;
mod home;
mod icons;
mod instance;
mod kept_file;
mod kept_path;
mod lane;
mod loading;
mod menus;
mod metrics;
mod narrow_fields;
mod ops;
mod overlays;
mod places;
mod preferences;
mod reveal;
mod scrolls;
mod search_bar;
mod search_filters;
mod search_query;
mod settings;
mod sorts;
mod state;
mod stops;
mod stored;
mod tabs;
#[cfg(test)]
mod tests;
mod text_menu;
mod theme;
mod view;
mod wake;
mod watching;
mod words;

use std::borrow::Cow;

use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

gpui_kit::assets::icon_assets!(
    ExtraIcons,
    [
        X,
        ChevronsLeft,
        Check,
        Circle,
        CircleCheck,
        SlidersHorizontal,
        StarFill,
        Pin,
        PinOff,
        Scissors,
        ClipboardPaste,
        Link,
        PencilLine,
        FolderPlus,
        Trash,
        Clock,
        Funnel,
        ArrowUpDown,
        House,
        AppWindow,
        RotateCcw,
        Send
    ]
);

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
    let Some(instance) = instance::claim() else { return };
    application().with_assets(AppAssets).run(|cx| {
        init(cx);
        gpui_kit::component::set_locale("ja");
        theme::init(cx);
        app::bind_keys(cx);
        let live: backend::Shared = std::sync::Arc::new(backend::Live::default());
        let settings = live.load_settings();
        let sorts = live.load_sorts();
        // Where it was left, or centered on the screen the first time.
        let (display_id, window_bounds) = match settings.value.window {
            Some(w) => {
                let displays: Vec<_> = cx.displays().iter().map(|d| (d.id(), d.bounds())).collect();
                let primary = cx.primary_display().map(|d| d.id());
                let (display, bounds) = places::restore_bounds(w, &displays, primary);
                (display, if w.maximized { WindowBounds::Maximized(bounds) } else { WindowBounds::Windowed(bounds) })
            }
            None => (None, WindowBounds::Windowed(Bounds::centered(None, size(px(app::FIRST_WINDOW.0), px(app::FIRST_WINDOW.1)), cx))),
        };
        open_window(
            WindowOptions {
                window_bounds: Some(window_bounds),
                window_min_size: Some(size(px(app::MIN_WINDOW.0), px(app::MIN_WINDOW.1))),
                display_id,
                // The tab bar is the title bar (see `caption`): Windows keeps the frame and the title it
                // names the window by in the taskbar, not the bar it would draw.
                titlebar: Some(TitlebarOptions { title: Some("Colet".into()), appears_transparent: true, ..Default::default() }),
                ..Default::default()
            },
            cx,
            |window, cx| {
                if let Ok(RawWindowHandle::Win32(handle)) = HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
                    instance.answer(handle.hwnd.get());
                }
                cx.new(|cx| app::Colet::new(live, settings, sorts, window, cx))
            },
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
