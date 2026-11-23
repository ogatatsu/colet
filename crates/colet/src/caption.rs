//! The window's own buttons, to minimize, maximize or restore and close it, drawn by Colet at the right
//! end of the tab bar since the window has no title bar of the system's. Each only tells Windows where it
//! lies, as a [`WindowControlArea`]: Windows presses it, as on a title bar of its own, and shows its snap
//! layouts over the one that maximizes. They look as Windows 11 draws them, in the theme's text color.
//!
//! What takes the window by is here too, [`takes_the_window`]: Windows moves it and maximizes it on a
//! double-click there, but leaves the window's menu to whoever drew the bar.
use std::sync::OnceLock;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::{
    Foundation::{HWND, LPARAM, POINT, WPARAM},
    Graphics::Gdi::ClientToScreen,
    UI::WindowsAndMessaging::{
        EnableMenuItem, GetSystemMenu, IsZoomed, MF_BYCOMMAND, MF_ENABLED, MF_GRAYED, PostMessageW, SC_CLOSE, SC_MAXIMIZE, SC_MINIMIZE,
        SC_MOVE, SC_RESTORE, SC_SIZE, SetMenuDefaultItem, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, WM_SYSCOMMAND,
    },
};

use crate::words;
use crate::{
    metrics::scaled,
    theme::{ActivePalette, Palette},
    view::TipExt,
};

/// One button's width, as Windows 11's.
const BUTTON_WIDTH: f32 = 46.;
/// The three side by side.
pub const WIDTH: f32 = BUTTON_WIDTH * 3.;
/// The glyphs' size, as Windows 11 draws them.
const GLYPH_SIZE: f32 = 10.;

/// What a button does, as Windows knows it.
#[derive(Clone, Copy)]
enum Caption {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Caption {
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "caption-minimize",
            Self::Maximize | Self::Restore => "caption-maximize",
            Self::Close => "caption-close",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Minimize => words::window::MINIMIZE,
            Self::Maximize => words::window::MAXIMIZE,
            Self::Restore => words::window::RESTORE,
            Self::Close => words::CLOSE,
        }
    }

    /// Its glyph in Windows' icon font.
    fn glyph(self) -> &'static str {
        match self {
            Self::Minimize => "\u{e921}",
            Self::Maximize => "\u{e922}",
            Self::Restore => "\u{e923}",
            Self::Close => "\u{e8bb}",
        }
    }

    fn area(self) -> WindowControlArea {
        match self {
            Self::Minimize => WindowControlArea::Min,
            Self::Maximize | Self::Restore => WindowControlArea::Max,
            Self::Close => WindowControlArea::Close,
        }
    }
}

/// The three buttons, as tall as what holds them: maximize turns to restore while the window is maximized,
/// and the glyphs fade while it is not the active one, as Windows' own do.
pub fn buttons(window: &Window, cx: &App) -> impl IntoElement + use<> {
    let p = cx.palette();
    let font = icon_font(cx);
    let active = window.is_window_active();
    let size = if window.is_maximized() { Caption::Restore } else { Caption::Maximize };
    div().flex_none().h_full().flex().children([Caption::Minimize, size, Caption::Close].map(|caption| button(caption, p, font, active)))
}

fn button(caption: Caption, p: Palette, font: &'static str, active: bool) -> impl IntoElement + use<> {
    let close = matches!(caption, Caption::Close);
    div()
        .id(caption.id())
        .flex_none()
        .w(scaled(BUTTON_WIDTH))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .window_control_area(caption.area())
        .role(Role::Button)
        .aria_label(caption.label())
        .tip(caption.label())
        .font_family(font)
        .text_size(scaled(GLYPH_SIZE))
        .text_color(if active { p.text } else { p.text_faint })
        .map(|d| {
            if close {
                d.hover(|s| s.bg(p.close_hover).text_color(p.on_close)).active(|s| s.bg(p.close_pressed).text_color(p.on_close))
            } else {
                d.hover(|s| s.bg(p.text.opacity(0.08)).text_color(p.text)).active(|s| s.bg(p.text.opacity(0.05)).text_color(p.text))
            }
        })
        .child(caption.glyph())
        .test_support()
}

/// Windows 11's icon font, or Windows 10's where 11's is not there: both hold the glyphs at the same points.
fn icon_font(cx: &App) -> &'static str {
    static FONT: OnceLock<&'static str> = OnceLock::new();
    FONT.get_or_init(|| {
        const FLUENT: &str = "Segoe Fluent Icons";
        if cx.text_system().all_font_names().iter().any(|name| name == FLUENT) { FLUENT } else { "Segoe MDL2 Assets" }
    })
}

/// `d` takes the window by, as a title bar does: Windows moves the window from it and maximizes or
/// restores it on a double-click, and a right-click opens the window's menu there.
pub fn takes_the_window(d: Stateful<Div>) -> Stateful<Div> {
    d.window_control_area(WindowControlArea::Drag).on_mouse_up(MouseButton::Right, |event: &MouseUpEvent, window, cx| {
        cx.stop_propagation();
        let Some((hwnd, at)) = on_screen(window, event.position) else { return };
        // Its menu runs a loop of its own: not inside the event being handed out.
        cx.spawn(async move |_| system_menu(hwnd, at)).detach();
    })
}

/// The window's handle, and `at` in it on the screen.
fn on_screen(window: &Window, at: Point<Pixels>) -> Option<(isize, POINT)> {
    let Ok(RawWindowHandle::Win32(handle)) = HasWindowHandle::window_handle(window).map(|h| h.as_raw()) else { return None };
    let hwnd = handle.hwnd.get();
    let scale = window.scale_factor();
    let mut point = POINT { x: (f32::from(at.x) * scale) as i32, y: (f32::from(at.y) * scale) as i32 };
    let _ = unsafe { ClientToScreen(HWND(hwnd as *mut _), &mut point) };
    Some((hwnd, point))
}

/// Opens the window's menu at `at` on the screen, what it offers as Windows' own would, and does what was
/// picked in it.
fn system_menu(hwnd: isize, at: POINT) {
    let hwnd = HWND(hwnd as *mut _);
    unsafe {
        let menu = GetSystemMenu(hwnd, false);
        if menu.is_invalid() {
            return;
        }
        let maximized = IsZoomed(hwnd).as_bool();
        for (item, on) in [
            (SC_RESTORE, maximized),
            (SC_MOVE, !maximized),
            (SC_SIZE, !maximized),
            (SC_MINIMIZE, true),
            (SC_MAXIMIZE, !maximized),
            (SC_CLOSE, true),
        ] {
            let _ = EnableMenuItem(menu, item, MF_BYCOMMAND | if on { MF_ENABLED } else { MF_GRAYED });
        }
        let _ = SetMenuDefaultItem(menu, SC_CLOSE, 0);
        let picked = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_RIGHTBUTTON, at.x, at.y, None, hwnd, None);
        if picked.0 != 0 {
            let _ = PostMessageW(Some(hwnd), WM_SYSCOMMAND, WPARAM(picked.0 as usize), LPARAM(0));
        }
    }
}
