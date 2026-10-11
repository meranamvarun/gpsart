use std::{collections::HashMap, fmt, net::SocketAddr, time::Duration};
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
    pub workload_limits: WorkloadLimits,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DatabaseConfig {
    pub database_url: SecretString,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkloadLimits {
    pub allowed_tolerance_percent: [u8; 3],
    pub max_distance_metres: i32,
    pub max_template_points: usize,
    pub max_anchors: usize,
    pub max_candidate_attempts: i32,
    pub max_provider_requests: usize,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    pub max_runtime: Duration,
    pub max_active_jobs_per_guest: i64,
    pub max_active_jobs_deployment: i64,
    pub max_submissions_per_guest_per_minute: i64,
    pub guest_retention: Duration,
}

impl Default for WorkloadLimits {
    fn default() -> Self {
        Self {
            allowed_tolerance_percent: [5, 10, 20],
            max_distance_metres: 100_000,
            max_template_points: 512,
            max_anchors: 64,
            max_candidate_attempts: 3,
            max_provider_requests: 50,
            max_request_bytes: 65_536,
            max_response_bytes: 2_097_152,
            max_runtime: Duration::from_secs(60),
            max_active_jobs_per_guest: 2,
            max_active_jobs_deployment: 20,
            max_submissions_per_guest_per_minute: 10,
            guest_retention: Duration::from_secs(86_400),
        }
    }
}

impl WorkloadLimits {
    fn from_values(values: &HashMap<String, String>) -> Result<Self, ConfigError> {
        let defaults = Self::default();
        let limits = Self {
            max_distance_metres: optional_positive(
                values,
                "GPSART_MAX_DISTANCE_METRES",
                defaults.max_distance_metres,
            )?,
            max_template_points: optional_positive(
                values,
                "GPSART_MAX_TEMPLATE_POINTS",
                defaults.max_template_points,
            )?,
            max_anchors: optional_positive(values, "GPSART_MAX_ANCHORS", defaults.max_anchors)?,
            max_candidate_attempts: optional_positive(
                values,
                "GPSART_MAX_CANDIDATE_ATTEMPTS",
                defaults.max_candidate_attempts,
            )?,
            max_provider_requests: optional_positive(
                values,
                "GPSART_MAX_PROVIDER_REQUESTS",
                defaults.max_provider_requests,
            )?,
            max_request_bytes: optional_positive(
                values,
                "GPSART_MAX_REQUEST_BYTES",
                defaults.max_request_bytes,
            )?,
            max_response_bytes: optional_positive(
                values,
                "GPSART_MAX_RESPONSE_BYTES",
                defaults.max_response_bytes,
            )?,
            max_active_jobs_per_guest: optional_positive(
                values,
                "GPSART_MAX_ACTIVE_JOBS_PER_GUEST",
                defaults.max_active_jobs_per_guest,
            )?,
            max_active_jobs_deployment: optional_positive(
                values,
                "GPSART_MAX_ACTIVE_JOBS_DEPLOYMENT",
                defaults.max_active_jobs_deployment,
            )?,
            max_submissions_per_guest_per_minute: optional_positive(
                values,
                "GPSART_MAX_SUBMISSIONS_PER_GUEST_PER_MINUTE",
                defaults.max_submissions_per_guest_per_minute,
            )?,
            max_runtime: Duration::from_secs(optional_positive(
                values,
                "GPSART_MAX_RUNTIME_SECONDS",
                defaults.max_runtime.as_secs(),
            )?),
            guest_retention: Duration::from_secs(optional_positive(
                values,
                "GPSART_GUEST_RETENTION_SECONDS",
                defaults.guest_retention.as_secs(),
            )?),
            ..defaults
        };
        if limits.max_active_jobs_per_guest > limits.max_active_jobs_deployment
            || limits.max_runtime > Duration::from_secs(60)
            || limits.guest_retention > Duration::from_secs(86_400)
        {
            return Err(ConfigError::Invalid("GPSART_WORKLOAD_LIMITS"));
        }
        Ok(limits)
    }
}

fn optional_positive<T>(
    values: &HashMap<String, String>,
    key: &'static str,
    default: T,
) -> Result<T, ConfigError>
where
    T: std::str::FromStr + PartialOrd + Copy + From<u8>,
{
    let Some(value) = values.get(key) else {
        return Ok(default);
    };
    let parsed = value.parse::<T>().map_err(|_| ConfigError::Invalid(key))?;
    if parsed <= T::from(0) {
        return Err(ConfigError::Invalid(key));
    }
    Ok(parsed)
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
        let workload_limits = WorkloadLimits::from_values(&values)?;
        Ok(Self {
            database_url,
            api_bind,
            workload_limits,
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
