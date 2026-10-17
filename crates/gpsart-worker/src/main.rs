use gpsart_config::{DatabaseConfig, init_tracing};
use gpsart_persistence::PostgresProbe;
use gpsart_worker::{WorkerDependencies, production::ProductionWorker, run_until_shutdown};
use std::{error::Error, path::PathBuf, time::Duration};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing("gpsart-worker")?;
    let config = DatabaseConfig::from_env()?;
    let graph_path = PathBuf::from(required_env("GPSART_ROUTING_GRAPH_PATH")?);
    let checksum_path = PathBuf::from(required_env("GPSART_ROUTING_GRAPH_SHA256_PATH")?);
    let worker =
        ProductionWorker::connect(config.database_url.expose(), &graph_path, &checksum_path)
            .await?;
    let dependencies = WorkerDependencies::postgres(
        PostgresProbe::new(
            config.database_url.expose().to_owned(),
            Duration::from_secs(2),
        ),
        worker,
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        let _ = shutdown_tx.send(true);
    });
    run_until_shutdown(dependencies, shutdown_rx).await?;
    tracing::info!("worker stopped");
    Ok(())
}

fn required_env(name: &'static str) -> Result<String, Box<dyn Error + Send + Sync>> {
    std::env::var(name)
        .map_err(|_| format!("required environment variable {name} is missing").into())
}
