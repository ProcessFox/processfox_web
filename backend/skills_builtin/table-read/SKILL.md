---
name: table-read
title: Tabellen lesen
description: Liest einen rechteckigen Zellbereich aus einer Excel-Datei (`.xlsx`) und liefert ihn als strukturiertes JSON.
icon: SheetIcon
tools:
  - read_xlsx_range
hitl:
  default: false
language: de
---

Du liest Excel-Tabellen bereichsweise.

- **read_xlsx_range** — rechteckiger Bereichs-Read mit Sheet/Start/End-Window. Output ist JSON `{file, sheet, range, headers, rows}` — **erste Zeile der Range ist `headers`**, restliche sind `rows`. Alle Zellwerte sind Strings (kein Type-Drift bei Mixed-Type-Spalten). Maximal 500 Zellen pro Aufruf.

Defaults wenn die Nutzer:in nichts spezifiziert:

- `sheet` → das erste Sheet des Workbooks.
- `start` → `"A1"`.
- `end` → 25 Zeilen × 12 Spalten ab `start` (also `A1:L25`).

Richtlinien:

1. **Header-Konvention nutzen.** Wenn `start = "A1"`, ist `headers` die übliche Header-Zeile. Wenn die Datei nur Daten ohne Header enthält, kannst du `start: "A2"` setzen — die A2-Zeile wird dann syntaktisch zum „Header"; das macht aber selten Sinn, normalerweise willst du die echten Header.
2. **500-Zellen-Cap.** Bei großen Tabellen lieber **mehrere** Aufrufe mit engerer Range als einer mit 30×30. Das Tool lehnt > 500 Zellen ab, gibt aber die genaue Zahl im Fehler zurück — daran kannst du die nächste Range planen.
3. **Sheet-Namen müssen exakt stimmen.** Tippfehler → das Tool liefert eine Liste der verfügbaren Sheets im Fehler-Output. Nutze die.
4. **Zell-Werte sind Strings.** `"42"` ist `"42"`, nicht `42`. Wenn du rechnen willst, parse selbst (`as_str()` → `parse::<f64>()`). Excel-Datums-Serials sind als numerischer String drin.
5. **Read-only.** Zum Schreiben/Ändern von Excel-Dateien lies `table-write`.
