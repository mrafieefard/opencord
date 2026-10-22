//! A channel's queue and its rules (Phase 3 plan §14.1–14.2): what plays,
//! what comes next, history, repeat and shuffle. Pure: the session decides
//! what Lavalink does about it.

use std::collections::VecDeque;

use opencord_proto::v1 as proto;
use serde::{Deserialize, Serialize};

use super::tracks::Track;

/// History kept (§14.1).
pub const HISTORY: usize = 50;
/// Previous restarts the track when it is further in than this.
pub const RESTART_AFTER_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub track: Track,
    pub requester_id: i64,
    pub added_at_ms: i64,
}

impl Item {
    pub fn to_proto(&self) -> proto::MusicQueueItem {
        proto::MusicQueueItem {
            item_id: self.id.clone(),
            track: Some(self.track.to_proto()),
            requester_id: self.requester_id,
            added_at_ms: self.added_at_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Repeat {
    #[default]
    Off,
    Queue,
    Track,
}

impl Repeat {
    pub fn from_proto(repeat: i32) -> Option<Self> {
        match proto::MusicRepeat::try_from(repeat).ok()? {
            proto::MusicRepeat::Off => Some(Self::Off),
            proto::MusicRepeat::Queue => Some(Self::Queue),
            proto::MusicRepeat::Track => Some(Self::Track),
            proto::MusicRepeat::Unspecified => None,
        }
    }

    pub fn to_proto(self) -> proto::MusicRepeat {
        match self {
            Self::Off => proto::MusicRepeat::Off,
            Self::Queue => proto::MusicRepeat::Queue,
            Self::Track => proto::MusicRepeat::Track,
        }
    }
}

/// Where added items go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    End,
    /// Before this item, or at the end when it is gone.
    Before(String),
    Next,
    /// Replaces what plays.
    Now,
}

/// Why the current item ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// It played to the end: repeat applies.
    Finished,
    /// Someone skipped it.
    Skipped,
    /// It could not be played.
    Failed,
}

/// What Previous did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Previous {
    /// The current track starts again.
    Restart,
    /// The last history item plays; the current one is next.
    Back,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Queue {
    pub now: Option<Item>,
    /// Upcoming, in play order.
    pub upcoming: Vec<Item>,
    /// Most recent first.
    pub history: VecDeque<Item>,
    pub repeat: Repeat,
    pub shuffle: bool,
    /// While shuffled: the upcoming items' ids in the order they were added,
    /// to go back to.
    original: Vec<String>,
}

impl Queue {
    /// Upcoming items and the one playing.
    pub fn len(&self) -> usize {
        self.upcoming.len() + usize::from(self.now.is_some())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Items `user_id` added that have not played yet, the current one
    /// included.
    pub fn count_by(&self, user_id: i64) -> usize {
        self.now
            .iter()
            .chain(&self.upcoming)
            .filter(|item| item.requester_id == user_id)
            .count()
    }

    pub fn upcoming_item(&self, id: &str) -> Option<&Item> {
        self.upcoming.iter().find(|item| item.id == id)
    }

    fn position(&self, id: &str) -> Option<usize> {
        self.upcoming.iter().position(|item| item.id == id)
    }

    /// Adds items. Returns whether one started playing: nothing played, or
    /// `Now` replaced it.
    pub fn add(&mut self, items: Vec<Item>, placement: &Placement) -> bool {
        let mut items = items.into_iter();
        let mut started = false;
        if *placement == Placement::Now || self.now.is_none() {
            let Some(first) = items.next() else {
                return false;
            };
            if let Some(old) = self.now.replace(first) {
                self.remember(old);
            }
            started = true;
        }
        let rest: Vec<Item> = items.collect();
        if self.shuffle {
            self.original
                .extend(rest.iter().map(|item| item.id.clone()));
        }
        let at = match placement {
            Placement::End => self.upcoming.len(),
            Placement::Before(id) => self.position(id).unwrap_or(self.upcoming.len()),
            Placement::Next | Placement::Now => 0,
        };
        self.upcoming.splice(at..at, rest);
        started
    }

    /// The current item ends; returns whether another plays now.
    pub fn advance(&mut self, ended: Ended) -> bool {
        if ended == Ended::Finished && self.repeat == Repeat::Track && self.now.is_some() {
            return true;
        }
        if let Some(old) = self.now.take() {
            if self.repeat == Repeat::Queue && ended != Ended::Failed {
                let mut again = old.clone();
                // A new id, so moves and removes name exactly one item.
                again.id = format!("{}r", old.id);
                if self.shuffle {
                    self.original.push(again.id.clone());
                }
                self.upcoming.push(again);
            }
            self.remember(old);
        }
        if self.upcoming.is_empty() {
            return false;
        }
        let next = self.upcoming.remove(0);
        self.original.retain(|id| *id != next.id);
        self.now = Some(next);
        true
    }

    /// Previous (§14.2): further than 5 s in, the track restarts; else the
    /// last history item plays and the current one goes back to the front.
    pub fn previous(&mut self, position_ms: u64) -> Previous {
        if position_ms > RESTART_AFTER_MS || self.history.is_empty() {
            return Previous::Restart;
        }
        let back = self.history.pop_front().expect("history is not empty");
        if let Some(current) = self.now.replace(back) {
            if self.shuffle {
                self.original.insert(0, current.id.clone());
            }
            self.upcoming.insert(0, current);
        }
        Previous::Back
    }

    /// Puts `id` before `before`, or at the end (§14.2: moves are relative,
    /// so two people moving at once never corrupt the order). False when
    /// `id` is not upcoming.
    pub fn move_item(&mut self, id: &str, before: Option<&str>) -> bool {
        if before == Some(id) {
            return self.position(id).is_some();
        }
        let Some(from) = self.position(id) else {
            return false;
        };
        let item = self.upcoming.remove(from);
        let to = before
            .and_then(|before| self.position(before))
            .unwrap_or(self.upcoming.len());
        self.upcoming.insert(to, item);
        if self.shuffle {
            // Where it was put counts in the order kept too.
            self.original.retain(|kept| kept != id);
            let to = before
                .and_then(|before| self.original.iter().position(|kept| kept == before))
                .unwrap_or(self.original.len());
            self.original.insert(to, id.to_owned());
        }
        true
    }

    /// Removes upcoming items; returns those removed.
    pub fn remove(&mut self, ids: &[String]) -> Vec<Item> {
        let (removed, kept): (Vec<Item>, Vec<Item>) = std::mem::take(&mut self.upcoming)
            .into_iter()
            .partition(|item| ids.contains(&item.id));
        self.upcoming = kept;
        self.original.retain(|id| !ids.contains(id));
        removed
    }

    /// Empties the upcoming items; what plays goes on.
    pub fn clear(&mut self) {
        self.upcoming.clear();
        self.original.clear();
    }

    /// Shuffle on: the upcoming order is randomised and the old one kept.
    /// Off: the remaining items go back to the order they were added in.
    pub fn set_shuffle(&mut self, on: bool, random: &mut impl FnMut(usize) -> usize) {
        if on == self.shuffle {
            return;
        }
        self.shuffle = on;
        if on {
            self.original = self.upcoming.iter().map(|item| item.id.clone()).collect();
            // Fisher–Yates.
            for i in (1..self.upcoming.len()).rev() {
                let j = random(i + 1);
                self.upcoming.swap(i, j);
            }
        } else {
            let original = std::mem::take(&mut self.original);
            let rank = |item: &Item| {
                original
                    .iter()
                    .position(|id| *id == item.id)
                    .unwrap_or(usize::MAX)
            };
            self.upcoming.sort_by_key(rank);
        }
    }

    fn remember(&mut self, item: Item) {
        self.history.push_front(item);
        self.history.truncate(HISTORY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::music::tracks::TrackInfo;

    fn item(id: &str) -> Item {
        Item {
            id: id.to_owned(),
            track: Track {
                encoded: format!("enc-{id}"),
                info: TrackInfo {
                    identifier: id.to_owned(),
                    title: id.to_owned(),
                    author: String::new(),
                    length_ms: 1000,
                    is_stream: false,
                    is_seekable: true,
                    uri: String::new(),
                    artwork_url: String::new(),
                    source_name: "youtube".to_owned(),
                },
            },
            requester_id: 1,
            added_at_ms: 0,
        }
    }

    fn items(ids: &[&str]) -> Vec<Item> {
        ids.iter().map(|id| item(id)).collect()
    }

    fn ids(queue: &Queue) -> Vec<&str> {
        queue.upcoming.iter().map(|item| item.id.as_str()).collect()
    }

    fn now(queue: &Queue) -> Option<&str> {
        queue.now.as_ref().map(|item| item.id.as_str())
    }

    #[test]
    fn adding_when_idle_plays_at_once_and_the_rest_waits() {
        let mut queue = Queue::default();
        assert!(queue.add(items(&["a", "b"]), &Placement::End));
        assert_eq!(now(&queue), Some("a"));
        assert!(!queue.add(items(&["c"]), &Placement::End));
        assert_eq!(ids(&queue), ["b", "c"]);
        assert!(!queue.add(items(&["d"]), &Placement::Next));
        assert_eq!(ids(&queue), ["d", "b", "c"]);
        assert!(!queue.add(items(&["e"]), &Placement::Before("c".to_owned())));
        assert_eq!(ids(&queue), ["d", "b", "e", "c"]);
        assert!(!queue.add(items(&["f"]), &Placement::Before("gone".to_owned())));
        assert_eq!(ids(&queue), ["d", "b", "e", "c", "f"]);
        assert_eq!(queue.len(), 6);
    }

    #[test]
    fn play_now_replaces_the_current_track_which_goes_to_history() {
        let mut queue = Queue::default();
        queue.add(items(&["a", "b"]), &Placement::End);
        assert!(queue.add(items(&["x", "y"]), &Placement::Now));
        assert_eq!(now(&queue), Some("x"));
        assert_eq!(ids(&queue), ["y", "b"]);
        assert_eq!(queue.history[0].id, "a");
    }

    #[test]
    fn tracks_end_into_history_and_the_queue_runs_out() {
        let mut queue = Queue::default();
        queue.add(items(&["a", "b"]), &Placement::End);
        assert!(queue.advance(Ended::Finished));
        assert_eq!(now(&queue), Some("b"));
        assert!(!queue.advance(Ended::Finished));
        assert_eq!(now(&queue), None);
        let history: Vec<&str> = queue.history.iter().map(|item| item.id.as_str()).collect();
        assert_eq!(history, ["b", "a"]);
    }

    #[test]
    fn repeat_track_replays_until_skipped() {
        let mut queue = Queue {
            repeat: Repeat::Track,
            ..Queue::default()
        };
        queue.add(items(&["a", "b"]), &Placement::End);
        assert!(queue.advance(Ended::Finished));
        assert_eq!(now(&queue), Some("a"));
        assert!(queue.advance(Ended::Skipped));
        assert_eq!(now(&queue), Some("b"));
    }

    #[test]
    fn repeat_queue_puts_finished_items_at_the_end_but_not_failed_ones() {
        let mut queue = Queue {
            repeat: Repeat::Queue,
            ..Queue::default()
        };
        queue.add(items(&["a", "b"]), &Placement::End);
        queue.advance(Ended::Finished);
        assert_eq!(now(&queue), Some("b"));
        assert_eq!(ids(&queue), ["ar"]);
        queue.advance(Ended::Skipped);
        assert_eq!(now(&queue), Some("ar"));
        assert_eq!(ids(&queue), ["br"]);
        queue.advance(Ended::Failed);
        assert_eq!(now(&queue), Some("br"));
        assert!(
            queue.upcoming.is_empty(),
            "the failed one does not come back"
        );
    }

    #[test]
    fn previous_restarts_after_five_seconds_and_goes_back_before() {
        let mut queue = Queue::default();
        queue.add(items(&["a", "b", "c"]), &Placement::End);
        assert_eq!(queue.previous(0), Previous::Restart, "no history yet");
        queue.advance(Ended::Finished);
        assert_eq!(queue.previous(6_000), Previous::Restart);
        assert_eq!(now(&queue), Some("b"));
        assert_eq!(queue.previous(4_000), Previous::Back);
        assert_eq!(now(&queue), Some("a"));
        assert_eq!(ids(&queue), ["b", "c"]);
        assert!(queue.history.is_empty());
    }

    #[test]
    fn relative_moves_keep_every_item_once() {
        let mut queue = Queue::default();
        queue.add(items(&["now", "a", "b", "c", "d"]), &Placement::End);
        assert!(queue.move_item("d", Some("a")));
        assert_eq!(ids(&queue), ["d", "a", "b", "c"]);
        assert!(queue.move_item("d", None));
        assert_eq!(ids(&queue), ["a", "b", "c", "d"]);
        assert!(
            queue.move_item("a", Some("gone")),
            "a gone target means the end"
        );
        assert_eq!(ids(&queue), ["b", "c", "d", "a"]);
        assert!(queue.move_item("b", Some("b")));
        assert!(!queue.move_item("now", None), "what plays is not upcoming");
        // Two people drag at once: whatever order the moves come in, each
        // item is there once.
        queue.move_item("c", Some("a"));
        queue.move_item("a", Some("c"));
        let mut sorted = ids(&queue);
        sorted.sort_unstable();
        assert_eq!(sorted, ["a", "b", "c", "d"]);
    }

    #[test]
    fn removing_and_clearing_leave_what_plays() {
        let mut queue = Queue::default();
        queue.add(items(&["now", "a", "b", "c"]), &Placement::End);
        let removed = queue.remove(&["a".to_owned(), "c".to_owned(), "now".to_owned()]);
        assert_eq!(removed.len(), 2);
        assert_eq!(ids(&queue), ["b"]);
        queue.clear();
        assert!(queue.upcoming.is_empty());
        assert_eq!(now(&queue), Some("now"));
    }

    #[test]
    fn shuffle_off_restores_the_order_items_were_added_in() {
        let mut queue = Queue::default();
        queue.add(items(&["now", "a", "b", "c", "d", "e"]), &Placement::End);
        let mut picks = [0usize, 1, 0, 2].into_iter().cycle();
        queue.set_shuffle(true, &mut |_| picks.next().unwrap());
        assert_ne!(ids(&queue), ["a", "b", "c", "d", "e"], "shuffled");
        queue.add(items(&["f"]), &Placement::End);
        queue.remove(&["c".to_owned()]);
        queue.advance(Ended::Finished);
        queue.set_shuffle(false, &mut |_| 0);
        let played = now(&queue).unwrap().to_owned();
        let expected: Vec<&str> = ["a", "b", "d", "e", "f"]
            .into_iter()
            .filter(|id| *id != played)
            .collect();
        assert_eq!(ids(&queue), expected);
    }

    #[test]
    fn counting_and_history_limits() {
        let mut queue = Queue::default();
        let mut mine = items(&["a", "b"]);
        mine[1].requester_id = 2;
        queue.add(mine, &Placement::End);
        assert_eq!(queue.count_by(1), 1);
        assert_eq!(queue.count_by(2), 1);
        for i in 0..60 {
            queue.add(vec![item(&format!("x{i}"))], &Placement::Now);
        }
        assert_eq!(queue.history.len(), HISTORY);
    }
}
