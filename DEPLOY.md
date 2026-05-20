# DEPLOY.md — Coolify-Deployment (Phase 1)

Schritt-für-Schritt-Anleitung für den Deploy von ProcessFox Web auf
`chat.processfox.ai` via Coolify. Stand: Phase 1 (Backend-Skeleton).

> **Was Phase 1 liefert:** Der Server liefert das Frontend aus und
> beantwortet `GET /api/v1/health`. Alle übrigen API-Endpunkte (Login,
> Workspaces, Agenten, Dateien, Chat) folgen in Phase 2–6. Die UI lädt
> also, aber Datenaktionen liefern noch 404. Dieser Deploy validiert die
> **Pipeline** (Build → Coolify → Postgres/Volume → Health), nicht die
> App-Funktion.

---

## 1. Coolify-Services anlegen

### a) PostgreSQL
- In Coolify: **+ New → Database → PostgreSQL** (Version 16 empfohlen).
- Coolify gibt dir einen **internen** Connection-String der Form
  `postgres://<user>:<pass>@<service-host>:5432/<db>`.
- **Keinen** öffentlichen Port aktivieren (nicht nötig, Sicherheitsrisiko).

### b) Datei-Storage (lokales Volume — kein MinIO/S3)
Ab dem Storage-Umbau speichert die App Dateien auf einem **lokalen
Persistent Volume** (Single-Instance, self-hosted — kein Objektspeicher).
- In der App-Resource → **Persistent Storage** → ein Volume mit dem
  Mount-Pfad **`/data`** anlegen.
- Env-Var `STORAGE_DIR=/data` (Default ist bereits `/data`).
- Kein MinIO-Service, kein Bucket, keine S3-Credentials nötig.
- **Wichtig:** Das Volume sichern (Coolify-Backup) — die App ist dadurch
  bewusst *stateful* und an diese Instanz gebunden (kein H-Scaling).

## 2. Image bauen (GitHub Actions) statt auf dem VPS

> **Warum:** Der 2-vCPU-VPS ist zu schwach, um Frontend (großer Bundle) +
> Rust-Backend zu *bauen* — Builds liefen >25 min ins Memory-Thrashing.
> Deshalb baut **GitHub Actions** das Image und pusht es nach GHCR; Coolify
> **zieht** nur das fertige Image (Pull = Sekunden, VPS baut nichts).

1. Workflow `.github/workflows/docker.yml` ist im Repo. Bei jedem Push auf
   `main` (oder manuell via „Run workflow") baut er das Multi-Stage-Image
   und pusht nach `ghcr.io/<owner>/<repo>` (Tags `latest` + Commit-SHA).
2. **Package auf öffentlich stellen** (einmalig, nach dem ersten erfolg-
   reichen Run): GitHub → Repo/Org → **Packages** → `processfox_web` →
   **Package settings → Change visibility → Public**. Danach kann Coolify
   ohne Credentials ziehen.

## 2b. Application in Coolify anlegen (Docker Image, kein Build)

- **+ New → Docker Image** (nicht „Repository"/Dockerfile-Build).
- **Image:** `ghcr.io/<owner-lowercase>/processfox_web:latest`
  (z. B. `ghcr.io/processfox/processfox_web:latest`).
- **Port:** `3000`.
- **Health Check Path:** `/api/v1/health` (erwartete Antwort
  `{"status":"ok"}`, HTTP 200). Coolify führt den Check **im Container**
  via `curl` aus — `curl` ist deshalb im Runtime-Image installiert
  (Dockerfile). Ohne curl/wget meldet Coolify „unhealthy" und rollt zurück
  → Traefik „no available server".
- Redeploy nach neuem Image: in Coolify **Redeploy** drücken, sobald der
  GitHub-Action-Run grün ist (optional später per Coolify-Deploy-Webhook
  am Ende des Workflows automatisieren).

> Eine bereits als „Dockerfile-Build aus Repository" angelegte Application
> kann nicht zuverlässig auf Image-Pull umgestellt werden — sauberer ist
> eine **neue** Docker-Image-Resource (Env-Vars/Domain dorthin übernehmen,
> alte Resource danach löschen).

## 3. Umgebungsvariablen (Coolify → Environment Variables)

Vorlage siehe `.env.example`. Konkret:

| Variable | Wert |
|---|---|
| `DATABASE_URL` | interner Postgres-Connection-String aus Schritt 1a |
| `STORAGE_DIR` | `/data` (Mount-Pfad des Persistent Volume aus Schritt 1b) |
| `JWT_SECRET` | `openssl rand -base64 48` (≥ 32 Zeichen) |
| `API_KEY_ENCRYPTION_KEY` | `openssl rand -hex 32` (genau 64 Hex-Zeichen) |
| `PUBLIC_BASE_URL` | `https://chat.processfox.ai` (ohne Slash am Ende) |
| `MAGIC_LINK_WEBHOOK_URL` | n8n-Webhook-URL, die die Login-Mail versendet |
| `MAGIC_LINK_WEBHOOK_SECRET` | optional; Shared-Secret (Header `X-Webhook-Secret`) |
| `PORT` | `3000` |
| `STATIC_DIR` | `/app/static` |

> Secrets niemals committen. `JWT_SECRET`/`API_KEY_ENCRYPTION_KEY` einmalig
> erzeugen und nur im Coolify-UI hinterlegen. Ändert man
> `API_KEY_ENCRYPTION_KEY` später, sind alle verschlüsselten API-Keys
> (ab Phase 4) unbrauchbar.

## 4. Domain & TLS

- In Coolify die Domain **`chat.processfox.ai`** auf die Application legen.
- DNS: `chat` als A/AAAA bzw. CNAME auf den Coolify-Host zeigen lassen.
- TLS-Zertifikat via Coolify (Let's Encrypt) aktivieren.

## 5. Deploy & Verifikation

1. Push auf `main` → **GitHub-Action** „Build & Push Image" abwarten
   (Actions-Tab; erster Run lädt/baut alles, danach via Cache schnell).
2. In Coolify **Redeploy** der Docker-Image-Resource auslösen (zieht das
   frische `:latest`).
3. Nach „Healthy":

```bash
curl -fsS https://chat.processfox.ai/api/v1/health
# erwartet: {"status":"ok"}

curl -fsS https://chat.processfox.ai/ | head        # liefert index.html
curl -s -o /dev/null -w '%{http_code}\n' \
  https://chat.processfox.ai/api/v1/list_workspaces  # erwartet 404 (Phase 1)
```

4. **Migrations:** laufen automatisch beim Start. In den Coolify-Logs
   erscheint `Datenbank verbunden, Migrationen angewendet`. Optional in der
   Postgres-Konsole prüfen: `\dt` zeigt `organizations`, `users`,
   `workspaces`, … (Schema aus `0001_init.sql`).

## 6. Erste Organisation + Owner anlegen (Seed-SQL, einmalig)

Es gibt **keinen** Org-Erstellungs-Endpunkt — Registrierung erfordert immer
einen Org-Invite-Code. Die erste Org + den Owner legst du **einmalig per
SQL** an.

**Zugang zur internen DB** (kein öffentlicher Port nötig):

- *Weg A (empfohlen):* Coolify → PostgreSQL-Resource → Tab **„Terminal"**
  → `psql -U postgres -d postgres` (User/DB wie in `DATABASE_URL`).
- *Weg B:* SSH auf den Host →
  `docker exec -i <pg-container> psql -U postgres -d postgres`.

**Seed (ein Statement — kein manuelles UUID-Kopieren).** `invite_code`:
6 Zeichen aus `A–Z`/`2–9` ohne mehrdeutige (`O/I/L`/`0/1`); E-Mail
**kleingeschrieben** (Backend normalisiert beim Login auf lowercase):

```sql
WITH org AS (
  INSERT INTO organizations (name, invite_code)
  VALUES ('ProcessFox', 'ABCD23')
  RETURNING id
), new_user AS (
  INSERT INTO users (email, org_id, org_role)
  SELECT 'christian@xplrs.net', id, 'owner' FROM org
)
INSERT INTO org_settings (org_id) SELECT id FROM org;
```

Kontrolle:

```sql
SELECT o.name, o.invite_code, u.email, u.org_role
FROM organizations o JOIN users u ON u.org_id = o.id;
```

Danach: auf `chat.processfox.ai` → **Anmelden** → E-Mail eingeben →
Magic-Link aus der Mail → eingeloggt. Weitere Mitglieder registrieren sich
selbst über **Registrieren** mit dem Invite-Code (`ABCD23`).

## 7. n8n-Webhook-Vertrag (Magic-Link-Versand)

Das Backend POSTet bei jedem Login-/Registrierungs-Wunsch an
`MAGIC_LINK_WEBHOOK_URL`:

```json
{ "email": "user@example.com",
  "magicLink": "https://chat.processfox.ai/auth/callback?token=…",
  "purpose": "login" }
```

`purpose` ist `login` oder `register`. Ist `MAGIC_LINK_WEBHOOK_SECRET`
gesetzt, kommt zusätzlich der Header `X-Webhook-Secret`. Dein n8n-Flow muss
nur eine E-Mail mit `magicLink` als klickbarem Link an `email` versenden.
Der Link ist 15 Minuten gültig und einmalig nutzbar.

## 8. Bekannte Grenzen (Stand Phase 6 vollständig)

- Live: Auth, Workspaces/Mitglieder, Agenten, Org-Settings/API-Keys,
  Datei-Upload/Vorschau, Streaming-Chat (shared session), Tools+HITL,
  Rückfragen, Excel-/Word-Schreiben, Word-aus-Vorlage, Word-Anhängen,
  Excel-Zell-Edits **und Bulk-Delegation** (Zeile-für-Zeile-Worker mit
  Fortschrittsanzeige). Skill **„Dateien"** deckt Lesen + alle
  Schreib-Operationen + Delegation ab — jeweils nach Freigabe-Dialog,
  live für alle Workspace-Mitglieder.
- **Grenze Vorlagen:** Platzhalter müssen im Vorlagentext zusammen-
  hängend stehen (Word splittet sie sonst über Runs).
- **Grenze Zell-Edits/Delegation:** Schreiben erzeugt das Zielblatt neu;
  Formeln/Formate/weitere Blätter gehen verloren. Delegation max. 200
  Zeilen pro Lauf.
- **Grenze `read_pdf`:** PDFs > 20 MB werden vor dem Parsen abgelehnt
  (Multi-Tenant-Schutz, Upload-Limit bleibt 50 MB). Gescannte PDFs
  ohne OCR-Layer liefern leeren Text — `read_pdf` weist explizit darauf
  hin. Ausgabe-Cap 200 KB Klartext, danach Truncation-Footer.
- **Grenze `read_docx`:** DOCX > 20 MB werden vor dem Inflaten abgelehnt
  (gleicher Multi-Tenant-Schutz). Extraktion liefert nur Lauftext —
  Tabellen-Zellen-Inhalt bleibt erhalten, Bilder/eingebettete Objekte
  werden gestrippt. Ausgabe-Cap 200 KB Klartext.
- **Grenze `read_xlsx_range`:** maximal 500 Zellen pro Aufruf
  (Range eingrenzen, dann erneut lesen). Output ist strukturiertes
  JSON `{file, sheet, range, headers, rows}` — erste Zeile der Range
  landet in `headers`, restliche in `rows`; alle Zellwerte sind
  Strings (kein Type-Drift bei Mixed-Type-Spalten). Kein zusätzlicher
  Größen-Cap auf dem Workbook — das 50-MB-Upload-Limit ist die obere
  Schranke.
- **Grenze `rewrite_file`:** überschreibt nur Text-Dateien mit den
  Endungen `.md`, `.markdown`, `.txt`, `.text`, `.csv` (Word/Excel
  haben eigene Schreib-Tools, PDFs sind nicht text-überschreibbar).
- **Reasoning/Thinking (Phase 6d-2):** Per-Agent-Toggle „Reasoning-
  Modus aktivieren" (Default aus). An: Backend schickt Anthropic
  `thinking`-Feld bzw. OpenAI/OR `reasoning`-Feld mit. **Kosten:**
  Anthropic rechnet Extended-Thinking-Tokens separat ab (4 000-Token-
  Budget pro Antwort), OpenRouter abhängig vom Modell (DeepSeek R1 /
  OpenAI o-Serie /…) — siehe deren Preisseite. Bei Nicht-Reasoning-
  Modellen (gpt-4o, Claude 3.x, Llama, …) ist der Toggle wirkungslos
  und kostet **nichts** extra; das Feld wird gar nicht erst gesendet.
  Bestand max. 5 MB — sonst BadRequest mit Hinweis auf
  append-Workflow. Bestand muss UTF-8 sein (sonst BadRequest). HITL
  ist Pflicht; der Nutzer sieht einen Zeilen-Diff (`DiffSection` im
  Frontend) bevor er freigibt.
- **Progressive Skill-Disclosure (Phase 6c-3):** Das LLM braucht
  jetzt **zwei Schritte**, bevor es ein domain-spezifisches Tool
  aufruft: erst `read_skill({ skillId: "<id>" })` mit der `id` aus
  der Skill-Liste im System-Prompt, dann das eigentliche Tool. Ohne
  vorher geladenen Skill ist das Tool-Schema beim Provider gar nicht
  deklariert und der Aufruf scheitert mit einer freundlichen
  „lies zuerst den Skill"-Meldung. `ask_user` ist immer verfügbar.
- **Härtung erledigt:** HTTP/DB-Integrationstests (`backend/tests/
  integration.rs`) laufen in CI (`.github/workflows/ci.yml`, Postgres-
  Service) — Auth, Refresh-Rotation, Workspace-Berechtigungen.
- **Optional offen (klein):** `delegationProfile`-Override (eigenes
  Worker-Modell je Agent), Vorlage via Agent-Attachment.
