#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod icons;
mod reveal;
mod state;
#[cfg(test)]
mod tests;
mod theme;
mod view;

use std::borrow::Cow;

use gpui_kit::*;

gpui_kit::assets::icon_assets!(ExtraIcons, [X, ChevronsLeft, Check]);

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
        let bounds = Bounds::centered(None, size(px(1200.), px(780.)), cx);
        open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions { title: Some("Colet".into()), ..Default::default() }),
                ..Default::default()
            },
            cx,
            |window, cx| cx.new(|cx| app::Colet::new(std::sync::Arc::new(backend::Live::default()), window, cx)),
        )
        .expect("failed to open window");
        cx.activate(true);
    });
}
