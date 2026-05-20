-- Phase 6e (CLAUDE.md §4, Rollen-Modell vereinfacht): die zweite Rollen-
-- Ebene `workspace_members.role` (editor|viewer) ist abgeschafft. Wer im
-- Workspace ist, ist gleichberechtigter Nutzer. Admin-Rechte hängen
-- ausschließlich an `users.org_role = 'owner'`.
--
-- Bestandsdaten: alle Mitgliedschaften werden bewahrt (egal ob sie vorher
-- 'editor' oder 'viewer' waren) — niemand verliert dadurch Zugriff. Wer
-- vorher als Viewer angelegt war, hat jetzt Lese- und Schreibrechte.
-- Das ist beabsichtigt; siehe Migration-Notes in CLAUDE.md §1a.

ALTER TABLE workspace_members DROP COLUMN role;
