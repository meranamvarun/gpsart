use std::{collections::HashMap, fmt, net::SocketAddr};
use thiserror::Error;
use tracing_subscriber::EnvFilter;

#[derive(Clone, Eq, PartialEq)]
pub struct SecretString(String);

impl SecretString {
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for SecretString {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("[REDACTED]")
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AppConfig {
    pub database_url: SecretString,
    pub api_bind: SocketAddr,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseConfig {
    pub database_url: SecretString,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_pairs(std::env::vars())
    }

    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let values: HashMap<String, String> = pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        let database_url = parse_database_url(&values)?;
        let api_bind = values
            .get("GPSART_API_BIND")
            .ok_or(ConfigError::Missing("GPSART_API_BIND"))?
            .parse()
            .map_err(|_| ConfigError::Invalid("GPSART_API_BIND"))?;
        Ok(Self {
            database_url,
            api_bind,
        })
    }
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_pairs(std::env::vars())
    }

    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        let values: HashMap<String, String> = pairs
            .into_iter()
            .map(|(key, value)| (key.into(), value.into()))
            .collect();
        Ok(Self {
            database_url: parse_database_url(&values)?,
        })
    }
}

fn parse_database_url(values: &HashMap<String, String>) -> Result<SecretString, ConfigError> {
    let database_url = values
        .get("GPSART_DATABASE_URL")
        .ok_or(ConfigError::Missing("GPSART_DATABASE_URL"))?;
    if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
        return Err(ConfigError::Invalid("GPSART_DATABASE_URL"));
    }
    Ok(SecretString(database_url.clone()))
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum ConfigError {
    #[error("required environment variable {0} is missing")]
    Missing(&'static str),
    #[error("environment variable {0} is invalid")]
    Invalid(&'static str),
}

pub fn init_tracing(service: &'static str) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt()
        .json()
        .with_target(false)
        .with_current_span(false)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .try_init()?;
    tracing::info!(service, "tracing initialized");
    Ok(())
}
