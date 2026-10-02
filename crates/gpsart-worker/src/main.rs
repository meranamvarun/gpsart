use gpsart_config::{DatabaseConfig, init_tracing};
use gpsart_persistence::PostgresProbe;
use gpsart_worker::{WorkerDependencies, run_until_shutdown};
use std::{error::Error, time::Duration};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing("gpsart-worker")?;
    let config = DatabaseConfig::from_env()?;
    let dependencies = WorkerDependencies::postgres(PostgresProbe::new(
        config.database_url.expose().to_owned(),
        Duration::from_secs(2),
    ));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx.send(true);
    });
    run_until_shutdown(dependencies, shutdown_rx).await?;
    tracing::info!("worker stopped");
    Ok(())
}
