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

## 2. Application anlegen

- **+ New → Application → Public/Private Repository** → dieses GitHub-Repo.
- **Build Pack: Dockerfile** (das `Dockerfile` liegt im Repo-Root,
  Build-Kontext = Repo-Root — nichts umstellen).
- **Port:** `3000`.
- **Health Check Path:** `/api/v1/health` (erwartete Antwort
  `{"status":"ok"}`, HTTP 200).
- **Branch:** `main`, „Deploy on push" aktivieren.

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

1. **Deploy** in Coolify auslösen (oder Push auf `main`).
2. Build dauert beim ersten Mal länger (Rust-Dependency-Kompilierung).
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
