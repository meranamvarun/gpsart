use gpsart_api::auth::SessionPolicy;
use gpsart_config::{AppConfig, init_tracing};
use gpsart_domain::starts::{
    ConfiguredNetworkPoint, ConfiguredNetworkProvider, Coverage, StartResolver,
};
use gpsart_persistence::PostgresGuestSessionStore;
use gpsart_persistence::jobs::PostgresJobCoordinator;
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
    let coverage_version = required_env("GPSART_COVERAGE_VERSION")?;
    let bounds = parse_bounds(&required_env("GPSART_COVERAGE_BOUNDS")?)?;
    let data_version = required_env("GPSART_MAP_DATA_VERSION")?;
    let network_points: Vec<ConfiguredNetworkPoint> =
        serde_json::from_str(&required_env("GPSART_ROUTABLE_STARTS_JSON")?)
            .map_err(|_| "GPSART_ROUTABLE_STARTS_JSON is invalid")?;
    let provider = ConfiguredNetworkProvider::new(network_points)
        .map_err(|_| "GPSART_ROUTABLE_STARTS_JSON must contain profiled points")?;
    let coverage =
        Coverage::rectangle(coverage_version, bounds[0], bounds[1], bounds[2], bounds[3])
            .map_err(|_| "GPSART_COVERAGE_BOUNDS is invalid")?;
    let starts = gpsart_api::ConfiguredStartService::new(
        StartResolver::new(coverage, provider, Duration::from_secs(300)),
        data_version,
    );
    let sessions =
        Arc::new(PostgresGuestSessionStore::connect(config.database_url.expose()).await?);
    let jobs = PostgresJobCoordinator::connect(config.database_url.expose()).await?;
    let public_origin = std::env::var("GPSART_PUBLIC_ORIGIN")?;
    let session_policy = SessionPolicy::new(
        public_origin,
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )?;
    let listener = tokio::net::TcpListener::bind(config.api_bind).await?;
    tracing::info!(bind = %config.api_bind, "API listening");
    gpsart_api::serve_with_all_services(
        listener,
        readiness,
        starts,
        sessions,
        session_policy,
        jobs,
        async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown requested");
        },
    )
    .await?;
    Ok(())
}

fn required_env(name: &'static str) -> Result<String, Box<dyn Error + Send + Sync>> {
    std::env::var(name)
        .map_err(|_| format!("required environment variable {name} is missing").into())
}

fn parse_bounds(value: &str) -> Result<[f64; 4], Box<dyn Error + Send + Sync>> {
    let values = value
        .split(',')
        .map(str::parse::<f64>)
        .collect::<Result<Vec<_>, _>>()?;
    values
        .try_into()
        .map_err(|_| "GPSART_COVERAGE_BOUNDS requires four coordinates".into())
}
