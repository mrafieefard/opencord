-- Media uploads (design 2026-10-18): storage backends, attachments, and the
-- queue of stored objects to delete. Files are encrypted; their keys are
-- sealed with secrets.key (key_enc), as are S3 secrets (secret_enc).

CREATE TABLE storage_backends (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('local', 's3')),
    name TEXT NOT NULL,
    endpoint TEXT,
    region TEXT,
    bucket TEXT,
    key_prefix TEXT NOT NULL DEFAULT '',
    path_style BOOLEAN NOT NULL DEFAULT FALSE,
    allow_http BOOLEAN NOT NULL DEFAULT FALSE,
    access_key_id TEXT,
    secret_enc BLOB,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    last_error TEXT,
    created_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX storage_backends_one_active ON storage_backends (active) WHERE active;

INSERT INTO storage_backends (id, kind, name, active, created_at)
VALUES (1, 'local', 'This server', TRUE, 0);

CREATE TABLE attachments (
    id INTEGER PRIMARY KEY,
    uploader_id INTEGER NOT NULL REFERENCES users (id),
    channel_id INTEGER NOT NULL REFERENCES channels (id) ON DELETE CASCADE,
    message_id INTEGER REFERENCES messages (id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL
        CHECK (state IN ('uploading', 'processing', 'ready', 'failed', 'sent')),
    kind TEXT NOT NULL CHECK (kind IN ('file', 'image', 'video', 'audio', 'voice')),
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

CREATE INDEX attachments_by_message ON attachments (message_id);
CREATE INDEX attachments_by_state ON attachments (state, updated_at);
CREATE INDEX attachments_by_storage ON attachments (storage_id);
CREATE INDEX attachments_by_uploader ON attachments (uploader_id, state);

CREATE TABLE blob_deletions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    storage_id INTEGER NOT NULL,
    object_key TEXT NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt_at INTEGER NOT NULL DEFAULT 0
);

-- Every way a row goes (cancel, expiry, message delete, channel cascade)
-- queues its objects for deletion.
CREATE TRIGGER attachments_queue_blobs AFTER DELETE ON attachments
BEGIN
    INSERT INTO blob_deletions (storage_id, object_key)
    VALUES (OLD.storage_id, 'a/' || OLD.id);
    INSERT INTO blob_deletions (storage_id, object_key)
    SELECT OLD.storage_id, 't/' || OLD.id WHERE OLD.has_thumbnail;
END;

ALTER TABLE roles ADD COLUMN max_upload_bytes INTEGER;

-- Attach files (bit 10) was reserved and did nothing until now; Send voice
-- messages (bit 14) is new. Existing servers' @everyone gets both, and 50 MB.
UPDATE roles
SET max_upload_bytes = 52428800,
    permissions = permissions | 1024 | 16384
WHERE id = (SELECT CAST(value AS INTEGER) FROM server_meta WHERE key = 'everyone_role_id');
