-- Passwordless Magic-Link (PLAN.md Phase 2, geänderte Entscheidung).
-- Kein Passwort mehr → password_hash entfällt.

ALTER TABLE users DROP COLUMN password_hash;

-- Einmalige, kurzlebige Magic-Link-Tokens (nur als SHA-256-Hash gespeichert).
CREATE TABLE login_tokens (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email       TEXT NOT NULL,
    purpose     TEXT NOT NULL CHECK (purpose IN ('login', 'register')),
    -- Bei 'register': die Org, der beigetreten wird (Invite-Code bereits
    -- serverseitig aufgelöst). Bei 'login': NULL.
    org_id      UUID REFERENCES organizations(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX login_tokens_email_idx ON login_tokens(email);
