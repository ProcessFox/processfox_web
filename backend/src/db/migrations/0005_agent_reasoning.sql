-- Phase 6d-2: per-Agent-Toggle für Reasoning/Extended Thinking. Default
-- aus — Reasoning kostet Tokens (Anthropic separates `budget_tokens`,
-- OpenRouter modellabhängig), und das soll der Editor explizit
-- aktivieren statt überraschend abzurechnen.
ALTER TABLE agents
    ADD COLUMN reasoning_enabled BOOLEAN NOT NULL DEFAULT false;
