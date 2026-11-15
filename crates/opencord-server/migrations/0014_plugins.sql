-- Plugins (design 2026-11-07).

-- Plugin users: a users row nobody can sign in as (a random public key
-- with no private half), set for the plugin with this id. Kept after
-- uninstalling, so old messages keep their author.
ALTER TABLE users ADD COLUMN plugin_id TEXT;
CREATE UNIQUE INDEX users_by_plugin ON users (plugin_id) WHERE plugin_id IS NOT NULL;

-- A plugin's managed role.
ALTER TABLE roles ADD COLUMN plugin_id TEXT;

CREATE TABLE plugins (
    id TEXT PRIMARY KEY,
    user_id INTEGER NOT NULL UNIQUE REFERENCES users (id),
    role_id INTEGER NOT NULL UNIQUE REFERENCES roles (id),
    version TEXT NOT NULL,
    -- The installed version's folder name under data/plugins/<id>/.
    folder TEXT NOT NULL,
    -- The evaluated manifest, as JSON.
    manifest TEXT NOT NULL,
    signature TEXT NOT NULL CHECK (signature IN ('unsigned', 'signed', 'dev')),
    -- The pinned key (32 bytes) once a signed version was installed.
    public_key BLOB,
    github_check TEXT NOT NULL DEFAULT 'none'
        CHECK (github_check IN ('none', 'verified', 'mismatch', 'failed')),
    -- The admin's switch.
    running INTEGER NOT NULL,
    -- Set when it was turned off after failing; cleared when started.
    crashed_reason TEXT,
    license_accepted_by INTEGER REFERENCES users (id),
    installed_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE TABLE plugin_permissions (
    plugin_id TEXT NOT NULL REFERENCES plugins (id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    granted_by INTEGER NOT NULL REFERENCES users (id),
    granted_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, name)
);

CREATE TABLE plugin_config (
    plugin_id TEXT NOT NULL REFERENCES plugins (id) ON DELETE CASCADE,
    key TEXT NOT NULL,
    -- UTF-8 text, or sealed bytes for secrets.
    value BLOB NOT NULL,
    sealed INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, key)
);

CREATE TABLE plugin_files (
    plugin_id TEXT NOT NULL,
    path TEXT NOT NULL,
    size INTEGER NOT NULL,
    public INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, path)
);

CREATE TABLE plugin_dev_tokens (
    id INTEGER PRIMARY KEY,
    -- The plugin may not be installed yet.
    plugin_id TEXT NOT NULL,
    token_hash BLOB NOT NULL UNIQUE,
    created_by INTEGER NOT NULL REFERENCES users (id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    last_used_at INTEGER
);

-- Uninstalled with its data kept: a reinstall with the same id reconnects,
-- and must carry this key when one is set (design §9.6).
CREATE TABLE plugin_leftovers (
    id TEXT PRIMARY KEY,
    public_key BLOB,
    removed_at INTEGER NOT NULL
);
