//! What Colet keeps between starts and how it is written: the settings and the columns' orders, the widths
//! and where the window was left.
use std::time::Duration;

use gpui_kit::*;

use crate::{
    app::{Colet, Widths},
    kept_file::Queued,
    notices::Tone,
    settings::{Placement, Settings},
    sorts::Sorts,
};

/// Where the window opens again: on the display holding the middle of where it was left, which the window
/// must name or it opens on the primary one; otherwise centered on `primary`, as after that display was
/// unplugged. Either way it is kept within the display it opens on, and no smaller than the window goes.
pub fn restore_bounds(
    placement: Placement,
    displays: &[(DisplayId, Bounds<Pixels>)],
    primary: Option<DisplayId>,
) -> (Option<DisplayId>, Bounds<Pixels>) {
    let saved = Bounds::new(point(px(placement.x), px(placement.y)), size(px(placement.width), px(placement.height)));
    let held = displays.iter().find(|(_, d)| d.contains(&saved.center()));
    let Some(&(id, display)) = held.or_else(|| displays.iter().find(|(id, _)| Some(*id) == primary)).or(displays.first()) else {
        return (None, saved);
    };
    let (min_width, min_height) = crate::app::MIN_WINDOW;
    let size =
        size(saved.size.width.max(px(min_width)).min(display.size.width), saved.size.height.max(px(min_height)).min(display.size.height));
    let origin = if held.is_some() {
        point(
            saved.origin.x.clamp(display.left(), display.right() - size.width),
            saved.origin.y.clamp(display.top(), display.bottom() - size.height),
        )
    } else {
        display.center() - point(size.width / 2., size.height / 2.)
    };
    (Some(id), Bounds::new(origin, size))
}

impl Colet {
    /// Changes the settings and writes them a moment later, once a burst of changes (a drag) is over;
    /// never over a file that could not be read.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut Settings), cx: &mut Context<Self>) {
        let listed = |s: &Settings| (s.favorites.clone(), s.saved_searches.clone(), s.pinned.clone());
        let before = listed(self.stored.settings());
        let saved = self.stored.change_settings(change);
        if before != listed(self.stored.settings()) {
            // Home lists them.
            let home = self.state.columns[0].id;
            self.load(home, 0, true, cx);
        }
        cx.notify();
        if let Some(change) = saved {
            Self::save_later(cx, move |this| this.stored.settings_save(change));
        }
    }

    /// Changes the orders the columns are given apart from the default, and writes them as the settings
    /// are written.
    pub fn update_sorts(&mut self, change: impl FnOnce(&mut Sorts), cx: &mut Context<Self>) {
        let saved = self.stored.change_sorts(change);
        cx.notify();
        if let Some(change) = saved {
            Self::save_later(cx, move |this| this.stored.sorts_save(change));
        }
    }

    /// Sends the save `queue` makes a moment later, once a burst of changes (a drag) is over, and says
    /// if it fails; `queue` sends none when a later change has its own coming.
    fn save_later(cx: &mut Context<Self>, queue: impl FnOnce(&Self) -> Option<Queued> + 'static) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(250)).await;
            let Some(queued) = this.update(cx, |this, _| queue(this)).ok().flatten() else { return };
            let saved = cx.background_executor().spawn(async move { queued.written() }).await;
            if let Err(error) = saved {
                this.update(cx, |this, cx| this.notify(error, Tone::Error, cx)).ok();
            }
        })
        .detach();
    }

    /// Writes the settings and the columns' orders now, as the window closes, once the saves before are
    /// written; the saves still waiting out a burst are dropped.
    pub fn flush_settings(&mut self) -> Result<(), String> {
        self.stored.flush()
    }

    pub fn remember_window(&mut self, window: &Window, cx: &mut Context<Self>) {
        let (bounds, maximized) = match window.window_bounds() {
            WindowBounds::Windowed(bounds) => (bounds, false),
            WindowBounds::Maximized(bounds) | WindowBounds::Fullscreen(bounds) => (bounds, true),
        };
        let placement = Placement {
            x: f32::from(bounds.origin.x),
            y: f32::from(bounds.origin.y),
            width: f32::from(bounds.size.width),
            height: f32::from(bounds.size.height),
            maximized,
        };
        if self.stored.settings().window != Some(placement) {
            self.update_settings(|s| s.window = Some(placement), cx);
        }
    }

    /// A drag on an edge is over: keep the width it left.
    pub fn end_resize(&mut self, cx: &mut Context<Self>) {
        if !self.widths.end_resize() {
            return;
        }
        let Widths { sidebar, search, column, field, .. } = self.widths;
        self.update_settings(
            |s| {
                s.sidebar_width = sidebar;
                s.search_width = search;
                s.column_width = column;
                s.search_field_width = field;
            },
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Placement, restore_bounds};
    use gpui_kit::{Bounds, DisplayId, point, px, size};

    #[test]
    fn reopens_on_the_display_it_was_left_on_or_centered_on_the_primary_one() {
        let primary = (DisplayId::new(1), Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.))));
        let second = (DisplayId::new(2), Bounds::new(point(px(1920.), px(0.)), size(px(1280.), px(720.))));
        let at = |x: f32, y: f32, width: f32, height: f32| Placement { x, y, width, height, maximized: false };
        let bounds = |x: f32, y: f32, w: f32, h: f32| Bounds::new(point(px(x), px(y)), size(px(w), px(h)));
        // Left on the second display, it opens there as it was.
        assert_eq!(
            restore_bounds(at(2000., 50., 800., 600.), &[primary, second], Some(primary.0)),
            (Some(second.0), bounds(2000., 50., 800., 600.))
        );
        // Larger than that display now is, it is kept within it.
        assert_eq!(
            restore_bounds(at(2000., 0., 1600., 900.), &[primary, second], Some(primary.0)),
            (Some(second.0), bounds(1920., 0., 1280., 720.))
        );
        // That display unplugged, it opens centered on the primary one with the size it had.
        assert_eq!(
            restore_bounds(at(2000., 50., 800., 600.), &[primary], Some(primary.0)),
            (Some(primary.0), bounds(560., 240., 800., 600.))
        );
        // Left smaller than the window now goes, it opens at the least it goes, where it was.
        assert_eq!(
            restore_bounds(at(100., 100., 400., 300.), &[primary], Some(primary.0)),
            (Some(primary.0), bounds(100., 100., 720., 480.))
        );
    }
}
