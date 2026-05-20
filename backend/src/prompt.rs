//! System-Prompt-Composer (Phase 6c-3). Setzt den System-Prompt aus
//! Bausteinen zusammen: Datum-Anker, Agent-Vorgabe, Workspace-Übersicht,
//! Skill-Listing (nur Titel + Description + Tool-Namen — **keine**
//! Tool-Schemas, **keine** Skill-Bodies), Thoroughness-Policy und
//! Sprach-Direktive.
//!
//! Skill-Bodies kommen nicht hier rein — die holt das LLM via
//! `read_skill`-Tool ab (Progressive Disclosure). Das macht den
//! initialen Kontext schlank und schützt vor Tool-Choice-Verwirrung.

use sqlx::PgPool;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;
use uuid::Uuid;

use crate::error::{ApiError, ApiResult};
use crate::skills::SkillRegistry;

/// Maximale Anzahl Workspace-Dateinamen im System-Prompt-Block, bevor
/// gekürzt wird. 30 reicht für Orientierung; bei mehr Dateien gibt es
/// `list_files` als Tool.
const WORKSPACE_TREE_LIMIT: usize = 30;

/// Komponiert den vollständigen System-Prompt für einen Tool-Loop-
/// Schritt. `loaded_skills` ist im aktuellen Run-Lifecycle leer und
/// wird vom chat.rs-Loop verwaltet — er kommt nur dann ins Spiel, wenn
/// wir die Skill-Liste so rendern, dass geladene Skills markiert sind
/// („already loaded → don't reload").
pub async fn compose_system_prompt(
    pool: &PgPool,
    workspace_id: Uuid,
    agent_prompt: &str,
    skills_registry: &SkillRegistry,
    agent_skills: &[String],
    loaded_skills: &[String],
) -> ApiResult<String> {
    let workspace = workspace_summary(pool, workspace_id).await?;
    Ok(compose_with_summary(
        agent_prompt,
        &workspace,
        skills_registry,
        agent_skills,
        loaded_skills,
    ))
}

/// Reine Composer-Funktion ohne DB-Zugriff — wird vom asynchronen
/// `compose_system_prompt` (mit Workspace-Übersicht aus der DB) genutzt
/// und ist gleichzeitig DB-frei testbar.
pub fn compose_with_summary(
    agent_prompt: &str,
    workspace_summary: &str,
    skills_registry: &SkillRegistry,
    agent_skills: &[String],
    loaded_skills: &[String],
) -> String {
    let mut parts: Vec<String> = Vec::new();

    // Datum-Anker — sonst kann das Modell „heute"/„gestern" nicht
    // gegen seinen Trainings-Cutoff auflösen.
    parts.push(format!(
        "Today is {} (UTC).",
        OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| "unknown date".to_string()),
    ));

    if !agent_prompt.trim().is_empty() {
        parts.push(agent_prompt.trim().to_string());
    }

    if !workspace_summary.trim().is_empty() {
        parts.push(workspace_summary.to_string());
    }

    if let Some(block) = skills_block(skills_registry, agent_skills, loaded_skills) {
        parts.push(block);
    }

    // Globale Thoroughness-Policy — aus Local übernommen, knapp und
    // imperativ; lange Anweisungen werden von kleineren Modellen ignoriert.
    parts.push(
        "Bevor du eine Frage zu den Dateien, einem Thema oder einem Zeitraum \
         beantwortest, sammle erst Belege: erkunde den Workspace und lies die \
         relevanten Dokumente. Antworte nicht aus einer einzelnen Datei, außer \
         die Nutzer:in hat sie namentlich genannt. Frag dich nach jedem Lesen, \
         ob du genug Material für eine genaue Antwort hast — wenn nicht, suche \
         weiter, bevor du antwortest."
            .to_string(),
    );
    parts.push("Antworte in der Sprache, die die Nutzer:in benutzt hat.".to_string());

    parts.join("\n\n")
}

/// Workspace-Übersicht als kompakter Block: die ersten 30 Dateinamen
/// aus `workspace_files` (DB ist die Wahrheit — vgl. „DB ist Wahrheit,
/// Volume ist Bytes"). Bei mehr Dateien: Suffix „… N weitere".
///
/// `pub` weil der Chat-Loop die Übersicht einmal pro Run-Start aus der
/// DB zieht und dann zwischen den Tool-Loop-Iterationen wiederverwendet
/// (sie ändert sich innerhalb eines Runs nicht).
pub async fn workspace_summary(pool: &PgPool, workspace_id: Uuid) -> ApiResult<String> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT filename, size_bytes FROM workspace_files \
         WHERE workspace_id = $1 ORDER BY filename",
    )
    .bind(workspace_id)
    .fetch_all(pool)
    .await
    .map_err(|e| ApiError::Internal(e.into()))?;
    if rows.is_empty() {
        return Ok(String::new());
    }
    let total = rows.len();
    let mut lines: Vec<String> = Vec::with_capacity(WORKSPACE_TREE_LIMIT + 2);
    lines.push("## Workspace".to_string());
    lines.push(
        "Diese Dateien liegen im Workspace der Nutzer:in. Bei Fragen zu \
         „Projekt\"/Thema/Zeitraum erst hier passende Dateien sichten \
         (`folder-search`-Skill), nicht aus einer einzelnen Datei \
         antworten."
            .to_string(),
    );
    for (filename, size_bytes) in rows.iter().take(WORKSPACE_TREE_LIMIT) {
        lines.push(format!("- `{filename}` ({size_bytes} Bytes)"));
    }
    if total > WORKSPACE_TREE_LIMIT {
        lines.push(format!(
            "- … {} weitere (nutze `list_files` aus `folder-search` für die vollständige Liste)",
            total - WORKSPACE_TREE_LIMIT
        ));
    }
    Ok(lines.join("\n"))
}

/// Skill-Listing-Block: Titel + Description + Tool-Namen pro Skill, plus
/// die Lese-Anleitung obendrauf. **Keine** Schemas, **kein** Body.
pub fn skills_block(
    registry: &SkillRegistry,
    agent_skills: &[String],
    loaded_skills: &[String],
) -> Option<String> {
    let mut entries: Vec<String> = Vec::new();
    // Deterministische Reihenfolge: alphabetisch nach `name`, beschränkt
    // auf die vom Agent aktivierten Skills, die in der Registry existieren.
    let mut active: Vec<_> = agent_skills
        .iter()
        .filter_map(|name| registry.get(name))
        .collect();
    active.sort_by(|a, b| a.name.cmp(&b.name));
    if active.is_empty() {
        return None;
    }
    for skill in &active {
        let tools = if skill.tools.is_empty() {
            "(keine)".to_string()
        } else {
            skill.tools.join(", ")
        };
        let already_loaded = loaded_skills.iter().any(|n| n == &skill.name);
        let marker = if already_loaded {
            " — schon geladen"
        } else {
            ""
        };
        entries.push(format!(
            "- **{title}** (id: `{name}`{marker}) — {desc}\n  tools: {tools}",
            title = skill.title,
            name = skill.name,
            desc = skill.description.trim(),
            marker = marker,
            tools = tools,
        ));
    }
    let mut block = String::from("## Available skills\n");
    block.push_str(
        "Jeder Skill unten ist nur **zusammengefasst**; die volle Anleitung \
         lebt im Body. Wenn die Anfrage der Nutzer:in zu einem dieser Skills \
         passt, ruf `read_skill({ skillId: \"<id>\" })` auf, um den Body als \
         Tool-Result zu laden — und richte dich dann nach den Anweisungen \
         dort, bevor du eines der Tools des Skills nutzt. Skills, die schon \
         in dieser Konversation geladen wurden, sind markiert und bleiben in \
         Scope — lade sie nicht erneut. `ask_user` ist immer erlaubt.\n\n",
    );
    block.push_str(&entries.join("\n"));
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills::{parse_skill_md, SkillRegistry};
    use std::path::PathBuf;

    fn registry_with(skills: &[(&str, &str)]) -> SkillRegistry {
        // Wir bauen die Registry über den Public-Loader, indem wir ein
        // Temp-Verzeichnis mit den SKILL.md anlegen. Schöner Side-Effekt:
        // testet auch die Registry-Loading-Pfad noch mal.
        let root: PathBuf =
            std::env::temp_dir().join(format!("pfx-prompt-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        for (name, body) in skills {
            let dir = root.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("SKILL.md"), body).unwrap();
        }
        let reg = SkillRegistry::load_from_dir(&root).unwrap();
        std::fs::remove_dir_all(&root).ok();
        reg
    }

    const A_MD: &str = "\
---
name: folder-search
title: Ordner durchsuchen
description: Find files.
tools: [list_files, read_file]
---
You can search files.
";

    const B_MD: &str = "\
---
name: document-read
title: Dokumente lesen
description: Extract PDF/DOCX.
tools: [read_pdf, read_docx]
---
Read with care.
";

    #[test]
    fn skills_block_lists_titles_descriptions_and_tool_names() {
        let reg = registry_with(&[("folder-search", A_MD), ("document-read", B_MD)]);
        let block = skills_block(
            &reg,
            &["folder-search".to_string(), "document-read".to_string()],
            &[],
        )
        .unwrap();
        // Titel + Description + Tool-Namen
        assert!(block.contains("Ordner durchsuchen"), "{block}");
        assert!(block.contains("Find files."), "{block}");
        assert!(block.contains("tools: list_files, read_file"), "{block}");
        assert!(block.contains("Dokumente lesen"), "{block}");
        assert!(block.contains("tools: read_pdf, read_docx"), "{block}");
        // **Kein** Body
        assert!(
            !block.contains("You can search files."),
            "Body leaked: {block}"
        );
        assert!(!block.contains("Read with care."), "Body leaked: {block}");
    }

    #[test]
    fn skills_block_marks_already_loaded() {
        let reg = registry_with(&[("folder-search", A_MD)]);
        let block = skills_block(
            &reg,
            &["folder-search".to_string()],
            &["folder-search".to_string()],
        )
        .unwrap();
        assert!(block.contains("schon geladen"), "{block}");
    }

    #[test]
    fn skills_block_returns_none_when_no_active_skills() {
        let reg = registry_with(&[("folder-search", A_MD)]);
        assert!(skills_block(&reg, &[], &[]).is_none());
        // Agent listet einen nicht-existenten Skill → ignoriert.
        assert!(skills_block(&reg, &["nope".to_string()], &[]).is_none());
    }

    #[test]
    fn skills_block_is_sorted_alphabetically() {
        let reg = registry_with(&[("folder-search", A_MD), ("document-read", B_MD)]);
        let block = skills_block(
            &reg,
            &["folder-search".to_string(), "document-read".to_string()],
            &[],
        )
        .unwrap();
        let pos_doc = block.find("document-read").unwrap();
        let pos_folder = block.find("folder-search").unwrap();
        assert!(pos_doc < pos_folder, "alphabetische Ordnung fehlt: {block}");
    }

    #[test]
    fn compose_includes_date_skills_policy_and_language_directive() {
        let reg = registry_with(&[("folder-search", A_MD)]);
        let s = compose_with_summary(
            "Du bist Beispiel-Agent.",
            "## Workspace\n- `notes.md` (12 Bytes)",
            &reg,
            &["folder-search".to_string()],
            &[],
        );
        assert!(s.contains("Today is "), "{s}");
        assert!(s.contains("Du bist Beispiel-Agent."), "{s}");
        assert!(s.contains("## Workspace"), "{s}");
        assert!(s.contains("notes.md"), "{s}");
        assert!(s.contains("## Available skills"), "{s}");
        assert!(s.contains("Ordner durchsuchen"), "{s}");
        // Thoroughness + Sprach-Direktive
        assert!(s.contains("Bevor du eine Frage"), "{s}");
        assert!(s.contains("Antworte in der Sprache"), "{s}");
        // **Kein** Skill-Body im Composer-Output (Progressive Disclosure!)
        assert!(!s.contains("You can search files."), "Body leaked: {s}");
    }

    #[test]
    fn compose_omits_empty_pieces() {
        let reg = registry_with(&[]);
        let s = compose_with_summary("", "", &reg, &[], &[]);
        // Datum + Thoroughness + Sprache bleiben immer
        assert!(s.contains("Today is "));
        assert!(s.contains("Bevor du eine Frage"));
        assert!(s.contains("Antworte in der Sprache"));
        // Aber kein Workspace-Header und kein Skills-Header
        assert!(!s.contains("## Workspace"), "{s}");
        assert!(!s.contains("## Available skills"), "{s}");
    }

    #[test]
    fn parses_well_with_extra_blank_lines() {
        // Smoketest dass der Parser-Wrapper hier noch stimmt.
        let s = parse_skill_md(A_MD).unwrap();
        assert_eq!(s.name, "folder-search");
    }
}
