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
| Reihenfolge | Erst Plan, dann phasenweise Umsetzung mit Abnahme pro Phase. |

## Ausgangslage (Kurz)

Nur Frontend vorhanden (1:1-Port aus `processfox_local`). Kein Backend, kein
Auth, kein Workspace-Konzept. Bridge `src/lib/tauri.ts` ruft RPC-Endpunkte ins
Leere. Local-Paradigma (Modelle/Hardware/Ordner) noch durchgängig.

---

## Phase 0 — Frontend-Bereinigung (Local-Paradigma raus)

Ziel: Frontend von Local-Annahmen befreien, bevor Backend gebaut wird.

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

## Phase 1 — Backend-Skeleton (Axum)

`backend/`-Crate gemäß CLAUDE.md §6.

- `Cargo.toml`, `main.rs`, `lib.rs`, `config.rs` (Env-Vars §12).
- sqlx-Pool (Postgres), `db/migrations/` mit Schema §8.
- S3-Client (`aws-sdk-s3` oder `object_store`), `storage/`.
- Axum-Router `/api/v1/*`, Fehler-Envelope `{code,message,details?}` (§7).
- Static-File-Serving (`STATIC_DIR`) + SPA-Fallback.
- Healthcheck `GET /api/v1/health`.

**Abnahme:** `cargo fmt`/`clippy -D warnings`/`test` grün; Server startet,
Migrations laufen, `/api/v1/health` antwortet.

---

## Phase 2 — Auth (JWT)

- Tabellen `organizations`, `users`. Argon2-Passwort-Hash.
- `POST /api/v1/auth/register`, `/auth/login`, `/auth/refresh`, `/auth/logout`.
- Access-Token 15 min (Bearer), Refresh-Token 7 Tage (httpOnly-Cookie).
- JWT-Middleware extrahiert `user_id`.
- Frontend: `src/hooks/useAuth.ts`, `src/views/Login.tsx`, Token-Refresh-Logik,
  Bridge injiziert `Authorization`-Header + 401→Refresh→Retry.

**Abnahme:** Auth-Tests (Login, Refresh, abgelaufener Token); Login-View
funktioniert gegen Backend.

---

## Phase 3 — Workspaces & Mitglieder

- Tabellen `workspaces`, `workspace_members`.
- Endpunkte: Workspace CRUD, Member-Invite/-Rollen.
- Middleware: `require_workspace_member`, `require_editor`.
- Frontend: `components/workspace/` (WorkspaceSwitcher, MemberList, InviteDialog).

**Abnahme:** Berechtigungs-Matrix (CLAUDE.md §4) testabgedeckt; Nicht-Mitglied
bekommt 403.

---

## Phase 4 — Agents & Settings (DB-backed)

- Tabellen `agents` (workspace-scoped), `org_settings`, `api_keys`
  (AES-256-GCM, Key aus `API_KEY_ENCRYPTION_KEY`).
- REST-CRUD für Agents; Settings-Endpunkte org-scoped.
- API-Keys nie ans Frontend (nur `hasKey`-Bool / Validierung).

**Abnahme:** Agent-CRUD über REST; Key verschlüsselt in DB, nicht im Klartext.

---

## Phase 5 — Dateien (Upload statt OS-Ordner)

- Tabelle `workspace_files`. `POST /workspaces/:id/files` multipart
  (≤ 50 MB, Typ-Whitelist §5). S3-Key `workspaces/<id>/<filename>`.
- `sandbox.rs::ensure_in_workspace` (§9). Presigned Download-URLs (15 min).
- Preview-Endpunkte laden serverseitig aus S3 → bestehendes Preview-JSON.
- Frontend: Drag&Drop → Upload, FileTree aus `workspace_files`,
  Preview via Presign/Preview-Endpunkt. WS-Broadcast `fs-changed`.

**Abnahme:** Upload→Liste→Preview→Download e2e; Path-Traversal abgewehrt.

---

## Phase 6 — Chat & Realtime

- ReAct-Loop + ChatRepo aus `processfox_local/core` portiert (Postgres statt JSON).
- LLM-Provider Anthropic/OpenAI/OpenRouter (§10), Key-Injection serverseitig.
- WS-Hub: `GET /ws?token=<access>`; Channels `chat:run:<runId>`,
  `fs-changed`, `agent-attachments-changed`; Broadcast nur an Workspace-Mitglieder.
- HITL-Endpunkte (approve/reject/respond) + WS-Events.

**Abnahme:** Chat mit Streaming + HITL über WS; mehrere Mitglieder sehen
denselben Verlauf live.

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

## Offene Punkte / vor Phasenstart zu klären

1. **Migrationsstrategie Bestands-User Local → Web?** (vermutlich keine — Greenfield).
2. **Org-Self-Service vs. invite-only** für Registrierung in v1.
3. **MinIO vs. AWS S3** als Default-Storage für die erste Deployment-Stufe.
4. **`@anthropic-ai/sdk` / Provider-SDK-Versionen** und Prompt-Caching im Backend.
5. Reihenfolge: Phase 0 zuerst (empfohlen) oder Backend-Skeleton parallel?
