-- Songs uploaded for the music player (design 2026-10-24). A song's file
-- is its upload's attachment row, kept in the new state 'music' with the
-- new kind 'music'. A song outlives the voice channel it was dropped in,
-- so the channel cascade becomes a trigger that spares songs.
CREATE TABLE attachments_new (
    id INTEGER PRIMARY KEY,
    uploader_id INTEGER NOT NULL REFERENCES users (id),
    channel_id INTEGER NOT NULL,
    message_id INTEGER REFERENCES messages (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL
        CHECK (state IN ('uploading', 'processing', 'ready', 'failed', 'sent', 'music')),
    kind TEXT NOT NULL
        CHECK (kind IN ('file', 'image', 'video', 'audio', 'voice', 'music')),
    filename TEXT NOT NULL,
    content_type TEXT NOT NULL DEFAULT 'application/octet-stream',
    size_bytes INTEGER NOT NULL,
    received_bytes INTEGER NOT NULL DEFAULT 0,
    storage_id INTEGER NOT NULL REFERENCES storage_backends (id),
    key_enc BLOB NOT NULL,
    width INTEGER,
    height INTEGER,
    duration_ms INTEGER,
    animated BOOLEAN NOT NULL DEFAULT FALSE,
    waveform BLOB,
    has_thumbnail BOOLEAN NOT NULL DEFAULT FALSE,
    thumbnail_width INTEGER,
    thumbnail_height INTEGER,
    error_code INTEGER,
    error_message TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

INSERT INTO attachments_new SELECT * FROM attachments;
-- Its trigger goes with it; the implicit delete fires none.
DROP TABLE attachments;
ALTER TABLE attachments_new RENAME TO attachments;

CREATE INDEX attachments_by_message ON attachments (message_id);
CREATE INDEX attachments_by_state ON attachments (state, updated_at);
CREATE INDEX attachments_by_storage ON attachments (storage_id);
CREATE INDEX attachments_by_uploader ON attachments (uploader_id, state);

CREATE TRIGGER channels_delete_attachments AFTER DELETE ON channels
BEGIN
    DELETE FROM attachments WHERE channel_id = OLD.id AND state != 'music';
END;

CREATE TRIGGER attachments_queue_blobs AFTER DELETE ON attachments
BEGIN
    INSERT INTO blob_deletions (storage_id, object_key)
    VALUES (OLD.storage_id, 'a/' || OLD.id);
    INSERT INTO blob_deletions (storage_id, object_key)
    SELECT OLD.storage_id, 't/' || OLD.id WHERE OLD.has_thumbnail;
END;

-- The songs, kept once per server by their SHA-256; a song goes with its
-- file's row.
CREATE TABLE music_local_songs (
    id INTEGER PRIMARY KEY REFERENCES attachments (id) ON DELETE CASCADE,
    sha256 BLOB NOT NULL UNIQUE,
    size_bytes INTEGER NOT NULL,
    filename TEXT NOT NULL,
    title TEXT NOT NULL,
    artist TEXT NOT NULL,
    duration_ms INTEGER NOT NULL,
    uploader_id INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);

-- Who has each song as their own: whoever uploaded it, and anyone who
-- later dropped the same file. A song nobody owns is deleted.
CREATE TABLE music_local_song_owners (
    song_id INTEGER NOT NULL REFERENCES music_local_songs (id) ON DELETE CASCADE,
    user_id INTEGER NOT NULL,
    added_at INTEGER NOT NULL,
    PRIMARY KEY (song_id, user_id)
);

CREATE INDEX music_local_song_owners_user ON music_local_song_owners (user_id);
