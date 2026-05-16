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
3. **Shared-Session-Nebenläufigkeit** (Phase 6): zwei Editoren senden
   gleichzeitig an denselben Agenten. → Pro Agent max. ein aktiver Run;
   zweiter Send wird abgelehnt/gequeued; Run-State per WS an alle Mitglieder.
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
