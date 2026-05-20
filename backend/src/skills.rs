//! Skill-Registry (Phase 6c-1). Parser für `SKILL.md`-Dateien
//! (YAML-Frontmatter zwischen `---`-Linien + Markdown-Body) plus eine
//! `SkillRegistry`, die ein Verzeichnis rekursiv einliest. Reine
//! Library-Schicht — die Verdrahtung in `AppState` und in den
//! System-Prompt-Composer kommt in Phase 6c-2/6c-3.
//!
//! Vergleichbar mit `processfox_local/src-tauri/src/core/skill/`, aber
//! mit `ApiError`-Fehlertyp statt `CoreError` und Web-spezifischem
//! Default-Verhalten (Sprache `en`, HITL aus, keine Attachments).

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};

/// Ein Skill ist ein Bundle aus Tools + Markdown-Anleitung
/// (`body`), die dem LLM erklärt, wann und wie es die Tools nutzt.
/// Frontmatter-Felder camelCase-renamt für die TS-Bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub hitl: SkillHitl,
    /// Attachment-Slots, die der Skill konsumiert (Frontend nutzt das,
    /// um z. B. den Vorlagen-Picker bedingt anzuzeigen). `alias`
    /// erlaubt SKILL.md-Autoren die YAML-natürliche snake_case-Form.
    #[serde(default, alias = "accepts_attachments")]
    pub accepts_attachments: Vec<String>,
    #[serde(default = "default_language")]
    pub language: String,
    /// Markdown unter dem Frontmatter. Wird vom Composer **nicht**
    /// in den initialen System-Prompt gepackt — nur via `read_skill`-
    /// Tool-Result (Progressive Disclosure, Phase 6c-3).
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHitl {
    /// Default-HITL-Verhalten des Skills (true = Schreiboperationen
    /// brauchen Freigabe). Tools, die in `per_tool` explizit gesetzt
    /// sind, überschreiben den Default.
    #[serde(default)]
    pub default: bool,
    /// Pro-Tool-Override. `false` für ein bestimmtes Tool heißt: in
    /// diesem Skill-Kontext **kein** HITL. `alias` siehe oben.
    #[serde(default, alias = "per_tool")]
    pub per_tool: BTreeMap<String, bool>,
}

fn default_language() -> String {
    "en".to_string()
}

/// Trennt YAML-Frontmatter und Markdown-Body und deserialisiert das
/// Frontmatter in einen `Skill`. Strukturelle Fehler werden mit
/// aussagekräftiger Meldung als `BadRequest` zurückgegeben — der
/// Aufrufer (Bootstrap) bricht harte ab.
pub fn parse_skill_md(input: &str) -> ApiResult<Skill> {
    let stripped = input
        .strip_prefix("---\n")
        .ok_or_else(|| ApiError::BadRequest("SKILL.md muss mit '---' beginnen".to_string()))?;
    let end = stripped.find("\n---\n").ok_or_else(|| {
        ApiError::BadRequest("SKILL.md: kein Frontmatter-Ende ('---') gefunden".to_string())
    })?;
    let (frontmatter, rest) = stripped.split_at(end);
    // `rest` beginnt mit `\n---\n`; den Trenner überspringen.
    let body = rest.trim_start_matches("\n---\n").trim_start_matches('\n');

    let mut skill: Skill = serde_yaml::from_str(frontmatter)
        .map_err(|e| ApiError::BadRequest(format!("SKILL.md YAML-Fehler: {e}")))?;
    if skill.name.trim().is_empty() {
        return Err(ApiError::BadRequest(
            "SKILL.md: `name` darf nicht leer sein".to_string(),
        ));
    }
    if skill.title.trim().is_empty() {
        return Err(ApiError::BadRequest(format!(
            "SKILL.md '{}': `title` darf nicht leer sein",
            skill.name
        )));
    }
    skill.body = body.to_string();
    Ok(skill)
}

/// In-Memory-Index der geladenen Skills. Lookup per `name`, Listung
/// alphabetisch (deterministisch → der System-Prompt-Skill-Listing-
/// Block ist über Requests hinweg stabil cacheable).
#[derive(Debug, Clone, Default)]
pub struct SkillRegistry {
    by_name: HashMap<String, Arc<Skill>>,
}

impl SkillRegistry {
    /// Lädt rekursiv alle `SKILL.md`-Dateien unter `root`. Fehler beim
    /// Parsen oder doppelter Skill-Name → harter Abbruch (built-ins
    /// sind unter unserer Kontrolle; wir wollen keine versteckten Bugs).
    pub fn load_from_dir(root: &Path) -> ApiResult<Self> {
        let mut registry = SkillRegistry::default();
        let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            let entries = std::fs::read_dir(&dir).map_err(|e| {
                ApiError::Internal(anyhow::anyhow!(
                    "Skill-Verzeichnis {} nicht lesbar: {e}",
                    dir.display()
                ))
            })?;
            for entry in entries.flatten() {
                let path = entry.path();
                let Ok(ft) = entry.file_type() else { continue };
                if ft.is_dir() {
                    stack.push(path);
                    continue;
                }
                let is_skill_md = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.eq_ignore_ascii_case("SKILL.md"))
                    .unwrap_or(false);
                if !is_skill_md {
                    continue;
                }
                let bytes = std::fs::read(&path).map_err(|e| {
                    ApiError::Internal(anyhow::anyhow!(
                        "SKILL.md {} nicht lesbar: {e}",
                        path.display()
                    ))
                })?;
                let text = String::from_utf8(bytes).map_err(|_| {
                    ApiError::BadRequest(format!("SKILL.md {} ist nicht UTF-8", path.display()))
                })?;
                let skill = parse_skill_md(&text).map_err(|e| match e {
                    ApiError::BadRequest(msg) => {
                        ApiError::BadRequest(format!("{} ({})", msg, path.display()))
                    }
                    other => other,
                })?;
                if registry.by_name.contains_key(&skill.name) {
                    return Err(ApiError::BadRequest(format!(
                        "Doppelter Skill-Name '{}' (gefunden in {})",
                        skill.name,
                        path.display()
                    )));
                }
                registry.by_name.insert(skill.name.clone(), Arc::new(skill));
            }
        }
        Ok(registry)
    }

    pub fn get(&self, name: &str) -> Option<Arc<Skill>> {
        self.by_name.get(name).cloned()
    }

    /// Alle bekannten Skills, alphabetisch nach `name` sortiert.
    pub fn list(&self) -> Vec<Arc<Skill>> {
        let mut out: Vec<Arc<Skill>> = self.by_name.values().cloned().collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    pub fn len(&self) -> usize {
        self.by_name.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_name.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "\
---
name: example
title: Beispiel
description: Demo skill.
---
Body text here.
";

    const FULL: &str = "\
---
name: folder-search
title: Ordner durchsuchen
description: Find files.
icon: FolderSearch
tools:
  - list_files
  - read_file
hitl:
  default: false
  perTool:
    rewrite_file: true
acceptsAttachments:
  - template
language: de
---

Choose the right tool:

- list_files first.
";

    #[test]
    fn parse_minimal_skill_md() {
        let skill = parse_skill_md(MINIMAL).unwrap();
        assert_eq!(skill.name, "example");
        assert_eq!(skill.title, "Beispiel");
        assert_eq!(skill.description, "Demo skill.");
        assert!(skill.icon.is_none());
        assert!(skill.tools.is_empty());
        assert!(!skill.hitl.default);
        assert!(skill.hitl.per_tool.is_empty());
        assert!(skill.accepts_attachments.is_empty());
        assert_eq!(skill.language, "en"); // Default
        assert_eq!(skill.body, "Body text here.\n");
    }

    #[test]
    fn parse_full_skill_md() {
        let skill = parse_skill_md(FULL).unwrap();
        assert_eq!(skill.name, "folder-search");
        assert_eq!(skill.title, "Ordner durchsuchen");
        assert_eq!(skill.icon.as_deref(), Some("FolderSearch"));
        assert_eq!(skill.tools, vec!["list_files", "read_file"]);
        assert!(!skill.hitl.default);
        assert_eq!(skill.hitl.per_tool.get("rewrite_file"), Some(&true));
        assert_eq!(skill.accepts_attachments, vec!["template"]);
        assert_eq!(skill.language, "de");
        assert!(skill.body.starts_with("Choose the right tool:"));
        assert!(skill.body.contains("list_files first."));
    }

    #[test]
    fn snake_case_aliases_for_yaml_authors() {
        let txt = "\
---
name: x
title: X
description: x
accepts_attachments:
  - template
hitl:
  per_tool:
    write_xlsx: false
---
";
        let skill = parse_skill_md(txt).unwrap();
        assert_eq!(skill.accepts_attachments, vec!["template"]);
        assert_eq!(skill.hitl.per_tool.get("write_xlsx"), Some(&false));
    }

    #[test]
    fn rejects_missing_frontmatter() {
        let err = parse_skill_md("# no frontmatter\nbody\n").unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "{err:?}");
    }

    #[test]
    fn rejects_unclosed_frontmatter() {
        let err = parse_skill_md("---\nname: x\ntitle: X\ndescription: d\n").unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "{err:?}");
    }

    #[test]
    fn rejects_invalid_yaml() {
        // `:` ohne Schlüssel ist kaputt
        let txt = "---\nname: x\n:::\n---\nbody\n";
        let err = parse_skill_md(txt).unwrap_err();
        assert!(matches!(err, ApiError::BadRequest(_)), "{err:?}");
    }

    #[test]
    fn rejects_empty_name() {
        let txt = "---\nname: \"\"\ntitle: X\ndescription: d\n---\n";
        let err = parse_skill_md(txt).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => assert!(msg.contains("name"), "{msg}"),
            other => panic!("falscher Fehlertyp: {other:?}"),
        }
    }

    #[test]
    fn rejects_empty_title() {
        let txt = "---\nname: x\ntitle: \"\"\ndescription: d\n---\n";
        let err = parse_skill_md(txt).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => assert!(msg.contains("title"), "{msg}"),
            other => panic!("falscher Fehlertyp: {other:?}"),
        }
    }

    fn write_skill(dir: &Path, sub: &str, content: &str) {
        let path = dir.join(sub).join("SKILL.md");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }

    fn temp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("pfx-skills-{tag}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn registry_loads_and_lists_deterministically() {
        let root = temp_dir("ok");
        // Files in nicht-alphabetischer Erstell-Reihenfolge.
        write_skill(
            &root,
            "table-read",
            "---\nname: table-read\ntitle: T\ndescription: d\n---\n",
        );
        write_skill(
            &root,
            "folder-search",
            "---\nname: folder-search\ntitle: F\ndescription: d\n---\n",
        );
        write_skill(
            &root,
            "document-read",
            "---\nname: document-read\ntitle: D\ndescription: d\n---\n",
        );
        let reg = SkillRegistry::load_from_dir(&root).unwrap();
        assert_eq!(reg.len(), 3);
        let names: Vec<String> = reg.list().iter().map(|s| s.name.clone()).collect();
        assert_eq!(names, vec!["document-read", "folder-search", "table-read"]);
        assert!(reg.get("folder-search").is_some());
        assert!(reg.get("nope").is_none());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn registry_rejects_duplicate_names() {
        let root = temp_dir("dupe");
        write_skill(&root, "a", "---\nname: x\ntitle: A\ndescription: d\n---\n");
        write_skill(&root, "b", "---\nname: x\ntitle: B\ndescription: d\n---\n");
        let err = SkillRegistry::load_from_dir(&root).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => assert!(msg.contains("Doppelter"), "{msg}"),
            other => panic!("falscher Fehlertyp: {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn registry_surfaces_parse_errors_with_path() {
        let root = temp_dir("bad");
        write_skill(&root, "broken", "kein frontmatter\n");
        let err = SkillRegistry::load_from_dir(&root).unwrap_err();
        match err {
            ApiError::BadRequest(msg) => {
                assert!(msg.contains("---"), "{msg}");
                assert!(msg.contains("SKILL.md"), "{msg}");
            }
            other => panic!("falscher Fehlertyp: {other:?}"),
        }
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn registry_ignores_non_skill_files() {
        let root = temp_dir("mixed");
        write_skill(
            &root,
            "real",
            "---\nname: real\ntitle: R\ndescription: d\n---\n",
        );
        // Junk-Dateien werden ignoriert (keine SKILL.md-Name-Übereinstimmung).
        std::fs::write(root.join("real").join("README.md"), "ignore me").unwrap();
        std::fs::write(root.join("loose.txt"), "junk").unwrap();
        let reg = SkillRegistry::load_from_dir(&root).unwrap();
        assert_eq!(reg.len(), 1);
        std::fs::remove_dir_all(&root).ok();
    }
}
