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

## Phase 6b-2k — read_xlsx_range (Excel-Bereichs-Read) ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Tool registriert (`backend/src/tools.rs`,
`READ_XLSX_TOOL = "read_xlsx_range"`), Eintrag im `files`-Skill
(`skills_json()`), Dispatch in `execute_read_tool` ohne HITL/Broadcast.
Lesepfad: `sanitize_filename` + `.xlsx`-Endung → Range-Parse via
`parse_cell_ref` (mit Default-Fenster 25×12 ab `start = A1`) →
500-Zellen-Cap **vor** dem DB-Lookup (billig auszuwerten und gibt dem
LLM schnelles Feedback) → DB-Existenz-Check → `ensure_in_workspace` →
`tokio::task::spawn_blocking` für `calamine::open_workbook_from_rs` +
Range-Extraktion (gleiche §11-Ausnahme wie bei `read_pdf`/`read_docx`).
Output ist **JSON** (entschieden 2026-05-20, Abweichung von Local-CSV)
`{file, sheet, range, headers, rows}` mit erster Range-Zeile als
`headers`, restlichen als `rows`, alle Werte als Strings (kein
Type-Drift bei Mixed-Type-Columns / Excel-Datums-Serials). Cell-
Formatter `format_xlsx_cell` (Floats ganzzahlig kompakt, ISO-Strings
unverändert, `#err:`-Marker für Excel-Errors).

Wiederverwendete Helfer ohne Duplikate: `parse_cell_ref` und
`col_letter` (beide aus dem `update_cells`/`delegate`-Pfad). Keine
neue Cargo-Dependency — `calamine` ist seit der Office-Vorschau drin.

Tests: 8 Integrationstests in `backend/tests/integration.rs`
(Default-Range mit Header+Rows, explizite Range-Eingrenzung,
unbekanntes Sheet → 400 mit Liste verfügbarer Namen, falsche Endung
→ 400, fehlende Datei → freundlich, Cross-Workspace-No-Leak,
500-Zellen-Cap, JSON-Quoting für Komma/Anführungszeichen/Newline-
Zellinhalt) plus zwei DB-freie Unit-Tests
`tools::xlsx_range_fixture_tests::roundtrips_known_cells` und
`parse_cell_ref_roundtrips`, die `build_xlsx_bytes` gegen den
Cell-Formatter round-trippen.

Gates grün: `cargo fmt --check`, `cargo clippy --all-targets
-D warnings`, `cargo test --lib` (13 Tests), `cargo test --no-run`
(Integration kompiliert), `npm run build`. Doku-Patches: CLAUDE.md
§1a-Header + Backend-Bullet (Tool-Inventar mit JSON-Format-Hinweis),
DEPLOY.md §8 (neue Grenzen + JSON-Output-Notiz).

Ziel: Read-only-Tool, das einem Agent einen rechteckigen Zellbereich
einer Workspace-`.xlsx` (inkl. der Header-Zeile, wenn der Bereich mit
`A1` startet) als CSV liefert. Pendant zu `read_xlsx_range` aus
ProcessFox Local (`processfox_local/src-tauri/src/core/tool/tools/
read_xlsx_range.rs`). Aktueller Web-Stand: nur ein interner
`read_xlsx_grid`-Helfer für `update_cells` und Delegation; der lädt
immer das ganze Sheet und ist über die Tool-API nicht erreichbar.
Frontend-Icon (`src/lib/toolIcons.ts:21`, `read_xlsx_range:
FileSpreadsheet`) ist da.

**Aufruf-Vertrag (Tool-Input):**
- `filename: string` — Workspace-Datei, muss auf `.xlsx` enden.
- `sheet?: string` — Name; Default = erstes Sheet des Workbooks.
- `start?: string` — Top-Left, z. B. `"A1"` (Default `"A1"`).
- `end?: string` — Bottom-Right, z. B. `"F40"` (Default: 25×12-
  Fenster von `start` aus, also bei `A1`-Default die Range `A1:L25`).

**Output-Format:** **JSON** (entschieden 2026-05-20 — Abweichung von
Local-CSV, weil JSON die Header-/Daten-Trennung explizit macht und das
LLM die Struktur ohne erneutes Parsen versteht). Pretty-printed, alle
Zell-Werte als **Strings** (verhindert Type-Drift bei Mixed-Type-
Columns, Excel-Datums-Serials etc.):

```json
{
  "file": "report.xlsx",
  "sheet": "Tabelle1",
  "range": "A1:L25",
  "headers": ["Name", "Rolle"],
  "rows": [
    ["Alice", "Owner"],
    ["Bob", "Editor"]
  ]
}
```

Konvention: erste Zeile der Range → `headers`, restliche Zeilen →
`rows`. Range mit nur einer Zeile → `headers` gesetzt, `rows: []`.
Wer reine Daten ohne Header-Konnotation braucht, setzt z. B.
`start: "A2"` — dann ist die A2-Zeile syntaktisch der „Header".

Bewusst **kein** `path` (Web-Workspace ist flach). Konsistent mit
`read_file`/`read_pdf`/`read_docx`/`grep_in_files`.

**Keine neue Dependency:** `calamine` ist bereits eingebunden
(Office-Vorschau).

**Implementierung in `backend/src/tools.rs`:**

1. Konstante `READ_XLSX_TOOL: &str = "read_xlsx_range"`.
2. Cap: `READ_XLSX_MAX_CELLS: usize = 500` (analog Local — hält den
   LLM-Kontext vorhersehbar, größere Ranges → erneuter Aufruf mit
   engerem Fenster).
3. `ToolSpec` in `all_tools()` direkt nach `read_docx`. Beschreibung
   nennt: JSON-Output `{file, sheet, range, headers, rows}` (erste
   Zeile der Range → `headers`, Rest → `rows`, alle Werte als
   Strings), 500-Zellen-Cap, Defaults.
4. `skills_json()` `tools`-Array um `"read_xlsx_range"` erweitern.
5. Neue Funktion `async fn read_xlsx_range(state, wid, input) ->
   ApiResult<String>`:
   - `filename` → `sanitize_filename` → `.xlsx`-Endung erzwingen
     (sonst `400`).
   - DB-Vorabcheck `workspace_files` (Existenz). Kein Eingabe-
     Größen-Cap nötig, weil der 500-Zellen-Cap die LLM-Kontextlast
     deckelt und `calamine` das ganze Workbook ohnehin in den
     Speicher liest (bei xlsx > 50 MB greift dafür schon das
     Upload-Limit).
   - `ensure_in_workspace` (Defense-in-Depth).
   - Bytes vom Volume, **`tokio::task::spawn_blocking`** für
     `calamine::open_workbook_from_rs` + Range-Extraktion (gleiche
     §11-Ausnahme wie bei den anderen Office-/PDF-Readern). Die
     Existenz-Prüfung am Volume vor dem Spawn lassen, um den
     Blocking-Pool nicht für jeden 404-Versuch zu belegen.
   - Sheet-Name: `params.sheet`, sonst `wb.sheet_names()[0]`.
     Unbekannter Sheet-Name → `BadRequest` mit Liste der
     verfügbaren Namen (gibt dem LLM was es zum Korrigieren
     braucht).
   - Range parsen via vorhandenes `parse_cell_ref` (tools.rs:832,
     0-basiert) — `start` Default `A1`, `end` Default
     `(start_row + 24, start_col + 11)`. End vor Start → `BadRequest`.
   - Zell-Anzahl prüfen, > 500 → `BadRequest` mit Hinweis „bitte
     Range einschränken".
   - **Zell-Formatierung** (alles in Strings — kein JSON-Quoting im
     Cell-Layer, das übernimmt `serde_json` beim Serialisieren):
     - `Empty` → leerer String.
     - `String` → unverändert.
     - `Float`: ganzzahlig + `|f| < 1e15` → `(*f as i64).to_string()`;
       sonst `f.to_string()` (Default-Format).
     - `Int`/`Bool` → `to_string`.
     - `DateTime` → numerischer Excel-Seriendatum-Wert (`as_f64()`).
     - `DateTimeIso`/`DurationIso` → ISO-String.
     - `Error` → `#err:<debug>`.
   - JSON-Aufbau: erste Range-Zeile → `headers: [String]`, restliche
     Zeilen → `rows: [[String]]`. Mit `serde_json::to_string_pretty`
     serialisieren — Lesbarkeit im Chat-Log gewinnt gegen die
     paar zusätzlichen Tokens.
6. Dispatcher-Arm in `execute_read_tool` für `READ_XLSX_TOOL` —
   kein HITL, kein Broadcast.

**Sandbox & Sicherheit:**
- `sanitize_filename` + `ensure_in_workspace` (Defense-in-Depth).
- Zell-Cap 500 begrenzt die LLM-Kontextlast und damit auch die
  Output-Größe.
- Kein zusätzlicher Größen-Cap auf dem Workbook — der schon
  bestehende 50-MB-Upload-Limit ist die obere Schranke, und
  `calamine` ist deutlich weniger DoS-anfällig als
  `pdf-extract`/Zip-Bomben (xlsx ist ein ZIP, aber wir lesen die
  ganze Mappe nicht aus, sondern adressieren Zellen über
  `worksheet_range`).
- `spawn_blocking` schützt vor Tokio-Runtime-Blockaden bei großen
  Mappen mit vielen Sheets.

**Frontend:**
- Icon ist schon da; keine WS-Channel-Änderungen.

**Tests (`backend/tests/integration.rs`):**
- **Happy-Path:** `build_xlsx_bytes("Tabelle1", &[["Name", "Rolle"],
  ["Alice", "Owner"], ["Bob", "Editor"]])` (vorhandener Helfer aus
  Phase 6b-2b) → seeden → `read_xlsx_range` ohne `start`/`end`
  → CSV enthält die Header-Zeile + beide Datenzeilen.
- **Range-Eingrenzung:** `start: "A1", end: "B2"` → `headers` mit
  2 Spalten, `rows` mit 1 Zeile à 2 Spalten.
- **Sheet-Auswahl + Fehler:** Workbook mit Sheet `"Tabelle1"`,
  Tool mit `sheet: "Anderes"` → `BadRequest`, Liste verfügbarer
  Namen enthalten.
- **Falsche Endung:** `filename: "notes.txt"` → `400`.
- **Fehlende Datei:** → freundliche „nicht gefunden"-Meldung.
- **Cross-Workspace-No-Leak:** xlsx in `ws_a`, Abfrage aus `ws_b`
  → „nicht gefunden", keine Bytes geleakt.
- **Zellen-Cap:** `start: "A1", end: "T30"` (20 × 30 = 600 Zellen)
  → `BadRequest` mit Cap-Hinweis.
- **JSON-Quoting:** Zelle mit Komma, Anführungszeichen und Newline
  → `serde_json` quotet das automatisch korrekt; Test parsed das
  Tool-Result-JSON wieder und prüft den genauen String-Wert.
- **DB-freier Roundtrip-Unit-Test** `tools::xlsx_range_fixture_tests::
  roundtrips_known_cells`: `build_xlsx_bytes` → durch den Cell-
  Formatter → erwartete JSON-Struktur. Hält Schreib- und Lese-Pfad
  ohne Postgres geeicht.

**Doku-Patches:**
- `CLAUDE.md` §1a-Backend-Bullet um `read_xlsx_range` ergänzen.
- `DEPLOY.md` §8: neue Zeile zu den Grenzen
  (500-Zellen-Cap pro Aufruf, JSON-Output `{file, sheet, range,
  headers, rows}`, kein extra Größen-Cap).

**Build-Gates:** `cargo fmt`, `cargo clippy --all-targets -- -D
warnings`, `cargo test --all-targets`, `npm run build`.

**Abnahme:** Agent mit `files`-Skill kann eine Workspace-`.xlsx`
per `read_xlsx_range` lesen, bekommt strukturiertes JSON mit
`file`/`sheet`/`range`/`headers`/`rows` zurück, scannt nur den
eigenen Workspace, respektiert den 500-Zellen-Cap, blockiert den
Tokio-Runtime nicht und bleibt bei fehlerhaften Sheets/Zellen-
Adressen mit lesbaren Meldungen stabil.

## Phase 6b-2l — rewrite_file (Komplett-Überschreiben mit Diff-HITL) ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Tool registriert (`backend/src/tools.rs`,
`REWRITE_TOOL = "rewrite_file"`), als Eintrag im `files`-Skill
(`skills_json()`), in `is_write_tool` aufgenommen → der generische
HITL-Loop in `chat.rs:635` übernimmt Preview/Approval/Execute ohne
Sonderpfad. Endungs-Whitelist `.md`, `.markdown`, `.txt`, `.text`,
`.csv` (entschieden 2026-05-20: CSV mit aufgenommen, Web-Erweiterung
gegenüber Local). 5-MiB-Bestands-Cap (entschieden 2026-05-20: 5 MiB
statt 1 MiB lässt große Protokolle/Daten-CSVs durch und schützt
trotzdem den Client-Diff vor pathologischen Größen).

Schreibpfad: `sanitize_filename` → Endungs-Check → `ensure_in_workspace`
→ Bestand laden + UTF-8-Prüfung + Cap-Check → JSON-Preview mit `kind:
"rewriteFile"`, `before`, `after`, `createsFile` → HITL-Dialog (das
Frontend rendert die Zeilen-Diff via vorhandener `DiffSection`/
`diffLines`-Logik) → nach Approve: `std::fs::write` + `workspace_files`-
Upsert mit content-type-Ableitung (`text/markdown` / `text/plain` /
`text/csv`) + `fs-changed`-Broadcast.

Helper `rewrite_extension` und `rewrite_content_type` zentralisieren
die Endungs-Whitelist-Logik (eine Quelle, zwei Aufrufer: Preview und
Do). Defense-in-Depth: `do_rewrite` prüft die Endung nochmal, falls
`execute_write` direkt aufgerufen wird.

Tests: 8 Integrationstests in `backend/tests/integration.rs`:
Preview-Happy-Path mit `before`/`after`, neue Datei `createsFile`,
`.docx` → 400, Nicht-UTF-8-Bestand → 400, 6-MiB-Bestand → 400 mit
Limit-Hinweis, `do_rewrite`-Roundtrip mit Volume+DB-Verifizierung,
CSV-Anlage mit content-type-Check, Cross-Workspace-No-Leak.

Gates grün: `cargo fmt --check`, `cargo clippy --all-targets
-D warnings`, `cargo test --lib` (13 Tests — keine neuen Lib-Tests,
weil die Logik in DB/FS-abhängige Pfade fällt), `cargo test --no-run`
(Integration kompiliert), `npm run build`. Doku-Patches: CLAUDE.md
§1a-Header + Backend-Bullet, DEPLOY.md §8 (Whitelist + 5-MiB-Cap +
HITL-Diff-Hinweis).

Ziel: Schreib-Tool, mit dem ein Agent eine Markdown-/Text-Datei im
Workspace **komplett ersetzt** — mit Zeilen-Diff (`before`/`after`)
in der HITL-Vorschau. Pendant zu `rewrite_file` aus ProcessFox Local
(`processfox_local/src-tauri/src/core/tool/tools/rewrite_file.rs`).
Aktueller Web-Stand: das **Frontend ist komplett vorbereitet** —
`types/chat.ts:29` definiert die `rewriteFile`-Preview-Variante,
`HitlCard.tsx:118` + `:208` rendern Header + `DiffSection` über
`diffLines(before, after)`, Icon-Mapping da. **Nur das Backend-Tool
fehlt.**

**Aufruf-Vertrag (Tool-Input):**
- `filename: string` — Workspace-Datei, muss auf `.md`, `.markdown`,
  `.txt`, `.text` oder `.csv` enden (entschieden 2026-05-20, Web-
  Erweiterung gegenüber Local: CSV ist im Web ein reguläres Upload-
  Format und Text-Editierung sinnvoll). **Bewusst nicht** `.docx`/
  `.xlsx` (eigene Tools mit eigenen Diff-Modellen) — auch nicht
  `.pdf` (nicht text-überschreibbar).
- `content: string` — kompletter neuer Inhalt.

**Implementierung in `backend/src/tools.rs`:**

1. Konstante `REWRITE_TOOL: &str = "rewrite_file"`.
2. Cap: `REWRITE_MAX_EXISTING_BYTES: i64 = 5 * 1024 * 1024` (5 MiB)
   auf den **Bestand** (entschieden 2026-05-20). Begründung: der
   Diff in der HITL-Vorschau rendert client-seitig per `diffLines`;
   bei mehr als ein paar MiB Vergleichstext wird das spürbar zäh.
   5 MiB lässt auch große Protokoll-/Daten-Dateien zu und schützt
   gleichzeitig vor pathologischen Total-Rewrites von 50-MiB-
   Uploads. Local kannte das Problem nicht (Single-User).
   Output-Cap auf `after` ist nicht nötig — das LLM-Context-Limit
   deckelt das natürlich.
3. Endungs-Whitelist:
   ```rust
   const REWRITE_ALLOWED: &[&str] = &["md", "markdown", "txt", "text", "csv"];
   ```
4. `ToolSpec` in `all_tools()`. Beschreibung erklärt: nur Text-
   Formate, **vor** dem Aufruf bestenfalls `read_file` machen damit
   der `content` den existierenden Inhalt sinnvoll fortführt, für
   reines Anhängen `append_to_file` (sicherer — keine Risiko-
   Löschung), für Word/Excel die jeweiligen eigenen Tools.
5. `skills_json()` `tools`-Array um `"rewrite_file"` erweitern.
6. `is_write_tool` um `name == REWRITE_TOOL` ergänzen — schaltet
   den HITL-Pfad in `chat.rs:635` ein.
7. `rewrite_preview(state, wid, filename, content) -> ApiResult<Value>`:
   - `sanitize_filename` + Endungs-Check (sonst `400` mit Hinweis
     auf die Spezial-Tools für docx/xlsx).
   - `workspace_key` + `ensure_in_workspace`.
   - Bestand laden: `std::fs::read(path)` → `String::from_utf8`. Wenn
     nicht UTF-8: `400` mit Hinweis (das Tool ist für Text-Dateien
     gedacht; binäre Bestände würden im Diff sowieso nicht
     funktionieren).
   - Bestands-Cap prüfen: `before.len() > REWRITE_MAX_EXISTING_BYTES`
     → `400` mit Hinweis „Bestand zu groß für rewrite_file
     (… Bytes, Limit 1 MB) — bitte append-Tools oder gezielte
     Edits nutzen".
   - JSON liefern:
     ```json
     {
       "kind": "rewriteFile",
       "path": "<sanitized>",
       "before": "<existing UTF-8 content or empty>",
       "after": "<content>",
       "createsFile": <bool>
     }
     ```
8. `do_rewrite(state, wid, uploaded_by, filename, content)
   -> ApiResult<String>`:
   - Erneut `sanitize_filename` + Endungs-Check (Defense-in-Depth —
     `write_preview` und `execute_write` werden über die WS sicher
     mit derselben Input-JSON aufgerufen, aber die Prüfung wiegt
     nichts und schließt jede Race-Lücke).
   - `workspace_key` + `ensure_in_workspace`.
   - `std::fs::write(path, content.as_bytes())` — überschreibt
     komplett, legt bei Bedarf an.
   - `content_type` aus Endung ableiten:
     `.md`/`.markdown` → `text/markdown`,
     `.txt`/`.text` → `text/plain`,
     `.csv` → `text/csv`.
   - `workspace_files`-Upsert mit `ON CONFLICT (workspace_id,
     filename) DO UPDATE SET size_bytes, content_type, uploaded_by,
     uploaded_at` — dasselbe Muster wie `do_append`.
   - `state.ws.publish(Some(wid), "fs-changed", Null)`.
   - Result-String: `"Datei '{fname}' überschrieben (N Bytes)."`
     bzw. `"… angelegt (N Bytes)."` je nach `created`.
9. `write_preview`-Match-Arm und `execute_write`-Match-Arm
   ergänzen (analog zu den anderen sechs Schreib-Tools).

**Sandbox & Sicherheit:**
- `sanitize_filename` + `ensure_in_workspace` (Defense-in-Depth).
- Endungs-Whitelist erzwingt Text-Modell.
- Bestands-Cap (1 MiB) deckt UX-Risiko (großer Diff im Browser) und
  DoS-Risiko (Bestand komplett in Speicher gezogen) gleichzeitig.
- HITL ist **Pflicht** — wie alle Write-Tools. `hitl_disabled`-Pfad
  (für Org-weite Auto-Approve-Settings) verhält sich wie bei den
  anderen Schreib-Tools.

**Frontend:**
- Komplett vorbereitet. Keine Änderungen.

**Tests (`backend/tests/integration.rs`):**
Da der HITL-Loop in `chat.rs` läuft (nicht in `tools.rs`), testen
wir die beiden öffentlichen Funktionen direkt:

- **Preview Happy-Path:** `notes.md` mit Inhalt seedten →
  `write_preview` → JSON enthält `kind: "rewriteFile"`, `before`
  exakt der Seed-Inhalt, `after` der neue Content, `createsFile:
  false`.
- **Preview neue Datei:** `new.md` (nicht existent) → `before: ""`,
  `createsFile: true`.
- **Preview falsche Endung:** `notes.docx` → `BadRequest` mit
  Hinweis auf docx-Tools.
- **Preview UTF-8-Bruch:** Datei mit Bytes `0xFF 0xFE …` mit
  `.txt`-Endung → `BadRequest` (binär).
- **Preview Bestands-Cap:** Datei mit 6 MiB Inhalt seedten →
  `BadRequest` mit „zu groß" + Limit-Hinweis.
- **Execute-Roundtrip:** `do_rewrite` aufrufen → Volume-Bytes
  passen, `workspace_files`-Zeile auf neue `size_bytes`/`content_type`/
  `uploaded_by` aktualisiert.
- **Execute neue Datei:** `do_rewrite` auf nicht-existente
  `new.md` → Volume hat die Datei, DB-Zeile angelegt.
- **Cross-Workspace-No-Leak (Preview):** Datei in `ws_a`, Preview-
  Aufruf aus `ws_b` mit gleichem `filename` → `createsFile: true`,
  `before: ""` — keine Bytes geleakt.

`is_write_tool`-Erweiterung wird durch das schon vorhandene HITL-
Integrationsmuster im Chat-Loop abgedeckt; ein zusätzlicher
HITL-E2E-Test ist nicht nötig (gleiche Verdrahtung wie die sechs
anderen Schreib-Tools).

**Doku-Patches:**
- `CLAUDE.md` §1a-Backend-Bullet um `rewrite_file` ergänzen.
- `DEPLOY.md` §8: neue Zeile zu Endungs-Whitelist (`.md`,
  `.markdown`, `.txt`, `.text`, `.csv`) + 5-MiB-Bestands-Cap.

**Build-Gates:** `cargo fmt`, `cargo clippy --all-targets -- -D
warnings`, `cargo test --all-targets`, `npm run build`.

**Abnahme:** Agent mit `files`-Skill kann eine `.md`/`.markdown`/
`.txt`/`.text`/`.csv`-Datei im Workspace komplett ersetzen; der User sieht
vor der Freigabe einen Zeilen-Diff in der HITL-Card; nach Freigabe
ist die Datei überschrieben (oder neu angelegt), `workspace_files`
ist aktualisiert, ein `fs-changed`-Broadcast erreicht alle Workspace-
Mitglieder live; nach Reject bleibt die Datei unberührt.

## Phase 6c — Skill-Registry + Progressive Tool-Disclosure

Ziel: Den hartkodierten `"files"`-Catch-all-Skill durch eine echte
Skill-Registry mit `SKILL.md`-Frontmatter-Parsing ersetzen und das LLM
vor Tool-Choice-Verwirrung schützen. Im Vergleich zu Local
**strikter:** dort sind alle Skill-Tool-Schemas immer an den Provider
deklariert (`processfox_local/.../chat/run.rs:948
collect_tool_schemas`); Progressive Disclosure passiert nur am
Skill-Body-Layer. Hier wollen wir Progressive Disclosure **bis auf
den Tool-Schema-Layer** — das LLM sieht initial nur eine Skill-Liste
(Titel + Description + Tool-Namen) und kann durch `read_skill` die
volle Anleitung **und** die Provider-Tool-Schemas freischalten. So
wird das Modell nicht durch unpassende `update_cells`/`delegate_…`-
Schemas gestört, wenn es nur lesen will.

---

### Entscheidungen vorab (gilt für 6c-1 … 6c-3)

| Thema | Entscheidung |
|---|---|
| Variante | **Tool-Schema-Disclosure** (strikter als Local). `read_skill` ist immer deklariert; jedes andere Tool-Schema kommt erst in den Provider-Calls nach einem erfolgreichen `read_skill`-Aufruf für seinen Skill. |
| Skill-Quelle | Built-in im Docker-Image (`backend/skills_builtin/`). User-Skills pro Org sind 6c-4, kein v1-Muss. |
| Skill-Schnitt | Web bekommt **5 Skills statt 11** (Web-Tool-Inventar ist kompakter, kein `document-edit` per Find-Replace, kein `chat-context`). Schnitt unten in 6c-2. |
| `ask_user` | Bleibt **außerhalb** des Skill-Systems und ist immer deklariert — es ist kein User-fakultatives Tool, sondern Infrastruktur. |
| Cache-Trade-off | Anthropic-Prompt-Caching wird durch wachsende Tool-Listen pro Skill-Load weniger effektiv (jeder Schema-Wechsel invalidiert den Tools-Cache-Block). Bewusst hingenommen — Tool-Choice-Verwirrung kostet das LLM in der Antwortqualität mehr als ein paar Token-Cents kosten. Mitigation: gleicher Skill-Load-Stand → gleicher Tools-Block → Cache-Hit. |
| Persistenz | Geladene Skills gelten **pro Run** (also pro User-Turn mit Tool-Loop). Im System-Prompt-Hinweis steht „Skills already loaded earlier in this conversation stay in scope" — wir vermerken den Load-State nicht persistent. |
| Migration | Bestehende Agents in der DB haben `skills = ["files"]`. Eine SQL-Migration (`0004_skills_resharded.sql`) setzt sie auf das neue 5er-Set um (semantisch äquivalent: gleiche Tool-Menge). Der „files"-Slot fällt aus `skills_json()` raus. |

---

### Phase 6c-1 — SKILL.md-Parser + Registry (Fundament) ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Neues Modul `backend/src/skills.rs` mit `Skill`/
`SkillHitl`-Strukturen (camelCase + `snake_case`-Alias für YAML-
Autoren), `parse_skill_md(&str) -> ApiResult<Skill>` und
`SkillRegistry::load_from_dir(&Path) -> ApiResult<Self>` (rekursiv,
deterministische `list()`-Sortierung). Neue Dependency `serde_yaml`.
12 Unit-Tests grün — Frontmatter-Parsing (minimal/full), camelCase-
und snake_case-Aliase, alle vier strukturellen Fehlerfälle (kein
Frontmatter, unbeendet, kaputtes YAML, leerer `name`/`title`),
Registry (Listing, doppelte Namen, Parse-Fehler-mit-Pfad, Ignorieren
von Nicht-SKILL.md-Dateien). Gates grün: `cargo fmt --check`,
`cargo clippy --all-targets -D warnings`, `cargo test --lib`
(25 Tests). Reine Library-Schicht, keine Aufrufer im Backend
geändert — Fundament für 6c-2/6c-3.

**Ziel:** Reine, in-memory-Bibliothek; noch nichts mit Files-on-Disk
oder LLM-Verdrahtung.

**Neues Modul `backend/src/skills.rs`** (flaches Layout wie der Rest
des Backends, vgl. §6):

1. Datentypen mit `serde`-camelCase-Bridge:
   ```rust
   pub struct Skill {
       pub name: String,
       pub title: String,
       pub description: String,
       pub icon: Option<String>,
       pub tools: Vec<String>,
       pub hitl: SkillHitl,
       pub accepts_attachments: Vec<String>,
       pub language: String,
       pub body: String,
   }
   pub struct SkillHitl {
       pub default: bool,
       pub per_tool: BTreeMap<String, bool>,
   }
   ```
2. `parse_skill_md(text: &str) -> ApiResult<Skill>`:
   - Erwartet ein YAML-Frontmatter zwischen `---\n` (Anfang) und
     `\n---\n` (Ende), Rest = Body.
   - Frontmatter via `serde_yaml::from_str` in die Struct. Body
     `.trim_start_matches('\n').to_string()`.
   - Defaults: `language = "en"`, `hitl.default = false`,
     `accepts_attachments = []`.
   - Fehlerfälle: kein `---`-Anfang, kein `---`-Ende, ungültiges
     YAML, `name` leer → `BadRequest` mit aussagekräftigem Hinweis.
3. `pub struct SkillRegistry { by_name: HashMap<String, Arc<Skill>> }`:
   - `SkillRegistry::load_from_dir(path: &Path) -> ApiResult<Self>`:
     rekursiv alle `SKILL.md` einsammeln, parsen, in die Map
     einhängen. Doppelter `name` → Fehler (kein „last wins" — das
     versteckt Bugs).
   - `get(name) -> Option<Arc<Skill>>`.
   - `list() -> Vec<Arc<Skill>>` (alphabetisch nach `name`, damit das
     System-Prompt-Listing deterministisch ist → Cache-Stabilität).
4. **Neue Dependency:** `serde_yaml = "0.9"`.

**Tests (`backend/src/skills.rs`, `#[cfg(test)] mod tests`):**
- `parse_minimal_skill_md` — minimales SKILL.md mit nur den
  Pflichtfeldern wird sauber geparst, Defaults stimmen.
- `parse_full_skill_md` — alle Felder gesetzt (inkl. `hitl.perTool`,
  `acceptsAttachments`), Body wird vom Frontmatter sauber abgetrennt.
- `rejects_missing_frontmatter` — Body ohne `---` → `BadRequest`.
- `rejects_unclosed_frontmatter` — `---` am Anfang, kein Ende.
- `rejects_invalid_yaml` — kaputtes YAML im Frontmatter.
- `registry_rejects_duplicate_names` — zwei `SKILL.md` mit gleichem
  `name` im selben Verzeichnis → Fehler.
- `registry_lists_deterministically` — Files in beliebiger
  Filesystem-Order → `list()` ist alphabetisch.

**Gates:** `cargo fmt`, `cargo clippy -D warnings`, `cargo test --lib`.

**Abnahme:** `skills.rs` ist als Modul aufgebaut und getestet, kein
Aufrufer im Backend muss sich noch ändern. Reines Fundament.

---

### Phase 6c-2 — Built-in Skills im Image + Registry in AppState ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Fünf `SKILL.md`-Dateien in `backend/skills_builtin/`
(`folder-search`, `document-read`, `document-write`, `table-read`,
`table-write`) mit eigenständig formulierten Choose-the-right-tool-
Bodies auf Web-Tool-Inventar (Caps, JSON-Output von `read_xlsx_range`,
HITL-Hinweis bei Schreib-Bundles, Verweise zwischen Skills).
`SKILLS_DIR`-Env-Var im `config.rs` (Default `/app/skills_builtin`).
`AppState` bekommt `pub skills: Arc<SkillRegistry>`, beim Bootstrap in
`main.rs` aus dem Verzeichnis geladen (harter Abbruch bei Parse-
Problemen — built-ins sind unter unserer Kontrolle). `GET /api/v1/skills`
liest jetzt aus der Registry und serialisiert deterministisch
alphabetisch; das alte `tools::skills_json()` ist gelöscht.
`available_tools` ist registry-aware: nimmt `(&SkillRegistry,
&[skill_names])`, baut die Tool-Schemas als Vereinigung der gelisteten
Skill-Tools, plus `ask_user` immer dazu. Legacy-Slot `"files"` wird
weiterhin als Catch-all akzeptiert (Defense-in-Depth gegen Race
zwischen Migration und Pre-Deploy-Agent-Anlage).

Migration `0004_skills_resharded.sql` setzt alle Agents mit
`skills = ["files"]` auf
`["folder-search","document-read","document-write","table-read","table-write"]`
(semantisch äquivalent — gleiche Tool-Menge). Dockerfile-Patch:
`COPY backend/skills_builtin /app/skills_builtin` in der Runtime-Stage
+ `ENV SKILLS_DIR=/app/skills_builtin`.

Tests: zwei neue Integrationstests in `backend/tests/integration.rs`:
`skills_endpoint_returns_built_in_set` (deterministische 5er-Liste +
Vertrags-Check pro Skill: `title`, `description`, `tools` Array,
`hitl.default` bool, `language: "de"`) und
`migration_resharded_existing_agents` (Legacy-Agent mit
`["files"]` → SQL-Resharding → fünf Skills im Array). `test_state`
lädt die echten Built-ins aus `CARGO_MANIFEST_DIR/skills_builtin` —
fehlerhafte SKILL.md würde im Test-Setup panicen.

Gates grün: `cargo fmt --check`, `cargo clippy --all-targets
-D warnings`, `cargo test --lib` (25 Tests), `cargo test --no-run`
(Integration kompiliert), `npm run build`. Doku: CLAUDE.md §1a-Header
+ Backend-Bullet (Skill-Registry), §6 (Verzeichnis-Layout) und §12
(Env-Var-Tabelle: `SKILLS_DIR`).

**Ziel:** Fünf neue Web-Skills als Markdown-Dateien im Repo,
Verzeichnis ins Docker-Image, beim App-Start einlesen, über die
bestehende Bridge `GET /api/v1/skills` ausliefern. Hartkodierter
`skills_json()` verschwindet.

**Skill-Schnitt für Web (5 Bundles, 13 Tools + `ask_user` global):**

| Skill | Titel (DE) | Tools |
|---|---|---|
| `folder-search` | Ordner durchsuchen | `list_files`, `read_file`, `grep_in_files` |
| `document-read` | Dokumente lesen | `read_pdf`, `read_docx` |
| `document-write` | Text-/Word-Dokumente schreiben | `write_docx`, `write_docx_from_template`, `append_to_docx`, `append_to_file`, `rewrite_file` |
| `table-read` | Tabellen lesen | `read_xlsx_range` |
| `table-write` | Tabellen schreiben | `write_xlsx`, `update_cells`, `delegate_into_xlsx_column` |

Begründung der Aufteilung:
- **Lesen ↔ Schreiben getrennt** (am wichtigsten für Tool-Choice-
  Verwirrung — ein Agent, der nur lesen soll, sieht keine `write_*`-
  Schemas).
- **Text/Word ↔ Tabellen getrennt** (XLSX-Semantik unterscheidet sich
  deutlich von Markdown/Word).
- **`folder-search` als Startpunkt-Skill** (das LLM lädt diesen zuerst,
  wenn es nicht weiß was im Workspace liegt).
- `delegate_into_xlsx_column` bleibt im `table-write`-Bundle (LLM
  versteht „Spalten füllen" als Sub-Variante von „Tabellen schreiben").

**Verzeichnis `backend/skills_builtin/<skill>/SKILL.md`** (5 Dateien).
Pro Datei: YAML-Frontmatter wie Local + Markdown-Body mit Choose-the-
right-tool-Guidance. Bodies werden **selbst geschrieben**, nicht
1:1 aus Local übernommen — Web-Tool-Namen weichen ab (`list_files`
statt `list_folder`, `read_xlsx_range`-CSV → -JSON, Default-Range,
HITL-Defaults usw.). HITL-Defaults pro Skill:
- `folder-search`/`document-read`/`table-read`: `hitl.default: false`
- `document-write`/`table-write`: `hitl.default: true`

**Backend:**
1. `config.rs`: neue Env-Var `SKILLS_DIR` (Default `/app/skills_builtin`).
2. `lib.rs`: `AppState` bekommt `pub skills: Arc<SkillRegistry>`.
   In `main.rs` beim Bootstrap aus `config.skills_dir` laden; harter
   Fehler bei Parse-Problem (built-ins sind unter unserer Kontrolle).
3. `routes/mod.rs`: `GET /api/v1/skills` antwortet aus der Registry
   statt aus `tools::skills_json()` — gleiche JSON-Shape (das
   Frontend bleibt unverändert). `tools::skills_json()` wird gelöscht.
4. **Migration `0004_skills_resharded.sql`:** alle Agents mit
   `skills = ["files"]` → setzen auf
   `["folder-search","document-read","document-write","table-read","table-write"]`.
   Idempotent geschrieben (`WHERE skills = '["files"]'::jsonb`).
5. **Dockerfile-Patch:** `COPY backend/skills_builtin /app/skills_builtin`
   in der Runtime-Stage. `.dockerignore` so anpassen, dass
   `backend/skills_builtin` mitkopiert wird.

**Tests (`backend/tests/integration.rs`):**
- `skills_endpoint_returns_built_in_set`: nach `build_app(state)` mit
  in-Tests geladener Registry liefert `GET /api/v1/skills` ein Array
  mit den 5 erwarteten `name`-Strings, deterministisch sortiert.
- `migration_resharded_existing_agents`: vor der Migration einen
  Agent mit `skills = ["files"]` seeden, dann den `0004`-Schritt
  einzeln laufen lassen (manuell per `sqlx::query`, weil
  `#[sqlx::test]` alle Migrationen vorab anwendet) — oder
  alternativ verifizieren, dass nach dem Test-Setup ein
  vor-existierender Agent mit `["files"]` auf das neue Set steht.
  Pragmatisch: einen post-migration State seeden und prüfen, dass
  beide Patterns (Legacy + neu) durch `available_tools` (siehe 6c-3)
  die richtigen Tool-Schemas ergeben.

**Doku-Patch:**
- `CLAUDE.md` §6 (Verzeichnis-Layout) um `backend/skills_builtin/`
  ergänzen.
- `CLAUDE.md` §12 (Deployment): `SKILLS_DIR` zur Env-Var-Tabelle.

**Gates:** wie üblich.

**Abnahme:** `GET /api/v1/skills` liefert die 5 Skills aus den
SKILL.md-Dateien; bestehende Agents zeigen im UI die richtigen
Skill-Toggles; `tools::skills_json` ist gelöscht; Docker-Image
enthält die `skills_builtin/`.

---

### Phase 6c-3 — `compose_system_prompt` + `read_skill` + Progressive Tool-Disclosure ✅ ABGESCHLOSSEN (2026-05-20)

**Ergebnis:** Der Tool-Loop in `chat.rs` läuft jetzt mit echter
Progressive Disclosure auf Tool-Schema-Ebene:

- Neues Modul `backend/src/prompt.rs` mit asynchronem
  `compose_system_prompt` (zieht Workspace-Übersicht aus
  `workspace_files`) und der reinen `compose_with_summary`-Funktion
  (DB-frei testbar). Pro Iteration neu komponiert: Datum-Anker,
  Agent-Vorgabe (DB-Spalte), Workspace-Übersicht (top 30 Dateien als
  Bullets, Suffix „… N weitere" bei mehr; `pub workspace_summary`
  einmal pro Run aus der DB gezogen, dann wiederverwendet),
  Available-Skills-Block (deterministisch alphabetisch, nur Titel +
  Description + Tool-Namen — **kein** Body), Thoroughness-Policy,
  Sprach-Direktive.
- Neues `read_skill`-Tool (`READ_SKILL_TOOL`), in `all_tools()`
  registriert, schema `{ skillId: string }`, im Tool-Loop als
  eigener Branch (kein HITL, kein Broadcast). Auf erfolgreiche
  Aufrufe wird `loaded` (Vec<String> im Run-Scope) erweitert und
  beim **nächsten** Provider-Call die Tool-Schemas via
  `tools_for_step(registry, &loaded)` mitgerechnet.
- Zwei neue Tool-Schema-Helfer: `base_tool_schemas()` (immer:
  `read_skill` + `ask_user`) und `schemas_for_loaded(registry,
  loaded)` (die deduplizierten Tools der geladenen Skills).
  `tools_for_step` setzt beide zusammen.
- `effective_hitl(registry, loaded, tool_name)` ersetzt das alte
  `is_write_tool` als HITL-Schaltstelle: startet bei
  `is_write_tool`, Skill-`perTool: true` kann **strenger** machen,
  `perTool: false` darf **nicht** abschalten — das Sicherheitsnetz
  bleibt.
- Tool-Loop-Guard: ein vom LLM aufgerufenes Tool, das nicht im
  aktuellen `tools_for_step`-Output steht, kriegt eine freundliche
  „lies zuerst den passenden Skill"-Meldung statt 500.
- Legacy-Defense-in-Depth: ein Agent mit `skills = ["files"]` wird
  serverseitig auf das 5er-Set gemappt (Migration `0004` macht das
  global; das hier schützt vor Race-Conditions).
- Reines Streaming bleibt nur noch der Fall **ohne aktivierte
  Skills** (= reiner Chat-Agent ohne Tools).

Tests: 7 neue DB-freie Unit-Tests in `tools::progressive_disclosure_tests`
(`base_tools_are_just_read_skill_and_ask_user`,
`nothing_loaded_means_only_base_tools_are_exposed`,
`reading_a_skill_unlocks_exactly_its_tools`,
`loaded_set_is_deduplicated_and_deterministic`,
`effective_hitl_keeps_writes_gated_without_skill`,
`effective_hitl_per_tool_override_can_only_tighten`,
`effective_hitl_per_tool_override_can_tighten_a_read_tool`) plus 6
Composer-Tests in `prompt::tests` (Skill-Block-Layout mit Titel/
Description/Tool-Namen, „schon geladen"-Marker, alphabetische
Sortierung, kein Body-Leak, Composer-Vollständigkeit, Omit-leere-
Pieces). `cargo test --lib` summa 39 Tests grün.

Gates: `cargo fmt --check`, `cargo clippy --all-targets -D warnings`,
`cargo test --lib`, `cargo test --no-run`, `npm run build`. Doku:
CLAUDE.md §1a-Header + Backend-Bullet (mit explizitem Verweis auf
`prompt.rs` + `tools_for_step` + `effective_hitl`), §6 Verzeichnis-
Layout (prompt.rs), §10 (Progressive-Disclosure-Cache-Trade-off).
DEPLOY.md §8 (Zwei-Schritte-Hinweis: erst `read_skill`, dann Tool).

**Ziel:** Das LLM sieht im System-Prompt eine kompakte Skill-Liste
(Titel + Description + Tool-Namen, **keine** Schemas). Initial sind
auf Provider-Ebene nur `read_skill` und `ask_user` deklariert. Jeder
erfolgreiche `read_skill(id)` schaltet den Body **und** die Tool-
Schemas dieses Skills für die folgenden Provider-Calls frei.

**Neuer Tool-Eintrag `read_skill`** (in `tools.rs`):
- Konstante `READ_SKILL_TOOL: &str = "read_skill"`.
- Schema: `{ skillId: string }`.
- Beschreibung: „Lädt die volle Anleitung für einen Skill. Lies einen
  Skill, bevor du seine Tools nutzt — der Skill-Body erklärt wann
  und wie. Nicht für Skills, die du schon geladen hast."
- Implementierung als **Read-Tool ohne HITL** in einem neuen
  Sub-Modul (es ist semantisch anders als die Workspace-Read-Tools —
  liefert Skill-Body statt Workspace-Bytes). Dispatch in `chat.rs`
  läuft über einen separaten Zweig (siehe unten), nicht über
  `execute_read_tool` — wir brauchen den Side-Effect „Skill als
  geladen markieren" pro Run.
- `is_read_skill_tool(name)` Helfer.

**Neuer System-Prompt-Composer (`backend/src/prompt.rs`)**:
```rust
pub fn compose_system_prompt(
    agent_prompt: &str,
    skills: &SkillRegistry,
    agent_skills: &[String],
    workspace_summary: &str,
    loaded_skills: &[String], // Bodies, die im aktuellen Run schon geladen wurden
) -> String { ... }
```
Zusammensetzung in dieser Reihenfolge:
1. Datum-Anker (`Today is YYYY-MM-DD (Weekday).`).
2. `agent_prompt` (DB-Spalte, falls nicht leer).
3. **Workspace-Übersicht** (kompakt): erste 30 Dateinamen + Größe
   aus `workspace_files`, mehr → `[… N weitere Dateien]`. Stützt
   die „erst orientieren"-Disziplin ohne `read_file`-Spam.
4. **Available skills** als bulleted list — pro Skill: Titel,
   `(id: <name>)`, Description, plus eine Zeile
   `tools: tool_a, tool_b, …`. Erklärt-Block oben dass die
   Anleitung erst nach `read_skill` sichtbar wird und dass `ask_user`
   immer erlaubt ist.
5. **Geladene Skill-Bodies** (die seit dem letzten User-Turn schon
   via `read_skill` reinkamen, falls dieser Schritt im Run-Lifecycle
   nochmal komponiert wird). Wird im einfachsten Fall leer sein,
   weil der Composer pro Provider-Call ohnehin neu läuft und Bodies
   im History als Tool-Results stehen.
6. Globale Thoroughness-Policy (aus Local übernommen).
7. Sprach-Direktive (sprich die Sprache der Nutzer:in).

**Workspace-Übersicht** baut eine kleine Funktion
`workspace_summary(state, wid) -> String` direkt aus `workspace_files`
(DB ist die Wahrheit). Kein FS-Walk.

**Tool-Loop-Anpassung (`backend/src/routes/chat.rs`)**:
Bisher (Phase 6a/6b):
```
tools = available_tools(skills)   // einmal pro Run
loop: provider call → tool calls → tool results → repeat
```
Neu:
```
loaded: HashSet<String> = ∅
provider_tools = base_tools()  // = [read_skill, ask_user]
loop:
  provider call mit (system_prompt(...,loaded), provider_tools, history)
  für jedes tool_call:
    wenn name == read_skill:
       body = registry.get(skillId).body
       loaded.insert(skillId)
       provider_tools = base_tools() + ⋃{skill.tools | skill ∈ loaded}
       tool_result = body
    sonst wenn name in provider_tools.names():
       ... wie bisher (HITL für write, etc.)
    sonst:
       tool_result = "Tool nicht verfügbar — lies erst den passenden Skill"
  history.append(tool_results)
```

Dafür neu in `tools.rs`:
- `pub fn base_tool_schemas() -> Vec<ToolSpec>` — `read_skill` + `ask_user`.
- `pub fn schemas_for_skills(registry, skill_names) -> Vec<ToolSpec>` —
  alle Tool-Schemas der gelisteten Skills, dedupliziert.
- `available_tools(skills)`-Funktion in der jetzigen Form wird durch
  diese zwei ersetzt; `chat.rs` ruft sie pro Provider-Call neu auf.

**HITL `perTool`-Override:**
- Heute kennt `is_write_tool` nur globale Tool-Klassen. Mit Skill-
  Frontmatter `hitl.perTool` kann ein Skill-Author sagen
  „rewrite_file für diesen Workflow ohne HITL". Neue Funktion
  `effective_hitl(skill_registry, loaded_skills, tool_name) -> bool`:
  startet bei `is_write_tool(tool_name)`, läuft dann durch alle
  geladenen Skills, schaut nach `hitl.perTool[tool_name]`. Bei
  Override `false` → kein HITL. Mehrere Skills setzen Konflikt →
  „strenger gewinnt" (HITL bleibt an, sicher ist sicher).
- In `chat.rs:635` ersetzen `is_write_tool(call.name)` durch
  `effective_hitl(&st.skills, &loaded, &call.name)`.

**Caching-Hinweis** (Anthropic):
- Tools-Block bekommt `cache_control: ephemeral` am letzten Element.
  Bei gleichem Skill-Load-Stand identischer Block → Cache-Hit. Bei
  Wachsen invalidiert. Akzeptabel.
- System-Prompt-Block ebenfalls cacheable, wobei der Workspace-
  Summary-Teil pro Workspace bei jedem Send neu kommt — der wird
  bewusst **nach** dem statischen Skill-Listing positioniert, damit
  zumindest die Skill-Liste cacheable bleibt.

**Tests (`backend/tests/integration.rs`):**
- `prompt_lists_skills_with_tool_names_not_schemas`: composer-Output
  enthält für jeden Skill die Tool-Namen, aber **nicht** die
  Tool-Beschreibung (Heuristik: keine Substrings aus den ToolSpec-
  `description`s).
- `read_skill_returns_body_and_unlocks_tools`: zwei-Step-Test, der
  den Tool-Loop direkt fährt — erst `read_skill("folder-search")`,
  dann Provider-Call mit dem erweiterten Tools-Block. Wir prüfen
  die zurückgegebenen Tool-Schemas im zweiten Schritt (über eine
  testbare `next_provider_tools(loaded, registry)`-Funktion, nicht
  über echten Provider-Call).
- `tool_call_for_not_loaded_skill_is_rejected_gracefully`: das LLM
  „versucht" `grep_in_files` ohne `folder-search` geladen zu haben.
  Tool-Result ist die freundliche „lies erst den Skill"-Meldung,
  kein 500.
- `effective_hitl_respects_per_tool_override`: ein SKILL.md mit
  `hitl.perTool: { rewrite_file: false }` ist geladen →
  `effective_hitl(…, "rewrite_file")` = false. Ohne Skill geladen →
  Fallback auf `is_write_tool` = true.
- `system_prompt_includes_workspace_summary`: nach Seedten dreier
  Dateien enthält der Composer-Output ihre Namen.
- `workspace_summary_truncates_at_30`: bei 50 Files → erste 30 +
  „… 20 weitere".

DB-freie Unit-Tests in `prompt.rs` ergänzen für die rein
funktionalen Teile (Composer-Layout, Skill-Listing-Format), damit
sie ohne Postgres laufen.

**Doku-Patches:**
- `CLAUDE.md` §1a-Backend-Bullet: Skill-Registry + Progressive
  Disclosure (read_skill).
- `CLAUDE.md` §4 (Mehrbenutzer-Modell) ist unverändert; aber §10
  (LLM-Provider-Strategie) bekommt einen Satz zur Cache-Wirkung
  der dynamischen Tools-Liste.
- Neuer Abschnitt §6a „Skill-System" im CLAUDE.md mit kurzem
  Architekturbild (Composer → System-Prompt + Tools-Block;
  read_skill als Disclosure-Tor).
- `DEPLOY.md` §8: Hinweis dass das LLM jetzt zwei Schritte braucht
  (Skill lesen → Tool nutzen).

**Gates:** wie üblich.

**Abnahme:**
- Frischer Run: nur `read_skill` + `ask_user` an Anthropic
  deklariert; System-Prompt enthält die Skill-Liste.
- Nach `read_skill("folder-search")`: der Body kommt als
  Tool-Result; im nächsten Provider-Call sind zusätzlich
  `list_files`/`read_file`/`grep_in_files` deklariert.
- Bestehende Use-Cases („User stellt einfache Frage zu einer Datei")
  laufen mit einem zusätzlichen `read_skill`-Step, aber stabil.
- Tool-Choice-Verwirrung sichtbar reduziert in einem A/B-Vergleich
  mit dem Pre-6c-Verhalten — qualitatives Abnahmekriterium, kein
  automatisierter Test (würde Provider-Call-Vergleich brauchen).

---

### Phase 6c-4 (optional, kein v1-Muss) — User-Skills pro Org

**Ziel:** Org-Owner können eigene `SKILL.md` hochladen, die nur
innerhalb der Org sichtbar sind. Built-ins bleiben global.

**Datenmodell:** Storage-Pfad `STORAGE_DIR/orgs/<org_id>/skills/
<skill_name>/SKILL.md`. Keine eigene DB-Tabelle — die Markdown-Datei
ist die Wahrheit, gleiches Muster wie Workspace-Files.

**Routes:**
- `POST /api/v1/orgs/{id}/skills` (Multipart, Owner-Only) —
  Upload + Parse + Konflikt-Check (kein Built-in-Name, kein
  duplizierter User-Name in derselben Org).
- `DELETE /api/v1/orgs/{id}/skills/{name}` (Owner-Only).
- `GET /api/v1/skills` erweitert: liefert built-ins ∪ user-skills
  der eigenen Org.

**Registry-Lookup:** `effective_registry(org_id) ->
SkillRegistry`-Funktion baut on-demand die Vereinigung (built-ins
sind Arc-cached; User-Skills werden bei jedem Lookup neu aus dem
Volume gelesen — kein Cache-Invalidation-Problem für v1).

**Sandbox:** User-Skill-Body ist **Markdown** — kein
Code-Ausführungs-Vektor. Trotzdem `tools`-Liste eines User-Skills
darf nur **bereits existierende Tool-Namen** referenzieren; ein
hochgeladener Skill kann keine neuen Tools deklarieren. Validierung
beim Upload.

**Tests:** Upload-Roundtrip, Cross-Org-No-Leak, Built-in-Override-
Schutz, Validation gegen unbekannte Tool-Namen.

**Abnahme:** Org-A-Owner lädt einen Skill hoch; Org-A-Member sieht
ihn im Skill-Picker; Org-B-Member sieht ihn nicht.

---

### Reihenfolge & Abhängigkeiten

- 6c-1 ist eigenständig (reine Library + Tests).
- 6c-2 baut auf 6c-1 auf (nutzt die Registry).
- 6c-3 baut auf 6c-1 + 6c-2 auf (Composer + Tool-Loop).
- 6c-4 baut auf 6c-1/-2/-3 auf, ist aber bewusst nach v1 verschoben.

Jede Sub-Phase ist eigenständig deploybar — das Backend bleibt
zwischen den Etappen funktionsfähig.

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
