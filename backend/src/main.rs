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

    let storage = Storage::new(&config.s3);

    let state = AppState {
        pool,
        storage,
        config: Arc::new(config),
    };

    let app = build_app(state);

    let addr = format!("0.0.0.0:{port}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("Konnte nicht an {addr} binden"))?;
    tracing::info!("ProcessFox Web lauscht auf http://{addr}");

    axum::serve(listener, app)
        .await
        .context("Server abgebrochen")?;
    Ok(())
}
