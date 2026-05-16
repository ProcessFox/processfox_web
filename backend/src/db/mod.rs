//! Postgres-Pool + Migrations. `sqlx::migrate!` bettet die SQL-Dateien zur
//! Compile-Zeit ein (keine DB-Verbindung beim Bauen nötig) und wird beim
//! Start gegen die Ziel-DB ausgeführt (CLAUDE.md §8).

use anyhow::{Context, Result};
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn connect(database_url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .context("Verbindung zur Datenbank fehlgeschlagen")?;

    sqlx::migrate!("./src/db/migrations")
        .run(&pool)
        .await
        .context("Datenbank-Migration fehlgeschlagen")?;

    Ok(pool)
}
