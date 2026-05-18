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

    tracing::info!(
        endpoint = %config.s3.endpoint,
        bucket = %config.s3.bucket,
        region = %config.s3.region,
        "S3-Konfiguration geladen"
    );
    probe_s3_dns(&config.s3.endpoint).await;
    let storage = Storage::new(&config.s3);

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

/// Einmaliger DNS-Selbsttest aus dem App-Container heraus: löst der
/// `S3_ENDPOINT`-Host auf? Macht das Netzwerk-Problem ohne SSH sichtbar.
async fn probe_s3_dns(endpoint: &str) {
    let hostport = endpoint
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');
    let (host, port) = match hostport.split_once(':') {
        Some((h, p)) => (h.to_string(), p.to_string()),
        None => (hostport.to_string(), "9000".to_string()),
    };
    match tokio::net::lookup_host(format!("{host}:{port}")).await {
        Ok(addrs) => {
            let ips: Vec<String> = addrs.map(|a| a.ip().to_string()).collect();
            tracing::info!(
                host = %host,
                ips = ?ips,
                "S3-DNS-Selbsttest OK"
            );
        }
        Err(e) => tracing::error!(
            host = %host,
            error = %e,
            "S3-DNS-Selbsttest FEHLGESCHLAGEN — App ist nicht im selben \
             Docker-Netzwerk wie MinIO bzw. Name nicht auflösbar"
        ),
    }
}
