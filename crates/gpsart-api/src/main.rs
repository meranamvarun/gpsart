use gpsart_api::auth::SessionPolicy;
use gpsart_config::{AppConfig, init_tracing};
use gpsart_persistence::PostgresGuestSessionStore;
use gpsart_persistence::{DependencyProbe, PostgresProbe};
use std::{error::Error, sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing("gpsart-api")?;
    let config = AppConfig::from_env()?;
    let readiness = PostgresProbe::new(
        config.database_url.expose().to_owned(),
        Duration::from_secs(2),
    );
    readiness.check().await?;
    let sessions =
        Arc::new(PostgresGuestSessionStore::connect(config.database_url.expose()).await?);
    let public_origin = std::env::var("GPSART_PUBLIC_ORIGIN")?;
    let session_policy = SessionPolicy::new(
        public_origin,
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )?;
    let listener = tokio::net::TcpListener::bind(config.api_bind).await?;
    tracing::info!(bind = %config.api_bind, "API listening");
    gpsart_api::serve_with_auth(listener, readiness, sessions, session_policy, async {
        let _ = tokio::signal::ctrl_c().await;
        tracing::info!("shutdown requested");
    })
    .await?;
    Ok(())
}
