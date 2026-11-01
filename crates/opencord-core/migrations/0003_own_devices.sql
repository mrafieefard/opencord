-- This person's other devices with chat keys of their own (design
-- 2026-11-01 §7).
CREATE TABLE own_devices (
    device_id BLOB PRIMARY KEY,
    -- As last listed.
    curve25519 BLOB NOT NULL,
    ed25519 BLOB NOT NULL,
    chat_key BLOB NOT NULL,
    -- 0 listed with another chat key, 1 "Not mine".
    state INTEGER NOT NULL DEFAULT 0,
    -- Sealed: the chat key it offered, until They match or They don't match,
    -- with the keys the offer came from (its Olm message decrypted with that
    -- Curve25519 key) and the chat key it was listed with then.
    share BLOB,
    share_curve25519 BLOB,
    share_ed25519 BLOB,
    share_chat_key BLOB,
    seen_at INTEGER NOT NULL
);
