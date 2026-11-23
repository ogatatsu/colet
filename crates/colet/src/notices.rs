//! The notices put up in the window: each numbered, so a test can find it and its button, and gone on its
//! own or kept until closed by its tone. What a notice's button does is the caller's.
use gpui_kit::component::{
    WindowExt,
    button::Button,
    notification::{Notification, NotificationType},
};
use gpui_kit::*;

use crate::words;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Info,
    /// Something to set right outside Colet: it stays until closed, as an error does.
    Warning,
    Error,
}

/// What a notice's "details" button does, given the press.
pub type Details = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

pub struct Notices {
    /// The window they go up in.
    window: AnyWindowHandle,
    /// The number the last notice went up with.
    last: u64,
}

impl Notices {
    pub fn new(window: AnyWindowHandle) -> Self {
        Self { window, last: 0 }
    }

    /// The number the last notice went up with; none went up while it is 0.
    #[cfg(test)]
    pub fn last(&self) -> u64 {
        self.last
    }

    /// Puts a notice up in the window, the newest few at a time, once the update it is asked in ends:
    /// errors and warnings stay until closed; the rest go on their own. With `details`, it has a button
    /// that closes it and does that.
    pub fn push(&mut self, text: SharedString, tone: Tone, details: Option<Details>, cx: &mut App) {
        self.last += 1;
        let id = self.last;
        let stays = tone != Tone::Info;
        let mut note = Notification::new()
            .id1::<Self>(("toast", id))
            .content(move |_, _, _| {
                div()
                    .id(("toast", id))
                    .test_support()
                    .role(if stays { Role::Alert } else { Role::Status })
                    .aria_label(text.clone())
                    .text_sm()
                    .child(text.clone())
                    .into_any_element()
            })
            .autohide(!stays);
        match tone {
            Tone::Error => note = note.with_type(NotificationType::Error),
            Tone::Warning => note = note.with_type(NotificationType::Warning),
            Tone::Info => {}
        }
        if let Some(details) = details {
            let details = std::rc::Rc::new(details);
            note = note.action(move |_, _, _| {
                let details = details.clone();
                // The mouse leaves the keys where they are; from the keys, they go on as `details` takes them.
                crate::view::keep_focus(Button::new(("toast-details", id)).label(words::window::details())).on_click(
                    move |event: &ClickEvent, window, cx| {
                        window.remove_notification1::<Notices>(("toast", id), cx);
                        details(event, window, cx);
                    },
                )
            });
        }
        let window = self.window;
        cx.defer(move |cx| {
            window.update(cx, |_, window, cx| window.push_notification(note, cx)).ok();
        });
    }
}
