use gpsart_config::{DatabaseConfig, init_tracing};
use gpsart_persistence::{DependencyProbe, PostgresProbe};
use gpsart_worker::{WorkerDependencies, production::ProductionWorker, run_until_shutdown};
use std::{error::Error, path::PathBuf, time::Duration};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing("gpsart-worker")?;
    if std::env::args().nth(1).as_deref() == Some("--check") {
        connect_worker().await?;
        let config = DatabaseConfig::from_env()?;
        let probe = PostgresProbe::new(
            config.database_url.expose().to_owned(),
            Duration::from_secs(2),
        );
        probe.check().await?;
        return Ok(());
    }
    let worker = connect_worker().await?;
    let config = DatabaseConfig::from_env()?;
    let dependencies = WorkerDependencies::postgres(
        PostgresProbe::new(
            config.database_url.expose().to_owned(),
            Duration::from_secs(2),
        ),
        worker,
    );
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    run_until_shutdown(dependencies, shutdown_rx).await?;
    tracing::info!("worker stopped");
    Ok(())
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn connect_worker() -> Result<ProductionWorker, Box<dyn Error + Send + Sync>> {
    let config = DatabaseConfig::from_env()?;
    let graph_path = PathBuf::from(required_env("GPSART_ROUTING_GRAPH_PATH")?);
    let checksum_path = PathBuf::from(required_env("GPSART_ROUTING_GRAPH_SHA256_PATH")?);
    let coverage_policy_version = required_env("GPSART_COVERAGE_VERSION")?;
    let worker = ProductionWorker::connect(
        config.database_url.expose(),
        &graph_path,
        &checksum_path,
        &coverage_policy_version,
        config.workload_limits.clone(),
    )
    .await?;
    Ok(worker)
}

fn required_env(name: &'static str) -> Result<String, Box<dyn Error + Send + Sync>> {
    std::env::var(name)
        .map_err(|_| format!("required environment variable {name} is missing").into())
}
