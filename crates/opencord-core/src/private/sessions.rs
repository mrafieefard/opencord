//! One peer device's Olm sessions (design 2026-10-22 §10.3), kept encrypted
//! in the private database. There can be several, for instance when both
//! sides started one at the same moment.

use sqlx::SqliteConnection;
use vodozemac::Curve25519PublicKey;
use vodozemac::olm::{
    DecryptionError, InboundCreationResult, Message, OlmMessage, PreKeyMessage, Session,
    SessionConfig, SessionCreationError, SessionPickle,
};

use super::PrivateError;
use super::device::Device;

/// Sessions kept per peer device; the ones that were active longest ago go.
const MAX_SESSIONS: usize = 5;

/// What decrypting one message gave.
pub enum Decrypted {
    Plaintext(Vec<u8>),
    /// A message this device already decrypted.
    Duplicate,
    /// Why it couldn't be decrypted, for logs only.
    Failed(String),
}

struct Kept {
    session: Session,
    id: String,
    created_at: i64,
    /// When it last decrypted something.
    last_received_at: Option<i64>,
    /// Not yet written to the database.
    changed: bool,
}

/// The sessions with one peer device, most preferred for sending first.
pub struct PeerSessions {
    peer_curve25519: [u8; 32],
    sessions: Vec<Kept>,
}

impl PeerSessions {
    pub async fn load(
        conn: &mut SqliteConnection,
        pickle_key: &[u8; 32],
        peer_curve25519: [u8; 32],
    ) -> Result<Self, PrivateError> {
        let rows: Vec<(String, String, i64, Option<i64>)> = sqlx::query_as(
            "SELECT session_id, pickle, created_at, last_received_at FROM sessions \
             WHERE peer_curve25519 = $1",
        )
        .bind(peer_curve25519.as_slice())
        .fetch_all(&mut *conn)
        .await?;
        let sessions = rows
            .into_iter()
            .map(|(id, pickle, created_at, last_received_at)| {
                let pickle =
                    SessionPickle::from_encrypted(&pickle, pickle_key).map_err(|error| {
                        PrivateError::Crypto(format!("a stored session doesn't open: {error}"))
                    })?;
                Ok(Kept {
                    session: Session::from_pickle(pickle),
                    id,
                    created_at,
                    last_received_at,
                    changed: false,
                })
            })
            .collect::<Result<Vec<_>, PrivateError>>()?;
        let mut loaded = Self {
            peer_curve25519,
            sessions,
        };
        loaded.sort();
        Ok(loaded)
    }

    pub fn has_session(&self) -> bool {
        !self.sessions.is_empty()
    }

    /// Starts a session with the peer device from one of its prekeys.
    pub fn add_outbound(
        &mut self,
        device: &Device,
        prekey: [u8; 32],
        now_ms: i64,
    ) -> Result<(), PrivateError> {
        let session = device
            .account()
            .create_outbound_session(
                SessionConfig::version_1(),
                Curve25519PublicKey::from_bytes(self.peer_curve25519),
                Curve25519PublicKey::from_bytes(prekey),
            )
            .map_err(|error| {
                PrivateError::Crypto(format!(
                    "a session can't start: {}",
                    creation_failure(&error)
                ))
            })?;
        self.add(session, now_ms, None);
        Ok(())
    }

    /// Encrypts with the session that last decrypted something from the
    /// peer, else the newest. Answers the Olm message type and body.
    pub fn encrypt(&mut self, plaintext: &[u8]) -> Result<(u32, Vec<u8>), PrivateError> {
        let kept = self
            .sessions
            .first_mut()
            .ok_or_else(|| PrivateError::Crypto("no session with this device".to_owned()))?;
        let message = kept
            .session
            .encrypt(plaintext)
            .map_err(|error| PrivateError::Crypto(format!("encrypting failed: {error}")))?;
        kept.changed = true;
        let (olm_type, body) = message.to_parts();
        let olm_type = u32::try_from(olm_type)
            .map_err(|_| PrivateError::Crypto("an unknown Olm message type".to_owned()))?;
        Ok((olm_type, body))
    }

    /// Decrypts a message from the peer device. A prekey message that starts
    /// a session changes `device`'s account: the caller saves the device as
    /// well as these sessions, in the transaction that keeps the plaintext.
    pub fn decrypt(
        &mut self,
        device: &mut Device,
        olm_type: u32,
        body: &[u8],
        now_ms: i64,
    ) -> Decrypted {
        match olm_type {
            0 => match PreKeyMessage::from_bytes(body) {
                Ok(message) => self.decrypt_prekey(device, message, now_ms),
                Err(error) => {
                    Decrypted::Failed(format!("a prekey message doesn't decode: {error}"))
                }
            },
            1 => match Message::from_bytes(body) {
                Ok(message) => self.decrypt_normal(&OlmMessage::Normal(message), now_ms),
                Err(error) => Decrypted::Failed(format!("a message doesn't decode: {error}")),
            },
            other => Decrypted::Failed(format!("unknown Olm message type {other}")),
        }
    }

    /// The session it belongs to decrypts it; failing that, it starts an
    /// inbound session, using up the one-time key it names.
    fn decrypt_prekey(
        &mut self,
        device: &mut Device,
        message: PreKeyMessage,
        now_ms: i64,
    ) -> Decrypted {
        let keys = message.session_keys();
        if let Some(index) = self
            .sessions
            .iter()
            .position(|kept| kept.session.session_keys() == keys)
        {
            return match self.sessions[index]
                .session
                .decrypt(&OlmMessage::PreKey(message))
            {
                Ok(plaintext) => {
                    self.received(index, now_ms);
                    Decrypted::Plaintext(plaintext)
                }
                Err(DecryptionError::MissingMessageKey(_)) => Decrypted::Duplicate,
                Err(error) => Decrypted::Failed(error.to_string()),
            };
        }
        match device.account_mut().create_inbound_session(
            SessionConfig::version_1(),
            Curve25519PublicKey::from_bytes(self.peer_curve25519),
            &message,
        ) {
            Ok(InboundCreationResult { session, plaintext }) => {
                self.add(session, now_ms, Some(now_ms));
                Decrypted::Plaintext(plaintext)
            }
            Err(error) => Decrypted::Failed(creation_failure(&error)),
        }
    }

    /// Tries every session. A failed attempt changes nothing in vodozemac,
    /// and only the session the message belongs to can call it a repeat.
    fn decrypt_normal(&mut self, message: &OlmMessage, now_ms: i64) -> Decrypted {
        let mut repeated = false;
        let mut failure = None;
        for index in 0..self.sessions.len() {
            match self.sessions[index].session.decrypt(message) {
                Ok(plaintext) => {
                    self.received(index, now_ms);
                    return Decrypted::Plaintext(plaintext);
                }
                Err(DecryptionError::MissingMessageKey(_)) => repeated = true,
                Err(error) => failure = Some(error.to_string()),
            }
        }
        if repeated {
            Decrypted::Duplicate
        } else {
            Decrypted::Failed(failure.unwrap_or_else(|| "no session with this device".to_owned()))
        }
    }

    /// Writes the changed sessions inside the caller's transaction, then
    /// drops all but the five most recently active. A session counts as
    /// active when it was made and whenever it decrypts, so one just started
    /// for a reset is kept.
    pub async fn save(
        &mut self,
        conn: &mut SqliteConnection,
        pickle_key: &[u8; 32],
    ) -> Result<(), PrivateError> {
        for kept in self.sessions.iter().filter(|kept| kept.changed) {
            sqlx::query(
                "INSERT INTO sessions \
                 (peer_curve25519, session_id, pickle, created_at, last_received_at) \
                 VALUES ($1, $2, $3, $4, $5) \
                 ON CONFLICT (peer_curve25519, session_id) DO UPDATE SET \
                 pickle = excluded.pickle, last_received_at = excluded.last_received_at",
            )
            .bind(self.peer_curve25519.as_slice())
            .bind(&kept.id)
            .bind(kept.session.pickle().encrypt(pickle_key))
            .bind(kept.created_at)
            .bind(kept.last_received_at)
            .execute(&mut *conn)
            .await?;
        }
        // Ordered as `active_rank`.
        sqlx::query(
            "DELETE FROM sessions WHERE peer_curve25519 = $1 AND session_id NOT IN ( \
                 SELECT session_id FROM sessions WHERE peer_curve25519 = $1 \
                 ORDER BY MAX(created_at, COALESCE(last_received_at, created_at)) DESC, \
                 created_at DESC, session_id DESC \
                 LIMIT $2)",
        )
        .bind(self.peer_curve25519.as_slice())
        .bind(MAX_SESSIONS as i64)
        .execute(&mut *conn)
        .await?;

        self.sessions
            .sort_by(|a, b| b.active_rank().cmp(&a.active_rank()));
        self.sessions.truncate(MAX_SESSIONS);
        for kept in &mut self.sessions {
            kept.changed = false;
        }
        self.sort();
        Ok(())
    }

    fn add(&mut self, session: Session, created_at: i64, last_received_at: Option<i64>) {
        self.sessions.push(Kept {
            id: session.session_id(),
            session,
            created_at,
            last_received_at,
            changed: true,
        });
        self.sort();
    }

    fn received(&mut self, index: usize, now_ms: i64) {
        let kept = &mut self.sessions[index];
        kept.last_received_at = Some(now_ms);
        kept.changed = true;
        self.sort();
    }

    /// Most preferred for sending first.
    fn sort(&mut self) {
        self.sessions
            .sort_by(|a, b| b.sending_rank().cmp(&a.sending_rank()));
    }
}

impl Kept {
    /// The one that last decrypted something, else the newest.
    fn sending_rank(&self) -> (bool, Option<i64>, i64, &str) {
        (
            self.last_received_at.is_some(),
            self.last_received_at,
            self.created_at,
            &self.id,
        )
    }

    /// When it was last made or used to decrypt.
    fn active_rank(&self) -> (i64, i64, &str) {
        let active_at = self
            .last_received_at
            .map_or(self.created_at, |at| at.max(self.created_at));
        (active_at, self.created_at, &self.id)
    }
}

/// Why a session couldn't start, without the keys some errors carry.
fn creation_failure(error: &SessionCreationError) -> String {
    match error {
        SessionCreationError::MissingOneTimeKey(_) => "its one-time key is unknown".to_owned(),
        SessionCreationError::MismatchedIdentityKey(..) => {
            "it comes from another device's identity key".to_owned()
        }
        SessionCreationError::MismatchedSessionConfig { .. } => {
            "it uses another session config".to_owned()
        }
        SessionCreationError::Decryption(error) => error.to_string(),
        SessionCreationError::NonContributoryKey => "a key isn't contributory".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use opencord_common::private::FALLBACK_ROTATION_MS;

    use super::*;
    use crate::identity::Identity;
    use crate::private::store::PrivateStore;

    struct Side {
        _dir: tempfile::TempDir,
        store: PrivateStore,
        device: Device,
    }

    async fn side() -> Side {
        let dir = tempfile::tempdir().unwrap();
        let identity = Identity::generate();
        let store = PrivateStore::open(dir.path(), &identity.public_key(), &[3; 32], 0)
            .await
            .unwrap()
            .store;
        let device = Device::load_or_create(&store, 0).await.unwrap();
        Side {
            _dir: dir,
            store,
            device,
        }
    }

    impl Side {
        async fn sessions_with(&self, peer: &Side) -> PeerSessions {
            let mut conn = self.store.pool().acquire().await.unwrap();
            PeerSessions::load(&mut conn, self.store.pickle_key(), peer.device.curve25519())
                .await
                .unwrap()
        }

        /// Saves the sessions and the device in one transaction, as callers do.
        async fn save(&self, sessions: &mut PeerSessions) {
            let mut tx = self.store.pool().begin().await.unwrap();
            sessions
                .save(&mut tx, self.store.pickle_key())
                .await
                .unwrap();
            self.device
                .save(&mut tx, self.store.pickle_key())
                .await
                .unwrap();
            tx.commit().await.unwrap();
        }

        async fn reload_device(&mut self) {
            self.device = Device::load_or_create(&self.store, 0).await.unwrap();
        }

        fn one_time_keys(&mut self, count: usize) -> Vec<[u8; 32]> {
            self.device
                .new_one_time_keys(count)
                .iter()
                .map(|prekey| prekey.key.as_slice().try_into().unwrap())
                .collect()
        }

        fn fallback_key(&self) -> [u8; 32] {
            self.device
                .fallback_prekey()
                .key
                .as_slice()
                .try_into()
                .unwrap()
        }

        fn decrypt(
            &mut self,
            sessions: &mut PeerSessions,
            (olm_type, body): &(u32, Vec<u8>),
            now_ms: i64,
        ) -> Decrypted {
            sessions.decrypt(&mut self.device, *olm_type, body, now_ms)
        }
    }

    fn plaintext(decrypted: Decrypted) -> Vec<u8> {
        match decrypted {
            Decrypted::Plaintext(plaintext) => plaintext,
            Decrypted::Duplicate => panic!("expected a plaintext, got a duplicate"),
            Decrypted::Failed(error) => panic!("expected a plaintext, got a failure: {error}"),
        }
    }

    fn sending_session(sessions: &PeerSessions) -> &str {
        &sessions.sessions[0].id
    }

    fn created_at(sessions: &PeerSessions, at: i64) -> String {
        let kept = sessions.sessions.iter().find(|kept| kept.created_at == at);
        kept.unwrap().id.clone()
    }

    fn session_ids(sessions: &PeerSessions) -> Vec<&str> {
        let mut ids: Vec<&str> = sessions
            .sessions
            .iter()
            .map(|kept| kept.id.as_str())
            .collect();
        ids.sort_unstable();
        ids
    }

    async fn stored_rows(side: &Side) -> i64 {
        sqlx::query_scalar("SELECT COUNT(*) FROM sessions")
            .fetch_one(side.store.pool())
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn a_session_carries_messages_both_ways_and_survives_reloading() {
        let mut alice = side().await;
        let mut bob = side().await;
        let bob_key = bob.one_time_keys(1)[0];
        bob.save(&mut bob.sessions_with(&alice).await).await;
        let mut to_bob = alice.sessions_with(&bob).await;
        assert!(!to_bob.has_session());

        to_bob.add_outbound(&alice.device, bob_key, 1).unwrap();
        assert!(to_bob.has_session());
        let first = to_bob.encrypt(b"hello bob").unwrap();
        alice.save(&mut to_bob).await;
        assert_eq!(first.0, 0, "the first message starts the session");

        bob.reload_device().await;
        let mut to_alice = bob.sessions_with(&alice).await;
        assert!(!to_alice.has_session());
        assert_eq!(
            plaintext(bob.decrypt(&mut to_alice, &first, 2)),
            b"hello bob"
        );
        bob.save(&mut to_alice).await;

        bob.reload_device().await;
        let mut to_alice = bob.sessions_with(&alice).await;
        assert!(to_alice.has_session());
        let reply = to_alice.encrypt(b"hello alice").unwrap();
        bob.save(&mut to_alice).await;
        assert_eq!(reply.0, 1);

        alice.reload_device().await;
        let mut to_bob = alice.sessions_with(&bob).await;
        assert_eq!(
            plaintext(alice.decrypt(&mut to_bob, &reply, 3)),
            b"hello alice"
        );
        let second = to_bob.encrypt(b"got it").unwrap();
        alice.save(&mut to_bob).await;
        assert_eq!(second.0, 1, "a session that received sends normal messages");

        let mut to_alice = bob.sessions_with(&alice).await;
        assert_eq!(plaintext(bob.decrypt(&mut to_alice, &second, 4)), b"got it");
        assert_eq!(stored_rows(&alice).await, 1);
        assert_eq!(stored_rows(&bob).await, 1);
    }

    #[tokio::test]
    async fn a_duplicate_is_reported_as_a_duplicate() {
        let mut alice = side().await;
        let mut bob = side().await;
        let bob_key = bob.one_time_keys(1)[0];
        let mut to_bob = alice.sessions_with(&bob).await;
        to_bob.add_outbound(&alice.device, bob_key, 1).unwrap();
        let prekey_message = to_bob.encrypt(b"once").unwrap();
        alice.save(&mut to_bob).await;
        let mut to_alice = bob.sessions_with(&alice).await;
        assert_eq!(
            plaintext(bob.decrypt(&mut to_alice, &prekey_message, 2)),
            b"once"
        );
        bob.save(&mut to_alice).await;

        bob.reload_device().await;
        let mut to_alice = bob.sessions_with(&alice).await;
        assert!(matches!(
            bob.decrypt(&mut to_alice, &prekey_message, 3),
            Decrypted::Duplicate
        ));

        let normal_message = to_alice.encrypt(b"twice").unwrap();
        assert_eq!(normal_message.0, 1);
        bob.save(&mut to_alice).await;
        assert_eq!(
            plaintext(alice.decrypt(&mut to_bob, &normal_message, 4)),
            b"twice"
        );
        alice.save(&mut to_bob).await;
        let mut to_bob = alice.sessions_with(&bob).await;
        assert!(matches!(
            alice.decrypt(&mut to_bob, &normal_message, 5),
            Decrypted::Duplicate
        ));
    }

    #[tokio::test]
    async fn crossed_sessions_settle_on_the_one_that_last_received() {
        let mut alice = side().await;
        let mut bob = side().await;
        let alice_key = alice.one_time_keys(1)[0];
        let bob_key = bob.one_time_keys(1)[0];
        let mut to_bob = alice.sessions_with(&bob).await;
        let mut to_alice = bob.sessions_with(&alice).await;
        to_bob.add_outbound(&alice.device, bob_key, 1).unwrap();
        to_alice.add_outbound(&bob.device, alice_key, 1).unwrap();
        let alices_session = sending_session(&to_bob).to_owned();
        let bobs_session = sending_session(&to_alice).to_owned();
        let from_alice = to_bob.encrypt(b"hi from alice").unwrap();
        let from_bob = to_alice.encrypt(b"hi from bob").unwrap();

        // Both start a session at the same moment, and each receives the
        // other's: each now sends on the one the other started.
        assert_eq!(
            plaintext(alice.decrypt(&mut to_bob, &from_bob, 2)),
            b"hi from bob"
        );
        assert_eq!(
            plaintext(bob.decrypt(&mut to_alice, &from_alice, 2)),
            b"hi from alice"
        );
        assert_eq!(sending_session(&to_bob), bobs_session);
        assert_eq!(sending_session(&to_alice), alices_session);

        // Alice's next message arrives on Bob's session, so Bob moves to it.
        let next = to_bob.encrypt(b"again").unwrap();
        assert_eq!(plaintext(bob.decrypt(&mut to_alice, &next, 3)), b"again");
        assert_eq!(sending_session(&to_alice), bobs_session);
        let answer = to_alice.encrypt(b"settled").unwrap();
        assert_eq!(
            plaintext(alice.decrypt(&mut to_bob, &answer, 4)),
            b"settled"
        );
        assert_eq!(sending_session(&to_bob), bobs_session);

        alice.save(&mut to_bob).await;
        bob.save(&mut to_alice).await;
        assert_eq!(
            sending_session(&alice.sessions_with(&bob).await),
            bobs_session
        );
        assert_eq!(
            sending_session(&bob.sessions_with(&alice).await),
            bobs_session
        );
    }

    #[tokio::test]
    async fn at_most_five_sessions_are_kept_per_peer_device() {
        let mut alice = side().await;
        let mut bob = side().await;
        let bob_keys = bob.one_time_keys(8);
        let mut to_bob = alice.sessions_with(&bob).await;
        to_bob.add_outbound(&alice.device, bob_keys[0], 1).unwrap();
        let oldest = sending_session(&to_bob).to_owned();
        let first = to_bob.encrypt(b"first").unwrap();
        let mut to_alice = bob.sessions_with(&alice).await;
        plaintext(bob.decrypt(&mut to_alice, &first, 1));
        let reply = to_alice.encrypt(b"reply").unwrap();
        let mut created = vec![oldest.clone()];
        for (at, key) in (2..=4).zip(&bob_keys[1..4]) {
            to_bob.add_outbound(&alice.device, *key, at).unwrap();
            created.push(created_at(&to_bob, at));
        }
        // The oldest session receives after three newer ones were made.
        plaintext(alice.decrypt(&mut to_bob, &reply, 5));
        for (at, key) in (6..=8).zip(&bob_keys[4..7]) {
            to_bob.add_outbound(&alice.device, *key, at).unwrap();
            created.push(created_at(&to_bob, at));
        }
        assert_eq!(to_bob.sessions.len(), 7);

        alice.save(&mut to_bob).await;

        // Kept: the four newest, and the oldest, which received at 5.
        let mut kept = vec![
            created[0].as_str(),
            created[3].as_str(),
            created[4].as_str(),
            created[5].as_str(),
            created[6].as_str(),
        ];
        kept.sort_unstable();
        assert_eq!(session_ids(&to_bob), kept);
        assert_eq!(stored_rows(&alice).await, 5);
        assert_eq!(session_ids(&alice.sessions_with(&bob).await), kept);
        assert_eq!(sending_session(&to_bob), oldest);

        // A session just started is kept, even when it hasn't received yet.
        to_bob.add_outbound(&alice.device, bob_keys[7], 9).unwrap();
        let newest = created_at(&to_bob, 9);
        alice.save(&mut to_bob).await;
        let reloaded = alice.sessions_with(&bob).await;
        assert!(session_ids(&reloaded).contains(&newest.as_str()));
        assert!(!session_ids(&reloaded).contains(&created[3].as_str()));
        assert_eq!(stored_rows(&alice).await, 5);
        assert_eq!(sending_session(&reloaded), oldest);
    }

    #[tokio::test]
    async fn a_rotated_fallback_key_still_opens_sessions_made_with_the_previous_one() {
        let alice = side().await;
        let mut bob = side().await;
        let first_fallback = bob.fallback_key();
        let mut to_bob = alice.sessions_with(&bob).await;
        to_bob
            .add_outbound(&alice.device, first_fallback, 1)
            .unwrap();
        let on_first = to_bob.encrypt(b"before the rotation").unwrap();

        assert!(bob.device.rotate_fallback_if_due(FALLBACK_ROTATION_MS));
        let second_fallback = bob.fallback_key();
        let mut to_alice = bob.sessions_with(&alice).await;
        assert_eq!(
            plaintext(bob.decrypt(&mut to_alice, &on_first, 2)),
            b"before the rotation"
        );

        // Two more sessions, one on each fallback key, read after a second
        // rotation: only the previous key still opens one.
        let mut later = alice.sessions_with(&bob).await;
        later
            .add_outbound(&alice.device, first_fallback, 3)
            .unwrap();
        let on_first_again = later.encrypt(b"too late").unwrap();
        let mut current = alice.sessions_with(&bob).await;
        current
            .add_outbound(&alice.device, second_fallback, 3)
            .unwrap();
        let on_second = current.encrypt(b"still fine").unwrap();
        assert!(bob.device.rotate_fallback_if_due(2 * FALLBACK_ROTATION_MS));

        assert!(matches!(
            bob.decrypt(&mut to_alice, &on_first_again, 4),
            Decrypted::Failed(_)
        ));
        assert_eq!(
            plaintext(bob.decrypt(&mut to_alice, &on_second, 4)),
            b"still fine"
        );
    }

    #[tokio::test]
    async fn a_prekey_message_from_another_identity_key_fails() {
        let alice = side().await;
        let mut bob = side().await;
        let carol = side().await;
        let bob_key = bob.one_time_keys(1)[0];
        let mut carol_to_bob = carol.sessions_with(&bob).await;
        carol_to_bob
            .add_outbound(&carol.device, bob_key, 1)
            .unwrap();
        let from_carol = carol_to_bob.encrypt(b"pretending").unwrap();

        let mut to_alice = bob.sessions_with(&alice).await;
        assert!(matches!(
            bob.decrypt(&mut to_alice, &from_carol, 2),
            Decrypted::Failed(_)
        ));
        assert!(!to_alice.has_session());

        // The failure used nothing up: from Carol's own device it opens.
        let mut to_carol = bob.sessions_with(&carol).await;
        assert_eq!(
            plaintext(bob.decrypt(&mut to_carol, &from_carol, 3)),
            b"pretending"
        );
    }

    #[tokio::test]
    async fn damaged_or_unknown_messages_fail() {
        let mut alice = side().await;
        let mut bob = side().await;
        let bob_key = bob.one_time_keys(1)[0];
        let mut to_bob = alice.sessions_with(&bob).await;
        to_bob.add_outbound(&alice.device, bob_key, 1).unwrap();
        let (olm_type, mut body) = to_bob.encrypt(b"hello").unwrap();
        let mut to_alice = bob.sessions_with(&alice).await;

        assert!(matches!(
            bob.decrypt(&mut to_alice, &(2, body.clone()), 2),
            Decrypted::Failed(_)
        ));
        assert!(matches!(
            bob.decrypt(&mut to_alice, &(olm_type, vec![1, 2, 3]), 2),
            Decrypted::Failed(_)
        ));
        let last = body.len() - 1;
        body[last] ^= 1;
        assert!(matches!(
            bob.decrypt(&mut to_alice, &(olm_type, body), 2),
            Decrypted::Failed(_)
        ));
        assert!(matches!(
            alice.decrypt(&mut to_bob, &(1, vec![3, 0, 0]), 2),
            Decrypted::Failed(_)
        ));
        assert!(!to_alice.has_session());
        assert!(
            PeerSessions {
                peer_curve25519: [0; 32],
                sessions: Vec::new(),
            }
            .encrypt(b"nobody")
            .is_err()
        );
    }
}
