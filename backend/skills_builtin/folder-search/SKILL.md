---
name: folder-search
title: Ordner durchsuchen
description: Erkundet den Workspace — listet Dateien auf, sucht per Regex über mehrere Textdateien und liest einzelne Dateien als Klartext. Lies diesen Skill zuerst, wenn du nicht weißt, was im Workspace liegt.
icon: FolderSearch
tools:
  - list_files
  - read_file
  - grep_in_files
hitl:
  default: false
language: de
---

Du erkundest den Workspace der Nutzer:innen. Wähle das richtige Tool:

- **list_files** — zuerst, wenn du nicht weißt, welche Dateien existieren. Liefert eine alphabetische Liste aller Workspace-Dateien.
- **grep_in_files** — Regex-Suche über die **Text-Dateien** des Workspaces (`.md`, `.txt`, `.csv`, `.json`, `.yaml`, `.toml`, `.html`, `.xml` und gängige Source-Endungen; binäre/Office-Formate werden übersprungen). Standardmäßig case-insensitive, max. 100 Treffer.
- **read_file** — den vollständigen Textinhalt einer Datei lesen, wenn du sie identifiziert hast. Für PDF/DOCX/XLSX **lies stattdessen** den passenden Skill (`document-read` für PDF/DOCX, `table-read` für XLSX) — `read_file` gibt dort nur „nicht als Text lesbar" zurück.

Richtlinien:

1. **Frische immer neu prüfen.** Wenn die Nutzer:in sinngemäß sagt „die neue Datei, die ich gerade hochgeladen habe" oder „schau nochmal", rufe `list_files` neu auf — die frühere Liste ist nur eine Momentaufnahme. Der Workspace ändert sich zwischen Turns.
2. **Erst Überblick, dann Inhalt.** Bei thematischen Fragen („was steht in den Notizen über X?") **immer zuerst** `list_files` + `grep_in_files`, dann gezielt `read_file` auf die 1–3 besten Treffer. Nicht blind eine einzelne Datei lesen, wenn die Nutzer:in sie nicht namentlich benannt hat.
3. **Pattern eng halten.** `grep_in_files` ist case-insensitive per Default; nutze kurze, präzise Patterns statt langer Phrasen — Regex-Sonderzeichen escapen.
4. **Pfade verbatim zitieren.** Wenn du eine Datei referenzierst, gib den exakten Dateinamen aus der `list_files`-Ausgabe an, damit die Nutzer:in sie nachschlagen kann.
5. **Keine Schreiboperationen.** Dieser Skill ist read-only. Wenn die Nutzer:in eine Datei ändern will, lies erst `document-write` oder `table-write`.
