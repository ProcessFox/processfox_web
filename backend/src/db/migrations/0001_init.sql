-- ProcessFox Web — Initiales Schema (CLAUDE.md §8).
-- Postgres 13+ (gen_random_uuid() ist eingebaut).

CREATE TABLE organizations (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    -- 6-stelliger Invite-Code; Pflicht bei jeder Registrierung. Owner kann
    -- ihn rotieren. Erste Org wird manuell per SQL angelegt.
    invite_code TEXT NOT NULL UNIQUE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    org_role      TEXT NOT NULL CHECK (org_role IN ('owner', 'member')),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX users_org_id_idx ON users(org_id);

CREATE TABLE refresh_tokens (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id    UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX refresh_tokens_user_id_idx ON refresh_tokens(user_id);

CREATE TABLE workspaces (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id     UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX workspaces_org_id_idx ON workspaces(org_id);

CREATE TABLE workspace_members (
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    user_id      UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role         TEXT NOT NULL CHECK (role IN ('editor', 'viewer')),
    PRIMARY KEY (workspace_id, user_id)
);

CREATE TABLE agents (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id       UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    name               TEXT NOT NULL,
    icon               TEXT NOT NULL DEFAULT 'Bot',
    system_prompt      TEXT NOT NULL DEFAULT '',
    provider           TEXT,
    model_id           TEXT,
    skills             JSONB NOT NULL DEFAULT '[]'::jsonb,
    skill_settings     JSONB NOT NULL DEFAULT '{}'::jsonb,
    hitl_disabled      BOOLEAN NOT NULL DEFAULT false,
    attachments        JSONB NOT NULL DEFAULT '{}'::jsonb,
    delegation_profile JSONB,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX agents_workspace_id_idx ON agents(workspace_id);

CREATE TABLE chat_messages (
    id         UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id   UUID NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    role       TEXT NOT NULL,
    content    JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX chat_messages_agent_id_idx ON chat_messages(agent_id, created_at);

CREATE TABLE workspace_files (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    workspace_id UUID NOT NULL REFERENCES workspaces(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    s3_key       TEXT NOT NULL,
    size_bytes   BIGINT NOT NULL,
    content_type TEXT NOT NULL,
    uploaded_by  UUID NOT NULL REFERENCES users(id),
    uploaded_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX workspace_files_workspace_id_idx ON workspace_files(workspace_id);

CREATE TABLE org_settings (
    org_id           UUID PRIMARY KEY REFERENCES organizations(id) ON DELETE CASCADE,
    default_provider TEXT,
    default_model    TEXT,
    first_run_done   BOOLEAN NOT NULL DEFAULT false,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE api_keys (
    org_id        UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    provider      TEXT NOT NULL,
    -- AES-256-GCM-Chiffrat (CLAUDE.md §9). Nie im Klartext, nie ans Frontend.
    encrypted_key BYTEA NOT NULL,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (org_id, provider)
);
