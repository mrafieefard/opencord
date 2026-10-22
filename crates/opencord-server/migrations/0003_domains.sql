-- Phase 3: domains the owner attaches, their check history, and invites
-- made for one of them.

CREATE TABLE domains (
    id INTEGER PRIMARY KEY,
    hostname TEXT NOT NULL UNIQUE,
    -- own, free_subdomain, tunnel or reverse_proxy
    kind TEXT NOT NULL,
    public_port INTEGER NOT NULL,
    -- pending, verified or dns_problem
    status TEXT NOT NULL,
    active BOOLEAN NOT NULL DEFAULT FALSE,
    is_primary BOOLEAN NOT NULL DEFAULT FALSE,
    -- When its checks started failing after it was verified.
    problem_since INTEGER,
    -- When it was last turned off; it keeps working for a while after.
    deactivated_at INTEGER,
    last_checked_at INTEGER,
    -- The last check's outcome, as JSON.
    last_result TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE domain_checks (
    id INTEGER PRIMARY KEY,
    domain_id INTEGER NOT NULL REFERENCES domains (id) ON DELETE CASCADE,
    checked_at INTEGER NOT NULL,
    result TEXT NOT NULL,
    -- The outcome, as JSON.
    details TEXT NOT NULL
);

CREATE INDEX domain_checks_by_domain ON domain_checks (domain_id, checked_at);

-- An invite for a domain stops working with the domain.
ALTER TABLE invites ADD COLUMN domain_id INTEGER REFERENCES domains (id) ON DELETE CASCADE;
