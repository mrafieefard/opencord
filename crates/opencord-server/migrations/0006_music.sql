-- Phase 3: music sessions, blocked members, and the music permissions.

-- A voice channel's player: queue, history, modes and what plays, kept so
-- music survives a restart. Encoded tracks stay on the server.
CREATE TABLE music_sessions (
    channel_id INTEGER PRIMARY KEY REFERENCES channels (id) ON DELETE CASCADE,
    state_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE music_blocks (
    user_id INTEGER PRIMARY KEY REFERENCES users (id) ON DELETE CASCADE,
    reason TEXT NOT NULL,
    blocked_by INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- @everyone may add and control music, as on new servers (bits 28, 29).
UPDATE roles SET permissions = permissions | 805306368
WHERE id = (SELECT CAST(value AS INTEGER) FROM server_meta WHERE key = 'everyone_role_id');
