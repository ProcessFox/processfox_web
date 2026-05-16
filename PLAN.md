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

- Tabellen `organizations` (inkl. `invite_code`), `users`, `refresh_tokens`.
  Argon2-Passwort-Hash.
- `POST /api/v1/auth/register` (**immer mit** 6-stelligem Org-Code),
  `/auth/login`, `/auth/refresh`, `/auth/logout`,
  `POST /api/v1/orgs/:id/rotate-invite-code` (nur Owner).
  **Kein** Org-Erstellungs-Endpunkt — erste Org + Owner manuell in DB.
- Access-Token 15 min (Bearer), Refresh-Token 7 Tage (httpOnly-Cookie,
  serverseitig gehasht + widerrufbar). Rate-Limit auf register/login.
- JWT-Middleware extrahiert `user_id`.
- Frontend: `src/hooks/useAuth.ts`, `src/views/Login.tsx` (zwei Modi:
  Login / Registrieren-mit-Org-Code), Token-Refresh-Logik,
  Bridge injiziert `Authorization`-Header + 401→Refresh→Retry.

**Abnahme:** Auth-Tests (Login, Refresh, abgelaufener Token, Registrierung
mit gültigem/ungültigem Code); Login-View funktioniert gegen Backend.

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
