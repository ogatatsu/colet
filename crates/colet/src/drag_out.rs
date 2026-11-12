//! Rows dragged out of the window into another app, as from Explorer: a row of a folder or the results
//! pressed and moved far enough carries the selection if it is part of it, or else is picked and carried
//! alone. Windows runs the drag; nothing is dropped back in Colet.
use std::path::PathBuf;

use gpui_kit::*;

use crate::{
    app::{Colet, Tone},
    state::HomeKey,
};

/// A row the left button went down on, which a drag may start from.
#[derive(Clone, Debug)]
pub struct Press {
    index: usize,
    path: PathBuf,
    at: Point<Pixels>,
}

impl Colet {
    /// The left button went down at `at` on the row of `path` in the column at `index`.
    pub fn press_row(&mut self, index: usize, path: PathBuf, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.press = Some(Press { index, path, at });
        // For the window-wide listener to be drawn.
        cx.notify();
    }

    /// The pointer moved with a row pressed: out past Windows' drag distance, the drag starts; with the
    /// button let go of unseen, the press is over.
    fn pressed_move(&mut self, event: &MouseMoveEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(press) = &self.press else { return };
        if event.pressed_button != Some(MouseButton::Left) {
            self.press = None;
            cx.notify();
            return;
        }
        let scale = window.scale_factor();
        let (x, y) = colet_core::shell::drag_distance((96. * scale).round() as u32);
        let moved = event.position - press.at;
        if moved.x.abs() * scale <= px(x as f32) && moved.y.abs() * scale <= px(y as f32) {
            return;
        }
        let press = self.press.take().expect("a press");
        self.drag_out(press, window, cx);
    }

    /// Carries the selection out if the pressed row is part of it, or else picks the row and carries it.
    fn drag_out(&mut self, press: Press, window: &mut Window, cx: &mut Context<Self>) {
        let Press { index, path, .. } = press;
        let Some(column) = self.state.columns.get(index) else { return };
        let paths: Vec<PathBuf> = if column.selected.contains(&path) {
            column.shown_selected().iter().map(|p| HomeKey::on_disk(p).into_owned()).collect()
        } else {
            self.pick(index, path.clone(), window, cx);
            vec![HomeKey::on_disk(&path).into_owned()]
        };
        cx.notify();
        let (backend, owner) = (self.backend.clone(), self.owner);
        // Out of this update, on this thread: the drag runs the window's messages until it ends, and
        // they draw the window and reach the app meanwhile.
        cx.spawn(async move |this, cx| {
            if let Err(error) = backend.drag_out(&paths, owner) {
                this.update(cx, |this, cx| this.notify(format!("ドラッグできません: {error}"), Tone::Error, cx)).ok();
            }
        })
        .detach();
    }

    /// While a row is pressed, hears the pointer anywhere, in or out of the window, until the button is
    /// let go of or the drag starts.
    pub(crate) fn press_listener(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.press.as_ref()?;
        let view = cx.entity().downgrade();
        let listen = move |_: Bounds<Pixels>, _: (), window: &mut Window, _: &mut App| {
            let moved = view.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    moved.update(cx, |this, cx| this.pressed_move(event, window, cx)).ok();
                }
            });
            let up = view.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    up.update(cx, |this, cx| {
                        if this.press.take().is_some() {
                            cx.notify();
                        }
                    })
                    .ok();
                }
            });
        };
        Some(canvas(|_, _, _| {}, listen).absolute().size_0().into_any_element())
    }
}
