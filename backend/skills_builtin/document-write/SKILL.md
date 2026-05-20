---
name: document-write
title: Text- und Word-Dokumente schreiben
description: Erstellt, ergänzt oder überschreibt Markdown-, Text- und Word-Dateien im Workspace. Jede Schreiboperation läuft mit Nutzer-Freigabe (HITL-Diff-Vorschau).
icon: FilePen
tools:
  - append_to_file
  - rewrite_file
  - write_docx
  - write_docx_from_template
  - append_to_docx
hitl:
  default: true
language: de
---

Du schreibst Text- und Word-Dokumente. **Jede Schreiboperation erfordert eine Freigabe der Nutzer:in** — die UI zeigt eine Vorschau (Text-Tail oder Diff), bevor geschrieben wird.

Wähle das richtige Tool:

- **append_to_file** — sicherer Zusatz an `.md`/`.txt`-Dateien (legt sie bei Bedarf neu an). Bevorzuge das vor `rewrite_file`, wenn Inhalt nur ergänzt werden soll — kein Risiko, Bestehendes zu löschen.
- **rewrite_file** — **ersetzt** den gesamten Inhalt einer Text-Datei (`.md`, `.markdown`, `.txt`, `.text`, `.csv`). Vor dem Aufruf am besten `read_file` (über `folder-search`), damit dein `content` den Bestand sinnvoll fortführt — bestehender Inhalt geht sonst verloren. Bestand max. 5 MB.
- **write_docx** — neue `.docx` aus einer Liste von Absätzen schreiben (überschreibt eine bestehende komplett). Nutze das für **neue** Word-Dokumente, nicht zum Erweitern.
- **append_to_docx** — Absätze an eine bestehende `.docx` anhängen (legt sie bei Bedarf neu an). Bevorzuge das vor `write_docx`, wenn nur ergänzt werden soll.
- **write_docx_from_template** — füllt eine Word-Vorlage (`.docx` im Workspace) aus: ersetzt `{{Platzhalter}}` durch Werte. Schreibt das Ergebnis als neue `.docx`. Die Vorlage selbst bleibt unverändert.

Richtlinien:

1. **Bevorzuge Anhängen vor Ersetzen.** `append_to_file` und `append_to_docx` sind risikoarm; `rewrite_file` und `write_docx` löschen Bestand. Wenn die Nutzer:in „füge X hinzu" sagt, ist Anhängen richtig.
2. **Vor `rewrite_file` immer lesen.** Lies den Skill `folder-search` und ruf `read_file` auf, damit dein `content` den Bestand integriert statt überschreibt.
3. **Platzhalter in Vorlagen müssen run-zusammenhängend sein.** Wenn `write_docx_from_template` einen Platzhalter nicht ersetzt, hat Word den Text vermutlich über Runs gesplittet. Die Vorlage muss dafür angepasst werden — das ist eine Eigenheit von `.docx`-XML, kein Bug des Tools.
4. **Dateiname mit korrekter Endung.** Tools lehnen falsche Endungen ab. Für `.xlsx` → `table-write`, für `.pdf` → kein direktes Tool (PDFs sind nicht text-überschreibbar).
5. **HITL = die Nutzer:in entscheidet.** Wenn der Vorschau-Dialog abgelehnt wird, ist das ein klares „Nein" — nicht erneut versuchen, sondern nachfragen, was geändert werden soll.
