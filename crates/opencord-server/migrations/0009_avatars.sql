-- Profile pictures: the SHA-256 of a user's picture, stored as
-- data/avatars/<hash>.png; NULL for none. The server icon's is in
-- server_meta (icon_hash).
ALTER TABLE users ADD COLUMN avatar_hash TEXT;
