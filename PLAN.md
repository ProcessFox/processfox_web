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

## Phase 6b-2d — Rest (offen, bewusst verschoben)

- `writeDocxFromTemplate` (Platzhalter-Ersetzung in einer Vorlage,
  nutzt Agent-Attachment `templateFileId`), `updateCells`/`appendToDocx`,
  Delegation/Bulk-Worker (`delegateIntoXlsxColumn`).

**Abnahme:** 6a Streaming live für alle · 6b-1 Datei-Tools+HITL ·
6b-2a Rückfragen · 6b-2b Excel-Schreiben · 6b-2c Word-Schreiben (jeweils
HITL-Vorschau, live für alle Mitglieder).

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
