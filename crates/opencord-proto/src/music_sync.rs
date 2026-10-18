//! A music session's changes (Phase 3 plan §14.3): after the first
//! snapshot, the server sends what changed, and clients apply it. Lists
//! change by item id: removals, then items placed before others, new ones
//! whole. Moving one track sends one placement, not the queue.

use std::collections::{HashMap, HashSet};

use crate::v1::{
    MusicListChange, MusicListPlacement, MusicNowPlayingChange, MusicQueueItem, MusicSession,
    MusicUpdate,
};

/// An update that does not follow the session at hand: something was
/// missed, and the whole session is needed again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Missed;

/// What changed from `old` to `new`, both of the same channel.
pub fn diff(old: &MusicSession, new: &MusicSession) -> MusicUpdate {
    MusicUpdate {
        channel_id: new.channel_id,
        base_version: old.version,
        version: new.version,
        state: new.state,
        position_ms: new.position_ms,
        server_time_ms: new.server_time_ms,
        repeat: new.repeat,
        shuffle: new.shuffle,
        waiting_for_node: new.waiting_for_node,
        paused_empty: new.paused_empty,
        skip_votes: new.skip_votes.clone(),
        skip_votes_needed: new.skip_votes_needed,
        now_playing: (old.now_playing != new.now_playing).then(|| MusicNowPlayingChange {
            item: new.now_playing.clone(),
        }),
        queue: Some(diff_list(&old.queue, &new.queue)),
        history: Some(diff_list(&old.history, &new.history)),
    }
}

/// Applies `update` to `session`. `Ok(false)`: it is older than the
/// session, and ignored. A session left unchanged by a failed update.
pub fn apply(session: &mut MusicSession, update: &MusicUpdate) -> Result<bool, Missed> {
    if update.channel_id != session.channel_id {
        return Err(Missed);
    }
    if update.version <= session.version {
        return Ok(false);
    }
    if update.base_version != session.version {
        return Err(Missed);
    }
    let mut queue = session.queue.clone();
    let mut history = session.history.clone();
    apply_list(&mut queue, update.queue.as_ref())?;
    apply_list(&mut history, update.history.as_ref())?;
    session.queue = queue;
    session.history = history;
    if let Some(change) = &update.now_playing {
        session.now_playing.clone_from(&change.item);
    }
    session.version = update.version;
    session.state = update.state;
    session.position_ms = update.position_ms;
    session.server_time_ms = update.server_time_ms;
    session.repeat = update.repeat;
    session.shuffle = update.shuffle;
    session.waiting_for_node = update.waiting_for_node;
    session.paused_empty = update.paused_empty;
    session.skip_votes.clone_from(&update.skip_votes);
    session.skip_votes_needed = update.skip_votes_needed;
    Ok(true)
}

/// The fewest placements, more or less: items whose order did not change
/// (the longest run kept in order) stay; the others are placed, from the
/// end, each before the item that follows it.
fn diff_list(old: &[MusicQueueItem], new: &[MusicQueueItem]) -> MusicListChange {
    if has_repeats(old) || has_repeats(new) {
        return replace_list(old, new);
    }
    let in_new: HashMap<&str, (usize, &MusicQueueItem)> = new
        .iter()
        .enumerate()
        .map(|(index, item)| (item.item_id.as_str(), (index, item)))
        .collect();
    // Kept: the same id with the same item.
    let mut removed = Vec::new();
    let mut kept = Vec::new();
    for item in old {
        match in_new.get(item.item_id.as_str()) {
            Some((index, now)) if *now == item => kept.push(*index),
            _ => removed.push(item.item_id.clone()),
        }
    }
    let stable: HashSet<usize> = longest_increasing(&kept).into_iter().collect();
    let kept: HashSet<usize> = kept.into_iter().collect();
    let placed = (0..new.len())
        .rev()
        .filter(|index| !stable.contains(index))
        .map(|index| MusicListPlacement {
            item_id: new[index].item_id.clone(),
            item: (!kept.contains(&index)).then(|| new[index].clone()),
            before_item_id: new.get(index + 1).map(|next| next.item_id.clone()),
        })
        .collect();
    MusicListChange { removed, placed }
}

/// Everything out, everything in: for lists an id appears in twice.
fn replace_list(old: &[MusicQueueItem], new: &[MusicQueueItem]) -> MusicListChange {
    MusicListChange {
        removed: old.iter().map(|item| item.item_id.clone()).collect(),
        placed: new
            .iter()
            .map(|item| MusicListPlacement {
                item_id: item.item_id.clone(),
                item: Some(item.clone()),
                before_item_id: None,
            })
            .collect(),
    }
}

fn has_repeats(list: &[MusicQueueItem]) -> bool {
    let mut seen = HashSet::new();
    !list.iter().all(|item| seen.insert(item.item_id.as_str()))
}

fn apply_list(
    list: &mut Vec<MusicQueueItem>,
    change: Option<&MusicListChange>,
) -> Result<(), Missed> {
    let Some(change) = change else {
        return Ok(());
    };
    let removed: HashSet<&str> = change.removed.iter().map(String::as_str).collect();
    list.retain(|item| !removed.contains(item.item_id.as_str()));
    for placement in &change.placed {
        // A whole item is new; one by id moves from where it is.
        let item = match &placement.item {
            Some(item) => item.clone(),
            None => list
                .iter()
                .position(|item| item.item_id == placement.item_id)
                .map(|index| list.remove(index))
                .ok_or(Missed)?,
        };
        let at = match &placement.before_item_id {
            Some(before) => list
                .iter()
                .position(|item| item.item_id == *before)
                .ok_or(Missed)?,
            None => list.len(),
        };
        list.insert(at, item);
    }
    Ok(())
}

/// The values of a longest strictly increasing subsequence of `values`.
fn longest_increasing(values: &[usize]) -> Vec<usize> {
    // tails[k]: the index in `values` of the smallest tail of a run of
    // length k + 1; previous[i]: the index before i in its run.
    let mut tails: Vec<usize> = Vec::new();
    let mut previous: Vec<Option<usize>> = vec![None; values.len()];
    for (index, value) in values.iter().enumerate() {
        let length = tails.partition_point(|&tail| values[tail] < *value);
        previous[index] = length.checked_sub(1).map(|before| tails[before]);
        if length == tails.len() {
            tails.push(index);
        } else {
            tails[length] = index;
        }
    }
    let mut run = Vec::with_capacity(tails.len());
    let mut at = tails.last().copied();
    while let Some(index) = at {
        run.push(values[index]);
        at = previous[index];
    }
    run.reverse();
    run
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::v1::MusicTrack;

    fn item(id: &str) -> MusicQueueItem {
        MusicQueueItem {
            item_id: id.to_owned(),
            track: Some(MusicTrack {
                title: format!("track {id}"),
                ..MusicTrack::default()
            }),
            ..MusicQueueItem::default()
        }
    }

    fn session(version: u64, queue: &[&str], history: &[&str]) -> MusicSession {
        MusicSession {
            channel_id: 5,
            version,
            queue: queue.iter().map(|id| item(id)).collect(),
            history: history.iter().map(|id| item(id)).collect(),
            ..MusicSession::default()
        }
    }

    fn ids(list: &[MusicQueueItem]) -> Vec<&str> {
        list.iter().map(|item| item.item_id.as_str()).collect()
    }

    /// A small deterministic generator, so failures repeat.
    struct Rng(u64);

    impl Rng {
        fn next(&mut self, below: usize) -> usize {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            usize::try_from(self.0 % below as u64).unwrap()
        }
    }

    #[test]
    fn moving_one_track_sends_one_placement_without_the_track() {
        let old = session(1, &["a", "b", "c", "d", "e"], &[]);
        let new = session(2, &["a", "d", "b", "c", "e"], &[]);

        let update = diff(&old, &new);

        let queue = update.queue.as_ref().unwrap();
        assert!(queue.removed.is_empty());
        assert_eq!(queue.placed.len(), 1);
        assert_eq!(queue.placed[0].item_id, "d");
        assert_eq!(queue.placed[0].item, None);
        assert_eq!(queue.placed[0].before_item_id.as_deref(), Some("b"));
        assert_eq!(update.now_playing, None);
    }

    #[test]
    fn the_next_track_playing_moves_it_out_of_the_queue_into_now_playing() {
        let mut old = session(1, &["b", "c"], &["z"]);
        old.now_playing = Some(item("a"));
        let mut new = session(2, &["c"], &["a", "z"]);
        new.now_playing = Some(item("b"));

        let update = diff(&old, &new);
        let mut applied = old.clone();

        assert_eq!(apply(&mut applied, &update), Ok(true));
        assert_eq!(applied, new);
        assert_eq!(update.queue.as_ref().unwrap().removed, ["b"]);
        assert_eq!(update.history.as_ref().unwrap().placed.len(), 1);
    }

    #[test]
    fn any_change_applied_gives_the_new_session() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        let names: Vec<String> = (0..40).map(|n| format!("i{n}")).collect();
        for round in 0..2_000 {
            let pick = |rng: &mut Rng| -> Vec<&str> {
                let mut chosen: Vec<&str> = names
                    .iter()
                    .map(String::as_str)
                    .filter(|_| rng.next(3) > 0)
                    .collect();
                for index in (1..chosen.len()).rev() {
                    chosen.swap(index, rng.next(index + 1));
                }
                chosen.truncate(rng.next(names.len() + 1));
                chosen
            };
            let old = session(round, &pick(&mut rng), &pick(&mut rng));
            let mut new = session(round + 1, &pick(&mut rng), &pick(&mut rng));
            // Sometimes an item changes under the same id.
            if let Some(first) = new.queue.first_mut()
                && rng.next(4) == 0
            {
                first.requester_id = 7;
            }

            let update = diff(&old, &new);
            let mut applied = old.clone();

            assert_eq!(apply(&mut applied, &update), Ok(true), "round {round}");
            assert_eq!(ids(&applied.queue), ids(&new.queue), "round {round}");
            assert_eq!(applied, new, "round {round}");
        }
    }

    #[test]
    fn lists_with_an_id_twice_are_sent_whole() {
        let old = session(1, &["a", "a", "b"], &[]);
        let new = session(2, &["b", "a", "a"], &[]);

        let mut applied = old.clone();
        assert_eq!(apply(&mut applied, &diff(&old, &new)), Ok(true));
        assert_eq!(applied, new);
    }

    #[test]
    fn older_updates_are_ignored_and_a_gap_is_missed() {
        let v1 = session(1, &["a"], &[]);
        let v2 = session(2, &["a", "b"], &[]);
        let v3 = session(3, &["b"], &[]);
        let to_v2 = diff(&v1, &v2);
        let to_v3 = diff(&v2, &v3);

        let mut at_v2 = v2.clone();
        assert_eq!(apply(&mut at_v2, &to_v2), Ok(false));
        assert_eq!(at_v2, v2);
        let mut at_v1 = v1.clone();
        assert_eq!(apply(&mut at_v1, &to_v3), Err(Missed));
        assert_eq!(at_v1, v1, "left as it was");
    }

    #[test]
    fn a_placement_that_names_an_unknown_item_is_missed() {
        let mut held = session(1, &["a"], &[]);
        let update = MusicUpdate {
            channel_id: 5,
            base_version: 1,
            version: 2,
            queue: Some(MusicListChange {
                removed: Vec::new(),
                placed: vec![MusicListPlacement {
                    item_id: "x".to_owned(),
                    item: None,
                    before_item_id: None,
                }],
            }),
            ..MusicUpdate::default()
        };

        assert_eq!(apply(&mut held, &update), Err(Missed));
        assert_eq!(held.version, 1);
    }

    #[test]
    fn the_longest_increasing_run_is_found() {
        assert_eq!(longest_increasing(&[3, 1, 2, 5, 4, 6]), [1, 2, 4, 6]);
        assert_eq!(longest_increasing(&[]), Vec::<usize>::new());
        assert_eq!(longest_increasing(&[5, 4, 3]).len(), 1);
    }
}
