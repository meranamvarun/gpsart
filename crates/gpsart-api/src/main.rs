use gpsart_config::{AppConfig, init_tracing};
use gpsart_persistence::PostgresProbe;
use std::{error::Error, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing("gpsart-api")?;
    let config = AppConfig::from_env()?;
    let listener = tokio::net::TcpListener::bind(config.api_bind).await?;
    let readiness = PostgresProbe::new(
        config.database_url.expose().to_owned(),
        Duration::from_secs(2),
    );
    tracing::info!(bind = %config.api_bind, "API listening");
    gpsart_api::serve(listener, readiness, async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown requested");
    })
    .await?;
    Ok(())
}
