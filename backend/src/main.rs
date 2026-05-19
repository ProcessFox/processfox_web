use std::sync::Arc;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use processfox_web::config::Config;
use processfox_web::storage::Storage;
use processfox_web::{build_app, db, AppState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let config = Config::from_env().context("Konfiguration ungültig")?;
    let port = config.port;

    let pool = db::connect(&config.database_url).await?;
    tracing::info!("Datenbank verbunden, Migrationen angewendet");

    // Storage-Wurzel sicherstellen (Coolify-Volume).
    std::fs::create_dir_all(&config.storage_dir)
        .with_context(|| format!("Storage-Verzeichnis nicht anlegbar: {}", config.storage_dir))?;
    tracing::info!(dir = %config.storage_dir, "Datei-Storage (lokales Volume)");
    let storage = Storage::new(&config.storage_dir);

    // Max. 10 Auth-Versuche pro IP in 5 Minuten.
    let ratelimit = Arc::new(processfox_web::ratelimit::RateLimiter::new(
        10,
        std::time::Duration::from_secs(300),
    ));

    let state = AppState {
        pool,
        storage,
        config: Arc::new(config),
        ratelimit,
        http: reqwest::Client::new(),
        ws: processfox_web::ws::WsHub::new(),
        cancels: Arc::new(std::sync::Mutex::new(std::collections::HashSet::new())),
    };

    let app = build_app(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Konnte nicht an {addr} binden"))?;
    tracing::info!("ProcessFox Web lauscht auf http://{addr}");

    // ConnectInfo<SocketAddr> wird für das Auth-Rate-Limit (Client-IP) gebraucht.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .await
    .context("Server abgebrochen")?;
    Ok(())
}
