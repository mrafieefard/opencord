-- Private message keys apart from the identity key (design 2026-11-01).
-- Devices are now signed by their owner's chat key, so every device, its
-- keys and whatever waits for it go; friends, requests, blocks and privacy
-- settings stay.

DELETE FROM private_pending;
DELETE FROM private_one_time_keys;
DELETE FROM private_recent_sends;
DELETE FROM private_devices;

-- The public half of the owner's chat key, which signed the device. The
-- table is empty: the default only satisfies SQLite.
ALTER TABLE private_devices ADD COLUMN chat_key BLOB NOT NULL DEFAULT x'';
