use gpsart_domain::starts::{ConfiguredNetworkPoint, ConfiguredNetworkProvider, Coverage, Point};
use gpsart_domain::{ActivityProfile, ProfileVersion};
use gpsart_routing::LocalOsmGraph;
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet},
    fmt,
    net::SocketAddr,
    path::{Path, PathBuf},
    time::Duration,
};
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
    pub workload_limits: WorkloadLimits,
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

#[derive(Clone, Debug)]
pub struct ProductionRoutingConfig {
    map_data_version: String,
    coverage: Coverage,
    reviewed_starts: Vec<ConfiguredNetworkPoint>,
    reviewed_workload: ReviewedWorkload,
    graph_path: PathBuf,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionRoutingManifest {
    schema_version: String,
    configuration_version: String,
    map_data_version: String,
    graph: GraphManifest,
    coverage: CoverageManifest,
    routable_starts: Vec<ReviewedStart>,
    workload: ReviewedWorkload,
    approval: Approval,
    update_procedure: UpdateProcedure,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GraphManifest {
    path: String,
    sha256: String,
    snapshot_id: String,
    source_api: String,
    source_url: String,
    source_retrieved_at: String,
    source_raw_sha256: String,
    attribution: String,
    license: String,
    extraction_command: String,
    extraction_script_path: String,
    extraction_script_sha256: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CoverageManifest {
    version: String,
    bounds: [f64; 4],
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewedStart {
    osm_node_id: u64,
    point: Point,
    profile_versions: Vec<ProfileVersion>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct ReviewedWorkload {
    distance_unit: String,
    allowed_tolerance_percent: [u8; 3],
    max_distance_metres: i32,
    max_template_points: usize,
    max_anchors: usize,
    max_candidate_attempts: i32,
    max_provider_requests: usize,
    max_request_bytes: usize,
    max_response_bytes: usize,
    max_runtime_seconds: u64,
    max_active_jobs_per_guest: i64,
    max_active_jobs_deployment: i64,
    max_submissions_per_guest_per_minute: i64,
    guest_retention_seconds: u64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Approval {
    status: String,
    reviewed_at: String,
    reviewed_by: String,
    reviewer_sign_off: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateProcedure {
    document: String,
    immutable_version_required: bool,
    old_snapshot_compatibility_test: String,
}

impl ProductionRoutingConfig {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ProductionRoutingError> {
        let path = path.as_ref();
        let bytes = std::fs::read(path).map_err(|_| ProductionRoutingError::ManifestUnavailable)?;
        let manifest: ProductionRoutingManifest = serde_json::from_slice(&bytes)
            .map_err(|_| ProductionRoutingError::ManifestMalformed)?;
        if manifest.schema_version != "gpsart-production-routing-v1"
            || manifest.configuration_version.trim().is_empty()
            || manifest.approval.status != "approved"
            || !valid_date(&manifest.approval.reviewed_at)
            || manifest.approval.reviewed_by.trim().is_empty()
            || manifest.approval.reviewer_sign_off.trim().is_empty()
            || !manifest.update_procedure.immutable_version_required
            || manifest.update_procedure.document.trim().is_empty()
            || manifest
                .update_procedure
                .old_snapshot_compatibility_test
                .trim()
                .is_empty()
            || !valid_sha256(&manifest.graph.sha256)
            || !valid_sha256(&manifest.graph.source_raw_sha256)
            || !valid_sha256(&manifest.graph.extraction_script_sha256)
            || manifest.graph.snapshot_id.trim().is_empty()
            || manifest.graph.source_api != "OpenStreetMap API 0.6"
            || !manifest
                .graph
                .source_url
                .starts_with("https://api.openstreetmap.org/api/0.6/map.json?bbox=")
            || !manifest.graph.source_retrieved_at.ends_with('Z')
            || manifest.graph.attribution.trim().is_empty()
            || manifest.graph.license != "ODbL-1.0"
            || manifest.graph.extraction_command.trim().is_empty()
            || manifest.workload.distance_unit != "metres"
            || manifest.routable_starts.is_empty()
        {
            return Err(ProductionRoutingError::ManifestInvalid);
        }
        let graph_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&manifest.graph.path);
        let graph_bytes =
            std::fs::read(&graph_path).map_err(|_| ProductionRoutingError::GraphUnavailable)?;
        let graph = LocalOsmGraph::from_json(&graph_bytes, &manifest.graph.sha256)
            .map_err(|_| ProductionRoutingError::GraphInvalid)?;
        let extraction_script_path = path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(&manifest.graph.extraction_script_path);
        let extraction_script = std::fs::read(extraction_script_path)
            .map_err(|_| ProductionRoutingError::ManifestInvalid)?;
        if LocalOsmGraph::sha256_hex(&extraction_script) != manifest.graph.extraction_script_sha256
            || !graph.has_provenance(
                &manifest.graph.snapshot_id,
                &manifest.graph.source_api,
                &manifest.graph.source_url,
                &manifest.graph.source_retrieved_at,
                &manifest.graph.source_raw_sha256,
                &manifest.graph.attribution,
                &manifest.graph.license,
            )
            || !graph.all_points_within(manifest.coverage.bounds)
        {
            return Err(ProductionRoutingError::ManifestInvalid);
        }
        let expected_version = format!(
            "{}:sha256:{}",
            manifest.graph.snapshot_id, manifest.graph.sha256
        );
        if manifest.map_data_version != expected_version
            || graph.data_version() != manifest.map_data_version
        {
            return Err(ProductionRoutingError::GraphVersionMismatch);
        }
        let coverage = Coverage::rectangle(
            manifest.coverage.version,
            manifest.coverage.bounds[0],
            manifest.coverage.bounds[1],
            manifest.coverage.bounds[2],
            manifest.coverage.bounds[3],
        )
        .map_err(|_| ProductionRoutingError::ManifestInvalid)?;
        let mut reviewed_starts = Vec::with_capacity(manifest.routable_starts.len());
        let mut reviewed_node_ids = HashSet::new();
        let mut reviewed_points = HashSet::new();
        for start in manifest.routable_starts {
            if start.profile_versions.is_empty()
                || !reviewed_node_ids.insert(start.osm_node_id)
                || !reviewed_points.insert((
                    start.point.longitude.to_bits(),
                    start.point.latitude.to_bits(),
                ))
                || start
                    .profile_versions
                    .iter()
                    .copied()
                    .collect::<HashSet<_>>()
                    .len()
                    != start.profile_versions.len()
            {
                return Err(ProductionRoutingError::StartInvalid);
            }
            let Some(graph_point) = graph.point(start.osm_node_id) else {
                return Err(ProductionRoutingError::StartInvalid);
            };
            let expected = Point::new(
                f64::from(graph_point.longitude_e7) / 10_000_000.0,
                f64::from(graph_point.latitude_e7) / 10_000_000.0,
            )
            .map_err(|_| ProductionRoutingError::StartInvalid)?;
            if start.point != expected || !coverage.contains(start.point) {
                return Err(ProductionRoutingError::StartInvalid);
            }
            for profile in &start.profile_versions {
                let activity = match profile {
                    ProfileVersion::WalkingV1 => ActivityProfile::Walking,
                    ProfileVersion::RunningV1 => ActivityProfile::Running,
                    ProfileVersion::CyclingV1 => ActivityProfile::Cycling,
                };
                if !graph.is_reviewable_start(start.osm_node_id, activity) {
                    return Err(ProductionRoutingError::StartInvalid);
                }
            }
            reviewed_starts.push(ConfiguredNetworkPoint {
                point: start.point,
                profile_versions: start.profile_versions,
            });
        }
        Ok(Self {
            map_data_version: manifest.map_data_version,
            coverage,
            reviewed_starts,
            reviewed_workload: manifest.workload,
            graph_path,
        })
    }

    pub fn map_data_version(&self) -> &str {
        &self.map_data_version
    }
    pub fn coverage(&self) -> &Coverage {
        &self.coverage
    }
    pub fn reviewed_starts(&self) -> &[ConfiguredNetworkPoint] {
        &self.reviewed_starts
    }
    pub fn graph_path(&self) -> &Path {
        &self.graph_path
    }
    pub fn network_provider(&self) -> Result<ConfiguredNetworkProvider, ProductionRoutingError> {
        ConfiguredNetworkProvider::new(self.reviewed_starts.clone())
            .map_err(|_| ProductionRoutingError::StartInvalid)
    }
    pub fn validate_workload(&self, limits: &WorkloadLimits) -> Result<(), ProductionRoutingError> {
        let reviewed = ReviewedWorkload {
            distance_unit: "metres".into(),
            allowed_tolerance_percent: limits.allowed_tolerance_percent,
            max_distance_metres: limits.max_distance_metres,
            max_template_points: limits.max_template_points,
            max_anchors: limits.max_anchors,
            max_candidate_attempts: limits.max_candidate_attempts,
            max_provider_requests: limits.max_provider_requests,
            max_request_bytes: limits.max_request_bytes,
            max_response_bytes: limits.max_response_bytes,
            max_runtime_seconds: limits.max_runtime.as_secs(),
            max_active_jobs_per_guest: limits.max_active_jobs_per_guest,
            max_active_jobs_deployment: limits.max_active_jobs_deployment,
            max_submissions_per_guest_per_minute: limits.max_submissions_per_guest_per_minute,
            guest_retention_seconds: limits.guest_retention.as_secs(),
        };
        (reviewed == self.reviewed_workload)
            .then_some(())
            .ok_or(ProductionRoutingError::WorkloadMismatch)
    }
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn valid_date(value: &str) -> bool {
    if !(value.len() == 10
        && value.as_bytes()[4] == b'-'
        && value.as_bytes()[7] == b'-'
        && value
            .bytes()
            .enumerate()
            .all(|(index, byte)| index == 4 || index == 7 || byte.is_ascii_digit()))
    {
        return false;
    }
    let year = value[0..4].parse::<u16>().unwrap_or(0);
    let month = value[5..7].parse::<u8>().unwrap_or(0);
    let day = value[8..10].parse::<u8>().unwrap_or(0);
    year >= 2020 && (1..=12).contains(&month) && (1..=31).contains(&day)
}

#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum ProductionRoutingError {
    #[error("production routing manifest is unavailable")]
    ManifestUnavailable,
    #[error("production routing manifest is malformed")]
    ManifestMalformed,
    #[error("production routing manifest is invalid")]
    ManifestInvalid,
    #[error("production routing graph is unavailable")]
    GraphUnavailable,
    #[error("production routing graph is invalid")]
    GraphInvalid,
    #[error("production routing graph version does not match the manifest")]
    GraphVersionMismatch,
    #[error("production routable start is invalid")]
    StartInvalid,
    #[error("runtime workload limits do not match the reviewed production configuration")]
    WorkloadMismatch,
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
            workload_limits: WorkloadLimits::from_values(&values)?,
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
