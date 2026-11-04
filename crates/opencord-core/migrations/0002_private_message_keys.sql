-- Private message keys apart from the identity key (design 2026-11-01 §10).
-- The device was vouched for by the identity key, so it goes, with its
-- sessions, the devices it knew, and every chat. Friends, requests, blocks,
-- names, mutes and settings stay.

DELETE FROM sessions;
DELETE FROM peer_devices;
DELETE FROM device;
DELETE FROM messages;
DELETE FROM received_events;
UPDATE contacts SET conversation = 0, relay_server_id = NULL, last_activity_at = NULL,
    last_read_at = 0, hidden = 0;

-- This person's chat key (§6.1). The secret is sealed under the seal key.
CREATE TABLE chat_key (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    public BLOB NOT NULL,
    secret BLOB NOT NULL,
    created_at INTEGER NOT NULL
);

-- The chat key that signed each known device.
ALTER TABLE peer_devices ADD COLUMN chat_key BLOB NOT NULL DEFAULT x'';
