---
name: document-read
title: Dokumente lesen
description: Extrahiert Text aus PDF- und Word-Dokumenten im Workspace. Lies diesen Skill, wenn die Nutzer:in eine `.pdf` oder `.docx` referenziert.
icon: FileText
tools:
  - read_pdf
  - read_docx
hitl:
  default: false
language: de
---

Du extrahierst Text aus Office-Dokumenten. Wähle das richtige Tool:

- **read_pdf** — für `.pdf`-Dateien. Funktioniert für digitale PDFs; gescannte PDFs ohne OCR-Layer liefern leeren Text (das Tool sagt das explizit). Eingabe max. 20 MB, Ausgabe max. ~200 KB Klartext (danach gekürzt).
- **read_docx** — für `.docx`-Dateien. Liefert Lauftext mit Absätzen durch Leerzeilen getrennt. Tabellen-Zellen-Inhalt bleibt erhalten, Bilder und eingebettete Objekte werden gestrippt. Gleiche Caps wie `read_pdf`.

Richtlinien:

1. **Dateiname genau angeben** (z. B. `report.pdf`). Wenn du die Datei nicht aus dem Workspace kennst, lies erst den `folder-search`-Skill und nutze `list_files`.
2. **Falsche Endung → falsches Tool.** `read_pdf` lehnt non-`.pdf` ab; `read_docx` lehnt non-`.docx` ab. Für `.xlsx` ist `table-read` zuständig.
3. **Truncation respektieren.** Ist das Ergebnis am Ende mit „[gekürzt — Extraktion überschreitet 200 KB]" markiert, weißt du, dass der Rest fehlt. Sag das der Nutzer:in, statt so zu tun, als hättest du das ganze Dokument.
4. **Leere Extraktion → wahrscheinlich Scan ohne OCR.** Das Tool weist explizit darauf hin („leere Extraktion — vermutlich gescanntes PDF ohne OCR"); reiche diesen Hinweis weiter, statt zu raten.
5. **Read-only.** Wenn die Nutzer:in das Dokument bearbeiten will, lies `document-write`.
