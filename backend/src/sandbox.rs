//! Workspace-Sandbox (CLAUDE.md §5/§9). Jeder S3-Key muss unter
//! `workspaces/<workspace_id>/` liegen, keine `..`-Segmente. Dateinamen
//! werden vor dem Bilden des Keys saniert (kein Pfad-Traversal).

use uuid::Uuid;

use crate::error::ApiError;

pub fn workspace_prefix(workspace_id: Uuid) -> String {
    format!("workspaces/{workspace_id}/")
}

/// Validiert, dass `s3_key` zum Workspace gehört.
pub fn ensure_in_workspace(workspace_id: Uuid, s3_key: &str) -> Result<(), ApiError> {
    let prefix = workspace_prefix(workspace_id);
    if !s3_key.starts_with(&prefix) {
        return Err(ApiError::forbidden("Pfad außerhalb des Workspace"));
    }
    if s3_key.contains("..") {
        return Err(ApiError::forbidden("Pfad-Traversal erkannt"));
    }
    Ok(())
}

/// Reduziert einen hochgeladenen Dateinamen auf einen sicheren Basisnamen
/// (keine Verzeichnisse, kein `..`, keine Steuerzeichen).
pub fn sanitize_filename(name: &str) -> Result<String, ApiError> {
    let base = name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches('.')
        .to_string();
    if base.is_empty()
        || base == ".."
        || base.contains('\0')
        || base.chars().any(|c| c.is_control())
    {
        return Err(ApiError::BadRequest("Ungültiger Dateiname".into()));
    }
    Ok(base)
}

/// Baut den S3-Key für eine Datei in einem Workspace.
pub fn workspace_key(workspace_id: Uuid, filename: &str) -> String {
    format!("{}{}", workspace_prefix(workspace_id), filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_paths_and_rejects_traversal() {
        assert_eq!(sanitize_filename("a/b/report.docx").unwrap(), "report.docx");
        assert_eq!(sanitize_filename("..\\..\\evil.txt").unwrap(), "evil.txt");
        assert!(sanitize_filename("..").is_err());
        assert!(sanitize_filename("   ").is_err());
        assert!(sanitize_filename("bad\u{0}name").is_err());
    }

    #[test]
    fn ensure_in_workspace_guards_prefix_and_dotdot() {
        let w = Uuid::new_v4();
        let ok = workspace_key(w, "file.txt");
        assert!(ensure_in_workspace(w, &ok).is_ok());
        assert!(ensure_in_workspace(w, "workspaces/other/x").is_err());
        assert!(ensure_in_workspace(w, &format!("workspaces/{w}/../x")).is_err());
    }
}
