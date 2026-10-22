-- Phase 2: voice channel settings, the soundboard, and the new defaults for
-- @everyone. Voice and soundboard settings live in server_meta.

ALTER TABLE channels ADD COLUMN bitrate INTEGER NOT NULL DEFAULT 64000;
ALTER TABLE channels ADD COLUMN user_limit INTEGER NOT NULL DEFAULT 0;
ALTER TABLE channels ADD COLUMN text_in_voice BOOLEAN NOT NULL DEFAULT TRUE;

-- USE_VOICE_ACTIVITY, USE_SOUNDBOARD and USE_EXTERNAL_SOUNDS (bits 24 to
-- 26) are on for @everyone on new servers; give existing servers the same.
UPDATE roles SET permissions = permissions | 117440512
WHERE id = (
    SELECT CAST(value AS INTEGER) FROM server_meta WHERE key = 'everyone_role_id'
);

CREATE TABLE soundboard_sounds (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    emoji TEXT,
    volume INTEGER NOT NULL,
    sha256 BLOB NOT NULL,
    size_bytes INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    channel_count INTEGER NOT NULL,
    uploader_id INTEGER REFERENCES users (id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE external_sounds (
    sha256 BLOB PRIMARY KEY NOT NULL,
    size_bytes INTEGER NOT NULL,
    duration_ms INTEGER NOT NULL,
    uploaded_by INTEGER REFERENCES users (id) ON DELETE SET NULL,
    last_used_at INTEGER NOT NULL
);
