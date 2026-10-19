//! Tracks what audio or media is currently playing locally (Milestone A6, Task 29).

use std::sync::{Mutex, PoisonError};

pub(crate) trait Stoppable: Send {
    fn stop(&self);
}

#[derive(Default)]
pub(crate) struct NowPlaying {
    current: Mutex<Option<Box<dyn Stoppable>>>,
}

impl NowPlaying {
    pub fn replace(&self, next: Box<dyn Stoppable>) {
        let previous = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            (*current).replace(next)
        };
        if let Some(prev) = previous {
            prev.stop();
        }
    }

    pub fn clear(&self) {
        let previous = self
            .current
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(prev) = previous {
            prev.stop();
        }
    }
}
