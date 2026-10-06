//! Sessions outlive connections: events are numbered and buffered so a
//! client that reconnects within [`RESUME_WINDOW`] can resume where it
//! left off.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use bytes::Bytes;
use opencord_proto::v1 as proto;
use sha2::{Digest, Sha256};
use tokio::sync::mpsc;
use tokio::sync::mpsc::error::TrySendError;
use tokio_util::sync::CancellationToken;

use super::frames;

pub const REPLAY_MAX_EVENTS: usize = 1_000;
pub const REPLAY_MAX_AGE: Duration = Duration::from_secs(60);
pub const RESUME_WINDOW: Duration = Duration::from_secs(60);
/// Larger than the replay buffer, so a full replay always fits.
pub const OUTBOUND_CAPACITY: usize = 2_048;

/// WebSocket close codes; see docs/protocol.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CloseCode(pub u16);

impl CloseCode {
    pub const GOING_AWAY: Self = Self(1001);
    pub const UNKNOWN: Self = Self(4000);
    pub const INVALID_FRAME: Self = Self(4001);
    pub const NOT_AUTHENTICATED: Self = Self(4002);
    pub const AUTHENTICATION_FAILED: Self = Self(4003);
    pub const HANDSHAKE_TIMEOUT: Self = Self(4004);
    pub const HEARTBEAT_TIMEOUT: Self = Self(4005);
    pub const RATE_LIMITED: Self = Self(4008);
    pub const SESSION_REPLACED: Self = Self(4009);
    pub const KICKED: Self = Self(4010);
    pub const BANNED: Self = Self(4011);
    pub const TOO_SLOW: Self = Self(4012);

    /// Whether the client may resume its session after this close.
    pub fn is_resumable(self) -> bool {
        matches!(
            self,
            Self::GOING_AWAY
                | Self::UNKNOWN
                | Self::INVALID_FRAME
                | Self::HEARTBEAT_TIMEOUT
                | Self::TOO_SLOW
        )
    }
}

/// Asks a connection to close with a code. The first code set wins; a
/// server shutdown (parent token cancelled) closes with 1001.
#[derive(Debug, Clone)]
pub struct CloseHandle {
    token: CancellationToken,
    code: Arc<AtomicU16>,
}

impl CloseHandle {
    pub fn new(token: CancellationToken) -> Self {
        Self {
            token,
            code: Arc::new(AtomicU16::new(0)),
        }
    }

    pub fn close(&self, code: CloseCode) {
        let _ = self
            .code
            .compare_exchange(0, code.0, Ordering::SeqCst, Ordering::SeqCst);
        self.token.cancel();
    }

    pub async fn closed(&self) {
        self.token.cancelled().await;
    }

    pub fn is_closed(&self) -> bool {
        self.token.is_cancelled()
    }

    pub fn code(&self) -> CloseCode {
        match self.code.load(Ordering::SeqCst) {
            0 => CloseCode::GOING_AWAY,
            code => CloseCode(code),
        }
    }
}

/// One live WebSocket: where to send frames and how to close it.
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: u64,
    pub outbound: mpsc::Sender<Bytes>,
    pub closer: CloseHandle,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ResumeError {
    #[error("unknown session or wrong resume token")]
    Unknown,
    #[error("the missed events are no longer buffered")]
    TooOld,
}

#[derive(Debug)]
pub struct Session {
    pub id: String,
    pub user_id: i64,
    resume_token_hash: [u8; 32],
    inner: Mutex<Inner>,
}

#[derive(Debug)]
struct Inner {
    next_seq: u64,
    replay: VecDeque<Buffered>,
    connection: Option<Connection>,
    detached_at: Option<Instant>,
    ended: bool,
}

impl Inner {
    fn trim(&mut self, now: Instant) {
        while self.replay.front().is_some_and(|buffered| {
            self.replay.len() > REPLAY_MAX_EVENTS
                || now.duration_since(buffered.at) > REPLAY_MAX_AGE
        }) {
            self.replay.pop_front();
        }
    }
}

#[derive(Debug)]
struct Buffered {
    seq: u64,
    at: Instant,
    frame: Bytes,
}

impl Session {
    /// A new session attached to `connection`.
    pub fn new(id: String, user_id: i64, resume_token: &[u8], connection: Connection) -> Self {
        Self {
            id,
            user_id,
            resume_token_hash: Sha256::digest(resume_token).into(),
            inner: Mutex::new(Inner {
                next_seq: 1,
                replay: VecDeque::new(),
                connection: Some(connection),
                detached_at: None,
                ended: false,
            }),
        }
    }

    /// Numbers, buffers and sends an event. A connection too slow to keep
    /// up is closed (4012); the session stays resumable.
    pub fn push_event(&self, event: &proto::Event, now: Instant) {
        let mut inner = self.lock();
        if inner.ended {
            return;
        }
        let seq = inner.next_seq;
        inner.next_seq += 1;
        let frame = frames::event(seq, event);
        inner.replay.push_back(Buffered {
            seq,
            at: now,
            frame: frame.clone(),
        });
        inner.trim(now);
        let failure = inner.connection.as_ref().and_then(|connection| {
            match connection.outbound.try_send(frame) {
                Ok(()) => None,
                Err(TrySendError::Full(_)) => Some(Some(connection.closer.clone())),
                Err(TrySendError::Closed(_)) => Some(None),
            }
        });
        if let Some(too_slow) = failure {
            if let Some(closer) = too_slow {
                closer.close(CloseCode::TOO_SLOW);
            }
            inner.connection = None;
            inner.detached_at = Some(now);
        }
    }

    /// Moves the session to `connection`: replays every event after
    /// `last_seq`, then queues `resumed` (built from the replay count).
    /// Closes the previous connection, if any, with 4009.
    pub fn resume(
        &self,
        resume_token: &[u8],
        last_seq: u64,
        connection: Connection,
        now: Instant,
    ) -> Result<u64, ResumeError> {
        let token_hash: [u8; 32] = Sha256::digest(resume_token).into();
        if token_hash != self.resume_token_hash {
            return Err(ResumeError::Unknown);
        }
        let mut inner = self.lock();
        if inner.ended || last_seq >= inner.next_seq {
            return Err(ResumeError::Unknown);
        }
        inner.trim(now);
        let first_missed = last_seq + 1;
        let missed_any = first_missed < inner.next_seq;
        let oldest = inner.replay.front().map(|buffered| buffered.seq);
        if missed_any && oldest.is_none_or(|oldest| oldest > first_missed) {
            return Err(ResumeError::TooOld);
        }
        let missed: Vec<Bytes> = inner
            .replay
            .iter()
            .filter(|buffered| buffered.seq > last_seq)
            .map(|buffered| buffered.frame.clone())
            .collect();
        let replayed = u64::try_from(missed.len()).unwrap_or(u64::MAX);
        for frame in missed.into_iter().chain([frames::resumed(replayed)]) {
            if connection.outbound.try_send(frame).is_err() {
                return Err(ResumeError::Unknown);
            }
        }
        if let Some(previous) = inner.connection.replace(connection) {
            previous.closer.close(CloseCode::SESSION_REPLACED);
        }
        inner.detached_at = None;
        Ok(replayed)
    }

    /// Marks the connection as gone, if it is still the current one.
    pub fn detach(&self, connection_id: u64, now: Instant) {
        let mut inner = self.lock();
        let is_current = inner
            .connection
            .as_ref()
            .is_some_and(|connection| connection.id == connection_id);
        if is_current {
            inner.connection = None;
            inner.detached_at = Some(now);
        }
    }

    /// Closes the current connection but keeps the session resumable.
    pub fn drop_connection(&self, code: CloseCode, now: Instant) {
        let mut inner = self.lock();
        if let Some(connection) = inner.connection.take() {
            connection.closer.close(code);
            inner.detached_at = Some(now);
        }
    }

    /// Ends the session for good and closes its connection with `code`.
    pub fn end(&self, code: CloseCode) {
        let mut inner = self.lock();
        inner.ended = true;
        inner.replay.clear();
        if let Some(connection) = inner.connection.take() {
            connection.closer.close(code);
        }
    }

    /// Ended, or detached for longer than the resume window.
    pub fn is_expired(&self, now: Instant) -> bool {
        let inner = self.lock();
        inner.ended
            || inner
                .detached_at
                .is_some_and(|at| now.duration_since(at) > RESUME_WINDOW)
    }

    pub fn is_ended(&self) -> bool {
        self.lock().ended
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Every session on the server, by id.
#[derive(Debug, Default)]
pub struct SessionRegistry {
    sessions: Mutex<HashMap<String, Arc<Session>>>,
}

impl SessionRegistry {
    pub fn insert(&self, session: Arc<Session>) {
        self.lock().insert(session.id.clone(), session);
    }

    pub fn get(&self, id: &str) -> Option<Arc<Session>> {
        self.lock().get(id).cloned()
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Session>> {
        self.lock().remove(id)
    }

    pub fn all(&self) -> Vec<Arc<Session>> {
        self.lock().values().cloned().collect()
    }

    pub fn for_user(&self, user_id: i64) -> Vec<Arc<Session>> {
        self.lock()
            .values()
            .filter(|session| session.user_id == user_id)
            .cloned()
            .collect()
    }

    pub fn has_user(&self, user_id: i64) -> bool {
        self.lock()
            .values()
            .any(|session| session.user_id == user_id)
    }

    /// Removes and returns every expired session.
    pub fn remove_expired(&self, now: Instant) -> Vec<Arc<Session>> {
        let mut sessions = self.lock();
        let expired: Vec<String> = sessions
            .values()
            .filter(|session| session.is_expired(now))
            .map(|session| session.id.clone())
            .collect();
        expired
            .iter()
            .filter_map(|id| sessions.remove(id))
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<String, Arc<Session>>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use prost::Message as _;

    use super::*;

    const TOKEN: &[u8] = b"resume-token";

    struct Client {
        connection: Connection,
        frames: mpsc::Receiver<Bytes>,
    }

    fn client(id: u64, capacity: usize) -> Client {
        let (outbound, frames) = mpsc::channel(capacity);
        Client {
            connection: Connection {
                id,
                outbound,
                closer: CloseHandle::new(CancellationToken::new()),
            },
            frames,
        }
    }

    fn typing(user_id: i64) -> proto::Event {
        proto::Event {
            kind: Some(proto::event::Kind::TypingStart(proto::TypingStart {
                channel_id: 1,
                user_id,
            })),
        }
    }

    fn received(client: &mut Client) -> Vec<proto::Envelope> {
        let mut envelopes = Vec::new();
        while let Ok(frame) = client.frames.try_recv() {
            envelopes.push(proto::Envelope::decode(frame).unwrap());
        }
        envelopes
    }

    fn seqs(envelopes: &[proto::Envelope]) -> Vec<u64> {
        envelopes
            .iter()
            .filter(|envelope| matches!(envelope.payload, Some(proto::envelope::Payload::Event(_))))
            .map(|envelope| envelope.seq)
            .collect()
    }

    #[test]
    fn events_are_numbered_from_one_and_sent() {
        let mut first = client(1, 16);
        let session = Session::new("s".into(), 7, TOKEN, first.connection.clone());
        let now = Instant::now();

        session.push_event(&typing(1), now);
        session.push_event(&typing(2), now);

        assert_eq!(seqs(&received(&mut first)), [1, 2]);
    }

    #[test]
    fn resume_replays_missed_events_then_resumed() {
        let mut first = client(1, 16);
        let session = Session::new("s".into(), 7, TOKEN, first.connection.clone());
        let now = Instant::now();
        session.push_event(&typing(1), now);
        received(&mut first);
        session.detach(1, now);
        session.push_event(&typing(2), now);
        session.push_event(&typing(3), now);
        let mut second = client(2, 16);

        let replayed = session
            .resume(TOKEN, 1, second.connection.clone(), now)
            .unwrap();

        let envelopes = received(&mut second);
        assert_eq!(replayed, 2);
        assert_eq!(seqs(&envelopes), [2, 3]);
        assert!(matches!(
            envelopes.last().unwrap().payload,
            Some(proto::envelope::Payload::Resumed(proto::Resumed {
                replayed_events: 2
            }))
        ));
        session.push_event(&typing(4), now);
        assert_eq!(seqs(&received(&mut second)), [4]);
    }

    #[test]
    fn resume_closes_the_previous_connection() {
        let first = client(1, 16);
        let session = Session::new("s".into(), 7, TOKEN, first.connection.clone());

        session
            .resume(TOKEN, 0, client(2, 16).connection, Instant::now())
            .unwrap();

        assert!(first.connection.closer.is_closed());
        assert_eq!(first.connection.closer.code(), CloseCode::SESSION_REPLACED);
    }

    #[test]
    fn resume_rejects_a_wrong_token() {
        let session = Session::new("s".into(), 7, TOKEN, client(1, 16).connection);

        let result = session.resume(b"guess", 0, client(2, 16).connection, Instant::now());

        assert_eq!(result, Err(ResumeError::Unknown));
    }

    #[test]
    fn resume_fails_once_missed_events_fell_out_of_the_buffer() {
        let session = Session::new("s".into(), 7, TOKEN, client(1, 4_096).connection);
        let now = Instant::now();
        for user in 0..=i64::try_from(REPLAY_MAX_EVENTS).unwrap() {
            session.push_event(&typing(user), now);
        }

        let too_old = session.resume(TOKEN, 0, client(2, OUTBOUND_CAPACITY).connection, now);
        let recent = session.resume(TOKEN, 1, client(3, OUTBOUND_CAPACITY).connection, now);

        assert_eq!(too_old, Err(ResumeError::TooOld));
        assert_eq!(recent, Ok(1_000));
    }

    #[test]
    fn resume_fails_for_events_older_than_the_replay_window() {
        let session = Session::new("s".into(), 7, TOKEN, client(1, 16).connection);
        let start = Instant::now();
        session.push_event(&typing(1), start);
        session.push_event(&typing(2), start + REPLAY_MAX_AGE + Duration::from_secs(1));

        let result = session.resume(
            TOKEN,
            0,
            client(2, 16).connection,
            start + REPLAY_MAX_AGE + Duration::from_secs(1),
        );

        assert_eq!(result, Err(ResumeError::TooOld));
    }

    #[test]
    fn resume_rejects_a_seq_from_the_future() {
        let session = Session::new("s".into(), 7, TOKEN, client(1, 16).connection);

        let result = session.resume(TOKEN, 5, client(2, 16).connection, Instant::now());

        assert_eq!(result, Err(ResumeError::Unknown));
    }

    #[test]
    fn slow_connections_are_closed_but_the_session_survives() {
        let slow = client(1, 1);
        let session = Session::new("s".into(), 7, TOKEN, slow.connection.clone());
        let now = Instant::now();

        session.push_event(&typing(1), now);
        session.push_event(&typing(2), now);

        assert_eq!(slow.connection.closer.code(), CloseCode::TOO_SLOW);
        assert!(!session.is_expired(now));
        let mut resumed = client(2, 16);
        assert_eq!(
            session.resume(TOKEN, 1, resumed.connection.clone(), now),
            Ok(1)
        );
        assert_eq!(seqs(&received(&mut resumed)), [2]);
    }

    #[test]
    fn detached_sessions_expire_after_the_resume_window() {
        let session = Session::new("s".into(), 7, TOKEN, client(1, 16).connection);
        let now = Instant::now();

        session.detach(1, now);

        assert!(!session.is_expired(now + RESUME_WINDOW));
        assert!(session.is_expired(now + RESUME_WINDOW + Duration::from_millis(1)));
    }

    #[test]
    fn detaching_an_old_connection_keeps_the_new_one() {
        let session = Session::new("s".into(), 7, TOKEN, client(1, 16).connection);
        let now = Instant::now();
        let mut second = client(2, 16);
        session
            .resume(TOKEN, 0, second.connection.clone(), now)
            .unwrap();
        received(&mut second);

        session.detach(1, now);
        session.push_event(&typing(1), now);

        assert_eq!(seqs(&received(&mut second)), [1]);
    }

    #[test]
    fn ended_sessions_close_and_cannot_resume() {
        let first = client(1, 16);
        let session = Session::new("s".into(), 7, TOKEN, first.connection.clone());

        session.end(CloseCode::KICKED);

        assert_eq!(first.connection.closer.code(), CloseCode::KICKED);
        assert!(session.is_expired(Instant::now()));
        assert_eq!(
            session.resume(TOKEN, 0, client(2, 16).connection, Instant::now()),
            Err(ResumeError::Unknown)
        );
    }

    #[test]
    fn registry_removes_only_expired_sessions() {
        let registry = SessionRegistry::default();
        let live = Arc::new(Session::new(
            "live".into(),
            1,
            TOKEN,
            client(1, 16).connection,
        ));
        let dead = Arc::new(Session::new(
            "dead".into(),
            2,
            TOKEN,
            client(2, 16).connection,
        ));
        dead.end(CloseCode::KICKED);
        registry.insert(live);
        registry.insert(dead);

        let removed = registry.remove_expired(Instant::now());

        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].id, "dead");
        assert!(registry.has_user(1));
        assert!(!registry.has_user(2));
    }
}
