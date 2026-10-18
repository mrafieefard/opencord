-- Phase 3: Lavalink nodes music plays on. Passwords are sealed with
-- secrets.key.

CREATE TABLE lavalink_nodes (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    host TEXT NOT NULL,
    port INTEGER NOT NULL,
    secure BOOLEAN NOT NULL,
    password_enc BLOB NOT NULL,
    -- preferred, normal or backup
    priority TEXT NOT NULL DEFAULT 'normal',
    -- 0 for no cap
    max_players INTEGER NOT NULL DEFAULT 0,
    strict_address BOOLEAN NOT NULL DEFAULT TRUE,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    draining BOOLEAN NOT NULL DEFAULT FALSE,
    -- The node's session, resumed after Opencord restarts.
    resume_session TEXT,
    created_at INTEGER NOT NULL
);
