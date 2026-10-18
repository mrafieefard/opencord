-- Phase 3: certificates for domains, the ACME account, and DNS providers
-- for the DNS challenge. Private keys, account credentials and tokens are
-- sealed with secrets.key.

CREATE TABLE domain_certificates (
    domain_id INTEGER PRIMARY KEY REFERENCES domains (id) ON DELETE CASCADE,
    chain_pem TEXT NOT NULL,
    key_enc BLOB NOT NULL,
    issuer TEXT NOT NULL,
    not_before INTEGER NOT NULL,
    not_after INTEGER NOT NULL,
    -- From Let's Encrypt's test environment: kept, not served.
    is_test BOOLEAN NOT NULL DEFAULT FALSE,
    -- Uploaded by the owner: never renewed here.
    uploaded BOOLEAN NOT NULL DEFAULT FALSE,
    created_at INTEGER NOT NULL
);

CREATE TABLE acme_accounts (
    -- One account per ACME directory (production, test environment).
    directory TEXT PRIMARY KEY NOT NULL,
    credentials_enc BLOB NOT NULL,
    email TEXT,
    created_at INTEGER NOT NULL
);

CREATE TABLE dns_providers (
    id INTEGER PRIMARY KEY,
    -- cloudflare, duckdns or desec
    kind TEXT NOT NULL UNIQUE,
    token_enc BLOB NOT NULL,
    created_at INTEGER NOT NULL
);
