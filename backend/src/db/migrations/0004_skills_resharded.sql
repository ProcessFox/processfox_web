-- Phase 6c-2: Skill-Resharding.
--
-- Vor 6c-2 gab es genau einen Catch-all-Skill `"files"`, der alle Tools
-- bündelte. Mit der neuen `SkillRegistry` (aus `backend/skills_builtin/`)
-- wird das auf fünf semantisch geschnittene Skills aufgeteilt:
--   folder-search, document-read, document-write, table-read, table-write
--
-- Diese Migration setzt alle Agents, die noch `skills = ["files"]` haben,
-- auf die volle 5er-Liste um. Tool-Menge bleibt identisch — semantisch
-- äquivalente Migration. Idempotent: nur Treffer auf dem exakten Pattern.

UPDATE agents
SET skills = '["folder-search","document-read","document-write","table-read","table-write"]'::jsonb
WHERE skills = '["files"]'::jsonb;
