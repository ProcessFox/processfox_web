# CLAUDE.md — Arbeits-Anweisungen für Claude Code (ProcessFox Web)

Dieses Dokument richtet sich an Claude Code (und andere LLM-gestützte Codier-Assistenten), die an **ProcessFox Web** mitarbeiten. Es ist das Pendant zu `CLAUDE.md` aus dem `processfox_local`-Repo — adaptiert für eine Browser-basierte, mehrbenutzer-fähige Team-Anwendung.

**Pflicht-Lektüre vor jedem größeren Task:**
- `CONCEPT.md` im `processfox_local`-Repo — die gemeinsame Produkt-Vision (Taxonomie, UI-Modell, HITL-Konzept, Skill-Inventar)
- Dieses Dokument — Web-spezifische Architektur, Tech-Stack und Abweichungen vom Local-Repo

---

## 1. Projekt-Kurzprofil

- **Produkt:** ProcessFox Web — Browser-basierte, team-fähige KI-Agenten-Plattform für kollaborative Dokumentenarbeit.
- **Domain:** Wird unter der Subdomain **`chat.processfox.ai`** ausgeliefert.
- **Produkt-Familie (gemeinsames Design, unterschiedliche Architektur):**
  - **`www.processfox.ai`** — reguläre Marketing-/Produkt-Webseite (eigenes Repo).
  - **ProcessFox Local** — lokal installierbare Desktop-App (Tauri, `processfox_local`-Repo).
  - **ProcessFox Web** — dieses Repo, `chat.processfox.ai`.
  - Alle drei teilen sich Design-Sprache, Farb-/Typo-System und UI-Komponenten-Look. Änderungen am gemeinsamen visuellen Erscheinungsbild müssen mit den anderen Flächen konsistent bleiben.
- **Zielgruppe:** Kleine Teams und NGOs, die gemeinsam remote an Dateien arbeiten. Einzel-Anwender ohne lokale Modell-Infrastruktur.
- **Deployment:** Self-hosted via [Coolify](https://coolify.io/) (Docker), alternativ als Managed-Service.
- **Kernunterschied zu ProcessFox Local:**
  - Mehrbenutzer statt Einzelperson
  - Datei-Upload statt lokaler Ordnerzugriff
  - Cloud-LLMs als primärer Pfad — kein lokales GGUF
  - Browser-App statt Desktop-App

---

## 1a. Ist-Stand (Stand: 2026-05-20 — Phase 6 + Read-Tools + `rewrite_file` + Skill-Registry 6c-1/-2/-3 + Rollen-Modell vereinfacht 6e)

> **Wichtig:** Der Rest dieses Dokuments (§2–§16) beschreibt Architektur &
> Konventionen. Dieser Abschnitt beschreibt den **realen Umsetzungsstand**.
> Bei jedem Task zuerst `PLAN.md` (phasenweises Logbuch) konsultieren.

**Realisiert (live deployt auf `chat.processfox.ai` via GHCR/Coolify):**

- **Frontend** (React/Vite/TS) — vom Local- aufs Web-Paradigma umgebaut:
  Local-Modell-/Hardware-/OS-Ordner-Code entfernt, Auth-Schicht
  (`useAuth`, `Login.tsx`, `auth.ts`, Magic-Link-Callback), Workspace-/
  Org-/Rollen-Konzept, `tauri.ts` durchgängig REST `/api/v1` + **eine**
  multiplexte WS. Portierte UI (HITL, AskUser, Tool-/Reasoning-Chips,
  Preview-Viewer, Delegation-Fortschritt) unverändert wiederverwendet.
- **Backend** (Rust/Axum, `backend/`) — vollständig: Auth (Magic-Link,
  JWT, rotierende Refresh-Tokens), Org/Workspace/Member, Agenten,
  Org-Settings + AES-256-GCM-verschlüsselte API-Keys, Datei-Upload/
  Vorschau (lokales Volume), Streaming-Chat (shared session), Tool-
  Loop + HITL, Rückfragen (`ask_user`), alle Datei-Schreiboperationen
  (Excel/Word/aus Vorlage/Anhängen/Zell-Edits) und Bulk-Delegation —
  jeweils live für alle Workspace-Mitglieder über die WS. Plus
  `grep_in_files` (read-only Regex-Suche über die Workspace-Textdateien,
  Caps 300 Dateien/2 MiB/100 Hits, Endungs-Whitelist), `read_pdf`
  (Text-Extraktion aus PDFs via `pdf-extract` auf dem Blocking-Pool,
  Caps 20 MB Eingabe / 200 KB Ausgabe), `read_docx` (Lauftext aus
  Word-Dateien via `quick_xml` auf dem Blocking-Pool, gleiche Caps;
  teilt den Extraktor mit der HITL-Tail-Vorschau bei `append_to_docx`)
  `read_xlsx_range` (rechteckiger Bereichs-Read aus Excel via
  `calamine`, **JSON-Output** `{file, sheet, range, headers, rows}` mit
  erster Range-Zeile als `headers`; Cap 500 Zellen pro Aufruf,
  Default-Fenster 25×12 ab `A1`) und `rewrite_file` (Komplett-
  Überschreiben einer Text-Datei mit HITL-Diff-Vorschau via `diffLines`
  im Frontend; Endungen `.md`/`.markdown`/`.txt`/`.text`/`.csv`,
  5-MiB-Bestands-Cap).
- **Skill-Registry + Progressive Tool-Disclosure (Phase 6c-1/-2/-3):**
  `backend/skills_builtin/` enthält fünf `SKILL.md`-Bündel
  (`folder-search`, `document-read`, `document-write`, `table-read`,
  `table-write`); beim App-Start in eine `SkillRegistry` eingelesen,
  via `GET /api/v1/skills` serviert. Migration `0004` migriert Agents
  vom Legacy-Slot `["files"]` auf das neue 5er-Set. **Tool-Loop läuft
  jetzt mit Progressive Disclosure** (`backend/src/prompt.rs` +
  `tools::tools_for_step`): initial sind nur `read_skill` und
  `ask_user` an den Provider deklariert; ein erfolgreicher
  `read_skill({skillId})`-Aufruf liefert den Body als Tool-Result und
  schaltet ab der nächsten Iteration die Tool-Schemas dieses Skills
  frei. System-Prompt-Composer rendert pro Iteration neu: Datum,
  Agent-Vorgabe, Workspace-Übersicht (top 30 aus `workspace_files`),
  Available-Skills-Block (nur Titel + Description + Tool-Namen, **kein**
  Body), Thoroughness-Policy, Sprach-Direktive. `effective_hitl(loaded,
  tool_name)` ersetzt das alte `is_write_tool` — Skill-`perTool`-
  Override kann HITL **nur strenger machen**, nie abschalten.
- **Strukturierte Chat-History (Phase 6d-1, 2026-05-20):**
  `chat_messages.content` (JSONB) trägt jetzt strukturierte Inhalte —
  assistant-Rows `{ text, reasoning?, toolCalls? }`, neue tool-Rows
  `{ toolResults }`, user-Rows bleiben Plain-String (kein Schema-
  Change, Backward-Compat: alte Bestandsdaten werden weiter als reine
  Text-Bubbles dekodiert). Tool-Loop persistiert pro Iteration eine
  intermediäre Assistant-Row mit ihren Tool-Calls plus eine Tool-Row
  mit den Results; Mitglieder, die den Agenten erst **nach** Run-Ende
  öffnen, sehen damit Tool-Chips und Resultate im Verlauf — vorher gab
  es nur die finale Text-Bubble.
- **Rollen-Modell vereinfacht (Phase 6e, 2026-05-20):** Die zweite
  Rollen-Ebene `workspace_members.role` (editor|viewer) wurde abgeschafft.
  Frontend und Backend kennen nur noch **Admin** (`users.org_role =
  'owner'`) vs. **Nutzer** (`users.org_role = 'member'`). Migration
  `0006_workspace_member_role_removed` droppt die Spalte (Bestandsdaten
  behalten ihre Mitgliedschaft, niemand verliert Zugriff). `require_editor`
  ist aus `perm.rs` entfernt — alle Routen nutzen jetzt `require_member`,
  weil jedes Workspace-Mitglied uneingeschränkt arbeiten darf. Admin-only
  bleiben: `POST/PATCH/DELETE /workspaces`, `POST/DELETE
  /workspaces/{id}/members/*`, alle `/secrets/*`-Schreibrouten, alle
  `/settings`-Schreibrouten **und `DELETE /agents/{id}`** — durchgängig
  per `require_org_owner` gegated. Frontend (`WorkspaceSwitcher`,
  `WorkspaceMembersDialog`, `SettingsDialog`, `AgentEditorDialog`)
  versteckt Admin-Aktionen für Nutzer; der Cloud-APIs-Tab ist für Nutzer
  unsichtbar, der Trash-Button im Agent-Editor erscheint nur für Admins
  beim Bearbeiten eines bestehenden Agenten. Neue UI-Aktionen:
  **Workspace umbenennen** im Switcher-Dropdown und **Agent löschen** im
  Editor-Footer — Routen gab's schon, Frontend-Trigger fehlten.
- **Reasoning-Stream / Extended Thinking (Phase 6d-2, 2026-05-20):**
  Per-Agent-Toggle `agents.reasoning_enabled` (Migration 0005, Default
  aus). Aktiviert schaltet `llm.rs` den Provider-spezifischen
  Reasoning-Pfad zu: **Anthropic** bekommt `thinking: { type:
  "enabled", budget_tokens: 4000 }` und liefert das CoT via
  `thinking_delta`-SSE bzw. `content[type=thinking]`-Blöcken;
  **OpenAI/OpenRouter** bekommt `reasoning: { effort: "medium" }`
  **und** das Modell muss ein Pattern aus `MODELS_WITH_REASONING`
  matchen (`o1*`/`o3*`/`o4*`, DeepSeek-R1, Qwen-QwQ, `*-thinking`,
  `grok-*-reasoning`) — sonst wird das Feld weggelassen (manche
  OR-Routen lehnen unbekannte Keys mit 400 ab). Der Stream broadcastet
  `reasoningDelta`-WS-Events live; Persistenz erfolgt im
  `assistant.content.reasoning`-Feld dank Phase 6d-1. Anthropic-Body
  wird für Claude-3-* defensiv gar nicht gesendet (lehnt es ab).
- **CI/Deploy:** Eine konsolidierte GitHub-Actions-Pipeline
  (`.github/workflows/ci.yml`, Stand 2026-05-20): Job `backend` läuft auf
  jedem PR und auf `push:main` (fmt + clippy + Tests gegen Postgres-Service);
  Job `image` läuft nur auf `push:main` **nach grünem `backend`-Job**
  (`needs: backend`), baut das Multi-Stage-Image → GHCR und pingt
  optional den Coolify-Webhook. Frühere Aufteilung in zwei Workflows
  (`ci.yml` + `docker.yml`, parallel und ungetrennt) hatte den Defekt,
  dass kaputte Tests den Image-Build nicht blockierten — jetzt gegated.
  Coolify zieht das Image (Docker-Image-Resource, kein VPS-Build),
  Postgres + lokales Persistent Volume `/data`, Domain in Coolify.

**Bewusst offen (klein/optional):** `delegationProfile`-Override (eigenes
Worker-Modell je Agent), Vorlage via Agent-Attachment-`templateFileId`.

**Härtung umgesetzt:** HTTP/DB-Integrationstests (`backend/tests/
integration.rs`) — echte Axum-Handler via `tower::oneshot` gegen eine pro
Test frische Postgres-DB (`#[sqlx::test]`). Deckt Magic-Link-`verify`
(inkl. single-use/expired), Refresh-Token-Rotation, die Admin/Nutzer-
Berechtigungen (Workspace-Mgmt, Mitglieder, Agent-Delete) und
Cross-Org-No-Leak ab. Läuft im konsolidierten CI/Deploy-Workflow
(`.github/workflows/ci.yml` → Job `backend`, Postgres-Service) zusammen
mit fmt+clippy; lokal mit erreichbarer `DATABASE_URL`. Der `image`-Job
hängt per `needs:` an `backend`, sodass kein Image nach GHCR landet,
ohne dass die Tests grün waren.

**Bekannte funktionale Grenzen:** siehe `DEPLOY.md` §8 (Word-Platzhalter
müssen run-zusammenhängend sein; Zell-Edits/Delegation schreiben das
Zielblatt neu → Formeln/Formate/weitere Blätter gehen verloren;
Delegation max. 200 Zeilen/Lauf).

---

## 2. Tech-Stack

| Bereich | Technologie |
|---|---|
| Frontend | React 19 + Vite + TypeScript + Tailwind CSS + shadcn/ui |
| API-Bridge | `src/lib/tauri.ts` → REST `/api/v1/*` + **eine** multiplexte WebSocket `/api/v1/ws` |
| Backend | Rust + **Axum** 0.8 |
| Realtime | Axum WebSocket (eine multiplexte Verbindung pro Client, `tokio::broadcast`-Hub) |
| Datenbank | **PostgreSQL** via `sqlx` (async, **Runtime-Queries** — bewusst **keine** `query!`-Makros, damit der Docker-Build ohne DB-Verbindung läuft) |
| Datei-Storage | **Lokales Persistent Volume** (Coolify, Single-Instance) — Pfad `STORAGE_DIR`. *(Geänderte Entscheidung 2026-05-19: ursprünglich S3/MinIO; wegen Self-Hosted-Single-Instance-Maßstab + Betriebskomplexität auf lokales Volume umgestellt.)* |
| Auth | **Passwordless Magic-Link** (E-Mail-only) → JWT-Access + rotierende Refresh-Tokens; Versand via n8n-Webhook |
| LLM-Provider | Anthropic (inkl. Prompt-Caching), OpenAI, OpenRouter — kein lokales GGUF in v1 |
| Deployment | GitHub Actions → GHCR-Image; Coolify zieht das Image (kein VPS-Build) |
| CI/CD | GitHub Actions |

---

## 3. Goldene Regeln

1. **Team zuerst, Einzelperson zweiter.** Jede Entscheidung muss mit mehreren parallelen Nutzern im Hintergrundmodell funktionieren. Geteilter State ist die Norm, nicht die Ausnahme.
2. **Cloud-LLM ist Default.** Kein lokales Modell, keine GGUF-Infrastruktur, kein Hardware-Check. Modell-Auswahl = Provider + Modell-String.
3. **Agent-Workspace statt lokaler Ordner.** Der „Agent-Ordner" ist ein logischer Workspace auf dem lokalen Persistent Volume (`STORAGE_DIR`). Datei-Upload und -Download ersetzen den OS-Dateibaum.
4. **Berechtigungen sind workspace-scoped.** Jede Backend-Operation prüft, ob der aufrufende User Mitglied des Workspaces ist. Kein Verlass auf Frontend-Filterung.
5. **HITL ist Default für Schreibaktionen.** Wie in Local: Freigabe vor jeder destruktiven oder schreibenden Datei-Aktion.
6. **WebSocket für Echtzeit.** Keine Polling-Lösung. Run-Events, HITL-Anfragen und FS-Änderungen laufen über **eine** persistente, multiplexte WS-Verbindung pro Client (channel-basiert, workspace-scoped).
7. **Kein User-Script in v1.** Gleiche Regel wie in Local — Sandbox-Infrastruktur kann vorbereitet, aber nicht für Endnutzer geöffnet werden.
8. **Kein Python im Backend.** Alles in Rust.

---

## 4. Mehrbenutzer-Modell

### Konzepte

| Begriff | Bedeutung |
|---|---|
| **Organisation** | Oberste Einheit. Eine Org hat mehrere Workspaces und mehrere User. |
| **Workspace** | Entspricht dem „Agent-Ordner" in Local. Hat einen Namen, Mitglieder und eine Datei-Liste. |
| **Agent** | Gehört zu einem Workspace. Alle Workspace-Mitglieder können ihn nutzen. |
| **User** | Hat genau eine Org-Rolle: `owner` (= **Admin**) oder `member` (= **Nutzer**). Workspace-Mitgliedschaft ist binär (drin / nicht drin) — siehe Migration 0006. |
| **Session** | Eine Chat-Session pro Agent, shared. Alle Mitglieder sehen den gleichen Chat-Verlauf live. |

### Berechtigungs-Matrix (Stand Migration 0006)

| Aktion | Admin (`org_role = 'owner'`) | Nutzer (`org_role = 'member'`) |
|---|---|---|
| Workspace anlegen / umbenennen / löschen | ✓ | — |
| Mitglieder einladen / entfernen | ✓ | — |
| Org-Settings (Default-Provider/Modell) | ✓ | — |
| API-Keys hinterlegen / validieren / löschen | ✓ | — |
| Agent **löschen** | ✓ | — |
| Agent anlegen / konfigurieren / Attachment setzen | ✓ | ✓ |
| Chat senden, HITL freigeben, `ask_user` beantworten | ✓ | ✓ |
| Dateien hoch-/herunterladen, löschen, editieren | ✓ | ✓ |
| Workspace-Liste / Mitglieder-Liste lesen | ✓ | ✓ (nur eigene Workspaces) |

Admin-Status wird ausschließlich an `users.org_role = 'owner'` erkannt.
Backend nutzt `require_org_owner(&user)` für Admin-Routen und
`require_member(state, &user, ws_id)` als Workspace-Sandbox-Check. Die
frühere Helper-Funktion `require_editor` ist entfernt.

### User-Identität

- **Passwordless / Magic-Link** (geänderte Entscheidung, Phase 2): Login &
  Registrierung nur über E-Mail. Das Backend erzeugt ein einmaliges,
  15-min-gültiges Token (nur als SHA-256-Hash gespeichert) und POSTet den
  Magic-Link an einen n8n-Webhook (`MAGIC_LINK_WEBHOOK_URL`), der die Mail
  versendet. **Kein Passwort, kein Argon2.** Registrierung erfordert
  zusätzlich den 6-stelligen Org-Invite-Code.
- JWT (15 min Access-Token, in-memory im Frontend) + Refresh-Token (7 Tage,
  httpOnly-Cookie, serverseitig gehasht + rotierend/widerrufbar).
- Kein OAuth/SSO in v1 — spätere Erweiterung.

---

## 5. Datei-Modell (Upload statt lokaler Ordner)

### Upload-Flow

1. **Frontend:** Drag & Drop oder Datei-Picker → `multipart/form-data` an `POST /api/v1/workspaces/{id}/files`.
2. **Backend:** Validierung (Dateityp, Größe ≤ 50 MB), Schreiben ins lokale Volume unter `STORAGE_DIR/workspaces/<workspace_id>/<filename>`.
3. **Datenbank:** Eintrag in `workspace_files`-Tabelle (workspace_id, filename, s3_key = Storage-Key, size, uploaded_by, uploaded_at). *(Spaltenname `s3_key` historisch beibehalten; enthält den relativen Storage-Pfad.)*
4. **Frontend:** WS-Broadcast an alle Workspace-Mitglieder → Datei-Baum aktualisiert sich live.

### Download / Vorschau

- Download nie als direkter Datei-Pfad. Das Backend gibt einen **kurzlebig signierten Link** aus (`GET /files/{id}/raw?token=…`, HMAC/JWT, 15 min) — funktioniert ohne Auth-Header als `<img>`/PDF-Quelle, liefert die Bytes mit korrektem Content-Type.
- Vorschau-Endpunkte (`/api/v1/files/{id}/preview/{docx|xlsx|pptx}`) lesen die Datei serverseitig vom Volume und liefern das bekannte Preview-JSON — gleiche Datenstrukturen wie in Local.

### Erlaubte Dateitypen

`.md`, `.txt`, `.pdf`, `.docx`, `.xlsx`, `.csv`, `.png`, `.jpg`, `.jpeg`, `.webp`, `.pptx`

### Sandbox-Prinzip

Jeder Storage-Key wird serverseitig gegen das Schema `workspaces/<workspace_id>/` validiert (`crate::sandbox::ensure_in_workspace`), Dateinamen werden saniert. Path-Traversal ist auf API-Ebene ausgeschlossen.

---

## 6. Verzeichnis-Layout (Ist-Stand)

```
processfox_web/
├── CLAUDE.md                    # dieses Dokument
├── index.html
├── package.json
├── vite.config.ts               # Proxy: /api + /ws → Backend-Port
├── tailwind.config.js
├── postcss.config.js
├── tsconfig.json
├── tsconfig.node.json
├── components.json              # shadcn/ui-Konfig
├── public/
│   └── vite.svg
├── src/                         # Frontend (React + TS)
│   ├── main.tsx
│   ├── App.tsx
│   ├── globals.css
│   ├── vite-env.d.ts
│   ├── lib/
│   │   ├── tauri.ts             # API-Bridge: fetch(/api/*) + WebSocket(/ws/*)
│   │   ├── utils.ts
│   │   ├── fileIcons.ts
│   │   ├── toolIcons.ts
│   │   └── starterPrompts.ts
│   ├── types/                   # geteilte TypeScript-Typen
│   │   ├── agent.ts
│   │   ├── chat.ts
│   │   ├── file.ts
│   │   ├── message.ts
│   │   ├── models.ts
│   │   ├── settings.ts
│   │   ├── skill.ts
│   │   ├── error.ts
│   │   └── auth.ts              # NEU: User, Org, Workspace, Rolle
│   ├── hooks/
│   │   ├── useAgentChat.ts
│   │   └── useAuth.ts           # NEU: Login-State, Token-Refresh
│   ├── views/
│   │   ├── Main.tsx
│   │   ├── Settings.tsx
│   │   ├── Welcome.tsx
│   │   └── Login.tsx            # NEU
│   └── components/
│       ├── agent/
│       ├── chat/
│       ├── filetree/
│       ├── preview/
│       ├── settings/
│       ├── theme-provider.tsx
│       ├── ui/                  # shadcn-Bausteine
│       └── workspace/           # NEU: WorkspaceSwitcher, MemberList, InviteDialog
└── backend/                     # Rust-Backend (Axum) — flaches Modul-Layout
    ├── Cargo.toml
    ├── skills_builtin/          # eingebaute SKILL.md-Dateien (Phase 6c-2)
    │   ├── folder-search/SKILL.md
    │   ├── document-read/SKILL.md
    │   ├── document-write/SKILL.md
    │   ├── table-read/SKILL.md
    │   └── table-write/SKILL.md
    └── src/
        ├── main.rs              # Bootstrap, AppState, axum::serve
        ├── lib.rs               # build_app(), AppState-Struct, Modul-Mounts
        ├── config.rs            # Env-Vars (DATABASE_URL, STORAGE_DIR, SKILLS_DIR, ...)
        ├── error.rs             # ApiError (thiserror) + IntoResponse
        ├── db/
        │   ├── migrations/      # 0001_init, 0002_magic_link, 0003_*, 0004_skills_resharded
        │   └── mod.rs           # connect() + migrate!
        ├── auth/                # mod, jwt, token, extractor (JWT-Middleware)
        ├── routes/              # ein Modul pro Feature
        │   ├── mod.rs  auth.rs  workspaces.rs  agents.rs
        │   ├── settings.rs  secrets.rs  files.rs  chat.rs
        ├── ws.rs                # WsHub (broadcast), ws_handler, pump
        ├── perm.rs              # require_member/_editor/_org_owner
        ├── sandbox.rs           # ensure_in_workspace / sanitize_filename
        ├── skills.rs            # SKILL.md-Parser + SkillRegistry (Phase 6c-1/-2)
        ├── prompt.rs            # System-Prompt-Composer (Phase 6c-3)
        ├── storage.rs           # lokales Volume (STORAGE_DIR), Pfad-Mapping
        ├── crypto.rs            # AES-256-GCM für API-Keys
        ├── ratelimit.rs         # IP-Rate-Limit für Auth
        ├── preview.rs           # docx/xlsx/pptx-Preview-JSON
        ├── llm.rs               # stream_chat + tool_step (Anthropic/OpenAI/OR)
        └── tools.rs             # Skill-/Tool-Registry, Datei-Schreib-Tools
```

> Hinweis: Die in §9 gezeigten Pfade `backend/src/core/...` sind historisch
> — das reale Layout ist flach (`backend/src/sandbox.rs` etc.).

---

## 7. API-Konventionen

> **✅ Stand 2026-05-19 (Phase 6a):** Die ehemalige RPC-Diskrepanz ist
> aufgelöst — die Bridge spricht durchgängig **REST `/api/v1/...`** plus
> **eine** multiplexte WebSocket-Verbindung. Kein `POST /api/<command>`
> mehr.

### HTTP-Endpunkte

- Alle Endpunkte unter `/api/v1/`.
- Authentifizierung via `Authorization: Bearer <access_token>`.
- Fehler-Antworten: `{ "code": "string", "message": "string", "details"?: any }`.
- Erfolgreiche Antworten: direkt das Objekt oder `{ "data": [...] }` bei Listen.
- HTTP-Status-Codes: 200 OK, 201 Created, 400 Bad Request, 401 Unauthorized, 403 Forbidden, 404 Not Found, 409 Conflict, 500 Internal Server Error.

### WebSocket-Protokoll

- Verbindung: **eine** multiplexte WS pro Client — `GET /api/v1/ws?token=<access_token>` (Token im Query-String, weil die WS-Browser-API keinen Auth-Header unterstützt).
- Server→Client-Frames: `{ "channel": "string", "payload": any }`. Der Client (Bridge `subscribeWs`) verteilt nach `channel`.
- Channels: `chat:agent:<agentId>` (Payload = `RunEvent`, inkl. `userMessage` für Shared-Session), `fs-changed`, `agent-attachments-changed` (Payload = Agent-ID).
- Workspace-Scoping: Broadcasts mit `workspace_id` erreichen nur Mitglieder dieses Workspaces; die Mitgliedschaft wird beim WS-Connect einmalig ermittelt (kein DB-Hit pro Event).
- Reconnect: der Client verbindet bei Close mit dem dann aktuellen Access-Token neu (Token-Refresh).

### Frontend-Bridge (`src/lib/tauri.ts`)

- REST-Helfer `v1()` / `v1Post()` / `v1Upload()` → `fetch('/api/v1/...')`
  mit automatischem 401 → Refresh → Retry. Domänen-APIs gruppiert
  (`authApi`, `workspaceApi`, `memberApi`, `agentApi`, `settingsApi`,
  `secretsApi`, `skillsApi`, `fileApi`, `previewApi`, `chatApi`).
- `subscribeWs(channel, handler)` registriert einen Handler auf **der
  einen** multiplexten WS (`wsConnect` → `/api/v1/ws?token=…`),
  Reconnect mit frischem Token. `chatApi.subscribeAgent` nutzt den
  Channel `chat:agent:<agentId>`.
- Kein direkter `fetch` außerhalb von `src/lib/tauri.ts`. Die Typen aus
  `src/types/` bleiben gegenüber Local unverändert, damit UI-Komponenten
  wiederverwendbar sind.

---

## 8. Datenbank-Schema (Überblick)

```sql
-- Organisationen
-- invite_code: 6-stellig, eindeutig, vom Owner rotierbar. Pflicht beim
-- Beitritt zu einer bestehenden Org (siehe §2 Registrierung).
organizations (id, name, invite_code, created_at)

-- Nutzer (email global eindeutig; ein User gehört zu genau einer Org)
-- KEIN password_hash (passwordless; in 0002 entfernt).
users (id, email, org_id, org_role, created_at)

-- Magic-Link-Tokens (einmalig, 15 min; nur als SHA-256-Hash gespeichert).
-- purpose: login | register. org_id nur bei register (Invite-Code aufgelöst).
login_tokens (id, email, purpose, org_id, token_hash, expires_at, consumed_at, created_at)

-- Refresh-Tokens (rotierend, widerrufbar; httpOnly-Cookie hält nur das Token,
-- der Server hält den Hash + Ablauf)
refresh_tokens (id, user_id, token_hash, expires_at, revoked_at, created_at)

-- Workspaces (= Agent-Ordner)
workspaces (id, org_id, name, created_at)

-- Workspace-Mitgliedschaft (Migration 0006: role-Spalte entfernt — Mitgliedschaft ist binär)
workspace_members (workspace_id, user_id)

-- Agenten
agents (id, workspace_id, name, icon, system_prompt, provider, model_id, skills jsonb, skill_settings jsonb, hitl_disabled, reasoning_enabled, attachments jsonb, delegation_profile jsonb, created_at, updated_at)
-- reasoning_enabled BOOLEAN DEFAULT false (Phase 6d-2): per-Agent-Toggle für
-- Anthropic Extended Thinking / OpenRouter Reasoning. Greift nur bei
-- Modellen aus MODELS_WITH_REASONING (siehe backend/src/llm.rs).

-- Chat-Nachrichten
-- content-JSONB:
--   role = 'user'      → Value::String(text)                            (Plain-String)
--   role = 'assistant' → { text, reasoning?, toolCalls? }               (Phase 6d-1+)
--   role = 'tool'      → { toolResults: [{ toolUseId, content, isError }] }
-- Bestandsdaten vor 6d-1 waren ausschließlich Plain-String; der
-- list_messages-Decoder behandelt sie weiter als reine Text-Nachricht.
chat_messages (id, agent_id, role, content jsonb, created_at)

-- Dateien im Workspace
workspace_files (id, workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by, uploaded_at)

-- Settings pro Organisation
org_settings (org_id, default_provider, default_model, updated_at)

-- API-Keys (verschlüsselt gespeichert)
api_keys (org_id, provider, encrypted_key, updated_at)
```

Migrations liegen unter `backend/src/db/migrations/` als nummerierte SQL-Dateien. `sqlx migrate run` beim Backend-Start.

---

## 9. Sicherheits-Pattern

### Workspace-Sandbox (Pendant zu `ensure_in_agent_folder`)

```rust
// backend/src/sandbox.rs (Prinzip-Skizze; Implementierung ergänzt
// zusätzlich sanitize_filename + workspace_key)
pub fn ensure_in_workspace(
    workspace_id: Uuid,
    s3_key: &str,
) -> Result<String, ApiError> {
    let prefix = format!("workspaces/{}/", workspace_id);
    if !s3_key.starts_with(&prefix) {
        return Err(ApiError::forbidden("path outside workspace"));
    }
    // Keine ..-Segmente erlaubt
    if s3_key.contains("..") {
        return Err(ApiError::forbidden("path traversal detected"));
    }
    Ok(s3_key.to_string())
}
```

### Weitere Pflicht-Checks in jedem Route-Handler

1. JWT-Middleware extrahiert `user_id` aus dem Token.
2. Route-Handler prüft, ob `user_id` Mitglied des angefragten Workspaces ist.
3. Schreibende Aktionen prüfen zusätzlich, ob die Rolle `editor` oder höher ist.
4. Datei-Operationen gehen immer durch `ensure_in_workspace`.

### Secrets

- API-Keys werden **nicht im Klartext** in der Datenbank gespeichert. Verschlüsselung mit AES-256-GCM, Key kommt aus Env-Var `API_KEY_ENCRYPTION_KEY`.
- JWT-Secret kommt aus `JWT_SECRET` (mindestens 32 Bytes, zufällig generiert bei Setup).
- Keine Secrets in Git. Lokale Entwicklung via `.env`-Datei (gitignored).

---

## 10. LLM-Provider-Strategie

Da kein lokales Modell unterstützt wird, vereinfacht sich die Provider-Logik gegenüber Local:

- **Implementierungen:** `AnthropicProvider`, `OpenAiProvider`, `OpenRouterProvider` — direkt aus `processfox_local/src-tauri/src/core/llm/` übernehm- und anpassbar.
- **Provider-Auswahl:** Pro Agent gespeichert (`provider`, `model_id`). Fallback auf Org-Default-Settings.
- **API-Keys:** Werden pro Organisation hinterlegt (nicht pro User). Der Backend-Prozess holt den Key aus der Datenbank und injiziert ihn in den LLM-Request — niemals ans Frontend exponiert.
- **Streaming:** LLM-Antworten werden per WebSocket live an den Client gestreamt, exakt wie die Tauri-Events in Local.
- **Tool-Calling:** Nur Provider, die `supports_tools()` zurückgeben, erhalten Tool-Schemas. `openai_compat.rs` und `anthropic.rs` aus Local können mit minimalen Anpassungen (kein `spawn_blocking` nötig, da kein lokales Modell) übernommen werden.
- **Progressive Disclosure & Cache (Phase 6c-3):** Der Tool-Loop sendet pro Iteration nur die Tool-Schemas der bereits geladenen Skills (`base_tool_schemas` + `schemas_for_loaded`). Anthropic-Prompt-Caching profitiert davon, solange der Load-Stand stabil bleibt; bei jedem `read_skill` wächst der Tools-Block und invalidiert seinen Cache — bewusst hingenommen, weil Tool-Choice-Qualität die paar Token-Cents schlägt. Der System-Prompt ist so geschichtet, dass das statische Skill-Listing **vor** dem volatilen Workspace-Block steht — Cache-Effekt bleibt für den oberen Teil erhalten.
- **Reasoning / Extended Thinking (Phase 6d-2):** Per-Agent-Toggle `agents.reasoning_enabled` (Default `false`). Aktiviert schickt `llm.rs` provider-spezifische Felder mit: bei Anthropic `thinking: { type: "enabled", budget_tokens: 4000 }` (nur Claude 4+; Claude-3-* lehnt das Feld ab und bekommt es defensiv gar nicht erst), bei OpenAI-compat `reasoning: { effort: "medium" }` **nur** wenn das Modell ein dokumentiertes Reasoning-Pattern matched (`MODELS_WITH_REASONING` in `backend/src/llm.rs` — `o1*`/`o3*`/`o4*`, DeepSeek-R1, Qwen-QwQ, `*-thinking`, `grok-*-reasoning`). Reasoning-Deltas kommen über `thinking_delta`-SSE (Anthropic) bzw. `/choices/0/delta/reasoning` und `/choices/0/delta/reasoning_content` (OpenAI/OR/DeepSeek-Style) rein und werden als `reasoningDelta`-WS-Event live broadcastet; persistiert wird das CoT als `assistant.content.reasoning` (Phase 6d-1). Reasoning-Tokens kosten extra — daher Default aus.

---

## 11. Code-Stil

### Rust (Backend)

- Rust 2021 Edition, `cargo fmt` + `cargo clippy -- -D warnings` müssen grün sein.
- **Fehler-Handling:** `thiserror` für Domain-Errors, `anyhow` nur in `main.rs`. Kein `unwrap()` in Production-Code.
- **Async:** `tokio`. DB-Calls async. Datei-I/O läuft via `std::fs` (kleine Dateien ≤ 50 MB, Single-Instance — bewusst simpel statt `tokio::fs`/Streaming); für I/O selbst kein `spawn_blocking` nötig. Ausnahme: genuin CPU-gebundene Parser/Decoder gehören auf den Blocking-Pool, sonst hängen sie den Tokio-Runtime. Beispiel: `pdf-extract` in `tools::read_pdf` (Phase 6b-2i). Neue CPU-gebundene Tools nach gleichem Muster wrappen.
- **Serde:** `#[serde(rename_all = "camelCase")]` an der Grenze zum Frontend. Getaggte Enums: zusätzlich `rename_all_fields = "camelCase"` setzen.
- **sqlx:** **Runtime-Queries** (`sqlx::query`/`query_as` mit `.bind()`) — **keine** `query!`/`query_as!`-Makros, damit das Docker-Image **ohne** DB-Verbindung baubar ist (vgl. §12). Kein ORM.
- **Module-Layout:** Ein Axum-Router-Modul pro Feature unter `backend/src/routes/`.

### TypeScript / React

- Gleiche Regeln wie in `processfox_local/CLAUDE.md` §3.
- `useAuth()`-Hook verwaltet Token-State + automatischen Refresh.
- Kein direkter `fetch`-Aufruf außerhalb von `src/lib/tauri.ts` — alle API-Calls gehen durch die Bridge.
- `src/types/auth.ts` definiert `User`, `Org`, `Workspace`, `WorkspaceRole`.

---

## 12. Deployment (Coolify)

### Docker-Setup

Das Projekt nutzt einen **Multi-Stage Docker Build**:

1. **Stage 1 — Frontend-Build:** Node.js-Image, `npm ci && npm run build` → `/dist`
2. **Stage 2 — Backend-Build:** Rust-Image, `cargo build --release` → Binary
3. **Stage 3 — Runtime:** Minimales Debian-Image, kopiert Frontend-`dist/` und Backend-Binary. Der Axum-Server serviert das Frontend als statische Dateien unter `/` und die API unter `/api/v1/`.

Das konkrete `Dockerfile` liegt im Repo-Root (seit Phase 1) und ist baubar.
Eckpunkte:

- **Build-Kontext = Repo-Root** (nicht `backend/`), damit Stage 1 das
  Frontend und Stage 2 `backend/` sieht.
- **Stage 2** nutzt `rust:1-bookworm` (`cargo build --release --locked`);
  das Runtime-Image ist `debian:bookworm-slim` (gleiche glibc) mit
  `ca-certificates` (TLS zu Postgres) **und `curl`** (Coolify-Healthcheck
  läuft im Container).
- Migrationen sind via `sqlx::migrate!` ins Binary eingebettet — beim
  Image-Bau ist **keine** DB-Verbindung nötig (durchgängig
  Runtime-Queries, keine `query!`-Makros, vgl. §11).
- Healthcheck-Pfad für Coolify: **`GET /api/v1/health`** → `{"status":"ok"}`.
- **Image wird in GitHub Actions gebaut → GHCR** (der 2-vCPU-VPS ist zu
  schwach zum Bauen). Coolify zieht nur das fertige Image.

> Stand 2026-05-19: voll funktionsfähig (Phase 6 vollständig). Der
> Phase-1-Hinweis (nur Health/Frontend) ist überholt.

### Pflicht-Umgebungsvariablen

| Variable | Beschreibung |
|---|---|
| `DATABASE_URL` | PostgreSQL-Connection-String |
| `STORAGE_DIR` | Mount-Pfad des Persistent Volume (Default `/data`) |
| `JWT_SECRET` | Mindestens 32-stelliger zufälliger String |
| `API_KEY_ENCRYPTION_KEY` | 32-Byte-Hex-String für API-Key-Verschlüsselung |
| `PUBLIC_BASE_URL` | Öffentliche App-URL (Magic-Links), ohne Slash |
| `MAGIC_LINK_WEBHOOK_URL` | n8n-Webhook für den Mailversand |
| `MAGIC_LINK_WEBHOOK_SECRET` | optional; Header `X-Webhook-Secret` |
| `PORT` | Backend-Port (Default: 3000) |
| `STATIC_DIR` | Frontend-Verzeichnis (Default `/app/static`) |
| `SKILLS_DIR` | Verzeichnis mit den eingebauten `SKILL.md`-Dateien (Default `/app/skills_builtin`; Phase 6c-2) |

### Coolify-Workflow (Details: `DEPLOY.md`)

1. Push auf `main` → GitHub-Actions-Pipeline (`ci.yml`): erst Tests
   (Job `backend`), dann **bei grünen Tests** baut und pusht Job `image`
   `ghcr.io/<owner>/processfox_web:latest` (Package public stellen).
   Auf PRs läuft nur `backend` — keine GHCR-Tags von PR-Branches.
2. Coolify-Resource als **Docker Image** (kein Dockerfile-Build), Port 3000,
   Healthcheck `/api/v1/health`.
3. PostgreSQL-Service + Persistent Volume (Mount `/data`) in Coolify anlegen.
4. Umgebungsvariablen im Coolify-UI hinterlegen (`DATABASE_URL`, `STORAGE_DIR`,
   Secrets …) **und Domain im Coolify-„Domains"-Feld** setzen (nicht nur
   `PUBLIC_BASE_URL` — sonst „no available server").
5. Nach grünem Action-Run in Coolify **Redeploy** (zieht frisches `:latest`).
6. Erste Org + Owner einmalig per Seed-SQL anlegen (`DEPLOY.md` §6).

---

## 13. Test-Strategie

- **Rust:** `cargo test` für Unit-Tests. **HTTP/DB-Integrationstests** in `backend/tests/integration.rs`: die echten Axum-Handler über `tower::ServiceExt::oneshot` (kein Port/Server), DB via `#[sqlx::test(migrations = "./src/db/migrations")]` mit pro Test frischer Wegwerf-Postgres-DB — **kein** In-Memory-SQLite (sqlx-Queries sind Postgres-spezifisch). Lokal eine erreichbare `DATABASE_URL` nötig; in CI ein Postgres-Service-Container (`.github/workflows/ci.yml`).
- **Frontend:** `vitest` für Hooks und Utility-Funktionen.
- **Auth-Tests:** Pflicht — Login, Token-Refresh, Workspace-Berechtigungs-Checks.
- **Build-Gates vor jedem Commit:** `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, `npm run build`.
- **E2E:** Playwright für Login → Workspace anlegen → Agent anlegen → Datei hochladen → Chat. Aufzusetzen ab dem ersten vollständigen Feature-Slice.

---

## 14. Sprach-Konvention

Identisch mit `processfox_local`:
- **Code:** Englisch (Variablen, Funktionsnamen, Kommentare, Commit-Messages).
- **UI-Strings:** Deutsch.
- **Dokumentation im Repo:** Deutsch.
- **SKILL.md-Bodies:** Englisch.

---

## 15. Was NICHT zu tun ist

- Kein lokales GGUF, keine `llama-cpp-2`-Abhängigkeit.
- Keine Hardware-Erkennung (RAM, VRAM).
- Kein Polling für Echtzeit-Updates — immer WebSocket.
- Keine API-Keys ans Frontend exponieren.
- Keine Datei-Pfade, die außerhalb des Workspace-Prefixes liegen, akzeptieren.
- Kein globaler Mutex um shared State — Axum nutzt `Arc<AppState>` mit internem Locking wo nötig.
- Keine direkte Datei-Pfad-/Roh-Ausgabe ans Frontend ohne signierten, kurzlebigen Link.
- Keine Chat-History-Sidebar. Auch hier gilt: Agenten sind die persistenten Einheiten.

---

## 16. Abweichungen von ProcessFox Local

| Aspekt | Local | Web |
|---|---|---|
| Runtime | Tauri v2 | Browser + Axum-Server |
| API-Bridge | `@tauri-apps/api` invoke/listen | fetch + WebSocket |
| LLM | Lokal (GGUF) + Cloud optional | Cloud-only (Anthropic, OpenAI, OpenRouter) |
| Datei-Zugriff | Lokaler OS-Ordner | Upload → lokales Volume (`STORAGE_DIR`) |
| Auth | Kein Auth (Single-User) | JWT + Rollen |
| Mehrbenutzer | Nein | Ja (Org → Workspace → User) |
| State-Persistenz | JSON-Dateien im App-Support-Ordner | PostgreSQL |
| Deployment | Desktop-App (GitHub Releases) | Docker + Coolify |
| Hardware-Check | Ja (RAM/VRAM für Modell-Empfehlung) | Entfällt |
| Secrets | OS-Keychain (Tauri Stronghold) | Verschlüsselt in DB + Env-Vars |

---

Wenn du dieses Dokument liest und etwas unvollständig oder widersprüchlich findest: bitte aktualisiere es im selben PR, in dem du die neue Arbeit hinzufügst.
