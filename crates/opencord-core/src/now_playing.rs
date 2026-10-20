//! Tracks what audio or media is currently playing locally (Milestone A6, Task 29).

use std::sync::{Mutex, PoisonError};

pub(crate) trait Stoppable: Send {
    fn stop(&self);
}

/// Which player holds the slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Player {
    Voice,
    Media,
}

#[derive(Default)]
pub(crate) struct NowPlaying {
    current: Mutex<Option<(Player, Box<dyn Stoppable>)>>,
}

impl NowPlaying {
    /// `player` starts playing: whatever played before stops.
    pub fn replace(&self, player: Player, next: Box<dyn Stoppable>) {
        let previous = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            (*current).replace((player, next))
        };
        if let Some((_, previous)) = previous {
            previous.stop();
        }
    }

    /// `player` plays on: when it already holds the slot nothing changes,
    /// else it takes the slot with what `make` returns and whatever played
    /// before stops.
    pub fn claim(&self, player: Player, make: impl FnOnce() -> Box<dyn Stoppable>) {
        let previous = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            if current
                .as_ref()
                .is_some_and(|(holder, _)| *holder == player)
            {
                return;
            }
            (*current).replace((player, make()))
        };
        if let Some((_, previous)) = previous {
            previous.stop();
        }
    }

    /// `player` stopped: the slot empties, unless another player holds it.
    pub fn clear(&self, player: Player) {
        let previous = {
            let mut current = self.current.lock().unwrap_or_else(PoisonError::into_inner);
            if current
                .as_ref()
                .is_some_and(|(holder, _)| *holder == player)
            {
                current.take()
            } else {
                None
            }
        };
        if let Some((_, previous)) = previous {
            previous.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    struct Flag(Arc<AtomicBool>);

    impl Stoppable for Flag {
        fn stop(&self) {
            self.0.store(true, Ordering::Relaxed);
        }
    }

    #[test]
    fn a_player_that_stops_leaves_the_other_playing() {
        let now_playing = NowPlaying::default();
        let media = Arc::new(AtomicBool::new(false));
        now_playing.replace(Player::Media, Box::new(Flag(media.clone())));
        now_playing.clear(Player::Voice);
        assert!(!media.load(Ordering::Relaxed));
        now_playing.clear(Player::Media);
        assert!(media.load(Ordering::Relaxed));
    }

    /// Media played again after a pause, or after its end, is the same
    /// player going on: stopping the one before would pause it at once.
    #[test]
    fn playing_again_leaves_the_same_player_going() {
        let now_playing = NowPlaying::default();
        let media = Arc::new(AtomicBool::new(false));
        now_playing.claim(Player::Media, || Box::new(Flag(media.clone())));
        now_playing.claim(Player::Media, || Box::new(Flag(media.clone())));
        assert!(!media.load(Ordering::Relaxed));

        let voice = Arc::new(AtomicBool::new(false));
        now_playing.replace(Player::Voice, Box::new(Flag(voice.clone())));
        assert!(media.load(Ordering::Relaxed));
        media.store(false, Ordering::Relaxed);
        now_playing.claim(Player::Media, || Box::new(Flag(media.clone())));
        assert!(voice.load(Ordering::Relaxed));
        assert!(!media.load(Ordering::Relaxed));
    }
}
