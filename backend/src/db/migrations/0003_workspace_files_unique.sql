-- Phase 5: gleicher Dateiname im Workspace = dieselbe Datei (überschreiben
-- statt Duplikat, PLAN.md Planungslücke #6).
ALTER TABLE workspace_files
    ADD CONSTRAINT workspace_files_ws_filename_uniq
    UNIQUE (workspace_id, filename);
