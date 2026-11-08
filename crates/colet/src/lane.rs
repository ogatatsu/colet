//! Work run on a thread of its own, one piece after another in the order sent.
use std::sync::mpsc;

/// Runs what is sent to it on a thread of its own, one after another in the order sent, so an earlier
/// piece never lands over a later one.
pub struct Lane(mpsc::Sender<Box<dyn FnOnce() + Send>>);

impl Default for Lane {
    fn default() -> Self {
        let (work, queued) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        // Ends once the lane is dropped and what was sent before is done.
        std::thread::spawn(move || {
            for work in queued {
                work();
            }
        });
        Self(work)
    }
}

impl Lane {
    /// Queues `work` behind what was sent before; the receiver hears how it went.
    pub fn run(&self, work: impl FnOnce() -> Result<(), String> + Send + 'static) -> mpsc::Receiver<Result<(), String>> {
        let (done, result) = mpsc::channel();
        let _ = self.0.send(Box::new(move || {
            let _ = done.send(work());
        }));
        result
    }
}
