use gpsart_persistence::{apply_migrations, run_migrations};
use sqlx::{PgPool, postgres::PgConnectOptions};

#[tokio::main]
async fn main() {
    if let Err(error) = migrate().await {
        eprintln!("database migration failed: {error}");
        std::process::exit(1);
    }
}

async fn migrate() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(database_url) = std::env::var("DATABASE_URL") {
        apply_migrations(&database_url).await?;
        return Ok(());
    }
    let options = PgConnectOptions::new()
        .host(&required("PGHOST")?)
        .port(
            std::env::var("PGPORT")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(5432),
        )
        .username(&required("PGUSER")?)
        .password(&required("PGPASSWORD")?)
        .database(&required("PGDATABASE")?);
    let pool = PgPool::connect_with(options).await?;
    run_migrations(&pool).await?;
    pool.close().await;
    Ok(())
}

fn required(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|_| format!("{name} must be set").into())
}
