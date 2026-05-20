//! HTTP/DB-Integrationstests (CLAUDE.md §13 „Härtung").
//!
//! Jeder Test bekommt von `#[sqlx::test]` eine **frische Wegwerf-Postgres-DB**
//! (Migrationen aus `./src/db/migrations` automatisch angewendet). Die echten
//! Axum-Handler werden via `tower::ServiceExt::oneshot` aufgerufen — kein Port,
//! kein laufender Server. Fokus: Auth (Magic-Link verify, Refresh-Rotation)
//! und die sicherheitskritischen Workspace-Berechtigungen (`perm.rs`).
//!
//! Voraussetzung: erreichbare Postgres-Instanz via `DATABASE_URL` (in CI ein
//! Service-Container; lokal z. B. `docker run -e POSTGRES_PASSWORD=postgres
//! -p 5432:5432 postgres:16`). Ohne DB werden diese Tests nicht ausgeführt.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{header, Request, StatusCode};
use serde_json::{json, Value};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

use processfox_web::auth::encode_access_token;
use processfox_web::config::Config;
use processfox_web::ratelimit::RateLimiter;
use processfox_web::storage::Storage;
use processfox_web::ws::WsHub;
use processfox_web::{build_app, AppState};

const JWT_SECRET: &str = "test-jwt-secret-at-least-32-bytes-long!!";

fn test_state(pool: PgPool) -> AppState {
    let config = Config {
        database_url: String::new(),
        storage_dir: std::env::temp_dir().to_string_lossy().into_owned(),
        jwt_secret: JWT_SECRET.to_string(),
        api_key_encryption_key: [7u8; 32],
        port: 0,
        static_dir: "/nonexistent-static".to_string(),
        public_base_url: "http://localhost".to_string(),
        // Bewusst tot: der Webhook-Versand schlägt fehl, wird geloggt und
        // bricht die Anfrage nicht ab (kein Enumeration-Signal). Tests, die
        // den Magic-Link brauchen, schreiben das login_token direkt in die DB.
        magic_link_webhook_url: "http://127.0.0.1:9/unreachable".to_string(),
        magic_link_webhook_secret: None,
    };
    AppState {
        pool,
        storage: Storage::new(&config.storage_dir),
        config: Arc::new(config),
        // Hoch genug, dass das Rate-Limit die Tests nicht stört.
        ratelimit: Arc::new(RateLimiter::new(10_000, Duration::from_secs(300))),
        http: reqwest::Client::new(),
        ws: WsHub::new(),
        cancels: Arc::new(Mutex::new(HashSet::new())),
        active_runs: Arc::new(Mutex::new(HashMap::new())),
        pending_hitl: Arc::new(Mutex::new(HashMap::new())),
        pending_questions: Arc::new(Mutex::new(HashMap::new())),
    }
}

/// Antwort eines Handler-Aufrufs: Status, evtl. `pfx_refresh`-Cookie und
/// der geparste JSON-Body (oder `Null` bei leerem Body).
struct Resp {
    status: StatusCode,
    refresh_cookie: Option<String>,
    body: Value,
}

/// Schickt eine Anfrage durch die echte App.
async fn call(
    pool: &PgPool,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    cookie: Option<&str>,
    body: Option<Value>,
) -> Resp {
    let app = build_app(test_state(pool.clone()));

    let mut req = Request::builder()
        .method(method)
        .uri(path)
        // `request_login`/`register` brauchen ConnectInfo (Rate-Limit-Key).
        .extension(ConnectInfo(SocketAddr::from(([127, 0, 0, 1], 4242))));
    if let Some(b) = bearer {
        req = req.header(header::AUTHORIZATION, format!("Bearer {b}"));
    }
    if let Some(c) = cookie {
        req = req.header(header::COOKIE, c);
    }
    let req = if let Some(j) = body {
        req.header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(serde_json::to_vec(&j).unwrap()))
            .unwrap()
    } else {
        req.body(Body::empty()).unwrap()
    };

    let res = app.oneshot(req).await.expect("router infallible");
    let status = res.status();
    let refresh_cookie = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|s| s.starts_with("pfx_refresh="))
        .map(|s| s.split(';').next().unwrap_or(s).to_string());

    let bytes = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    Resp {
        status,
        refresh_cookie,
        body,
    }
}

// --- DB-Seed-Helfer -------------------------------------------------------

async fn seed_org(pool: &PgPool, name: &str, invite: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO organizations (name, invite_code) VALUES ($1, $2) RETURNING id",
    )
    .bind(name)
    .bind(invite)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_user(pool: &PgPool, org_id: Uuid, email: &str, role: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO users (email, org_id, org_role) VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(email)
    .bind(org_id)
    .bind(role)
    .fetch_one(pool)
    .await
    .unwrap()
}

fn bearer_for(user_id: Uuid, org_id: Uuid, role: &str) -> String {
    encode_access_token(JWT_SECRET, user_id, org_id, role).unwrap()
}

// --- Tests: Verdrahtung & Auth-Guard --------------------------------------

#[sqlx::test(migrations = "./src/db/migrations")]
async fn health_is_public(pool: PgPool) {
    let r = call(&pool, "GET", "/api/v1/health", None, None, None).await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body, json!({ "status": "ok" }));
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn workspaces_require_authentication(pool: PgPool) {
    let r = call(&pool, "GET", "/api/v1/workspaces", None, None, None).await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn garbage_bearer_is_rejected(pool: PgPool) {
    let r = call(
        &pool,
        "GET",
        "/api/v1/workspaces",
        Some("not-a-jwt"),
        None,
        None,
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

// --- Tests: Magic-Link verify & Refresh-Rotation --------------------------

/// Schreibt ein gültiges Login-Magic-Link-Token in die DB und gibt den
/// Klartext zurück (der Webhook-Versand wird im Test umgangen).
async fn insert_login_token(pool: &PgPool, email: &str, valid: bool) -> String {
    let (raw, hash) = {
        // Eigene zufällige Zeichenkette + Hash über die echte Funktion.
        let raw = format!("tok-{}", Uuid::new_v4());
        (raw.clone(), processfox_web::auth::hash_token(&raw))
    };
    let expires = if valid {
        "now() + interval '15 minutes'"
    } else {
        "now() - interval '1 minute'"
    };
    sqlx::query(&format!(
        "INSERT INTO login_tokens (email, purpose, org_id, token_hash, expires_at) \
         VALUES ($1, 'login', NULL, $2, {expires})"
    ))
    .bind(email)
    .bind(&hash)
    .execute(pool)
    .await
    .unwrap();
    raw
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn verify_issues_session_and_refresh_cookie(pool: PgPool) {
    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let raw = insert_login_token(&pool, "owner@acme.test", true).await;

    let r = call(
        &pool,
        "POST",
        "/api/v1/auth/verify",
        None,
        None,
        Some(json!({ "token": raw })),
    )
    .await;

    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["user"]["email"], "owner@acme.test");
    assert_eq!(r.body["user"]["orgRole"], "owner");
    assert!(r.body["accessToken"].as_str().is_some());
    assert!(
        r.refresh_cookie.is_some(),
        "verify muss ein pfx_refresh-Cookie setzen"
    );

    // Das ausgegebene Access-Token öffnet authentifizierte Endpunkte.
    let token = r.body["accessToken"].as_str().unwrap().to_string();
    let ws = call(&pool, "GET", "/api/v1/workspaces", Some(&token), None, None).await;
    assert_eq!(ws.status, StatusCode::OK);
    assert_eq!(ws.body, json!([]));
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn verify_rejects_unknown_token(pool: PgPool) {
    let r = call(
        &pool,
        "POST",
        "/api/v1/auth/verify",
        None,
        None,
        Some(json!({ "token": "does-not-exist" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn verify_rejects_expired_token(pool: PgPool) {
    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let raw = insert_login_token(&pool, "owner@acme.test", false).await;

    let r = call(
        &pool,
        "POST",
        "/api/v1/auth/verify",
        None,
        None,
        Some(json!({ "token": raw })),
    )
    .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn verify_token_is_single_use(pool: PgPool) {
    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let raw = insert_login_token(&pool, "owner@acme.test", true).await;

    let body = json!({ "token": raw });
    let first = call(
        &pool,
        "POST",
        "/api/v1/auth/verify",
        None,
        None,
        Some(body.clone()),
    )
    .await;
    assert_eq!(first.status, StatusCode::OK);

    let second = call(&pool, "POST", "/api/v1/auth/verify", None, None, Some(body)).await;
    assert_eq!(
        second.status,
        StatusCode::UNAUTHORIZED,
        "ein konsumiertes Token darf nicht erneut funktionieren"
    );
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn refresh_rotates_and_revokes_old_cookie(pool: PgPool) {
    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let raw = insert_login_token(&pool, "owner@acme.test", true).await;

    let login = call(
        &pool,
        "POST",
        "/api/v1/auth/verify",
        None,
        None,
        Some(json!({ "token": raw })),
    )
    .await;
    let old_cookie = login.refresh_cookie.expect("verify set cookie");

    // Mit dem Cookie refreshen → neues Token + neues Cookie.
    let r1 = call(
        &pool,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(&old_cookie),
        None,
    )
    .await;
    assert_eq!(r1.status, StatusCode::OK);
    assert!(r1.body["accessToken"].as_str().is_some());
    let new_cookie = r1.refresh_cookie.expect("refresh rotates cookie");
    assert_ne!(old_cookie, new_cookie, "Refresh-Token muss rotieren");

    // Das alte (rotierte) Cookie ist jetzt widerrufen.
    let reuse = call(
        &pool,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(&old_cookie),
        None,
    )
    .await;
    assert_eq!(
        reuse.status,
        StatusCode::UNAUTHORIZED,
        "ein rotiertes Refresh-Token darf nicht wiederverwendbar sein"
    );

    // Das neue Cookie funktioniert weiter.
    let ok = call(
        &pool,
        "POST",
        "/api/v1/auth/refresh",
        None,
        Some(&new_cookie),
        None,
    )
    .await;
    assert_eq!(ok.status, StatusCode::OK);
}

// --- Tests: Workspace-Berechtigungen (perm.rs) ----------------------------

#[sqlx::test(migrations = "./src/db/migrations")]
async fn owner_creates_workspace_member_cannot(pool: PgPool) {
    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let owner = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let member = seed_user(&pool, org, "member@acme.test", "member").await;

    let as_owner = bearer_for(owner, org, "owner");
    let created = call(
        &pool,
        "POST",
        "/api/v1/workspaces",
        Some(&as_owner),
        None,
        Some(json!({ "name": "Projekt A" })),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    assert_eq!(created.body["name"], "Projekt A");

    let as_member = bearer_for(member, org, "member");
    let denied = call(
        &pool,
        "POST",
        "/api/v1/workspaces",
        Some(&as_member),
        None,
        Some(json!({ "name": "Heimlich" })),
    )
    .await;
    assert_eq!(
        denied.status,
        StatusCode::FORBIDDEN,
        "Nur der Org-Owner darf Workspaces anlegen"
    );
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn foreign_org_workspace_is_not_found_no_leak(pool: PgPool) {
    // Org A legt einen Workspace an.
    let org_a = seed_org(&pool, "Acme", "AAAA23").await;
    let owner_a = seed_user(&pool, org_a, "a@acme.test", "owner").await;
    let token_a = bearer_for(owner_a, org_a, "owner");
    let ws = call(
        &pool,
        "POST",
        "/api/v1/workspaces",
        Some(&token_a),
        None,
        Some(json!({ "name": "Geheim A" })),
    )
    .await;
    let ws_id = ws.body["id"].as_str().unwrap().to_string();

    // Ein Owner aus Org B fragt diesen Workspace ab → 404 (kein 403, das
    // würde die Existenz verraten).
    let org_b = seed_org(&pool, "Globex", "BBBB23").await;
    let owner_b = seed_user(&pool, org_b, "b@globex.test", "owner").await;
    let token_b = bearer_for(owner_b, org_b, "owner");
    let probe = call(
        &pool,
        "GET",
        &format!("/api/v1/workspaces/{ws_id}/members"),
        Some(&token_b),
        None,
        None,
    )
    .await;
    assert_eq!(
        probe.status,
        StatusCode::NOT_FOUND,
        "fremder Workspace muss als nicht gefunden erscheinen (kein Leak)"
    );

    // Und er taucht auch nicht in seiner Workspace-Liste auf.
    let list_b = call(
        &pool,
        "GET",
        "/api/v1/workspaces",
        Some(&token_b),
        None,
        None,
    )
    .await;
    assert_eq!(list_b.body, json!([]));
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn viewer_can_read_but_not_perform_owner_actions(pool: PgPool) {
    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let owner = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let viewer = seed_user(&pool, org, "viewer@acme.test", "member").await;
    let token_owner = bearer_for(owner, org, "owner");
    let token_viewer = bearer_for(viewer, org, "member");

    // Owner: Workspace anlegen und Viewer als `viewer` aufnehmen.
    let ws = call(
        &pool,
        "POST",
        "/api/v1/workspaces",
        Some(&token_owner),
        None,
        Some(json!({ "name": "Projekt" })),
    )
    .await;
    let ws_id = ws.body["id"].as_str().unwrap().to_string();

    let add = call(
        &pool,
        "POST",
        &format!("/api/v1/workspaces/{ws_id}/members"),
        Some(&token_owner),
        None,
        Some(json!({ "email": "viewer@acme.test", "role": "viewer" })),
    )
    .await;
    assert_eq!(add.status, StatusCode::NO_CONTENT);

    // Viewer darf Mitglieder lesen …
    let read = call(
        &pool,
        "GET",
        &format!("/api/v1/workspaces/{ws_id}/members"),
        Some(&token_viewer),
        None,
        None,
    )
    .await;
    assert_eq!(read.status, StatusCode::OK);
    assert_eq!(read.body.as_array().map(|a| a.len()), Some(1));

    // … aber keinen Owner-Vorbehalt ausführen (Mitglied hinzufügen).
    let forbidden = call(
        &pool,
        "POST",
        &format!("/api/v1/workspaces/{ws_id}/members"),
        Some(&token_viewer),
        None,
        Some(json!({ "email": "owner@acme.test", "role": "editor" })),
    )
    .await;
    assert_eq!(forbidden.status, StatusCode::FORBIDDEN);

    // … und keinen Workspace anlegen.
    let no_create = call(
        &pool,
        "POST",
        "/api/v1/workspaces",
        Some(&token_viewer),
        None,
        Some(json!({ "name": "Nope" })),
    )
    .await;
    assert_eq!(no_create.status, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn request_login_for_unknown_email_creates_no_token(pool: PgPool) {
    // Kein User mit dieser Adresse → generische 200, aber kein Token in der
    // DB (keine Account-Enumeration, kein Webhook-Aufruf).
    let r = call(
        &pool,
        "POST",
        "/api/v1/auth/request-login",
        None,
        None,
        Some(json!({ "email": "nobody@nowhere.test" })),
    )
    .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(r.body["ok"], true);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM login_tokens")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "für unbekannte E-Mail darf kein Token entstehen");
}

// --- Tests: grep_in_files (Phase 6b-2h) -----------------------------------
//
// Diese Tests rufen `tools::execute_read_tool` direkt auf — der Tool-Loop
// im Chat tut dasselbe und verlässt sich für Read-Tools auf identische
// Semantik (kein HITL, kein Broadcast).

/// AppState mit eigenem, eindeutigem Storage-Verzeichnis pro Test, damit
/// sich Volume-Inhalte zwischen parallel laufenden Tests nicht mischen.
fn tool_state(pool: PgPool) -> (AppState, PathBuf) {
    let dir = std::env::temp_dir().join(format!("pfx-grep-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut state = test_state(pool);
    state.storage = Storage::new(&dir.to_string_lossy());
    (state, dir)
}

/// Legt einen Workspace + Volume-Bytes + `workspace_files`-Zeile an.
async fn seed_file(
    pool: &PgPool,
    storage_root: &Path,
    workspace_id: Uuid,
    uploaded_by: Uuid,
    filename: &str,
    bytes: &[u8],
) {
    let key = format!("workspaces/{workspace_id}/{filename}");
    let path = storage_root.join(&key);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, bytes).unwrap();
    sqlx::query(
        "INSERT INTO workspace_files \
         (workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by) \
         VALUES ($1, $2, $3, $4, 'text/plain', $5)",
    )
    .bind(workspace_id)
    .bind(filename)
    .bind(&key)
    .bind(bytes.len() as i64)
    .bind(uploaded_by)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_workspace(pool: &PgPool, org_id: Uuid, name: &str) -> Uuid {
    sqlx::query_scalar::<_, Uuid>(
        "INSERT INTO workspaces (org_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn grep_finds_hits_with_path_and_line(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, GREP_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    seed_file(
        &pool,
        &root,
        ws,
        user,
        "notes.md",
        b"alpha bravo\nFoo line two\ncharlie\n",
    )
    .await;
    seed_file(
        &pool,
        &root,
        ws,
        user,
        "more.txt",
        b"unrelated\nanother foo here\n",
    )
    .await;

    let out = execute_read_tool(&state, ws, GREP_TOOL, &json!({ "pattern": "foo" }))
        .await
        .unwrap();
    // Case-insensitive Default: matched „Foo" und „foo".
    assert!(out.contains("notes.md:2: Foo line two"), "{out}");
    assert!(out.contains("more.txt:2: another foo here"), "{out}");
    assert!(out.starts_with("2 Treffer"), "{out}");

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn grep_respects_case_sensitive_flag(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, GREP_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    seed_file(&pool, &root, ws, user, "notes.md", b"Foo\nfoo\nFOO\n").await;

    let ci = execute_read_tool(&state, ws, GREP_TOOL, &json!({ "pattern": "foo" }))
        .await
        .unwrap();
    assert!(ci.starts_with("3 Treffer"), "{ci}");

    let cs = execute_read_tool(
        &state,
        ws,
        GREP_TOOL,
        &json!({ "pattern": "foo", "caseSensitive": true }),
    )
    .await
    .unwrap();
    assert!(cs.starts_with("1 Treffer"), "{cs}");
    assert!(cs.contains("notes.md:2: foo"), "{cs}");

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn grep_skips_non_whitelisted_extensions(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, GREP_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    // `.bin` ist nicht in der Whitelist — auch wenn da Text drin steht, wird
    // sie übersprungen. `.md` mit identischem Inhalt liefert den Treffer.
    seed_file(&pool, &root, ws, user, "data.bin", b"needle in haystack\n").await;
    seed_file(&pool, &root, ws, user, "ok.md", b"needle in haystack\n").await;

    let out = execute_read_tool(&state, ws, GREP_TOOL, &json!({ "pattern": "needle" }))
        .await
        .unwrap();
    assert!(out.starts_with("1 Treffer"), "{out}");
    assert!(out.contains("ok.md:1:"), "{out}");
    assert!(!out.contains("data.bin"), "{out}");

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn grep_does_not_leak_across_workspaces(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, GREP_TOOL};

    let org_a = seed_org(&pool, "Acme", "AAAA23").await;
    let user_a = seed_user(&pool, org_a, "a@acme.test", "owner").await;
    let ws_a = seed_workspace(&pool, org_a, "A").await;

    let org_b = seed_org(&pool, "Globex", "BBBB23").await;
    let user_b = seed_user(&pool, org_b, "b@globex.test", "owner").await;
    let ws_b = seed_workspace(&pool, org_b, "B").await;

    let (state, root) = tool_state(pool.clone());
    seed_file(
        &pool,
        &root,
        ws_a,
        user_a,
        "secret.md",
        b"alpha needle omega\n",
    )
    .await;
    // ws_b enthält das Wort nicht.
    seed_file(&pool, &root, ws_b, user_b, "other.md", b"nothing to see\n").await;

    let probe = execute_read_tool(&state, ws_b, GREP_TOOL, &json!({ "pattern": "needle" }))
        .await
        .unwrap();
    assert!(probe.starts_with("Keine Treffer"), "{probe}");
    assert!(!probe.contains("secret.md"), "{probe}");

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn grep_rejects_invalid_regex(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, GREP_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, _root) = tool_state(pool.clone());

    let err = execute_read_tool(&state, ws, GREP_TOOL, &json!({ "pattern": "(" }))
        .await
        .unwrap_err();
    assert!(
        matches!(err, processfox_web::error::ApiError::BadRequest(_)),
        "ungültiges Regex muss 400 werden, war: {err:?}"
    );
}

// --- Tests: read_pdf (Phase 6b-2i) ----------------------------------------

/// Baut eine minimale, gültige PDF-1.4-Datei mit genau einem Text-Run
/// (Helvetica, eine der 14 Standardschriften — `pdf-extract` braucht kein
/// eingebettetes Fontfile). Wird zur Laufzeit zusammengesetzt, damit wir
/// keine Binär-Fixture im Repo brauchen. xref-Offsets werden mitgezählt
/// und stimmen daher per Konstruktion.
fn make_minimal_pdf(text: &str) -> Vec<u8> {
    // Content-Stream: PDF-Operatoren BT/ET umschließen Text; Td positioniert,
    // Tj zeichnet. `(text)` ist die Literal-String-Syntax in PDF.
    let stream = format!("BT /F1 24 Tf 72 720 Td ({text}) Tj ET\n");
    let stream_bytes = stream.as_bytes();
    let mut out = Vec::<u8>::new();
    let mut offsets = [0usize; 6]; // 0 = freie Liste, 1..=5 = Objekte
    out.extend_from_slice(b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n");
    offsets[1] = out.len();
    out.extend_from_slice(b"1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n");
    offsets[2] = out.len();
    out.extend_from_slice(b"2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n");
    offsets[3] = out.len();
    out.extend_from_slice(
        b"3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
          /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>\nendobj\n",
    );
    offsets[4] = out.len();
    out.extend_from_slice(
        b"4 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n",
    );
    offsets[5] = out.len();
    let header = format!("5 0 obj\n<< /Length {} >>\nstream\n", stream_bytes.len());
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(stream_bytes);
    out.extend_from_slice(b"endstream\nendobj\n");
    let xref_offset = out.len();
    out.extend_from_slice(b"xref\n0 6\n");
    out.extend_from_slice(b"0000000000 65535 f \n");
    for off in offsets.iter().skip(1) {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes(),
    );
    out
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_pdf_extracts_text_with_header(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_PDF_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    let bytes = make_minimal_pdf("ProcessFox PDF Test");
    seed_file(&pool, &root, ws, user, "report.pdf", &bytes).await;

    let out = execute_read_tool(
        &state,
        ws,
        READ_PDF_TOOL,
        &json!({ "filename": "report.pdf" }),
    )
    .await
    .unwrap();

    assert!(out.starts_with("--- report.pdf ("), "{out}");
    assert!(
        out.contains("ProcessFox PDF Test"),
        "Text nicht extrahiert: {out}"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_pdf_rejects_non_pdf_extension(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_PDF_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, _root) = tool_state(pool.clone());

    let err = execute_read_tool(
        &state,
        ws,
        READ_PDF_TOOL,
        &json!({ "filename": "notes.txt" }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, processfox_web::error::ApiError::BadRequest(_)),
        "falsche Endung muss 400 werden: {err:?}"
    );
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_pdf_missing_file_is_friendly(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_PDF_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, _root) = tool_state(pool.clone());

    // Kein Seed → DB-Zeile fehlt → freundlicher Text statt 500.
    let out = execute_read_tool(
        &state,
        ws,
        READ_PDF_TOOL,
        &json!({ "filename": "missing.pdf" }),
    )
    .await
    .unwrap();
    assert!(out.contains("nicht gefunden"), "{out}");
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_pdf_does_not_leak_across_workspaces(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_PDF_TOOL};

    let org_a = seed_org(&pool, "Acme", "AAAA23").await;
    let user_a = seed_user(&pool, org_a, "a@acme.test", "owner").await;
    let ws_a = seed_workspace(&pool, org_a, "A").await;

    let org_b = seed_org(&pool, "Globex", "BBBB23").await;
    seed_user(&pool, org_b, "b@globex.test", "owner").await;
    let ws_b = seed_workspace(&pool, org_b, "B").await;

    let (state, root) = tool_state(pool.clone());
    let bytes = make_minimal_pdf("Acme Secret Memo");
    seed_file(&pool, &root, ws_a, user_a, "secret.pdf", &bytes).await;

    // ws_b kennt die Datei nicht (gleicher Volumes-Root, aber andere
    // workspace_id → DB-Lookup leer).
    let probe = execute_read_tool(
        &state,
        ws_b,
        READ_PDF_TOOL,
        &json!({ "filename": "secret.pdf" }),
    )
    .await
    .unwrap();
    assert!(probe.contains("nicht gefunden"), "{probe}");
    assert!(!probe.contains("Acme Secret Memo"), "{probe}");

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_pdf_input_cap_rejects_huge_files(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_PDF_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    // DB-Zeile mit über 20 MiB. Volume bleibt leer — der Cap-Check feuert
    // vor dem Lesen.
    let key = format!("workspaces/{ws}/huge.pdf");
    sqlx::query(
        "INSERT INTO workspace_files \
         (workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by) \
         VALUES ($1, 'huge.pdf', $2, $3, 'application/pdf', $4)",
    )
    .bind(ws)
    .bind(&key)
    .bind(30_i64 * 1024 * 1024)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();

    let err = execute_read_tool(
        &state,
        ws,
        READ_PDF_TOOL,
        &json!({ "filename": "huge.pdf" }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, processfox_web::error::ApiError::BadRequest(_)),
        "über dem Cap muss 400 werden: {err:?}"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_pdf_broken_bytes_yield_friendly_result(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_PDF_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    // Nicht-PDF-Bytes mit `.pdf`-Endung — `pdf-extract` schlägt fehl,
    // aber der Tool-Loop bekommt eine freundliche Meldung statt 500.
    seed_file(
        &pool,
        &root,
        ws,
        user,
        "broken.pdf",
        b"definitiv kein PDF\n",
    )
    .await;

    let out = execute_read_tool(
        &state,
        ws,
        READ_PDF_TOOL,
        &json!({ "filename": "broken.pdf" }),
    )
    .await
    .unwrap();
    // Beide Wege zulässig: extract liefert leeren String → „leere
    // Extraktion …" oder einen Parse-Fehler → „PDF konnte nicht gelesen
    // werden …".
    assert!(
        out.contains("leere Extraktion") || out.contains("konnte nicht gelesen werden"),
        "kaputtes PDF muss freundlich beendet werden: {out}"
    );

    std::fs::remove_dir_all(&root).ok();
}

// --- Tests: read_docx (Phase 6b-2j) ---------------------------------------

/// Baut eine minimale, gültige `.docx` mit den gegebenen Absätzen — ZIP
/// mit `[Content_Types].xml`, `_rels/.rels`, `word/document.xml`. Wir
/// nutzen das Backend nicht, weil die Tests an `tools.rs` privat sind;
/// stattdessen die Struktur direkt selbst zusammenbauen. Klein genug,
/// um inline zu leben.
fn make_minimal_docx(paragraphs: &[&str]) -> Vec<u8> {
    use std::io::Write;
    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
    }
    let body: String = paragraphs
        .iter()
        .map(|p| {
            format!(
                "<w:p><w:r><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
                xml_escape(p)
            )
        })
        .collect();
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">\
<w:body>{body}<w:sectPr/></w:body></w:document>"
    );
    const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">\
<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/>\
<Default Extension=\"xml\" ContentType=\"application/xml\"/>\
<Override PartName=\"/word/document.xml\" \
ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/>\
</Types>";
    const RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\
<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">\
<Relationship Id=\"rId1\" \
Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" \
Target=\"word/document.xml\"/>\
</Relationships>";
    let mut buf = Vec::<u8>::new();
    {
        let cursor = std::io::Cursor::new(&mut buf);
        let mut zw = zip::ZipWriter::new(cursor);
        let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        zw.start_file("[Content_Types].xml", opts).unwrap();
        zw.write_all(CONTENT_TYPES.as_bytes()).unwrap();
        zw.start_file("_rels/.rels", opts).unwrap();
        zw.write_all(RELS.as_bytes()).unwrap();
        zw.start_file("word/document.xml", opts).unwrap();
        zw.write_all(document.as_bytes()).unwrap();
        zw.finish().unwrap();
    }
    buf
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_docx_extracts_paragraphs(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_DOCX_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    let bytes = make_minimal_docx(&["Hallo Welt", "Zweite Zeile"]);
    seed_file(&pool, &root, ws, user, "doc.docx", &bytes).await;

    let out = execute_read_tool(
        &state,
        ws,
        READ_DOCX_TOOL,
        &json!({ "filename": "doc.docx" }),
    )
    .await
    .unwrap();

    assert!(out.starts_with("--- doc.docx ("), "{out}");
    assert!(out.contains("Hallo Welt"), "{out}");
    assert!(out.contains("Zweite Zeile"), "{out}");
    assert!(
        out.contains("Hallo Welt\n\nZweite Zeile"),
        "Paragraph-Trennung fehlt: {out}"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_docx_rejects_non_docx_extension(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_DOCX_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, _root) = tool_state(pool.clone());

    let err = execute_read_tool(
        &state,
        ws,
        READ_DOCX_TOOL,
        &json!({ "filename": "notes.txt" }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, processfox_web::error::ApiError::BadRequest(_)),
        "falsche Endung muss 400 werden: {err:?}"
    );
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_docx_missing_file_is_friendly(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_DOCX_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, _root) = tool_state(pool.clone());

    let out = execute_read_tool(
        &state,
        ws,
        READ_DOCX_TOOL,
        &json!({ "filename": "missing.docx" }),
    )
    .await
    .unwrap();
    assert!(out.contains("nicht gefunden"), "{out}");
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_docx_does_not_leak_across_workspaces(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_DOCX_TOOL};

    let org_a = seed_org(&pool, "Acme", "AAAA23").await;
    let user_a = seed_user(&pool, org_a, "a@acme.test", "owner").await;
    let ws_a = seed_workspace(&pool, org_a, "A").await;

    let org_b = seed_org(&pool, "Globex", "BBBB23").await;
    seed_user(&pool, org_b, "b@globex.test", "owner").await;
    let ws_b = seed_workspace(&pool, org_b, "B").await;

    let (state, root) = tool_state(pool.clone());
    let bytes = make_minimal_docx(&["Acme Top Secret"]);
    seed_file(&pool, &root, ws_a, user_a, "secret.docx", &bytes).await;

    let probe = execute_read_tool(
        &state,
        ws_b,
        READ_DOCX_TOOL,
        &json!({ "filename": "secret.docx" }),
    )
    .await
    .unwrap();
    assert!(probe.contains("nicht gefunden"), "{probe}");
    assert!(!probe.contains("Acme Top Secret"), "{probe}");

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_docx_input_cap_rejects_huge_files(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_DOCX_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    // DB-Zeile mit über 20 MiB; Volume bleibt leer — der Cap feuert vor
    // dem Lesen.
    let key = format!("workspaces/{ws}/huge.docx");
    sqlx::query(
        "INSERT INTO workspace_files \
         (workspace_id, filename, s3_key, size_bytes, content_type, uploaded_by) \
         VALUES ($1, 'huge.docx', $2, $3, \
           'application/vnd.openxmlformats-officedocument.wordprocessingml.document', $4)",
    )
    .bind(ws)
    .bind(&key)
    .bind(30_i64 * 1024 * 1024)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();

    let err = execute_read_tool(
        &state,
        ws,
        READ_DOCX_TOOL,
        &json!({ "filename": "huge.docx" }),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, processfox_web::error::ApiError::BadRequest(_)),
        "über dem Cap muss 400 werden: {err:?}"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn read_docx_broken_bytes_yield_friendly_result(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, READ_DOCX_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    // Nicht-ZIP-Bytes mit `.docx`-Endung — der Tool-Loop bekommt eine
    // freundliche Meldung statt 500.
    seed_file(
        &pool,
        &root,
        ws,
        user,
        "broken.docx",
        b"definitiv kein DOCX\n",
    )
    .await;

    let out = execute_read_tool(
        &state,
        ws,
        READ_DOCX_TOOL,
        &json!({ "filename": "broken.docx" }),
    )
    .await
    .unwrap();
    assert!(
        out.contains("konnte nicht gelesen werden"),
        "kaputtes DOCX muss freundlich beendet werden: {out}"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[sqlx::test(migrations = "./src/db/migrations")]
async fn grep_caps_hits_at_one_hundred(pool: PgPool) {
    use processfox_web::tools::{execute_read_tool, GREP_TOOL};

    let org = seed_org(&pool, "Acme", "ABCD23").await;
    let user = seed_user(&pool, org, "owner@acme.test", "owner").await;
    let ws = seed_workspace(&pool, org, "Projekt").await;
    let (state, root) = tool_state(pool.clone());

    // 150 Trefferzeilen → Tool kappt bei 100 und meldet den Cap.
    let body: String = (0..150).map(|i| format!("hit {i}\n")).collect();
    seed_file(&pool, &root, ws, user, "many.md", body.as_bytes()).await;

    let out = execute_read_tool(&state, ws, GREP_TOOL, &json!({ "pattern": "hit" }))
        .await
        .unwrap();
    assert!(out.starts_with("100 Treffer"), "{out}");
    assert!(out.contains("[Trefferlimit erreicht"), "{out}");

    std::fs::remove_dir_all(&root).ok();
}
