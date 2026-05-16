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

## 1a. Ist-Stand (Stand: 2026-05-16)

> **Wichtig:** Der Rest dieses Dokuments (§2–§16) beschreibt den **Soll-Zustand** /
> die Ziel-Architektur. Der folgende Abschnitt beschreibt, was **tatsächlich im
> Repo liegt**. Bei jedem Task zuerst hier prüfen, was schon existiert.

**Was realisiert ist:**

- **Nur das Frontend.** Das Repo enthält ausschließlich die React/Vite/TS-App —
  ein nahezu 1:1-Port des Frontends aus `processfox_local` (ein einziger Commit
  „Initial commit", 70 Dateien).
- **API-Bridge umgeschrieben:** `src/lib/tauri.ts` ersetzt die Tauri-`invoke`/`listen`-
  Bridge durch `fetch` (HTTP POST) + `WebSocket`. Die Funktions-Signaturen sind
  unverändert, damit die UI-Komponenten ungeändert bleiben.
- UI-Komponenten vollständig vorhanden: Agent-Editor/-Switcher, Chat-Pane
  (inkl. HITL-Karten, AskUser, Tool-/Reasoning-Chips), FileTree, Preview-Viewer
  (docx/xlsx/pptx/pdf/image/markdown/text), Settings (Models + Cloud-APIs),
  Welcome-Dialog, Theme-Provider, shadcn-UI-Bausteine.

**Was NICHT existiert (entgegen §6 ff.):**

- **Kein Backend.** Es gibt kein `backend/`-Verzeichnis, keinen Axum-Server,
  keine DB-Migrations, keine Rust-Crate. Die Bridge zeigt ins Leere
  (`vite.config.ts` proxyt `/api`+`/ws` → `localhost:3000`, dort läuft nichts).
- **Kein Auth / Mehrbenutzer.** `src/types/auth.ts`, `src/hooks/useAuth.ts`,
  `src/views/Login.tsx` und `src/components/workspace/` existieren **nicht**.
  Es gibt keinerlei Org-/Workspace-/Rollen-Konzept im Frontend.
- **Local-Paradigma noch durchgängig vorhanden** — d. h. das Frontend ist noch
  *nicht* an das Web-Konzept angepasst:
  - Lokale GGUF-Modelle, Hardware-Info, Modell-Katalog & -Download
    (`modelsApi`, `HardwareInfo`, `provider === "local"`-Pfade in `App.tsx`).
  - OS-Ordner-Zugriff statt Upload (`list_agent_folder`, `watch_agent_folder`,
    `import_files_to_agent`, `files-dropped`-Event, `agent.folder`).
  - `Agent` hat ein `folder`-Feld, kein `workspace_id`.
- **API-Bridge weicht von §7 ab:** Die Bridge nutzt **RPC-Stil**
  (`POST /api/<command>`, z. B. `/api/list_agents`), **nicht** das in §7
  beschriebene RESTful-Schema `/api/v1/...`. Auch ohne `Authorization`-Header.
  Diese Diskrepanz ist offen und muss bewusst entschieden werden (siehe §7-Notiz).

**Daraus folgende grobe Roadmap (noch nicht abgestimmt — siehe Fragen):**

1. Backend-Skeleton (Axum + sqlx + S3) anlegen.
2. Auth-Schicht (JWT, Login-View, `useAuth`) ergänzen.
3. Frontend vom Local- auf das Web-Paradigma umbauen (Workspaces statt Ordner,
   Upload statt OS-Zugriff, Local-Modell-/Hardware-Code entfernen).
4. Bridge-Konvention (RPC vs. REST) festziehen.

---

## 2. Tech-Stack

| Bereich | Technologie |
|---|---|
| Frontend | React 19 + Vite + TypeScript + Tailwind CSS + shadcn/ui |
| API-Bridge | `src/lib/tauri.ts` → HTTP POST `/api/*` + WebSocket `/ws/*` |
| Backend | Rust + **Axum** |
| Realtime | Axum WebSocket (tokio-tungstenite) |
| Datenbank | **PostgreSQL** via `sqlx` (async, compile-time-checked queries) |
| Datei-Storage | S3-kompatibler Objektspeicher (MinIO für Self-Hosted, AWS S3 optional) |
| Auth | JWT (Bearer-Token) + Refresh-Token; Ausgabe via `/api/auth/login` |
| LLM-Provider | Anthropic, OpenAI, OpenRouter — kein lokales GGUF in v1 |
| Deployment | Docker (multi-stage build) + Coolify |
| CI/CD | GitHub Actions |

---

## 3. Goldene Regeln

1. **Team zuerst, Einzelperson zweiter.** Jede Entscheidung muss mit mehreren parallelen Nutzern im Hintergrundmodell funktionieren. Geteilter State ist die Norm, nicht die Ausnahme.
2. **Cloud-LLM ist Default.** Kein lokales Modell, keine GGUF-Infrastruktur, kein Hardware-Check. Modell-Auswahl = Provider + Modell-String.
3. **Agent-Workspace statt lokaler Ordner.** Der „Agent-Ordner" ist ein logischer Workspace im Objekt-Storage. Datei-Upload und -Download ersetzen den OS-Dateibaum.
4. **Berechtigungen sind workspace-scoped.** Jede Backend-Operation prüft, ob der aufrufende User Mitglied des Workspaces ist. Kein Verlass auf Frontend-Filterung.
5. **HITL ist Default für Schreibaktionen.** Wie in Local: Freigabe vor jeder destruktiven oder schreibenden Datei-Aktion.
6. **WebSocket für Echtzeit.** Keine Polling-Lösung. Run-Events, HITL-Anfragen und FS-Änderungen laufen über einen persistenten WS-Kanal pro Nutzer/Session.
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
| **User** | Hat eine Org-Rolle (`owner`, `member`) und eine optionale Workspace-Rolle (`editor`, `viewer`). |
| **Session** | Eine Chat-Session pro Agent, shared. Alle Mitglieder sehen den gleichen Chat-Verlauf live. |

### Berechtigungs-Matrix

| Aktion | Owner | Editor | Viewer |
|---|---|---|---|
| Agent erstellen/löschen | ✓ | ✓ | — |
| Chat senden | ✓ | ✓ | — |
| HITL freigeben | ✓ | ✓ | — |
| Dateien hochladen/löschen | ✓ | ✓ | — |
| Workspace-Mitglieder verwalten | ✓ | — | — |
| API-Keys hinterlegen | ✓ | — | — |

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

1. **Frontend:** Drag & Drop oder Datei-Picker → `multipart/form-data` an `POST /api/workspaces/:id/files`.
2. **Backend:** Validierung (Dateityp, Größe ≤ 50 MB), Upload in S3-Bucket unter `workspaces/<workspace_id>/<filename>`.
3. **Datenbank:** Eintrag in `workspace_files`-Tabelle (workspace_id, filename, s3_key, size, uploaded_by, uploaded_at).
4. **Frontend:** WS-Broadcast an alle Workspace-Mitglieder → Datei-Baum aktualisiert sich live.

### Download / Vorschau

- Dateien werden nie direkt aus S3 ans Frontend gestreamt. Das Backend erzeugt **Pre-signed URLs** (Gültigkeit: 15 min) für den Browser-Download.
- Vorschau-Endpunkte (`/api/preview/docx`, `/api/preview/xlsx`, `/api/preview/pptx`) laden die Datei serverseitig aus S3 und liefern das bereits bekannte Preview-JSON zurück — gleiche Datenstrukturen wie in Local.

### Erlaubte Dateitypen

`.md`, `.txt`, `.pdf`, `.docx`, `.xlsx`, `.csv`, `.png`, `.jpg`, `.jpeg`, `.webp`, `.pptx`

### Sandbox-Prinzip

Jeder S3-Pfad wird serverseitig gegen das Schema `workspaces/<workspace_id>/` validiert. Path-Traversal ist auf API-Ebene ausgeschlossen.

---

## 6. Verzeichnis-Layout (Soll-Stand)

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
└── backend/                     # Rust-Backend (Axum)
    ├── Cargo.toml
    └── src/
        ├── main.rs
        ├── lib.rs
        ├── config.rs            # Env-Vars (DB_URL, S3_*, JWT_SECRET, ...)
        ├── db/                  # sqlx-Migrations + Query-Module
        │   ├── migrations/
        │   └── mod.rs
        ├── auth/                # JWT-Middleware, Login/Register-Handler
        ├── routes/              # Axum-Router
        │   ├── mod.rs
        │   ├── agents.rs
        │   ├── chat.rs
        │   ├── files.rs
        │   ├── preview.rs
        │   ├── settings.rs
        │   ├── skills.rs
        │   └── workspaces.rs
        ├── ws/                  # WebSocket-Hub, Broadcast
        ├── storage/             # S3-Client (aws-sdk-s3 oder object_store)
        ├── core/                # Wiederverwendete Logik aus processfox_local
        │   ├── chat/            # ReAct-Loop, ChatRepo
        │   ├── llm/             # LlmProvider-Trait + Cloud-Implementierungen
        │   ├── skill/           # SKILL.md-Parsing, SkillRegistry
        │   ├── tool/            # Tool-Trait, Registry, Tool-Implementierungen
        │   ├── sandbox.rs       # ensure_in_workspace()
        │   └── error.rs
        └── types.rs
```

---

## 7. API-Konventionen

> **⚠ Offene Diskrepanz (siehe §1a):** Die aktuell im Repo liegende Bridge
> (`src/lib/tauri.ts`) nutzt RPC-Stil (`POST /api/<command>`, ohne Versionierung,
> ohne Auth-Header). Das folgende RESTful-Schema ist der **Soll-Zustand**, aber
> noch nicht implementiert. Bevor das Backend gebaut wird, muss entschieden
> werden, welche Konvention gilt — und die jeweils andere Seite angepasst werden.

### HTTP-Endpunkte

- Alle Endpunkte unter `/api/v1/`.
- Authentifizierung via `Authorization: Bearer <access_token>`.
- Fehler-Antworten: `{ "code": "string", "message": "string", "details"?: any }`.
- Erfolgreiche Antworten: direkt das Objekt oder `{ "data": [...] }` bei Listen.
- HTTP-Status-Codes: 200 OK, 201 Created, 400 Bad Request, 401 Unauthorized, 403 Forbidden, 404 Not Found, 409 Conflict, 500 Internal Server Error.

### WebSocket-Protokoll

- Verbindung: `GET /ws?token=<access_token>` (Token im Query-String, weil WS-Browser-API keinen Auth-Header unterstützt).
- Nachrichten: JSON-Objekte mit `{ "type": "string", "payload": any }`.
- Event-Typen spiegeln die Tauri-Events aus Local: `chat:run:<runId>`, `fs-changed`, `agent-attachments-changed`, `model:download:<downloadId>`.
- Der Backend-WS-Hub sendet Events nur an User, die Mitglied des betreffenden Workspaces sind.

### Frontend-Bridge (`src/lib/tauri.ts`)

- `post<T>(command, body)` → `fetch('/api/v1/<command>', { method: 'POST', ... })`
- `subscribeWs<T>(channel, handler)` → `new WebSocket('/ws/...')`
- Die Typen (Interfaces, Enums) aus `src/types/` bleiben unverändert gegenüber Local, damit UI-Komponenten wiederverwendbar sind.

---

## 8. Datenbank-Schema (Überblick)

```sql
-- Organisationen
-- invite_code: 6-stellig, eindeutig, vom Owner rotierbar. Pflicht beim
-- Beitritt zu einer bestehenden Org (siehe §2 Registrierung).
organizations (id, name, invite_code, created_at)

-- Nutzer (email global eindeutig; ein User gehört zu genau einer Org)
users (id, email, password_hash, org_id, org_role, created_at)

-- Refresh-Tokens (rotierend, widerrufbar; httpOnly-Cookie hält nur das Token,
-- der Server hält den Hash + Ablauf)
refresh_tokens (id, user_id, token_hash, expires_at, revoked_at, created_at)

-- Workspaces (= Agent-Ordner)
workspaces (id, org_id, name, created_at)

-- Workspace-Mitgliedschaft
workspace_members (workspace_id, user_id, role)  -- role: editor | viewer

-- Agenten
agents (id, workspace_id, name, icon, system_prompt, provider, model_id, skills jsonb, skill_settings jsonb, created_at, updated_at)

-- Chat-Nachrichten
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
// backend/src/core/sandbox.rs
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

---

## 11. Code-Stil

### Rust (Backend)

- Rust 2021 Edition, `cargo fmt` + `cargo clippy -- -D warnings` müssen grün sein.
- **Fehler-Handling:** `thiserror` für Domain-Errors, `anyhow` nur in `main.rs`. Kein `unwrap()` in Production-Code.
- **Async:** `tokio`. Alle DB- und S3-Calls sind async; kein `spawn_blocking` nötig (kein lokales LLM).
- **Serde:** `#[serde(rename_all = "camelCase")]` an der Grenze zum Frontend. Getaggte Enums: zusätzlich `rename_all_fields = "camelCase"` setzen.
- **sqlx:** Compile-time-geprüfte Queries (`query_as!`, `query!`). Kein ORM.
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
- **Stage 2** nutzt `rust:1-bookworm`; das Runtime-Image ist
  `debian:bookworm-slim` (gleiche glibc). `ca-certificates` ist im Runtime
  installiert (TLS zu Postgres/S3).
- Migrationen sind via `sqlx::migrate!` ins Binary eingebettet — beim
  Image-Bau ist **keine** DB-Verbindung nötig (keine `query!`-Makros im
  Skeleton).
- Healthcheck-Pfad für Coolify: **`GET /api/v1/health`** → `{"status":"ok"}`.

> Stand Phase 1: Der Server liefert das Frontend aus und beantwortet
> `/api/v1/health`. Alle übrigen API-Endpunkte folgen in Phase 2–6 — bis
> dahin lädt die UI, aber Datenaktionen liefern 404. Der Phase-1-Deploy
> validiert die Pipeline (Build, Coolify, Postgres/MinIO, Health), nicht
> die App-Funktion.

### Pflicht-Umgebungsvariablen

| Variable | Beschreibung |
|---|---|
| `DATABASE_URL` | PostgreSQL-Connection-String |
| `S3_ENDPOINT` | MinIO/S3-URL |
| `S3_BUCKET` | Bucket-Name |
| `S3_ACCESS_KEY` | S3-Access-Key-ID |
| `S3_SECRET_KEY` | S3-Secret-Key |
| `JWT_SECRET` | Mindestens 32-stelliger zufälliger String |
| `API_KEY_ENCRYPTION_KEY` | 32-Byte-Hex-String für API-Key-Verschlüsselung |
| `PORT` | Backend-Port (Default: 3000) |

### Coolify-Workflow

1. GitHub-Repo verbinden.
2. Dockerfile als Build-Methode wählen.
3. Umgebungsvariablen im Coolify-UI hinterlegen.
4. PostgreSQL- und MinIO-Services in Coolify anlegen, `DATABASE_URL` + S3-Vars setzen.
5. Deploy on push to `main`.

---

## 13. Test-Strategie

- **Rust:** `cargo test` für Unit-Tests. Integration-Tests für Axum-Handler via `reqwest` gegen einen Test-Server. **DB:** Postgres-Testcontainer bzw. `#[sqlx::test]` mit Wegwerf-Datenbank — **kein** In-Memory-SQLite (sqlx-Queries sind Postgres-spezifisch + compile-time-geprüft, SQLite ist inkompatibel).
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
- Keine direkte S3-URL-Ausgabe an das Frontend ohne Pre-signing.
- Keine Chat-History-Sidebar. Auch hier gilt: Agenten sind die persistenten Einheiten.

---

## 16. Abweichungen von ProcessFox Local

| Aspekt | Local | Web |
|---|---|---|
| Runtime | Tauri v2 | Browser + Axum-Server |
| API-Bridge | `@tauri-apps/api` invoke/listen | fetch + WebSocket |
| LLM | Lokal (GGUF) + Cloud optional | Cloud-only (Anthropic, OpenAI, OpenRouter) |
| Datei-Zugriff | Lokaler OS-Ordner | Upload → S3-Bucket |
| Auth | Kein Auth (Single-User) | JWT + Rollen |
| Mehrbenutzer | Nein | Ja (Org → Workspace → User) |
| State-Persistenz | JSON-Dateien im App-Support-Ordner | PostgreSQL |
| Deployment | Desktop-App (GitHub Releases) | Docker + Coolify |
| Hardware-Check | Ja (RAM/VRAM für Modell-Empfehlung) | Entfällt |
| Secrets | OS-Keychain (Tauri Stronghold) | Verschlüsselt in DB + Env-Vars |

---

Wenn du dieses Dokument liest und etwas unvollständig oder widersprüchlich findest: bitte aktualisiere es im selben PR, in dem du die neue Arbeit hinzufügst.
