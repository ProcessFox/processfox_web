# PLAN.md — Umbau ProcessFox Web (Local-Port → Web-Architektur)

> Stand: 2026-05-16. Begleitdokument zu `CLAUDE.md` (Soll-Architektur) und
> `CLAUDE.md §1a` (Ist-Stand). Dieses Dokument ist der **Fahrplan**, noch
> kein Implementierungs-Code.

## Festgelegte Entscheidungen

| Thema | Entscheidung |
|---|---|
| API-Konvention | RESTful `/api/v1/...` + `Authorization: Bearer` (CLAUDE.md §7). RPC-Bridge wird ersetzt. |
| Geteiltes Design | Visuell replizieren. Kein geteiltes npm-Paket / Monorepo mit www & Local. |
| Local-Code | GGUF, Hardware-Info, Modell-Download, OS-Ordnerzugriff werden **entfernt**. |
| Reihenfolge | Phasen **strikt sequenziell** umsetzen, Abnahme pro Phase. |
| Bestands-Migration Local→Web | **Keine** — Greenfield. |
| Storage | **MinIO als Coolify-Service** (Coolify hat keinen eigenen S3-Dienst; Volumes reichen nicht — kein S3-API/Presign). AWS S3 optional via gleiche Env-Vars. |
| Registrierung | **Immer** mit 6-stelligem Org-Invite-Code. Keine Org-Erstellung über die App — erste Org + Owner werden manuell in der DB angelegt (Betreiber). |
| LLM-Layer | Provider-HTTP-Clients aus `processfox_local/core/llm` portieren; **Anthropic Prompt-Caching** (`cache_control`) von Anfang an mitnehmen. |

## Ausgangslage (Kurz)

Nur Frontend vorhanden (1:1-Port aus `processfox_local`). Kein Backend, kein
Auth, kein Workspace-Konzept. Bridge `src/lib/tauri.ts` ruft RPC-Endpunkte ins
Leere. Local-Paradigma (Modelle/Hardware/Ordner) noch durchgängig.

---

## Phase 0 — Frontend-Bereinigung (Local-Paradigma raus) ✅ ABGESCHLOSSEN (2026-05-16)

Ziel: Frontend von Local-Annahmen befreien, bevor Backend gebaut wird.

**Ergebnis:** `npm run build` (tsc + vite) grün, keine `@tauri-apps`-/
Local-/Hardware-Referenzen mehr. Bridge auf Workspace+Upload-Signaturen
umgestellt (Transport bleibt RPC bis zur REST-Etappe). Neue Typen
`src/types/auth.ts`; `models.ts`/`ModelsTab.tsx` entfernt.

**Entfernen:**
- `modelsApi` komplett (Katalog, installierte Modelle, Hardware, Downloads).
- `src/components/settings/ModelsTab.tsx`; Models-Tab aus `Settings.tsx`.
- `provider === "local"`-Zweige in `App.tsx`, `useAgentChat.ts`.
- `ModelRef`-Local-Variante → nur noch `{ provider, id }` (cloud-only).
- OS-Ordner-Pfade: `fileApi.listAgentFolder/watch/unwatch/openLogsFolder/importFilesToAgent`, `files-dropped`-Subscription, OS-Drag&Drop-Effekt in `App.tsx`, `agent.folder`.
- `HardwareInfo`, lokale Modell-Felder in `Settings`.
- Welcome-Dialog: Local-Modell-Onboarding-Schritte raus.

**Ersetzen / Neu (zunächst nur Typen + Bridge-Signaturen, Impl. später):**
- `src/types/auth.ts`: `User`, `Org`, `Workspace`, `WorkspaceRole`, `OrgRole`.
- `Agent.folder` → `Agent.workspaceId`.
- `fileApi` → Workspace-Datei-API (list/upload/delete/presigned-download).
- `Settings` → org-scoped Provider/Modell, kein lokaler Default.

**Abnahme:** `npm run build` grün, keine toten Local-Referenzen, UI rendert
(gegen Mock/leeres Backend) ohne Local-Konzepte.

---

## Phase 1 — Backend-Skeleton (Axum) ✅ ABGESCHLOSSEN (2026-05-16)

`backend/`-Crate gemäß CLAUDE.md §6.

**Ergebnis:** `cargo build` + `cargo fmt --check` + `clippy -D warnings`
grün. Multi-Stage-`Dockerfile`, `.dockerignore`, `.env.example`, `DEPLOY.md`
erstellt. Build via **GitHub Actions → GHCR** (`.github/workflows/docker.yml`,
public Package), Coolify zieht das Image (VPS baut nichts — 2 vCPU zu schwach
für den Build). **Live deployed auf `chat.processfox.ai`** (2026-05-16),
`/api/v1/health` erreichbar.

- `Cargo.toml`, `main.rs`, `lib.rs`, `config.rs` (Env-Vars §12).
- sqlx-Pool (Postgres), `db/migrations/` mit Schema §8.
- S3-Client (`aws-sdk-s3` oder `object_store`), `storage/`.
- Axum-Router `/api/v1/*`, Fehler-Envelope `{code,message,details?}` (§7).
- Static-File-Serving (`STATIC_DIR`) + SPA-Fallback.
- Healthcheck `GET /api/v1/health`.

**Abnahme:** `cargo fmt`/`clippy -D warnings`/`test` grün; Server startet,
Migrations laufen, `/api/v1/health` antwortet.

---

## Phase 2 — Auth (passwordless Magic-Link) ✅ ABGESCHLOSSEN + LIVE VERIFIZIERT (2026-05-16)

> **Geänderte Entscheidung:** statt E-Mail+Passwort (Argon2) nun
> **passwordless Magic-Link**. Mailversand via n8n-Webhook. CLAUDE.md §4
> entsprechend aktualisiert.

- Tabellen `organizations` (inkl. `invite_code`), `users` (ohne
  `password_hash`), `refresh_tokens`, `login_tokens` (Magic-Link, nur Hash).
- `POST /api/v1/auth/request-login` (E-Mail), `/auth/request-register`
  (E-Mail + 6-stelliger Org-Code), `/auth/verify` (Token → Session),
  `/auth/refresh`, `/auth/logout`,
  `POST /api/v1/orgs/{id}/rotate-invite-code` (nur Owner).
  **Kein** Org-Erstellungs-Endpunkt — erste Org + Owner per Seed-SQL
  (DEPLOY.md §6).
- Magic-Link-Token 15 min, einmalig (atomar konsumiert). Access-Token
  15 min (Bearer), Refresh-Token 7 Tage (httpOnly-Cookie, gehasht,
  rotierend/widerrufbar). In-Memory-Rate-Limit auf den Auth-Endpunkten.
- `AuthUser`-Extractor (Bearer → user_id/org_id/org_role).
- Frontend: `useAuth`, `Login.tsx` (Anmelden / Registrieren-mit-Code),
  `/auth/callback?token=`-Pickup, App hinter Auth-Gate, Bridge injiziert
  `Authorization` + 401→Refresh→Retry. Logout in Settings „Über".

**Ergebnis:** `cargo build/fmt/clippy -D warnings` grün, 4 DB-freie
Unit-Tests (JWT-Roundtrip, Token-Hash) grün; `npm run build` + `tsc` grün.
Webhook-Vertrag + Seed-SQL in DEPLOY.md §6/§7.

**Offen (bewusst verschoben):** HTTP/DB-Integrationstests (register/verify/
refresh-Flows) brauchen eine Postgres-Instanz → in CI mit `#[sqlx::test]`
nachziehen, sobald ein Test-Postgres steht (CLAUDE.md §13).

---

## Phase 3 — Workspaces & Mitglieder ✅ ABGESCHLOSSEN (2026-05-16)

- Tabellen `workspaces`, `workspace_members` (bereits aus Schema 0001).
- REST: `GET/POST /workspaces`, `PATCH/DELETE /workspaces/{id}`,
  `GET/POST /workspaces/{id}/members`, `PATCH/DELETE
  /workspaces/{id}/members/{userId}`.
- Helper `effective_role`/`require_member`/`require_editor`/
  `require_org_owner`: Org-`owner` = Vollzugriff in eigener Org; sonst
  `workspace_members.role`; fremde Org → 404 (kein Leak).
- Frontend: `components/workspace/` (`WorkspaceSwitcher`,
  `WorkspaceMembersDialog`), Bridge `workspaceApi`/`memberApi` auf REST,
  App-Init gegen fehlende Settings/Agent-Endpunkte (Phase 4) entkoppelt.

**Ergebnis:** `cargo build/fmt/clippy -D warnings` grün, 5 DB-freie
Unit-Tests grün (inkl. Rollen-Validierung); `tsc` + `npm run build` grün.

**Offen (bewusst verschoben):** HTTP/DB-Integrationstests der
Berechtigungs-Matrix (Nicht-Mitglied→404/403) brauchen Test-Postgres →
CI mit `#[sqlx::test]`, zusammen mit Phase-2-Integrationstests.

---

## Phase 4 — Agents & Settings (DB-backed) ✅ ABGESCHLOSSEN (2026-05-16)

- Tabellen `agents`/`org_settings`/`api_keys` (aus Schema 0001) — keine
  neue Migration nötig.
- REST: `GET/POST /workspaces/{wid}/agents`, `GET/PATCH/DELETE
  /agents/{id}`, `POST /agents/{id}/attachment`; `GET /settings`,
  `PUT /settings/provider|model`, `POST /settings/first-run-done`;
  `GET/POST/DELETE /secrets/{provider}`, `POST /secrets/{provider}/validate`;
  `GET /skills` (vorerst `[]`, Skills ab Phase 6).
- `crypto.rs`: AES-256-GCM (`nonce||ct`), Key = `API_KEY_ENCRYPTION_KEY`.
  Klartext-Keys nie ans Frontend (`GET /secrets/{p}` → nur `{hasKey}`),
  `validate` macht Live-Provider-Check. Settings-Schreiben + Key-Mgmt
  Owner-only; Lesen jedes Org-Mitglied.
- Berechtigungs-Helper nach `crate::perm` extrahiert (von Workspaces +
  Agenten geteilt). Frontend-Bridge `agentApi/settingsApi/secretsApi/
  skillsApi` auf REST (mit 401→Refresh→Retry), Signaturen unverändert.

**Ergebnis:** `cargo build/fmt/clippy -D warnings` grün, 6 DB-freie
Unit-Tests grün (inkl. AES-256-GCM Roundtrip + Tamper/Wrong-Key).
`tsc` + `npm run build` grün. PLAN-Lücke #5 (Attachment→fileId) im
Endpoint umgesetzt; Datei-Existenzprüfung folgt mit Phase 5.

**Offen (bewusst verschoben):** HTTP/DB-Integrationstests
(Agent-CRUD-Permissions, Key-Verschlüsselung in DB) → CI mit
`#[sqlx::test]`, gebündelt mit Phase 2/3.

---

## Phase 5 — Dateien (Upload statt OS-Ordner) ✅ ABGESCHLOSSEN (2026-05-16)

> **Architektur-Änderung 2026-05-19:** Storage von S3/MinIO auf **lokales
> Persistent Volume** (Coolify, `STORAGE_DIR`) umgestellt — Self-Hosted-
> Single-Instance-Maßstab + MinIO-Netzwerk-Betriebskomplexität. App damit
> bewusst *stateful* (Volume sichern, kein H-Scaling). Download nun über
> kurzlebig signierten Link (`/files/{id}/raw?token=`) statt Presigned-S3.
> `aws-sdk-s3`/`aws-config` entfernt (schlankerer Build). Frontend
> unverändert. CLAUDE.md §2/§5/§6/§9/§12/§16 + DEPLOY.md + `.env.example`
> nachgezogen. Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests grün,
> `tsc`/`vite build` grün.

- REST: `GET/POST /workspaces/{wid}/files` (Multipart ≤ 50 MB,
  Typ-Whitelist §5), `DELETE /files/{id}`, `GET /files/{id}/download-url`
  (presigned, 15 min), `GET/PUT /files/{id}/text` (ETag-Optimistic-
  Concurrency → `version_conflict`), `GET /files/{id}/preview/{docx|xlsx|
  pptx}`.
- `sandbox.rs`: `ensure_in_workspace` + `sanitize_filename` (Pfad-Traversal
  strukturell ausgeschlossen). Migration `0003` (unique
  `(workspace_id, filename)` → Re-Upload überschreibt, PLAN-Lücke #6).
- `preview.rs`: xlsx via `calamine`, docx/pptx via `zip`+`quick-xml`
  (Text-Extraktion, HTML escaped) — liefert exakt das bestehende
  Preview-JSON. Image/PDF laufen über die Presigned-URL.
- Frontend-Bridge `fileApi`/`previewApi` auf REST (+ authed Multipart-
  Upload, 401→Refresh→Retry). FileTree/Drag&Drop/Viewer/Editoren
  funktionieren ohne Änderung gegen die neuen Endpunkte.

**Ergebnis:** `cargo build/fmt/clippy -D warnings` grün, 8 DB-freie
Unit-Tests grün (inkl. Sandbox/Filename-Sanitizing, Pfad-Traversal).
`tsc` + `npm run build` grün.

**Offen (bewusst):** `fs-changed`-Broadcast ist No-op-Platzhalter bis der
WS-Hub in **Phase 6** steht; pptx-Notizen nicht extrahiert (Outline reicht);
HTTP/DB-Integrationstests (Upload→Preview→Download e2e) → CI mit
`#[sqlx::test]` + MinIO-Testcontainer, gebündelt mit Phase 2–4.

---

## Phase 6a — Chat-Kern & Realtime ✅ ABGESCHLOSSEN (2026-05-19)

- LLM-Provider Anthropic (Messages-API + Prompt-Caching) / OpenAI /
  OpenRouter, **Streaming** via SSE; Key serverseitig aus `api_keys`
  entschlüsselt injiziert (`llm.rs`).
- **Multiplexer WS-Hub** `GET /api/v1/ws?token=<access>` (`ws.rs`): eine
  Verbindung, Frames `{channel,payload}`, workspace-gescopeter Broadcast
  (Mitgliedschaft einmalig beim Connect ermittelt). Channels
  `chat:run:<runId>`, `fs-changed`, `agent-attachments-changed` verdrahtet.
- `routes/chat.rs`: `GET/POST /agents/{id}/messages` (Run starten →
  `delta`/`finish`/`error` über WS, Verlauf in `chat_messages`),
  `POST /runs/{id}/cancel` (kooperativer Abbruch). HITL-Endpunkte als
  204-Stubs (Tools erst 6b).
- Frontend-Bridge: `subscribeWs` auf **eine** multiplexte Verbindung
  (Reconnect mit frischem Token), `chatApi` auf REST. Letzte RPC-Reste
  (`post`/`/api/<command>`) entfernt — Bridge jetzt vollständig REST + 1×WS.

**Härtung (gleicher Tag):** Planungslücke #3 geschlossen — **genau ein
aktiver Run pro Agent** (`AppState.active_runs`; zweiter paralleler Send →
`409`, Slot-Freigabe bei finish/error/cancel und Früh-Fehlern). Streaming
läuft auf `chat:agent:<agentId>` statt `chat:run:<runId>`; `useAgentChat`
abonniert **pro aktivem Agenten** → alle Workspace-Mitglieder sehen den
laufenden Run live (echte Shared Session, CLAUDE.md §4).

**Ergebnis:** `cargo build/fmt/clippy -D warnings` + 8 Tests grün,
`tsc`/`vite build` grün.

## Phase 6b-1 — Tool-Loop + HITL ✅ ABGESCHLOSSEN (2026-05-19)

- `tools.rs`: gebündelte read-only Skill **`files`** (`GET /skills` liefert
  sie real), Tools `list_files`/`read_file` (read) + `append_to_file`
  (write → HITL). Provider-neutrale `ToolSpec`.
- `llm.rs`: non-streaming `tool_step` für Anthropic (`tool_use`) **und**
  OpenAI-kompatibel (`tool_calls`), neutrale `Turn`-Repräsentation,
  Iterations-Cap (8).
- `chat.rs`: kein Tool → Streaming (6a, beste UX); Tools aktiv →
  **ReAct-Loop**. Write-Tool → `hitlRequest` (Preview) + Run **parkt** via
  `oneshot` (Timeout 10 min) → approve/reject; `agent.hitl_disabled`
  überspringt HITL. `toolCallStarted/Completed`, `hitlResolved` über den
  Agenten-Channel → **alle Mitglieder sehen Tool-Lauf + Freigabe live**.
  HITL-Endpunkte real (`AppState.pending_hitl`).
- Frontend: **unverändert** — die portierte Local-UI (`HitlCard`,
  `ToolCallChip`, `useAgentChat`) + REST-Bridge unterstützen das bereits.

**Ergebnis:** `cargo build/fmt/clippy -D warnings` + 8 Tests grün,
`tsc`/`vite build` grün.

## Phase 6b-2a — ask_user ✅ ABGESCHLOSSEN (2026-05-19)

- `ask_user`-Tool (Teil des `files`-Skills): Agent stellt eine Rückfrage,
  der Run **parkt** (`AppState.pending_questions`, oneshot<String>,
  Timeout 10 min). `askUserRequest`/`askUserResolved` über den Agenten-
  Channel (live für alle). `/questions/{id}/respond` real (Body
  `{answer}`). Frontend unverändert (`AskUserCard`/`pendingQuestion`).
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

## Phase 6b-2b — write_xlsx ✅ ABGESCHLOSSEN (2026-05-19)

- `write_xlsx`-Tool (Skill `files`): erzeugt/überschreibt eine `.xlsx`
  (via `rust_xlsxwriter`), HITL-Vorschau `writeXlsx` (Frontend-`HitlCard`
  rendert das bereits). `is_write_tool` deckt jetzt beide Write-Tools ab.
- Write-Pfad in `chat.rs` aufgeräumt: Dispatch nach Tool-Name in
  `tools::write_preview` / `tools::execute_write` (append + xlsx); kein
  per-Tool-Code mehr im Run-Loop.
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

## Phase 6b-2c — write_docx ✅ ABGESCHLOSSEN (2026-05-19)

- `write_docx`-Tool (Skill `files`): erzeugt/überschreibt eine `.docx`
  aus Absätzen — **minimales OOXML-Zip ohne neue Dependency** (`zip` +
  XML-Strings, von `preview::docx_html` lesbar). HITL-Vorschau
  `writeDocx` (Frontend-`HitlCard` rendert das bereits). In den
  `write_preview`/`execute_write`-Dispatcher + `is_write_tool` +
  `skills_json` eingehängt.
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

## Phase 6b-2d — write_docx_from_template ✅ ABGESCHLOSSEN (2026-05-19)

- `write_docx_from_template`-Tool (Skill `files`): nimmt eine `.docx`-
  Workspace-Datei als Vorlage, ersetzt `{{Platzhalter}}` in
  `word/document.xml`, packt das Zip neu (alle anderen Teile verbatim →
  Formatierung bleibt). Platzhalter werden für die Vorschau heuristisch
  gescannt (run-übergreifende ignoriert — dokumentierte Grenze). Kein
  neues Dependency. HITL-Vorschau `writeDocxFromTemplate`
  (Frontend-`HitlCard` rendert das bereits). Vorlage per Dateiname
  (kein Agent-Attachment-Plumbing nötig).
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

## Phase 6b-2e — append_to_docx ✅ ABGESCHLOSSEN (2026-05-19)

- `append_to_docx`-Tool (Skill `files`): Absätze vor `<w:sectPr`/
  `</w:body>` in eine vorhandene `.docx` einfügen, Zip verbatim neu
  packen (Formatierung bleibt); fehlt die Datei → neu via `build_docx`.
  HITL-Vorschau `appendToDocx` inkl. `existingTail` (Text-Tail des
  Bestands). Docx-Repacking in `repack_docx` extrahiert, `fill_template`
  nutzt es jetzt mit. Kein neues Dependency.
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

## Phase 6b-2f — update_cells ✅ ABGESCHLOSSEN (2026-05-19)

- `update_cells`-Tool (Skill `files`): gezielte xlsx-Zell-Edits
  (`{"B2":"42"}`), Zellref-Parser `A1`→(row,col), Lesen via `calamine`,
  Before/After-Diff in HITL-Vorschau `updateCells` (Frontend-`HitlCard`
  rendert das bereits), Schreiben via `rust_xlsxwriter`.
- Refactor: xlsx-Bytes/Persist in `build_xlsx_bytes`/`save_xlsx`
  extrahiert (von `write_xlsx` mitgenutzt). Kein neues Dependency.
- **Grenze:** nur das Zielblatt wird neu geschrieben; Formeln/Formate/
  weitere Blätter gehen verloren (v1, dokumentiert).
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

## Phase 6b-2g — Delegation/Bulk-Worker ✅ ABGESCHLOSSEN (2026-05-19)

- `delegate_into_xlsx_column`-Tool: liest die xlsx, rendert pro Datenzeile
  ein Prompt-Template (`{{Header}}`/`{{A}}`), ruft je Zeile eine fokussierte
  Worker-Inferenz (`llm::tool_step` ohne Tools, knapper Worker-System-
  Prompt), schreibt die Ergebnisse in die Zielspalte. HITL-Vorschau
  `delegateIntoXlsxColumn` (Sample-Prompts) + Live-Events
  `delegationStarted/ItemDone/ItemFailed/Finished` über den Agenten-Channel
  (Frontend rendert das bereits). Row-Cap 200, cancel-bar. Sonderzweig in
  `chat.rs` (`is_delegate_tool`), nicht über den Write-Dispatcher.
- Gates: `cargo build/fmt/clippy -D warnings` + 8 Tests, `tsc`/`vite` grün.

**Damit ist die gesamte Phase 6 abgeschlossen** (6a + 6b-1…6b-2g): Chat,
Streaming, Shared-Session, Tools, HITL, Rückfragen, alle Datei-Schreib-
Operationen und Delegation — jeweils live für alle Workspace-Mitglieder.

**Bewusst verschoben (klein, optional):** `delegationProfile`-Override
(eigenes Worker-Modell/System je Agent), Agent-Attachment-`templateFileId`
als Komfort-Vorlagenquelle.

**Abnahme:** 6a Streaming live für alle · 6b-1 Datei-Tools+HITL ·
6b-2a Rückfragen · 6b-2b Excel · 6b-2c Word · 6b-2d Word-aus-Vorlage ·
6b-2e Word-Anhängen · 6b-2f Zell-Edits · 6b-2g Bulk-Delegation
(jeweils HITL, live für alle).

## Phase 6b-2h — grep_in_files (Workspace-Volltextsuche) ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Tool registriert (`backend/src/tools.rs`,
`GREP_TOOL = "grep_in_files"`), als Eintrag im `files`-Skill
(`skills_json()`), Dispatch in `execute_read_tool` ohne HITL/Broadcast.
Lesepfad: Kandidaten aus `workspace_files` (DB), Extension-Whitelist +
Größen-Cap pro Datei, `ensure_in_workspace` als Defense-in-Depth, Bytes
aus dem Volume, Treffer als `Datei:Zeile: Snippet`. Caps: 300 Dateien,
2 MiB pro Datei, 100 Hits, 200 Snippet-Chars. Integrationstests in
`backend/tests/integration.rs` decken: Happy-Path mit Pfad+Zeile,
`caseSensitive`-Schalter, Whitelist (`.bin` ignoriert), Cross-Workspace-
No-Leak, ungültiges Regex → 400, Hit-Cap mit Hinweis-Footer. Gates
grün: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
`cargo test --lib` (8), `cargo test --no-run` (Integration kompiliert),
`npm run build`. DB-Integrationstests laufen wie alle DB-Tests in CI
gegen den Postgres-Service-Container (§13).

Ziel: Read-only-Tool, mit dem ein Agent in **einem** Aufruf mehrere
Workspace-Dateien per Regex durchsucht und bis zu 100 Treffer mit
`Datei:Zeile: Snippet` zurückbekommt. Pendant zu `grep_in_files` aus
ProcessFox Local (`processfox_local/src-tauri/src/core/tool/tools/
grep_in_files.rs`), gehört dort zum `folder-search`-Skill; im Web
sitzt es im bestehenden `files`-Skill, damit kein neuer Skill nötig
ist. Konzeptionell schon in `CONCEPT.md` §3.3/§6 vermerkt — bei der
Skill-Migration in 6b-1 versehentlich rausgefallen.

**Aufruf-Vertrag (Tool-Input):**
- `pattern: string` — Rust-`regex`-Syntax.
- `caseSensitive?: boolean` — Default `false` (`(?i)`-Prefix vorne anhängen).

Bewusst **kein** `path`-Parameter: Der Web-Workspace ist flach
(siehe `sandbox::workspace_key`, alle Dateien direkt unter
`workspaces/<wid>/<filename>`), eine Unterordner-Begrenzung gäbe es nicht zu
filtern. Spart außerdem einen Eingabevektor für Pfad-Tricks.

**Implementierung in `backend/src/tools.rs`:**

1. Konstante `GREP_TOOL: &str = "grep_in_files"`.
2. `ToolSpec` in `all_tools()` mit obigem Schema und einer Beschreibung,
   die Caps + Whitelist nennt (das LLM braucht das, um sinnvolle Pattern
   zu wählen).
3. `skills_json()` `tools`-Array um `"grep_in_files"` erweitern (sonst
   filtert der Provider-Layer das Tool wieder weg).
4. Neue Funktion `async fn grep_in_files(state, wid, input) -> ApiResult<String>`:
   - Regex bauen (`(?i)`-Prefix wenn `caseSensitive != Some(true)`),
     Parse-Fehler → `ApiError::BadRequest`.
   - `SELECT filename, s3_key, size_bytes, content_type FROM
     workspace_files WHERE workspace_id = $1 ORDER BY filename`
     (Runtime-Query, kein Makro — CLAUDE.md §11). Nie `read_dir`
     aufs Volume: „DB ist Wahrheit, Volume ist Bytes" — alle
     Sichtbarkeits-/Permission-Invarianten leben in der DB.
   - Pro Zeile:
     - **Extension-Whitelist** analog Local: `md, txt, csv, json, yaml,
       yml, toml, html, htm, xml, rs, ts, tsx, js, jsx, py, go, c,
       cpp, h, hpp, sh`. Office-Formate (`pdf/docx/xlsx/pptx`) und
       Bilder sind binär und werden bewusst ausgeschlossen — für die
       gibt es eigene Reader (`preview.rs`, `read_xlsx_grid` etc.).
     - **Größen-Cap** `size_bytes > 2 MiB` → überspringen.
     - **Datei-Cap** total 300 (frühes Break, damit pathologische
       Workspaces nicht das Tool füllen).
   - `ensure_in_workspace(wid, &s3_key)` als Defense-in-Depth (auch wenn
     die DB-Zeile per Definition schon scoped ist).
   - Bytes via `std::fs::read(state.storage.path(&s3_key))`, dann
     `String::from_utf8` — nicht-UTF-8 still überspringen.
   - Zeile für Zeile matchen, max. 100 Treffer total. Pro Treffer:
     `format!("{filename}:{lineno}: {snippet}")`, `snippet =
     line.chars().take(200).collect()`.
   - Ausgabe wie in Local: Header-Zeile mit Anzahl Treffer + Anzahl
     gescannter Dateien, dann die Trefferliste, plus Cap-Hinweis
     („[hit cap reached — narrow the pattern]") wenn 100 erreicht.
5. Dispatcher-Arm in `execute_read_tool` (tools.rs:264) für `GREP_TOOL` —
   **kein** HITL, **kein** `fs-changed`-Broadcast (read-only).

**Sandbox & Sicherheit:**
- DB-Scoping (`WHERE workspace_id = $1`) + `ensure_in_workspace` pro
  `s3_key` → zwei Schichten gegen Cross-Workspace-Lecks.
- Kein `path`-Input vom LLM, also kein Traversal-Vektor.
- Regex-Compile via `regex` (kein Backtracking, lineare Worst-Case-
  Zeit) — kein ReDoS-Risiko nötig zu mitigieren.

**Frontend:**
- `src/lib/toolIcons.ts:19` (`grep_in_files: FileSearch`) ist schon
  vorhanden — keine Änderung nötig.
- Läuft im bestehenden Tool-Stream (`toolCallStarted/Completed` auf
  `chat:agent:<agentId>`); keine neuen WS-Channels.

**Tests (`backend/tests/integration.rs`):**
- Workspace + zwei `.md` + eine `.bin` (nicht-Whitelist) anlegen, über
  den Chat-Tool-Loop `grep_in_files` mit Pattern aufrufen, Treffer-
  Anzahl + Format prüfen, `.bin` ignoriert.
- Case-Insensitivity: `"Foo"` matched `"foo bar"` ohne `caseSensitive`,
  matched **nicht** mit `caseSensitive: true`.
- Cross-Workspace: Treffer-Pattern existiert nur in **anderer** Org →
  null Hits, kein Leak.
- Ungültiges Regex → `isError: true` mit lesbarer Meldung.
- Hit-Cap: viele Treffer in einer Datei → genau 100 Treffer, Cap-Hinweis
  in der Ausgabe.

**Build-Gates:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, `npm run build`.

**Abnahme:** Agent mit aktiviertem `files`-Skill kann in einem Chat-Turn
`grep_in_files` aufrufen, bekommt eine deterministische Trefferliste im
Format `Datei:Zeile: Snippet`, scannt nur den eigenen Workspace,
respektiert die Caps (300 Dateien, 2 MiB pro Datei, 100 Hits, Whitelist)
und ist read-only. CLAUDE.md §1a um „grep_in_files" im Tool-Inventar
ergänzen.

## Phase 6b-2i — read_pdf (PDF-Text-Extraktion) ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Tool registriert (`backend/src/tools.rs`,
`READ_PDF_TOOL = "read_pdf"`), Eintrag im `files`-Skill (`skills_json()`),
Dispatch in `execute_read_tool` ohne HITL/Broadcast. Lesepfad:
`sanitize_filename` + `.pdf`-Endung erzwingen → DB-Vorabcheck auf
`workspace_files` (Existenz + 20-MiB-Cap **vor** dem Parsen) →
`ensure_in_workspace` (Defense-in-Depth) → `pdf_extract::extract_text`
in `tokio::task::spawn_blocking` (schmal abgegrenzte §11-Ausnahme,
CLAUDE.md §11 entsprechend ergänzt). Output analog Local:
`--- filename (N Bytes) ---`-Header, 200-KiB-Truncation, expliziter
Hinweis bei leerer Extraktion. Kaputte PDFs werden zu einer freundlichen
Tool-Result-Meldung, **nicht** zu einem 500er — Stabilität für den
Chat-Loop. `pdf-extract = "0.7"` als neue Dependency (pure Rust, keine
native Lib → keine Dockerfile-Änderung).

Tests: 6 Integrationstests in `backend/tests/integration.rs`
(Happy-Path mit hand-gebauter Mini-PDF + bekanntem Text, falsche Endung
→ 400, fehlende Datei → freundliche Meldung, Cross-Workspace-No-Leak,
20-MiB-Cap, kaputte Bytes → freundliche Meldung) plus ein
DB-unabhängiger Unit-Test `tools::pdf_fixture_tests::
extract_roundtrips_known_text`, der die Fixture-Konstruktion (PDF mit
berechneten xref-Offsets, Helvetica als Standard-Font) gegen
`pdf-extract` round-trippt. Gates grün: `cargo fmt --check`,
`cargo clippy --all-targets -D warnings`, `cargo test --lib` (9 Tests),
`cargo test --no-run` (Integration kompiliert), `npm run build`.
Doku-Patches: CLAUDE.md §1a-Header + Backend-Bullet + §11
(`spawn_blocking`-Klarstellung), DEPLOY.md §8 (neue Grenzen).

Ziel: Read-only-Tool, mit dem ein Agent den Text einer hochgeladenen
PDF (`*.pdf` im Workspace) extrahiert. Pendant zu `read_pdf` aus
ProcessFox Local (`processfox_local/src-tauri/src/core/tool/tools/
read_pdf.rs`). Bisheriger Web-Stand: `.pdf` ist als Upload-Format
zugelassen (`routes/files.rs:41`), das Frontend-Icon
(`src/lib/toolIcons.ts:22`) ist da — aber das Tool selbst fehlt.

**Aufruf-Vertrag (Tool-Input):**
- `filename: string` — Workspace-Datei, muss auf `.pdf` enden.

Bewusst **kein** `path` (Web-Workspace ist flach, `sanitize_filename`
strippt Verzeichnisse). Bewusst **kein** `pageRange` in v1 — Local hat es
auch nicht; Truncation am Ende fängt zu lange Dokumente sauber ab.

**Dependency:** `pdf-extract = "0.7"` in `backend/Cargo.toml` (pure
Rust, keine native Lib — fügt sich ins schmale `debian:bookworm-slim`-
Runtime-Image ohne weitere Pakete).

**Implementierung in `backend/src/tools.rs`:**

1. Konstante `READ_PDF_TOOL: &str = "read_pdf"`.
2. Caps: `READ_PDF_MAX_INPUT_BYTES: i64 = 20 * 1024 * 1024` (20 MiB —
   Schutz vor pathologischen Parsen; Upload-Limit ist 50 MiB, hier
   tighter), `READ_PDF_MAX_OUTPUT_BYTES: usize = 200 * 1024` (analog
   Local).
3. `ToolSpec` in `all_tools()` direkt nach dem `grep_in_files`-Eintrag:
   Beschreibung nennt Limits und den Hinweis, dass gescannte PDFs ohne
   OCR-Layer leer zurückkommen können (der Agent muss das verstehen).
4. `skills_json()` `tools`-Array um `"read_pdf"` erweitern.
5. Funktion `async fn read_pdf(state, wid, input) -> ApiResult<String>`:
   - `filename` extrahieren → `sanitize_filename` → muss auf `.pdf`
     enden (lower-case-Vergleich), sonst `400 BadRequest`.
   - **Größen-Vorab-Prüfung aus der DB:**
     `SELECT size_bytes FROM workspace_files WHERE workspace_id = $1
     AND filename = $2` → wenn `> READ_PDF_MAX_INPUT_BYTES` →
     `400 BadRequest` mit lesbarer Meldung. „DB ist Wahrheit, Volume
     ist Bytes" — Sichtbarkeit/Existenz kommt aus der DB, nicht aus
     `std::fs::metadata`.
   - `workspace_key` + `ensure_in_workspace` (Defense-in-Depth).
   - Existenz auf dem Volume prüfen (`path.is_file()`); wenn nicht →
     freundliche Meldung „(Datei nicht gefunden)" (gleicher Stil wie
     `read_file`), **nicht** als Fehler, damit das LLM elegant
     fortsetzen kann.
   - **CPU-Bound auf den Blocking-Pool:**
     `tokio::task::spawn_blocking(move || pdf_extract::extract_text(&path))`.
     Begründete Ausnahme zu CLAUDE.md §11 („kein `spawn_blocking` nötig"
     — das galt für den LLM-Pfad; PDF-Parsing blockiert sonst den
     Tokio-Runtime). §11 in einem zweiten Patch um eine Klarstellung
     ergänzen.
   - Fehler aus `pdf-extract` → freundlicher String (kein 500), damit
     ein kaputtes PDF den ganzen Chat-Turn nicht abreißt; Form analog
     Local (`"PDF konnte nicht gelesen werden: …"`).
   - Output-Form analog Local:
     - Header: `--- {filename} ({total_bytes} Bytes) ---`.
     - Bei `extracted.trim().is_empty()`: Header + Hinweis „leere
       Extraktion — vermutlich gescanntes PDF ohne OCR".
     - Bei `total_bytes > 200 KiB`: erste ~50 000 Zeichen + Footer
       `[gekürzt — Extraktion überschreitet 200 KB]`.
     - Sonst: Header + Volltext.
6. Dispatcher-Arm in `execute_read_tool` für `READ_PDF_TOOL` — kein
   HITL, kein Broadcast.

**Sandbox & Sicherheit:**
- `sanitize_filename` strippt Pfad-Trennzeichen + `..`.
- `ensure_in_workspace` über den Storage-Key.
- 20-MiB-Input-Cap **vor** dem Parse: schützt gegen DoS durch sehr
  große PDFs (Single-Tenant-Local hatte das Problem nicht; Multi-Tenant-
  Web schon).
- `spawn_blocking` isoliert CPU-Last vom Async-Reaktor; eine teure PDF
  blockiert nicht die WS- und HTTP-Antworten der anderen Nutzer.

**Frontend:**
- `src/lib/toolIcons.ts:22` (`read_pdf: FileType`) ist schon da.
- Keine neuen WS-Channels, läuft im bestehenden Tool-Stream.

**Tests (`backend/tests/integration.rs`, analog `grep_in_files`):**
- **Happy-Path:** Mini-PDF (Bytes via `include_bytes!("fixtures/
  hello.pdf")` — die Fixture enthält einen bekannten String wie
  „ProcessFox PDF Test"), in einen Workspace seedet, `read_pdf`
  liefert den String + den `---`-Header.
- **Falsche Endung:** `filename: "notes.txt"` → `400 BadRequest`.
- **Nicht vorhandene Datei:** `filename: "missing.pdf"` →
  Treffer „Datei nicht gefunden" (kein Fehler, da das LLM gracefully
  fortsetzen können soll).
- **Cross-Workspace-No-Leak:** PDF in `ws_a`, Abfrage aus `ws_b` →
  „Datei nicht gefunden", keine Bytes geleakt.
- **Größen-Cap:** `workspace_files`-Zeile mit `size_bytes = 30 *
  1024 * 1024` (Volume-Bytes leer; der Cap-Check feuert vor dem
  Lesen) → `400 BadRequest` mit Größen-Hinweis.
- **Leere Extraktion / kaputtes PDF:** Bytes „nicht-PDF" hochladen →
  Tool liefert eine **freundliche** Meldung („PDF konnte nicht
  gelesen werden: …" oder „leere Extraktion …"), **kein** `unwrap_err()`
  (Stabilität für den Chat-Loop).

**Doku-Patches:**
- `CLAUDE.md` §11 um eine knappe Klarstellung ergänzen: „`spawn_blocking`
  nur dort, wo CPU-gebunden — z. B. `read_pdf`".
- `CLAUDE.md` §1a-Backend-Bullet um `read_pdf` anhängen.
- `DEPLOY.md` §8 (bekannte Grenzen) um „PDFs > 20 MiB werden für
  `read_pdf` abgelehnt; gescannte PDFs ohne OCR liefern leeren Text"
  ergänzen.

**Build-Gates:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, `npm run build`.

**Abnahme:** Agent mit `files`-Skill kann ein im Workspace liegendes
PDF per `read_pdf` lesen, bekommt den Text mit Header/Truncation-/
Empty-Hinweis zurück, scannt nichts außerhalb des Workspaces,
respektiert den 20-MiB-Input- und 200-KiB-Output-Cap, blockiert den
Tokio-Runtime nicht und bleibt bei kaputten PDFs stabil (freundlicher
Tool-Result statt 500).

## Phase 6b-2j — read_docx (Word-Text-Extraktion) ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Tool registriert (`backend/src/tools.rs`,
`READ_DOCX_TOOL = "read_docx"`), als Eintrag im `files`-Skill
(`skills_json()`), Dispatch in `execute_read_tool` ohne HITL/Broadcast.
Lesepfad: `sanitize_filename` + `.docx`-Endung → DB-Vorabcheck (Existenz
+ 20-MiB-Cap **vor** dem Inflaten) → `ensure_in_workspace` → Bytes vom
Volume → `tokio::task::spawn_blocking` für ZIP-Inflate + XML-Parse
(gleiche §11-Ausnahme wie `read_pdf`). Output analog `read_pdf`:
Header `--- filename (N Bytes) ---`, 200-KiB-Truncation-Footer,
Hinweis bei leerer Extraktion. Kaputte DOCX werden zu einer freundlichen
Tool-Result-Meldung, **kein** 500.

Refactor mit umgesetzt: die naive `<w:t>`-Find-Schleife (alter
`docx_text`-Helfer) ist durch `extract_docx_text_from_xml` ersetzt
(`quick_xml`-Events, `<w:t>`-Text, `<w:br>` → `\n`, `<w:p>`-Ende →
`\n\n`, Drei-Newline-Kollaps). Genutzt vom neuen Tool **und** der
HITL-Tail-Vorschau in `appenddocx_preview` — eine Quelle, kein Drift.
Keine neue Cargo-Dependency (`quick_xml` und `zip` waren schon drin
für Office-Vorschau und -Schreiben).

Tests: 6 Integrationstests in `backend/tests/integration.rs`
(Happy-Path mit hand-gebauter Mini-DOCX `make_minimal_docx`, falsche
Endung → 400, fehlende Datei → freundlich, Cross-Workspace-No-Leak,
20-MiB-Cap, kaputte Bytes → freundlich) plus zwei DB-unabhängige
Unit-Tests `tools::docx_fixture_tests::roundtrips_paragraph_text` und
`rejects_non_zip_input`, die das eigene `build_docx` gegen den neuen
Extraktor round-trippen.

Gates grün: `cargo fmt --check`, `cargo clippy --all-targets
-D warnings`, `cargo test --lib` (11 Tests), `cargo test --no-run`
(Integration kompiliert), `npm run build`. Doku-Patches: CLAUDE.md
§1a-Header + Backend-Bullet (Tool-Inventar mit Hinweis auf den
geteilten Extraktor), DEPLOY.md §8 (neue Grenzen).

Ziel: Read-only-Tool, das den Lauftext einer Workspace-`.docx` für das
LLM liefert. Pendant zu `read_docx` aus ProcessFox Local
(`processfox_local/src-tauri/src/core/tool/tools/read_docx.rs`).
Aktueller Web-Stand: Browser-Vorschau (`preview.rs::docx`) liefert
JSON nur fürs UI; der interne Helfer `tools.rs::read_docx_doc_opt` +
`docx_text` deckt nur die HITL-Tail-Anzeige bei `append_to_docx` ab
und ist über die Tool-API nicht erreichbar. Frontend-Icon
(`src/lib/toolIcons.ts:20`, `read_docx: FileText`) ist da.

**Aufruf-Vertrag (Tool-Input):**
- `filename: string` — Workspace-Datei, muss auf `.docx` enden.

Bewusst **kein** `path` (Web-Workspace ist flach). Konsistent mit
`read_file`/`read_pdf`/`grep_in_files`.

**Keine neue Dependency:** `zip` (Cargo.toml Zeile 60, deflate-Feature)
und `quick_xml` (Zeile 61) sind beide bereits eingebunden — für die
Office-Vorschau und das XLSX-/DOCX-Schreiben.

**Implementierung in `backend/src/tools.rs`:**

1. Konstante `READ_DOCX_TOOL: &str = "read_docx"`.
2. Caps konsistent zu `read_pdf`:
   - `READ_DOCX_MAX_INPUT_BYTES: i64 = 20 * 1024 * 1024` (Eingaberobust,
     Upload-Limit bleibt 50 MiB).
   - `READ_DOCX_MAX_OUTPUT_BYTES: usize = 200 * 1024`.
3. `ToolSpec` in `all_tools()` direkt nach dem `read_pdf`-Eintrag.
   Beschreibung nennt: extrahiert nur Lauftext (Tabellen-Zellen-Inhalte
   bleiben implizit erhalten, da `<w:t>` in `<w:tc>` steckt; Bilder/
   eingebettete Objekte werden gestrippt), Paragraph-Trennung mit
   Leerzeile.
4. `skills_json()` `tools`-Array um `"read_docx"` erweitern.
5. Neue Hilfsfunktion `extract_docx_text(bytes: &[u8]) -> ApiResult<String>`
   (oben im Datei, neben den anderen DOCX-Helpern):
   - ZIP aus `&[u8]` (`std::io::Cursor`), `word/document.xml` lesen.
   - `quick_xml::Reader::from_str` mit `trim_text(false)`, Events
     durchgehen: `<w:t>` → Text einsammeln (mit `unescape()`),
     `<w:p>` Ende → `"\n\n"`, `<w:br/>` (Empty) → `"\n"`. Namespace-
     Prefix `w:` per kleinem `local_name`-Helfer abschneiden.
   - Drei-oder-mehr-Newlines auf zwei kollabieren.
   - Parse-Fehler → `ApiError::BadRequest("DOCX-XML konnte nicht
     gelesen werden: …")`.
6. Funktion `async fn read_docx(state, wid, input) -> ApiResult<String>`:
   - `filename` extrahieren → `sanitize_filename` → `.docx`-Endung
     erzwingen (sonst `400`).
   - DB-Vorabcheck `workspace_files` (Existenz + 20-MiB-Cap) **vor**
     dem ZIP-Entpacken. „DB ist Wahrheit, Volume ist Bytes".
   - `ensure_in_workspace` (Defense-in-Depth).
   - Bytes laden (`std::fs::read`); wenn das Volume die Datei nicht
     hat → freundliche „nicht gefunden"-Meldung (nicht 500).
   - **`tokio::task::spawn_blocking`** für `extract_docx_text(&bytes)` —
     gleiche §11-Ausnahme wie bei `read_pdf` (ZIP-Inflate + XML-Parse
     sind CPU-gebunden; v. a. Decks mit großen `document.xml` würden
     sonst den Tokio-Runtime blocken).
   - Fehler aus dem Parser → freundlicher Tool-Result-String (Chat-
     Loop-Stabilität).
   - Output-Form analog `read_pdf`:
     - `--- {fname} ({N} Bytes) ---` Header.
     - Leerer Trim → `"[leere Extraktion — Dokument enthält keinen
       Lauftext]"`.
     - > 200 KiB → erste ~50 000 Zeichen + Footer `"[gekürzt — Extraktion
       überschreitet 200 KB]"`.
     - Sonst: Volltext.
7. Dispatcher-Arm in `execute_read_tool` für `READ_DOCX_TOOL` — kein
   HITL, kein Broadcast.
8. **Refactor:** Den bestehenden internen `docx_text`-Helfer (naive
   `<w:t>`-Find-Schleife) durch die neue `extract_docx_text` ersetzen
   und `appenddocx_preview` darauf umstellen. Damit nutzen Tool und
   HITL-Tail-Vorschau **denselben** Extraktor — eine Wahrheit, weniger
   Drift. `read_docx_doc_opt` bleibt als kombinierte „Bytes + raw XML"-
   Hilfe für `do_append_docx` (dort wird das XML auch geschrieben).

**Sandbox & Sicherheit:**
- `sanitize_filename` + `ensure_in_workspace`.
- Größen-Cap vor dem Parsen (DoS-Schutz).
- ZIP-Bomb-Risiko: wir lesen ausschließlich `word/document.xml`, nicht
  rekursiv alle Entries — die ZIP-Bibliothek dekomprimiert nur einen
  Entry. Sollte ein extrem aufgeblähtes `document.xml` eine ZIP-Bomb
  liefern, fängt der 200-KiB-Output-Cap das im Result ab (der Parser
  selbst läuft auf einem zur Verfügung gestellten String — die
  unkomprimierte Größe ist durch die ZIP-Entry-Größe begrenzt, die
  wiederum durch den 20-MiB-Input-Cap der gesamten Datei limitiert
  ist).

**Frontend:**
- Icon ist schon da.
- Keine WS-Channel-Änderungen, läuft im bestehenden Tool-Stream.

**Tests (`backend/tests/integration.rs`):**
- **Happy-Path:** `build_docx(&["Hallo Welt", "Zweite Zeile"])` (eigene
  schon vorhandene Builder-Funktion, siehe Phase 6b-2c) liefert
  Test-Bytes → seeden → `read_docx` enthält beide Strings mit
  Paragraph-Trennung (`\n\n`).
- **Falsche Endung:** `filename: "notes.txt"` → `400`.
- **Nicht vorhandene Datei:** → freundliche „nicht gefunden"-Meldung.
- **Cross-Workspace-No-Leak:** DOCX in `ws_a`, Abfrage aus `ws_b` →
  „nicht gefunden".
- **20-MiB-Cap:** `workspace_files`-Zeile mit `size_bytes = 30 MiB`
  → `400` mit Größen-Hinweis.
- **Kaputte Bytes / kein gültiges ZIP:** `b"definitiv kein DOCX"` mit
  `.docx`-Endung → freundliche Meldung, **kein** `unwrap_err()`.
- **Roundtrip-Smoketest** als zusätzlicher Unit-Test in `tools.rs`
  (`#[cfg(test)] mod docx_fixture_tests`): `build_docx` → `extract_docx_text`
  → Text enthält die erwarteten Absätze. DB-frei, läuft lokal.

**Doku-Patches:**
- `CLAUDE.md` §1a-Backend-Bullet um `read_docx` ergänzen.
- `DEPLOY.md` §8: neue Zeile zu `read_docx`-Grenzen
  (20-MiB-Eingabe, 200-KiB-Ausgabe, nur Lauftext — Tabellen-Inhalt ja,
  Bilder/eingebettete Objekte werden gestrippt).

**Build-Gates:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --all-targets`, `npm run build`.

**Abnahme:** Agent mit `files`-Skill kann eine im Workspace liegende
DOCX per `read_docx` lesen, bekommt den Lauftext mit Header-/Empty-/
Truncation-Hinweis zurück, scannt nichts außerhalb des Workspaces,
respektiert den 20-MiB-Input- und 200-KiB-Output-Cap, blockiert den
Tokio-Runtime nicht und bleibt bei kaputten DOCX stabil (freundlicher
Tool-Result statt 500). Tool **und** HITL-Tail-Vorschau bei
`append_to_docx` nutzen denselben Extraktor.

## Querschnitt — Härtung ✅ ABGESCHLOSSEN (2026-05-19)

- HTTP/DB-Integrationstests `backend/tests/integration.rs`: echte Axum-
  Handler via `tower::ServiceExt::oneshot`, pro Test frische Postgres-DB
  über `#[sqlx::test(migrations = "./src/db/migrations")]`. Abgedeckt:
  Health/Auth-Guard, Magic-Link-`verify` (Happy-Path, unbekannt, abge-
  laufen, single-use), Refresh-Token-Rotation + Revoke des alten Cookies,
  Workspace-Berechtigungen (Owner legt an, Member/Viewer → 403, Viewer
  liest aber keine Owner-Aktion, Cross-Org → 404 ohne Leak),
  Account-Enumeration-Schutz bei `request-login`.
- CI: neuer Workflow `.github/workflows/ci.yml` (push/PR) — fmt + clippy
  + `cargo test --all-targets` gegen einen Postgres-16-Service-Container.
- Gates: `cargo fmt/clippy -D warnings` grün, `cargo test --no-run`
  kompiliert die Integrationstests, Unit-Tests (8) grün. Die DB-Tests
  laufen in CI (lokal kein Postgres/Docker verfügbar).

**Damit ist auch die letzte über alle Phasen vorgemerkte Härtung
geschlossen.** Optional offen bleibt nur noch (klein):
`delegationProfile`-Override, Agent-Attachment-`templateFileId`.

---

## Phase 7 — Deployment

- Multi-Stage-Dockerfile (Frontend → Backend → Runtime, §12).
- Coolify-Env-Vars dokumentiert; Postgres + MinIO-Services.
- GitHub Actions: `cargo fmt/clippy/test`, `npm run build`, Docker-Build.
- Domain `chat.processfox.ai` (Reverse-Proxy/TLS via Coolify).

**Abnahme:** Deploy auf `main`; `chat.processfox.ai` erreichbar; Canary grün.

---

## Querschnitt: RPC → REST Mapping (Phase 0/1 vorbereitend)

Die Bridge wird von `POST /api/<command>` auf REST `/api/v1/...` umgestellt.
Beispiel-Abbildung (vollständige Liste beim Bridge-Rewrite):

| Bisher (RPC) | Künftig (REST) |
|---|---|
| `POST /api/list_agents` | `GET /api/v1/workspaces/:wid/agents` |
| `POST /api/get_agent {id}` | `GET /api/v1/agents/:id` |
| `POST /api/create_agent {draft}` | `POST /api/v1/workspaces/:wid/agents` |
| `POST /api/update_agent {id,update}` | `PATCH /api/v1/agents/:id` |
| `POST /api/delete_agent {id}` | `DELETE /api/v1/agents/:id` |
| `POST /api/list_messages {agentId}` | `GET /api/v1/agents/:id/messages` |
| `POST /api/send_message {...}` | `POST /api/v1/agents/:id/messages` |
| `POST /api/import_files_to_agent` | `POST /api/v1/workspaces/:wid/files` (multipart) |
| `POST /api/preview_docx {path}` | `GET /api/v1/workspaces/:wid/files/:fid/preview/docx` |
| WS `/ws/<channel>` | `GET /ws?token=<access>`, Nachrichten `{type,payload}` |

> Hinweis: `modelsApi`-Kommandos entfallen ersatzlos (Local-Cleanup).
> CLAUDE.md §7 und die §7-Diskrepanz-Notiz aktualisieren, sobald die Bridge steht.

## Registrierung (betrifft Phase 2/3)

**Modell:** Jede Organisation besitzt einen **6-stelligen Invite-Code**
(`organizations.invite_code`). **Jede** Registrierung erfordert diesen Code —
es gibt **keinen** Org-Erstellungs-Endpunkt in der App.

- **Erste Org + Owner:** werden vom Betreiber **manuell in der DB** angelegt
  (Org-Zeile inkl. `invite_code`, Owner-User mit `org_role = owner`).
  Kein Henne-Ei-Problem, kein App-Bootstrap-Pfad nötig.
- **Code-Format:** 6 Zeichen, Charset ohne mehrdeutige Zeichen
  (kein `0/O`, `1/I/L`) → 32^6 Raum. Eindeutig (DB-Constraint).
  Case-insensitive Eingabe, intern normalisiert.
- **Owner kann Code rotieren** (`POST /orgs/:id/rotate-invite-code`),
  falls geleakt. Alter Code wird sofort ungültig.
- **Rollen beim Beitritt:** neuer User → `org_role = member`, **keine**
  Workspace-Mitgliedschaft (Owner ordnet später Workspaces/Rollen zu).
- **E-Mail:** global eindeutig (ein Account = eine Org in v1).
- **Abuse-Schutz:** Rate-Limit auf `register`/`login`, Code-Versuche
  gedrosselt (Brute-Force auf 6 Zeichen sonst trivial).

## Planungslücken (in den Plan aufgenommen)

Bei der Durchsicht gefundene, vorher fehlende Punkte:

1. **Refresh-Token-Persistenz** (Phase 2): Schema um `refresh_tokens`
   (Hash + Ablauf + revoked_at) ergänzt → Logout/Revocation/Rotation
   möglich. (CLAUDE.md §8 aktualisiert.)
2. **WS-Auth-Lebensdauer** (Phase 6): Access-Token (15 min) < WS-Lebensdauer.
   → Single multiplexte WS-Verbindung `GET /ws?token=`, Client reconnectet
   mit frischem Token nach Refresh; Server schließt bei Token-Ablauf.
   Konsolidiert zugleich die Bridge-Divergenz `/ws/<channel>` → ein Kanal
   mit `{type,payload}` (CLAUDE.md §7).
3. **Shared-Session-Nebenläufigkeit** ✅ GELÖST (Phase 6a-Härtung,
   2026-05-19): genau ein aktiver Run pro Agent (`active_runs`, 2. Send →
   409); Run-State per `chat:agent:<id>` an alle Workspace-Mitglieder live.
4. **Skill-Quelle im Web** (Phase 4/6): Local liest `SKILL.md` von Disk.
   Web: Skills werden **mit dem Backend-Binary gebündelt** (read-only,
   kein User-Script — CLAUDE.md §3 Regel 7), `skillsApi.list()` liefert sie.
5. **Agent-Attachment-Referenz** (Phase 4/5): `attachments.templatePath`
   zeigt heute auf einen Pfad → muss auf eine `workspace_files`-ID umgestellt
   werden; Auto-Clear, wenn Datei gelöscht (WS-Event `agent-attachments-changed`).
6. **Datei-Namenskollision** (Phase 5): gleicher Dateiname erneut hochgeladen
   → Default: überschreiben + neue Version-Metadaten; Entscheidung vor Phase 5
   final festzurren.
7. **Test-DB-Widerspruch** (Querschnitt): CLAUDE.md §13 nannte In-Memory-SQLite
   — inkompatibel mit Postgres-spezifischem, compile-time-geprüftem sqlx.
   → korrigiert auf Postgres-Testcontainer / `#[sqlx::test]`.
8. **API-Key-Verschlüsselung** (Phase 4): Verhalten bei fehlendem/rotiertem
   `API_KEY_ENCRYPTION_KEY` definieren (Fail-fast beim Start, klare Fehlermeldung).
9. **Delegation/Sub-Agenten** (out of scope v1): `Agent.delegationProfile`
   bleibt im Typ, Backend implementiert es in v1 **nicht** — explizit als
   Nicht-Ziel dokumentieren, damit es nicht implizit erwartet wird.

## Vor dem jeweiligen Phasenstart final zu entscheiden

- **Phase 5:** Namenskollisions-Strategie (überschreiben vs. versionieren vs. ablehnen)?
- **Phase 6:** Provider-API-Versionen/Modell-IDs aktuell halten; Prompt-Caching-
  Granularität (System-Prompt + Tool-Schemas cachen).
