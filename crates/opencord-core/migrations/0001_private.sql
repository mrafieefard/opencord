-- The private messages database (design 2026-10-22 §11), one per identity.
-- Sealed columns hold nonce || XChaCha20-Poly1305 ciphertext.

CREATE TABLE device (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    device_id BLOB NOT NULL,
    -- vodozemac account pickle, encrypted with the pickle key.
    account TEXT NOT NULL,
    fallback_created_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- Other people's devices whose signatures this device checked.
CREATE TABLE peer_devices (
    public_key BLOB NOT NULL,
    device_id BLOB NOT NULL,
    curve25519 BLOB NOT NULL,
    ed25519 BLOB NOT NULL,
    first_seen_at INTEGER NOT NULL,
    PRIMARY KEY (public_key, device_id)
);

CREATE TABLE sessions (
    peer_curve25519 BLOB NOT NULL,
    session_id TEXT NOT NULL,
    -- vodozemac session pickle, encrypted with the pickle key.
    pickle TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    -- When it last decrypted something; NULL until then.
    last_received_at INTEGER,
    PRIMARY KEY (peer_curve25519, session_id)
);

CREATE TABLE contacts (
    public_key BLOB PRIMARY KEY,
    -- Sealed: the name last seen.
    name BLOB,
    -- 0 none, 1 outgoing request, 2 incoming request, 3 friends.
    friend_state INTEGER NOT NULL DEFAULT 0,
    request_server_id TEXT,
    request_id INTEGER,
    request_spam INTEGER NOT NULL DEFAULT 0,
    friend_server_id TEXT,
    blocked INTEGER NOT NULL DEFAULT 0,
    declines INTEGER NOT NULL DEFAULT 0,
    -- 0 none, 1 request, 2 accepted.
    conversation INTEGER NOT NULL DEFAULT 0,
    relay_server_id TEXT,
    -- Their permission slip for this person (64 bytes).
    their_slip BLOB,
    last_activity_at INTEGER,
    last_read_at INTEGER NOT NULL DEFAULT 0,
    muted INTEGER NOT NULL DEFAULT 0,
    hidden INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL
);

CREATE TABLE messages (
    -- 16 bytes from the author's device.
    id BLOB PRIMARY KEY,
    -- The conversation's other person.
    peer BLOB NOT NULL,
    outgoing INTEGER NOT NULL,
    -- 0 text, 1 divider, 2 moved card, 3 undecryptable line.
    kind INTEGER NOT NULL,
    -- Sealed: the text, or the server's name for a divider or card.
    body BLOB,
    -- A divider's or card's server, hex.
    server_id TEXT,
    -- The server's time; the local time while sending.
    sent_at INTEGER NOT NULL,
    edited_at INTEGER,
    -- 0 received, 1 sending, 2 sent, 3 failed.
    state INTEGER NOT NULL,
    -- Sealed encoded SendPrivateMessage while unsent, and its server.
    outbox BLOB,
    outbox_server_id TEXT
);
CREATE INDEX messages_by_conversation ON messages (peer, sent_at, id);
CREATE INDEX messages_sending ON messages (state) WHERE state = 1;

CREATE TABLE received_events (
    server_id TEXT NOT NULL,
    event_id INTEGER NOT NULL,
    received_at INTEGER NOT NULL,
    PRIMARY KEY (server_id, event_id)
);
CREATE INDEX received_events_by_age ON received_events (received_at);

CREATE TABLE settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
