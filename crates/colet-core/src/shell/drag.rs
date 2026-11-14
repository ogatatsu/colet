//! Items dragged out of Colet's window into another app, as Explorer drags them: the shell's own data
//! object, its drag image, and whatever the target makes of it, copied, moved or linked.
use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Ole::{DROPEFFECT_COPY, DROPEFFECT_LINK, DROPEFFECT_MOVE, IDropSource, RevokeDragDrop};
use windows::Win32::UI::HiDpi::GetSystemMetricsForDpi;
use windows::Win32::UI::Shell::{IDataObjectAsyncCapability, SHDoDragDrop};
use windows::Win32::UI::WindowsAndMessaging::{SM_CXDRAG, SM_CYDRAG};
use windows::core::{Interface, Result};

use super::Owner;

/// Drags the items at `paths`, from any folders, out of the window `owner` until the button is let go
/// of, or the drag put down with Esc: the target chooses to copy, move or link them, by the keys held and
/// the drive, as from Explorer. The shell draws the drag image. An Explorer target writes the items out
/// on a thread of its own, so this returns as they are dropped, before they are all copied.
///
/// Call it on the thread that owns `owner`, with its mouse button down and OLE entered there, and not
/// from within anything the window's messages call back into: the drag runs that thread's messages
/// until it ends. What it returns tells, on the same thread, whether the target is still writing them out.
pub fn drag_out(paths: &[PathBuf], owner: Owner) -> Result<Dragged> {
    let data = super::drop::items(paths)?;
    // An Explorer that may copy on its own thread does, and the drag returns at once.
    let asynchronous = data.cast::<IDataObjectAsyncCapability>().ok().filter(|a| unsafe { a.SetAsyncMode(true) }.is_ok());
    let allowed = DROPEFFECT_COPY | DROPEFFECT_MOVE | DROPEFFECT_LINK;
    unsafe { SHDoDragDrop(Some(HWND(owner as *mut _)), &data, None::<&IDropSource>, allowed) }?;
    Ok(Dragged(asynchronous))
}

/// Items dragged out and dropped, which the target may still be writing out on a thread of its own.
pub struct Dragged(Option<IDataObjectAsyncCapability>);

impl Dragged {
    /// The target is still copying or moving them: until it is done, a folder among them is to stay
    /// unwatched, or a move would fail on it.
    pub fn writing(&self) -> bool {
        self.0.as_ref().is_some_and(|a| unsafe { a.InOperation() }.is_ok_and(|b| b.as_bool()))
    }
}

/// How far the pointer moves, held down, before a drag begins, in pixels at `dpi` along each axis, as
/// Windows' own drags do: out of a box this wide and high centered on where it was pressed.
pub fn drag_distance(dpi: u32) -> (i32, i32) {
    unsafe { ((GetSystemMetricsForDpi(SM_CXDRAG, dpi) / 2).max(1), (GetSystemMetricsForDpi(SM_CYDRAG, dpi) / 2).max(1)) }
}

/// Takes nothing dropped on the window `owner`: Colet opens nothing dropped on it, so a drag over it,
/// from Explorer or from Colet's own rows, shows that it cannot be let go of there.
pub fn refuse_drops(owner: Owner) {
    if owner != 0 {
        let _ = unsafe { RevokeDragDrop(HWND(owner as *mut _)) };
    }
}
