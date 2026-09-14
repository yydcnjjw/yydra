-- SPDX-License-Identifier: MIT OR Apache-2.0
-- Embed in the product's one explicit migration history; never migrate on server startup.
CREATE TABLE yydra_auth_accounts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    provider TEXT NOT NULL,
    subject TEXT NOT NULL,
    auth_epoch TEXT NOT NULL,
    UNIQUE (provider, subject)
);

-- Matches tower-sessions-sqlx-store 0.15.0's published PostgreSQL store.
CREATE SCHEMA IF NOT EXISTS tower_sessions;
CREATE TABLE tower_sessions.session (
    id TEXT PRIMARY KEY NOT NULL,
    data BYTEA NOT NULL,
    expiry_date TIMESTAMPTZ NOT NULL
);
CREATE INDEX yydra_auth_session_expiry ON tower_sessions.session (expiry_date);

-- An independent authorization gate prevents an in-flight session-store UPSERT
-- from restoring access after logout. Only successful login creates this record.
CREATE TABLE yydra_auth_active_sessions (
    id TEXT PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES yydra_auth_accounts(id),
    expires_at TIMESTAMPTZ NOT NULL,
    transport TEXT NOT NULL CHECK (transport IN ('web', 'native')),
    csrf_token TEXT NOT NULL
);
CREATE INDEX yydra_auth_active_expiry ON yydra_auth_active_sessions (expires_at);

CREATE TABLE yydra_auth_attempts (
    state_hash TEXT PRIMARY KEY,
    binding_hash TEXT NOT NULL,
    verifier TEXT NOT NULL,
    native_challenge TEXT,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX yydra_auth_attempt_expiry ON yydra_auth_attempts (expires_at);
CREATE TABLE yydra_auth_handoffs (
    code_hash TEXT PRIMARY KEY,
    account_id UUID NOT NULL REFERENCES yydra_auth_accounts(id),
    challenge TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX yydra_auth_handoff_expiry ON yydra_auth_handoffs (expires_at);
