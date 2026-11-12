-- no-transaction
-- Plugin channels (design 2026-11-07 §14.2): the kind 'plugin', drawn by the
-- plugin `plugin_id` as its channel type `type_key`.
--
-- SQLite can't change a CHECK, so `channels` is rebuilt by the twelve steps
-- of https://www.sqlite.org/lang_altertable.html#otheralter. Foreign keys are
-- off for this connection meanwhile: with them on, dropping the old table
-- would delete every overwrite, message and music session (ON DELETE
-- CASCADE). The PRAGMA does nothing inside a transaction, hence no-transaction
-- and the BEGIN and COMMIT below.

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE channels_new (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('text', 'voice', 'category', 'plugin')),
    name TEXT NOT NULL,
    topic TEXT,
    parent_id INTEGER REFERENCES channels (id) ON DELETE SET NULL,
    position INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    bitrate INTEGER NOT NULL DEFAULT 64000,
    user_limit INTEGER NOT NULL DEFAULT 0,
    text_in_voice BOOLEAN NOT NULL DEFAULT TRUE,
    plugin_id TEXT,
    type_key TEXT,
    CHECK ((kind = 'plugin') = (plugin_id IS NOT NULL AND type_key IS NOT NULL))
);

INSERT INTO channels_new (id, kind, name, topic, parent_id, position, created_at,
                          bitrate, user_limit, text_in_voice)
SELECT id, kind, name, topic, parent_id, position, created_at,
       bitrate, user_limit, text_in_voice
FROM channels;

DROP TABLE channels;
ALTER TABLE channels_new RENAME TO channels;

-- `channels` had no index; its one trigger went with the old table.
CREATE TRIGGER channels_delete_attachments AFTER DELETE ON channels
BEGIN
    DELETE FROM attachments WHERE channel_id = OLD.id AND state != 'music';
END;

-- Step 10: a row whose reference broke fails the migration before COMMIT,
-- so none of it is kept and the server doesn't start.
CREATE TEMP TABLE migration_0016_check (
    broken INTEGER NOT NULL CONSTRAINT foreign_keys_intact CHECK (broken = 0)
);
INSERT INTO migration_0016_check SELECT COUNT(*) FROM pragma_foreign_key_check;
DROP TABLE migration_0016_check;

COMMIT;

PRAGMA foreign_keys = ON;
