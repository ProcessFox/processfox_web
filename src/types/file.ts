/**
 * Dateien leben im S3-Objektspeicher unter `workspaces/<workspace_id>/`
 * (CLAUDE.md §5). Kein OS-Dateibaum, keine Verzeichnisse — eine flache
 * Liste pro Workspace.
 */
export interface WorkspaceFile {
  id: string;
  workspaceId: string;
  filename: string;
  sizeBytes: number;
  contentType: string;
  uploadedBy: string;
  uploadedAt: string;
}
