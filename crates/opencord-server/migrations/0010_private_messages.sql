-- Private messages, friends and offline delivery (design 2026-10-22).

CREATE TABLE private_privacy (
    user_id INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    allow_friend_requests INTEGER NOT NULL,
    allow_message_requests INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE private_blocks (
    owner_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    blocked_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    PRIMARY KEY (owner_id, blocked_id)
);

CREATE TABLE private_devices (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    device_id BLOB NOT NULL,
    curve25519 BLOB NOT NULL,
    ed25519 BLOB NOT NULL,
    signature BLOB NOT NULL,
    fallback_key BLOB NOT NULL,
    fallback_signature BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, device_id)
);

CREATE TABLE private_one_time_keys (
    user_id INTEGER NOT NULL,
    device_id BLOB NOT NULL,
    key BLOB NOT NULL,
    signature BLOB NOT NULL,
    PRIMARY KEY (user_id, device_id, key),
    FOREIGN KEY (user_id, device_id)
        REFERENCES private_devices (user_id, device_id) ON DELETE CASCADE
);

CREATE TABLE private_friend_requests (
    id INTEGER PRIMARY KEY,
    from_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    to_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at INTEGER NOT NULL
);
-- One request per pair, whichever way it goes.
CREATE UNIQUE INDEX private_friend_requests_pair
    ON private_friend_requests (min(from_id, to_id), max(from_id, to_id));
CREATE INDEX private_friend_requests_to ON private_friend_requests (to_id);

CREATE TABLE private_pending (
    id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL,
    device_id BLOB NOT NULL,
    -- An encoded PendingEvent, id included.
    event BLOB NOT NULL,
    -- Set on FriendRequestReceived rows, so a cancel can take them back.
    friend_request_id INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id, device_id)
        REFERENCES private_devices (user_id, device_id) ON DELETE CASCADE
);
CREATE INDEX private_pending_by_device ON private_pending (user_id, device_id, id);
CREATE INDEX private_pending_by_request
    ON private_pending (friend_request_id) WHERE friend_request_id IS NOT NULL;
CREATE INDEX private_pending_by_age ON private_pending (created_at);

CREATE TABLE private_recent_sends (
    user_id INTEGER NOT NULL,
    device_id BLOB NOT NULL,
    send_id BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, device_id, send_id)
);
CREATE INDEX private_recent_sends_by_age ON private_recent_sends (created_at);
