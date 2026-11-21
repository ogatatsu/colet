//! What brings the window round to take in what changed off its thread: the folders watched, the watch on
//! the search, the running operation's items and the clipboard. Each tells it as it happens, from any
//! thread, and the window sleeps until one does.

/// Tells the window something changed. Any number told before it looks count as one.
#[derive(Clone)]
pub struct Wake(async_channel::Sender<()>);

/// What the window waits on.
pub struct Woken(async_channel::Receiver<()>);

pub fn wake() -> (Wake, Woken) {
    let (wake, woken) = async_channel::bounded(1);
    (Wake(wake), Woken(woken))
}

impl Wake {
    pub fn wake(&self) {
        // Full, it is already to look; closed, the window is gone.
        let _ = self.0.try_send(());
    }
}

impl Woken {
    /// Waits until told; false once nothing is left to tell it.
    pub async fn told(&self) -> bool {
        self.0.recv().await.is_ok()
    }
}
