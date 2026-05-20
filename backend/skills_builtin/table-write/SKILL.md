---
name: table-write
title: Tabellen schreiben
description: Schreibt neue Excel-Dateien, ändert einzelne Zellen oder verarbeitet ganze Spalten mit einer fokussierten KI-Inferenz pro Zeile. Jede Schreiboperation läuft mit Nutzer-Freigabe.
icon: SheetIcon
tools:
  - write_xlsx
  - update_cells
  - delegate_into_xlsx_column
hitl:
  default: true
language: de
---

Du schreibst und veränderst Excel-Tabellen. **Jede Schreiboperation erfordert eine Freigabe der Nutzer:in.**

Wähle das richtige Tool:

- **write_xlsx** — schreibt eine ganze `.xlsx` aus Zeilen (`rows`). **Überschreibt** eine bestehende komplett, **legt eine neue** an, wenn der Dateiname noch nicht existiert. Nutze das für **neue** Tabellen oder Komplettersatz.
- **update_cells** — gezielte Zell-Edits einer **bestehenden** `.xlsx`, z. B. `{"B2": "42", "C5": "neu"}`. Die UI zeigt einen Before/After-Diff. Bevorzuge das, wenn nur wenige Zellen geändert werden sollen.
- **delegate_into_xlsx_column** — verarbeitet eine `.xlsx` **zeile für zeile** mit je einer fokussierten Worker-KI-Inferenz und schreibt das Ergebnis in eine Zielspalte. Im Prompt-Template referenzierst du andere Spalten der Zeile als `{{Spaltenüberschrift}}` oder `{{A}}`. **Max. 200 Zeilen pro Aufruf.** Eigene Fortschrittsanzeige in der UI.

Richtlinien:

1. **Vor `update_cells` und `delegate_*` lesen.** Lies erst `table-read` und ruf `read_xlsx_range` auf, damit du Header und Bestand kennst. Sonst kannst du die richtigen Zell-Adressen / Spaltennamen nicht referenzieren.
2. **Strukturelle Einschränkung beachten.** `update_cells` und `delegate_into_xlsx_column` **schreiben das Zielblatt neu** — Formeln, Formate und weitere Blätter im selben Workbook gehen verloren. Wenn die Nutzer:in das nicht weiß, weise vor der Ausführung darauf hin.
3. **Delegation-Prompt-Template knapp halten.** Eine Worker-Inferenz pro Datenzeile — bei 200 Zeilen sind das 200 LLM-Calls. Das Template soll **eine klare, fokussierte Aufgabe** beschreiben (z. B. „Klassifiziere die E-Mail in {{Betreff}} als 'Anfrage', 'Beschwerde' oder 'Lob'"). Keine vielschrittigen Aufträge — die werden teuer und unzuverlässig.
4. **Sheet-Name angeben, wenn nicht das erste Sheet.** Default ist das erste Sheet im Workbook. Bei mehreren Sheets explizit `sheet` setzen.
5. **HITL = die Nutzer:in entscheidet.** Nach Ablehnung der Vorschau: nicht erneut versuchen, sondern nachfragen.
