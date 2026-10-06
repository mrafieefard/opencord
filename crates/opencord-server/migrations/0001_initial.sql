-- Ids are Snowflakes; timestamps are Unix milliseconds. Permission columns
-- hold the u64 bit set reinterpreted as a signed 64-bit integer.

CREATE TABLE server_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);

CREATE TABLE users (
    id INTEGER PRIMARY KEY,
    public_key BLOB NOT NULL UNIQUE,
    display_name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE members (
    user_id INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    nickname TEXT,
    joined_at INTEGER NOT NULL
);

CREATE TABLE roles (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    color INTEGER NOT NULL DEFAULT 0,
    position INTEGER NOT NULL,
    permissions INTEGER NOT NULL,
    hoist BOOLEAN NOT NULL DEFAULT FALSE,
    mentionable BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE TABLE member_roles (
    user_id INTEGER NOT NULL REFERENCES members (user_id) ON DELETE CASCADE,
    role_id INTEGER NOT NULL REFERENCES roles (id) ON DELETE CASCADE,
    PRIMARY KEY (user_id, role_id)
);

CREATE TABLE channels (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('text', 'voice', 'category')),
    name TEXT NOT NULL,
    topic TEXT,
    parent_id INTEGER REFERENCES channels (id) ON DELETE SET NULL,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE channel_overwrites (
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    target_kind TEXT NOT NULL CHECK (target_kind IN ('role', 'member')),
    target_id INTEGER NOT NULL,
    allow INTEGER NOT NULL,
    deny INTEGER NOT NULL,
    PRIMARY KEY (channel_id, target_kind, target_id)
);

CREATE TABLE messages (
    id INTEGER PRIMARY KEY,
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    author_id INTEGER NOT NULL REFERENCES users (id),
    content TEXT NOT NULL,
    edited_at INTEGER,
    deleted BOOLEAN NOT NULL DEFAULT FALSE
);

CREATE INDEX messages_by_channel ON messages (channel_id, id DESC);

CREATE TABLE invites (
    code TEXT PRIMARY KEY NOT NULL,
    created_by INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL,
    max_uses INTEGER,
    uses INTEGER NOT NULL DEFAULT 0,
    expires_at INTEGER
);

CREATE TABLE bans (
    user_id INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    reason TEXT,
    banned_by INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);
