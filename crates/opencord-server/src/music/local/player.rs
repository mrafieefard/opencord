//! The server's own player (design 2026-10-24 §5.4): plays an uploaded song
//! into a voice channel. A thread decodes and encodes a little ahead; a
//! task sends a frame every 20 ms to the node serving the channel, the
//! embedded one directly and an external one over its control channel.
//! Dropping the player stops it.

use std::sync::Arc;
use std::time::{Duration, Instant};

use opencord_voice::node::NodeCommand;
use opencord_voice::sfu::MusicPacket;
use tokio::sync::{mpsc, watch};

use super::decode::{FRAME, SongDecoder};
use crate::music::session::Command;
use crate::state::AppState;
use crate::voice::nodes::MusicTarget;

/// Frames decoded ahead of time: half a second.
const AHEAD: usize = 25;
const FRAME_TIME: Duration = Duration::from_millis(20);
/// Further behind than this, the music skips ahead instead of catching up
/// in a burst.
const BEHIND_MAX: Duration = Duration::from_millis(200);

/// What the player tells the channel's session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The first frame went out.
    Started,
    /// The song ended; with a reason when it couldn't be played.
    Ended { failed: Option<String> },
}

/// Said when a queued song was removed from the server.
pub const REMOVED: &str = "This song was removed from the server";

/// A song playing in a channel; it stops when dropped.
#[derive(Debug)]
pub struct LocalPlayer {
    paused: watch::Sender<bool>,
}

impl LocalPlayer {
    /// Plays `song_id` in `channel_id` from `position_ms`. Its events go to
    /// the channel's session, marked with `generation`.
    pub fn start(
        state: &Arc<AppState>,
        channel_id: i64,
        song_id: i64,
        position_ms: u64,
        paused: bool,
        generation: u64,
    ) -> Self {
        let (sender, receiver) = watch::channel(paused);
        let state = Arc::clone(state);
        tokio::spawn(async move {
            let tell = |event| {
                state
                    .music
                    .send_existing(channel_id, Command::Local { generation, event });
            };
            let failed = run(&state, channel_id, song_id, position_ms, receiver, &tell).await;
            if let Some(ended) = failed {
                tell(Event::Ended { failed: ended });
            }
        });
        Self { paused: sender }
    }

    pub fn set_paused(&self, paused: bool) {
        self.paused.send_replace(paused);
    }
}

/// Plays until the song ends (`Some(None)`), fails (`Some(Some(reason))`)
/// or the player is dropped (`None`).
async fn run(
    state: &Arc<AppState>,
    channel_id: i64,
    song_id: i64,
    position_ms: u64,
    mut paused: watch::Receiver<bool>,
    tell: &impl Fn(Event),
) -> Option<Option<String>> {
    let file = match super::stored_copy(state, song_id).await {
        Ok(Some(file)) => file,
        Ok(None) => return Some(Some(REMOVED.to_owned())),
        Err(_) => return Some(Some("its file could not be read".to_owned())),
    };
    let (frames_in, mut frames) = mpsc::channel::<Result<Vec<u8>, String>>(AHEAD);
    let decoding = std::thread::Builder::new()
        .name(format!("music-{channel_id}"))
        // The copy goes when the thread is done with it.
        .spawn(move || {
            let mut decoder = match SongDecoder::open(file.path(), position_ms) {
                Ok(decoder) => decoder,
                Err(error) => {
                    let _ = frames_in.blocking_send(Err(error.to_string()));
                    return;
                }
            };
            loop {
                let frame = match decoder.next_frame() {
                    Ok(Some(frame)) => Ok(frame),
                    Ok(None) => return,
                    Err(error) => Err(error.to_string()),
                };
                let last = frame.is_err();
                // The player was dropped: nobody listens any more.
                if frames_in.blocking_send(frame).is_err() || last {
                    return;
                }
            }
        });
    if decoding.is_err() {
        return Some(Some("no thread could be started for it".to_owned()));
    }

    let mut stream = Stream::new(channel_id);
    let mut next = Instant::now();
    let mut started = false;
    loop {
        if *paused.borrow_and_update() {
            paused.changed().await.ok()?;
            next = Instant::now();
            continue;
        }
        let frame = tokio::select! {
            biased;
            changed = paused.changed() => {
                changed.ok()?;
                continue;
            }
            frame = frames.recv() => frame,
        };
        let opus = match frame {
            None => return Some(None),
            Some(Err(reason)) => return Some(Some(reason)),
            Some(Ok(opus)) => opus,
        };
        tokio::select! {
            biased;
            changed = paused.changed() => changed.ok()?,
            () = tokio::time::sleep_until(next.into()) => {}
        }
        stream.send(state, opus);
        if !started {
            started = true;
            tell(Event::Started);
        }
        next += FRAME_TIME;
        let now = Instant::now();
        if now.saturating_duration_since(next) > BEHIND_MAX {
            next = now;
        }
    }
}

/// The numbering of the frames a player sends: a connection of its own, so
/// the node carries on the channel's stream from where it was.
struct Stream {
    channel_id: i64,
    connection: u64,
    sequence: u16,
    timestamp: u32,
}

impl Stream {
    fn new(channel_id: i64) -> Self {
        Self {
            channel_id,
            connection: u64::from_le_bytes(crate::random::bytes()),
            sequence: u16::from_le_bytes(crate::random::bytes()),
            timestamp: u32::from_le_bytes(crate::random::bytes()),
        }
    }

    fn send(&mut self, state: &AppState, opus: Vec<u8>) {
        let opus: Arc<[u8]> = opus.into();
        match state.voice_nodes.music_target(self.channel_id) {
            Some(MusicTarget::Embedded) => {
                if let Some(node) = state.voice_nodes.embedded() {
                    node.music_frame(
                        self.channel_id,
                        MusicPacket {
                            connection: self.connection,
                            sequence: self.sequence,
                            timestamp: self.timestamp,
                            opus,
                            received_at: Instant::now(),
                        },
                    );
                }
            }
            Some(MusicTarget::External { .. }) => state.voice_nodes.send(
                self.channel_id,
                NodeCommand::MusicFrame {
                    channel_id: self.channel_id,
                    connection: self.connection,
                    sequence: self.sequence,
                    timestamp: self.timestamp,
                    opus,
                },
            ),
            None => {}
        }
        self.sequence = self.sequence.wrapping_add(1);
        self.timestamp = self.timestamp.wrapping_add(FRAME as u32);
    }
}
