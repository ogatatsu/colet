//! Work run on a thread of its own, one piece after another in the order sent.
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc;

/// Runs what is sent to it on a thread of its own, one after another in the order sent, so an earlier
/// piece never lands over a later one.
pub struct Lane(mpsc::Sender<Box<dyn FnOnce() + Send>>);

impl Default for Lane {
    fn default() -> Self {
        let (work, queued) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        // Ends once the lane is dropped and what was sent before is done. A piece that panics is let go
        // of, its receiver hearing nothing, and the pieces after it still run.
        std::thread::spawn(move || {
            for work in queued {
                let _ = catch_unwind(AssertUnwindSafe(work));
            }
        });
        Self(work)
    }
}

impl Lane {
    /// Queues `work` behind what was sent before; the receiver hears how it went, or is disconnected
    /// when it panicked.
    pub fn run(&self, work: impl FnOnce() -> Result<(), String> + Send + 'static) -> mpsc::Receiver<Result<(), String>> {
        let (done, result) = mpsc::channel();
        let _ = self.0.send(Box::new(move || {
            let _ = done.send(work());
        }));
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_running_what_comes_after_a_piece_that_panics() {
        let lane = Lane::default();
        let panicked = lane.run(|| panic!("broken"));
        let after = lane.run(|| Ok(()));
        assert!(panicked.recv().is_err());
        assert_eq!(after.recv(), Ok(Ok(())));
    }
}
