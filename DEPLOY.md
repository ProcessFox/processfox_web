# DEPLOY.md — Coolify-Deployment (Phase 1)

Schritt-für-Schritt-Anleitung für den Deploy von ProcessFox Web auf
`chat.processfox.ai` via Coolify. Stand: Phase 1 (Backend-Skeleton).

> **Was Phase 1 liefert:** Der Server liefert das Frontend aus und
> beantwortet `GET /api/v1/health`. Alle übrigen API-Endpunkte (Login,
> Workspaces, Agenten, Dateien, Chat) folgen in Phase 2–6. Die UI lädt
> also, aber Datenaktionen liefern noch 404. Dieser Deploy validiert die
> **Pipeline** (Build → Coolify → Postgres/MinIO → Health), nicht die
> App-Funktion.

---

## 1. Coolify-Services anlegen

### a) PostgreSQL
- In Coolify: **+ New → Database → PostgreSQL** (Version 16 empfohlen).
- Coolify gibt dir einen **internen** Connection-String der Form
  `postgres://<user>:<pass>@<service-host>:5432/<db>`.
- **Keinen** öffentlichen Port aktivieren (nicht nötig, Sicherheitsrisiko).

### b) MinIO (S3)
- In Coolify: **+ New → Service → MinIO** (One-Click-Service).
- Notiere `S3_ACCESS_KEY` (Root-User) und `S3_SECRET_KEY` (Root-Passwort).
- Interner Endpunkt typischerweise `http://<minio-service>:9000`.
- **Bucket anlegen:** MinIO-Konsole öffnen → Bucket **`processfox`**
  erstellen (Phase 1 nutzt es noch nicht, aber so ist es ab Phase 5 bereit).

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
  `{"status":"ok"}`, HTTP 200).
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
| `S3_ENDPOINT` | interner MinIO-Endpunkt, z. B. `http://<minio>:9000` |
| `S3_BUCKET` | `processfox` |
| `S3_ACCESS_KEY` | MinIO Root-User |
| `S3_SECRET_KEY` | MinIO Root-Passwort |
| `S3_REGION` | `us-east-1` |
| `JWT_SECRET` | `openssl rand -base64 48` (≥ 32 Zeichen) |
| `API_KEY_ENCRYPTION_KEY` | `openssl rand -hex 32` (genau 64 Hex-Zeichen) |
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

## 6. Bekannte Phase-1-Grenzen

- Login/Registrierung, Workspaces, Agenten, Datei-Upload, Chat: **noch
  nicht implementiert** (Phase 2–6). Entsprechende Calls → HTTP 404.
- Erste Organisation + Owner werden später **einmalig per SQL** angelegt
  (Coolify-DB-Terminal), nicht über die App — relevant ab Phase 2.
- Frontend-Bridge spricht aktuell RPC (`POST /api/<command>`); Umstellung
  auf REST `/api/v1/...` ist eine spätere Etappe (siehe PLAN.md).
