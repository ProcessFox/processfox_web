//! Datei-Storage als lokales Verzeichnis (Coolify Persistent Volume,
//! CLAUDE.md §5). Single-Instance, self-hosted. Keys haben die Form
//! `workspaces/<workspace_id>/<datei>` (validiert in `crate::sandbox`)
//! und werden direkt unter `root` abgelegt.

use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Storage {
    pub root: PathBuf,
}

impl Storage {
    pub fn new(root: &str) -> Self {
        Self {
            root: PathBuf::from(root),
        }
    }

    /// Absoluter Pfad für einen (bereits sandbox-validierten) Storage-Key.
    pub fn path(&self, key: &str) -> PathBuf {
        self.root.join(key)
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
}
