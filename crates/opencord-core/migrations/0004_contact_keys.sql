-- Whose chat key this device trusts for each contact (design 2026-11-01 §8).
CREATE TABLE contact_keys (
    public_key BLOB PRIMARY KEY,
    -- Their chat key: their devices signed by it get messages.
    pinned BLOB NOT NULL,
    -- Another chat key on their listed devices, offered in the banner.
    candidate BLOB,
    -- The key pinned before this one: its devices are never offered again.
    replaced BLOB,
    -- The two chat keys when They match was tapped (mine, theirs).
    verified_mine BLOB,
    verified_theirs BLOB,
    -- They don't match was tapped.
    mismatch INTEGER NOT NULL DEFAULT 0,
    -- At the last look a device of theirs had the pinned key.
    reachable INTEGER NOT NULL DEFAULT 1
);
