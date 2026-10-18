use gpsart_api::auth::{SessionPolicy, run_cleanup_until_shutdown};
use gpsart_config::{AppConfig, ProductionRoutingConfig, init_tracing};
use gpsart_domain::starts::StartResolver;
use gpsart_persistence::jobs::PostgresJobCoordinator;
use gpsart_persistence::routes::PostgresRouteSnapshotStore;
use gpsart_persistence::{DependencyProbe, PostgresProbe};
use gpsart_persistence::{PostgresGuestSessionStore, PostgresResolvedStartStore};
use std::{error::Error, sync::Arc, time::Duration};
use tokio::sync::watch;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    init_tracing("gpsart-api")?;
    let config = AppConfig::from_env()?;
    let readiness = PostgresProbe::new(
        config.database_url.expose().to_owned(),
        Duration::from_secs(2),
    );
    readiness.check().await?;
    let routing = ProductionRoutingConfig::load(required_env("GPSART_ROUTING_MANIFEST")?)?;
    routing.validate_workload(&config.workload_limits)?;
    let provider = routing.network_provider()?;
    let coverage = routing.coverage().clone();
    let data_version = routing.map_data_version().to_owned();
    let start_store =
        Arc::new(PostgresResolvedStartStore::connect(config.database_url.expose()).await?);
    let starts = gpsart_api::ConfiguredStartService::with_store(
        StartResolver::new(coverage, provider, Duration::from_secs(300)),
        data_version,
        start_store,
    );
    let sessions =
        Arc::new(PostgresGuestSessionStore::connect(config.database_url.expose()).await?);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let cleanup_task = tokio::spawn(run_cleanup_until_shutdown(
        sessions.clone(),
        Duration::from_secs(30),
        shutdown_rx.clone(),
    ));
    let jobs = PostgresJobCoordinator::connect_with_limits(
        config.database_url.expose(),
        config.workload_limits.clone(),
    )
    .await?;
    let routes = PostgresRouteSnapshotStore::connect(config.database_url.expose()).await?;
    let public_origin = std::env::var("GPSART_PUBLIC_ORIGIN")?;
    let session_policy = SessionPolicy::new(
        public_origin,
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )?;
    let listener = tokio::net::TcpListener::bind(config.api_bind).await?;
    tracing::info!(bind = %config.api_bind, "API listening");
    let app = gpsart_api::app_with_all_services_and_routes_and_limits(
        readiness,
        starts,
        sessions,
        session_policy,
        jobs,
        routes,
        config.workload_limits,
    );
    let signal_tx = shutdown_tx.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown requested");
        let _ = signal_tx.send(true);
    });
    let mut server_shutdown = shutdown_rx;
    let server_result = axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            while !*server_shutdown.borrow() && server_shutdown.changed().await.is_ok() {}
        })
        .await;
    let _ = shutdown_tx.send(true);
    let _ = cleanup_task.await;
    server_result?;
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

fn required_env(name: &'static str) -> Result<String, Box<dyn Error + Send + Sync>> {
    std::env::var(name)
        .map_err(|_| format!("required environment variable {name} is missing").into())
}
