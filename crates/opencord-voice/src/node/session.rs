//! A voice session: one participant's voice gateway state, which outlives
//! its WebSocket for the resume window. Sequenced messages are numbered and
//! kept, so a resumed connection gets what it missed.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bytes::Bytes;
use opencord_proto::voice::v1 as voice;
use prost::Message as _;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;

use crate::sfu::PeerId;

/// Sequenced messages kept for a resume.
const REPLAY_MAX: usize = 512;

/// What the runtime sends a connection's writer.
#[derive(Debug)]
pub enum Outbound {
    Frame(Bytes),
    Close(u16),
}

/// One live WebSocket of a voice session.
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: u64,
    pub outbound: mpsc::Sender<Outbound>,
}

#[derive(Debug)]
pub struct VoiceSession {
    pub id: String,
    resume_token_hash: [u8; 32],
    pub user_id: i64,
    pub channel_id: i64,
    /// The main gateway session that joined.
    pub main_session_id: String,
    pub peer: PeerId,
    pub audio_ssrc: u32,
    pub permissions: u64,
    /// Whether ICE and DTLS came up.
    pub media_connected: bool,
    next_seq: u64,
    replay: VecDeque<(u64, Bytes)>,
    connection: Option<Connection>,
    detached_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the voice session cannot be resumed")]
pub struct ResumeRefused;

pub struct NewSession {
    pub id: String,
    pub resume_token: Vec<u8>,
    pub user_id: i64,
    pub channel_id: i64,
    pub main_session_id: String,
    pub peer: PeerId,
    pub audio_ssrc: u32,
    pub permissions: u64,
    pub connection: Connection,
}

impl VoiceSession {
    pub fn new(new: NewSession) -> Self {
        Self {
            id: new.id,
            resume_token_hash: Sha256::digest(&new.resume_token).into(),
            user_id: new.user_id,
            channel_id: new.channel_id,
            main_session_id: new.main_session_id,
            peer: new.peer,
            audio_ssrc: new.audio_ssrc,
            permissions: new.permissions,
            media_connected: false,
            next_seq: 1,
            replay: VecDeque::new(),
            connection: Some(new.connection),
            detached_at: None,
        }
    }

    /// Sends a message that a resume does not repeat.
    pub fn send(&mut self, payload: voice::envelope::Payload, now: Instant) {
        let frame = encode(0, payload);
        self.deliver(Outbound::Frame(frame), now);
    }

    /// Numbers, keeps and sends a message a resume repeats.
    pub fn send_sequenced(&mut self, payload: voice::envelope::Payload, now: Instant) {
        let seq = self.next_seq;
        self.next_seq += 1;
        let frame = encode(seq, payload);
        self.replay.push_back((seq, frame.clone()));
        while self.replay.len() > REPLAY_MAX {
            self.replay.pop_front();
        }
        self.deliver(Outbound::Frame(frame), now);
    }

    /// Closes the connection, if there is one.
    pub fn close(&mut self, code: u16) {
        if let Some(connection) = self.connection.take() {
            let _ = connection.outbound.try_send(Outbound::Close(code));
        }
    }

    /// The connection is gone, if it is still the current one.
    pub fn detach(&mut self, connection_id: u64, now: Instant) {
        if self
            .connection
            .as_ref()
            .is_some_and(|connection| connection.id == connection_id)
        {
            self.connection = None;
            self.detached_at = Some(now);
        }
    }

    /// Moves the session to `connection`, replaying every sequenced message
    /// after `last_seq`. Returns how many were replayed.
    pub fn resume(
        &mut self,
        resume_token: &[u8],
        last_seq: u64,
        connection: Connection,
        now: Instant,
    ) -> Result<u64, ResumeRefused> {
        let token_hash: [u8; 32] = Sha256::digest(resume_token).into();
        if token_hash != self.resume_token_hash || last_seq >= self.next_seq {
            return Err(ResumeRefused);
        }
        let first_missed = last_seq + 1;
        let missed_any = first_missed < self.next_seq;
        let oldest = self.replay.front().map(|(seq, _)| *seq);
        if missed_any && oldest.is_none_or(|oldest| oldest > first_missed) {
            return Err(ResumeRefused);
        }
        if let Some(previous) = self.connection.replace(connection) {
            let _ = previous.outbound.try_send(Outbound::Close(
                opencord_common::voice::close::SESSION_REPLACED,
            ));
        }
        self.detached_at = None;
        let missed: Vec<Bytes> = self
            .replay
            .iter()
            .filter(|(seq, _)| *seq > last_seq)
            .map(|(_, frame)| frame.clone())
            .collect();
        let replayed = u64::try_from(missed.len()).unwrap_or(u64::MAX);
        for frame in missed {
            self.deliver(Outbound::Frame(frame), now);
        }
        self.send(
            voice::envelope::Payload::Resumed(voice::Resumed { replayed }),
            now,
        );
        Ok(replayed)
    }

    /// Without a connection for longer than `window`.
    pub fn is_expired(&self, now: Instant, window: Duration) -> bool {
        self.detached_at
            .is_some_and(|at| now.saturating_duration_since(at) > window)
    }

    fn deliver(&mut self, outbound: Outbound, now: Instant) {
        let Some(connection) = &self.connection else {
            return;
        };
        match connection.outbound.try_send(outbound) {
            Ok(()) => {}
            // Too slow to keep up: drop the connection, keep the session.
            Err(TrySendError::Full(_)) => {
                let _ = connection.outbound.try_send(Outbound::Close(
                    opencord_common::voice::close::HEARTBEAT_TIMEOUT,
                ));
                self.connection = None;
                self.detached_at = Some(now);
            }
            Err(TrySendError::Closed(_)) => {
                self.connection = None;
                self.detached_at = Some(now);
            }
        }
    }
}

pub fn encode(seq: u64, payload: voice::envelope::Payload) -> Bytes {
    voice::Envelope {
        seq,
        payload: Some(payload),
    }
    .encode_to_vec()
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &[u8] = b"resume";

    fn connection(id: u64) -> (Connection, mpsc::Receiver<Outbound>) {
        let (outbound, frames) = mpsc::channel(64);
        (Connection { id, outbound }, frames)
    }

    fn session(connection: Connection) -> VoiceSession {
        VoiceSession::new(NewSession {
            id: "v1".to_owned(),
            resume_token: TOKEN.to_vec(),
            user_id: 7,
            channel_id: 5,
            main_session_id: "s1".to_owned(),
            peer: 1,
            audio_ssrc: 99,
            permissions: 0,
            connection,
        })
    }

    fn speaking(user_id: i64) -> voice::envelope::Payload {
        voice::envelope::Payload::Speaking(voice::Speaking { flags: 1, user_id })
    }

    fn seqs(frames: &mut mpsc::Receiver<Outbound>) -> Vec<u64> {
        let mut seqs = Vec::new();
        while let Ok(Outbound::Frame(frame)) = frames.try_recv() {
            seqs.push(voice::Envelope::decode(frame).unwrap().seq);
        }
        seqs
    }

    #[test]
    fn sequenced_messages_are_numbered_from_one() {
        let (first, mut frames) = connection(1);
        let mut session = session(first);
        let now = Instant::now();

        session.send_sequenced(speaking(1), now);
        session.send(
            voice::envelope::Payload::HeartbeatAck(voice::HeartbeatAck { nonce: 3 }),
            now,
        );
        session.send_sequenced(speaking(2), now);

        assert_eq!(seqs(&mut frames), [1, 0, 2]);
    }

    #[test]
    fn a_resume_replays_what_was_missed_then_says_resumed() {
        let (first, mut first_frames) = connection(1);
        let mut session = session(first);
        let now = Instant::now();
        session.send_sequenced(speaking(1), now);
        seqs(&mut first_frames);
        session.detach(1, now);
        session.send_sequenced(speaking(2), now);
        session.send_sequenced(speaking(3), now);
        let (second, mut frames) = connection(2);

        let replayed = session.resume(TOKEN, 1, second, now).unwrap();

        assert_eq!(replayed, 2);
        let mut kinds = Vec::new();
        while let Ok(Outbound::Frame(frame)) = frames.try_recv() {
            let envelope = voice::Envelope::decode(frame).unwrap();
            kinds.push((
                envelope.seq,
                matches!(envelope.payload, Some(voice::envelope::Payload::Resumed(_))),
            ));
        }
        assert_eq!(kinds, [(2, false), (3, false), (0, true)]);
    }

    #[test]
    fn a_wrong_token_or_a_seq_from_the_future_cannot_resume() {
        let (first, _frames) = connection(1);
        let mut session = session(first);
        let now = Instant::now();

        assert_eq!(
            session.resume(b"guess", 0, connection(2).0, now),
            Err(ResumeRefused)
        );
        assert_eq!(
            session.resume(TOKEN, 5, connection(3).0, now),
            Err(ResumeRefused)
        );
    }

    #[test]
    fn detached_sessions_expire_after_the_window() {
        let (first, _frames) = connection(1);
        let mut session = session(first);
        let now = Instant::now();
        let window = Duration::from_secs(30);

        session.detach(1, now);

        assert!(!session.is_expired(now + window, window));
        assert!(session.is_expired(now + window + Duration::from_millis(1), window));
    }

    #[test]
    fn resuming_closes_the_old_connection() {
        let (first, mut first_frames) = connection(1);
        let mut session = session(first);

        session
            .resume(TOKEN, 0, connection(2).0, Instant::now())
            .unwrap();

        assert!(matches!(
            first_frames.try_recv(),
            Ok(Outbound::Close(code)) if code == opencord_common::voice::close::SESSION_REPLACED
        ));
    }
}
