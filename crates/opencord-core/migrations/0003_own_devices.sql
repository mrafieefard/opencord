-- This person's other devices with chat keys of their own (design
-- 2026-11-01 §7).
CREATE TABLE own_devices (
    device_id BLOB PRIMARY KEY,
    ed25519 BLOB NOT NULL,
    chat_key BLOB NOT NULL,
    -- 0 listed with another chat key, 1 "Not mine".
    state INTEGER NOT NULL DEFAULT 0,
    -- Sealed: the chat key it offered, until They match or They don't match.
    share BLOB,
    seen_at INTEGER NOT NULL
);
