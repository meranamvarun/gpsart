use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    extract::{DefaultBodyLimit, Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use gpsart_config::WorkloadLimits;
use gpsart_domain::templates::{Category, embedded_catalogue, render_svg};
use gpsart_domain::{
    RouteRequestProfile,
    starts::{
        Point, ResolutionContext, ResolutionId, ResolveError, StartResolution, StartResolver,
        StartResolverProvider,
    },
};
use gpsart_persistence::jobs::{
    JobError, JobOutcomeCode, JobState, JobStatusSnapshot, PostgresJobCoordinator,
    RouteJobSubmission, SUPPORTED_ALGORITHM_VERSION,
};
use gpsart_persistence::routes::{
    CandidateSnapshot, DirectionMarker, PostgresRouteSnapshotStore, RouteSnapshotError,
    RouteSnapshotStore,
};
use gpsart_persistence::{
    DependencyProbe, GuestSessionStore, PostgresProbe, ResolvedStartRecord, ResolvedStartStore,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod auth;
mod gpx_export;
use auth::{
    AuthError, InMemorySessionStore, SessionPolicy, authenticate, csrf_cookie,
    enforce_request_protection, expired_cookies, issue_session, rotate_session, session_cookie,
};

#[async_trait]
pub trait Readiness: Send + Sync + 'static {
    async fn check(&self) -> Result<(), &'static str>;
}

#[async_trait]
impl Readiness for PostgresProbe {
    async fn check(&self) -> Result<(), &'static str> {
        DependencyProbe::check(self)
            .await
            .map_err(|error| error.code())
    }
}

#[derive(Clone)]
struct AppState {
    readiness: Arc<dyn Readiness>,
    starts: Arc<dyn StartService>,
    sessions: Arc<dyn GuestSessionStore>,
    session_policy: SessionPolicy,
    authenticate_starts: bool,
    jobs: Arc<dyn RouteJobService>,
    routes: Arc<dyn RouteSnapshotService>,
    workload_limits: WorkloadLimits,
}

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
    code: &'static str,
}

#[derive(Deserialize)]
struct DesignsQuery {
    category: Option<Category>,
}

#[derive(Serialize)]
struct DesignsBody {
    designs: Vec<DesignSummary>,
}

#[derive(Serialize)]
struct DesignSummary {
    id: String,
    version: u32,
    status: &'static str,
    category: Category,
    name: String,
    accessible_name: String,
    complexity_hint: String,
    preview_svg: String,
    definition_hash: String,
}

pub fn app(readiness: impl Readiness) -> Router {
    build_app(
        readiness,
        UnavailableStartService,
        Arc::new(InMemorySessionStore::default()),
        default_session_policy(),
        false,
        UnavailableRouteJobService,
        UnavailableRouteSnapshotService,
    )
}

pub fn app_with_starts(readiness: impl Readiness, starts: impl StartService) -> Router {
    build_app(
        readiness,
        starts,
        Arc::new(InMemorySessionStore::default()),
        default_session_policy(),
        false,
        UnavailableRouteJobService,
        UnavailableRouteSnapshotService,
    )
}

pub fn app_with_auth(
    readiness: impl Readiness,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
) -> Router {
    build_app(
        readiness,
        UnavailableStartService,
        sessions,
        session_policy,
        true,
        UnavailableRouteJobService,
        UnavailableRouteSnapshotService,
    )
}

pub fn app_with_services(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
) -> Router {
    build_app(
        readiness,
        starts,
        sessions,
        session_policy,
        true,
        UnavailableRouteJobService,
        UnavailableRouteSnapshotService,
    )
}

pub fn app_with_all_services(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    jobs: impl RouteJobService,
) -> Router {
    build_app(
        readiness,
        starts,
        sessions,
        session_policy,
        true,
        jobs,
        UnavailableRouteSnapshotService,
    )
}

pub fn app_with_routes(
    readiness: impl Readiness,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    routes: impl RouteSnapshotService,
) -> Router {
    build_app(
        readiness,
        UnavailableStartService,
        sessions,
        session_policy,
        true,
        UnavailableRouteJobService,
        routes,
    )
}

pub fn app_with_all_services_and_routes(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    jobs: impl RouteJobService,
    routes: impl RouteSnapshotService,
) -> Router {
    build_app(
        readiness,
        starts,
        sessions,
        session_policy,
        true,
        jobs,
        routes,
    )
}

fn build_app(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    authenticate_starts: bool,
    jobs: impl RouteJobService,
    routes: impl RouteSnapshotService,
) -> Router {
    build_app_with_limits(
        readiness,
        starts,
        sessions,
        session_policy,
        authenticate_starts,
        jobs,
        routes,
        WorkloadLimits::default(),
    )
}

#[allow(clippy::too_many_arguments)]
fn build_app_with_limits(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    authenticate_starts: bool,
    jobs: impl RouteJobService,
    routes: impl RouteSnapshotService,
    limits: WorkloadLimits,
) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/designs", get(designs))
        .route("/api/v1/coverage", get(coverage))
        .route("/api/v1/starts/resolve", post(resolve_start))
        .route("/api/v1/starts/{id}/confirm", post(confirm_start))
        .route("/api/v1/route-jobs", post(create_route_job))
        .route(
            "/api/v1/route-jobs/{id}",
            get(get_route_job).delete(cancel_route_job),
        )
        .route("/api/v1/routes/{id}", get(get_route_snapshot))
        .route("/api/v1/routes/{id}/export.gpx", get(export_route_gpx))
        .route(
            "/api/v1/guest-sessions",
            axum::routing::post(create_guest_session),
        )
        .route(
            "/api/v1/guest-sessions/current",
            axum::routing::delete(revoke_guest_session),
        )
        .route(
            "/api/v1/guest-sessions/current/rotate",
            axum::routing::post(rotate_guest_session),
        )
        .layer(DefaultBodyLimit::max(limits.max_request_bytes))
        .with_state(AppState {
            readiness: Arc::new(readiness),
            starts: Arc::new(starts),
            sessions,
            session_policy,
            authenticate_starts,
            jobs: Arc::new(jobs),
            routes: Arc::new(routes),
            workload_limits: limits,
        })
}

pub fn app_with_all_services_and_routes_and_limits(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    jobs: impl RouteJobService,
    routes: impl RouteSnapshotService,
    limits: WorkloadLimits,
) -> Router {
    build_app_with_limits(
        readiness,
        starts,
        sessions,
        session_policy,
        true,
        jobs,
        routes,
        limits,
    )
}

fn default_session_policy() -> SessionPolicy {
    SessionPolicy::new(
        "https://localhost",
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )
    .expect("default session policy")
}

#[derive(Clone, Debug, Serialize)]
pub struct CoveragePayload {
    pub version: String,
    pub geojson: serde_json::Value,
    pub local_route_feasibility: &'static str,
}

#[async_trait]
pub trait StartService: Send + Sync + 'static {
    fn coverage(&self) -> CoveragePayload;
    async fn resolve(
        &self,
        guest_session_id: &str,
        requested: Point,
        profile: RouteRequestProfile,
    ) -> Result<StartResolution, ResolveError>;
    async fn confirm(
        &self,
        guest_session_id: &str,
        id: ResolutionId,
        profile: RouteRequestProfile,
    ) -> Result<Point, ResolveError>;
}

#[async_trait]
pub trait RouteJobService: Send + Sync + 'static {
    async fn submit(
        &self,
        owner: uuid::Uuid,
        input: RouteJobSubmission,
    ) -> Result<uuid::Uuid, JobError>;
    async fn cancel(&self, owner: uuid::Uuid, job_id: uuid::Uuid) -> Result<JobState, JobError>;
    async fn status(
        &self,
        owner: uuid::Uuid,
        job_id: uuid::Uuid,
    ) -> Result<JobStatusSnapshot, JobError>;
}

#[async_trait]
impl RouteJobService for PostgresJobCoordinator {
    async fn submit(
        &self,
        owner: uuid::Uuid,
        input: RouteJobSubmission,
    ) -> Result<uuid::Uuid, JobError> {
        self.submit(owner, &input).await
    }
    async fn cancel(&self, owner: uuid::Uuid, job_id: uuid::Uuid) -> Result<JobState, JobError> {
        self.cancel(owner, job_id, SystemTime::now()).await
    }
    async fn status(
        &self,
        owner: uuid::Uuid,
        job_id: uuid::Uuid,
    ) -> Result<JobStatusSnapshot, JobError> {
        self.status(owner, job_id).await
    }
}

#[async_trait]
pub trait RouteSnapshotService: Send + Sync + 'static {
    async fn get(
        &self,
        owner: uuid::Uuid,
        route_id: uuid::Uuid,
    ) -> Result<CandidateSnapshot, RouteSnapshotError>;
}

#[async_trait]
impl RouteSnapshotService for PostgresRouteSnapshotStore {
    async fn get(
        &self,
        owner: uuid::Uuid,
        route_id: uuid::Uuid,
    ) -> Result<CandidateSnapshot, RouteSnapshotError> {
        RouteSnapshotStore::get(self, owner, route_id).await
    }
}

struct UnavailableRouteSnapshotService;

#[async_trait]
impl RouteSnapshotService for UnavailableRouteSnapshotService {
    async fn get(
        &self,
        _: uuid::Uuid,
        _: uuid::Uuid,
    ) -> Result<CandidateSnapshot, RouteSnapshotError> {
        Err(RouteSnapshotError::Unavailable)
    }
}

struct UnavailableRouteJobService;

#[async_trait]
impl RouteJobService for UnavailableRouteJobService {
    async fn submit(&self, _: uuid::Uuid, _: RouteJobSubmission) -> Result<uuid::Uuid, JobError> {
        Err(JobError::Unavailable)
    }
    async fn cancel(&self, _: uuid::Uuid, _: uuid::Uuid) -> Result<JobState, JobError> {
        Err(JobError::Unavailable)
    }
    async fn status(&self, _: uuid::Uuid, _: uuid::Uuid) -> Result<JobStatusSnapshot, JobError> {
        Err(JobError::Unavailable)
    }
}

pub struct ConfiguredStartService<P> {
    resolver: StartResolver<P>,
    data_version: String,
    store: Option<Arc<dyn ResolvedStartStore>>,
}

impl<P: StartResolverProvider> ConfiguredStartService<P> {
    pub fn new(resolver: StartResolver<P>, data_version: impl Into<String>) -> Self {
        Self {
            resolver,
            data_version: data_version.into(),
            store: None,
        }
    }

    pub fn with_store(
        resolver: StartResolver<P>,
        data_version: impl Into<String>,
        store: Arc<impl ResolvedStartStore>,
    ) -> Self {
        Self {
            resolver,
            data_version: data_version.into(),
            store: Some(store),
        }
    }

    fn context(&self, guest_session_id: &str, profile: RouteRequestProfile) -> ResolutionContext {
        ResolutionContext {
            guest_session_id: guest_session_id.to_owned(),
            profile,
            coverage_policy_version: self.resolver.coverage().version.clone(),
            data_version: self.data_version.clone(),
        }
    }
}

#[async_trait]
impl<P: StartResolverProvider> StartService for ConfiguredStartService<P> {
    fn coverage(&self) -> CoveragePayload {
        CoveragePayload {
            version: self.resolver.coverage().version.clone(),
            geojson: self.resolver.coverage().geojson.clone(),
            local_route_feasibility: "not_evaluated",
        }
    }

    async fn resolve(
        &self,
        guest_session_id: &str,
        requested: Point,
        profile: RouteRequestProfile,
    ) -> Result<StartResolution, ResolveError> {
        self.resolver.resolve(
            requested,
            self.context(guest_session_id, profile),
            std::time::SystemTime::now(),
        )
    }

    async fn confirm(
        &self,
        guest_session_id: &str,
        id: ResolutionId,
        profile: RouteRequestProfile,
    ) -> Result<Point, ResolveError> {
        let now = std::time::SystemTime::now();
        let resolution = self.resolver.confirmed_resolution(
            id,
            &self.context(guest_session_id, profile),
            now,
        )?;
        if let Some(store) = &self.store {
            let owner = uuid::Uuid::parse_str(guest_session_id)
                .map_err(|_| ResolveError::StorageUnavailable)?;
            let expires_at = UNIX_EPOCH
                .checked_add(Duration::from_secs(resolution.expires_at_epoch_seconds))
                .ok_or(ResolveError::StorageUnavailable)?;
            store
                .insert_resolved_start(ResolvedStartRecord {
                    id: resolution.id.as_uuid(),
                    guest_session_id: owner,
                    requested_longitude: resolution.requested_point.longitude,
                    requested_latitude: resolution.requested_point.latitude,
                    confirmed_longitude: resolution.candidate_network_point.longitude,
                    confirmed_latitude: resolution.candidate_network_point.latitude,
                    profile_version: resolution.profile_version.to_string(),
                    coverage_policy_version: resolution.coverage_policy_version.clone(),
                    map_data_version: resolution.data_version.clone(),
                    created_at: now,
                    expires_at,
                })
                .await
                .map_err(|_| ResolveError::StorageUnavailable)?;
        }
        Ok(resolution.candidate_network_point)
    }
}

struct UnavailableStartService;

#[async_trait]
impl StartService for UnavailableStartService {
    fn coverage(&self) -> CoveragePayload {
        CoveragePayload {
            version: "unavailable".into(),
            geojson: serde_json::Value::Null,
            local_route_feasibility: "not_evaluated",
        }
    }
    async fn resolve(
        &self,
        _: &str,
        _: Point,
        _: RouteRequestProfile,
    ) -> Result<StartResolution, ResolveError> {
        Err(ResolveError::ProviderFailure)
    }
    async fn confirm(
        &self,
        _: &str,
        _: ResolutionId,
        _: RouteRequestProfile,
    ) -> Result<Point, ResolveError> {
        Err(ResolveError::ProviderFailure)
    }
}

#[derive(Deserialize)]
struct PointInput {
    longitude: f64,
    latitude: f64,
}

#[derive(Deserialize)]
struct ResolveStartBody {
    requested_point: PointInput,
    profile: RouteRequestProfile,
}

#[derive(Deserialize)]
struct ConfirmStartBody {
    profile: RouteRequestProfile,
}

#[derive(Serialize)]
struct ErrorBody {
    outcome: ErrorOutcome,
}

#[derive(Serialize)]
struct ErrorOutcome {
    code: &'static str,
}

fn guest_session(headers: &HeaderMap) -> Result<&str, (StatusCode, Json<ErrorBody>)> {
    headers
        .get("x-gpsart-guest-session")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(ErrorBody {
                outcome: ErrorOutcome {
                    code: "authorization_required",
                },
            }),
        ))
}

async fn start_session_identity(
    state: &AppState,
    headers: &HeaderMap,
) -> Result<String, Box<axum::response::Response>> {
    if state.authenticate_starts {
        let session = authenticate(state.sessions.as_ref(), headers, SystemTime::now())
            .await
            .map_err(|error| Box::new(auth_error(error)))?;
        enforce_request_protection(&state.session_policy, &Method::POST, headers)
            .map_err(|error| Box::new(auth_error(error)))?;
        Ok(session.id.to_string())
    } else {
        guest_session(headers)
            .map(str::to_owned)
            .map_err(|error| Box::new(error.into_response()))
    }
}

fn error_response(error: ResolveError) -> axum::response::Response {
    let status = match error {
        ResolveError::InvalidCoordinates => StatusCode::BAD_REQUEST,
        ResolveError::WrongSession => StatusCode::NOT_FOUND,
        ResolveError::ProviderFailure => StatusCode::BAD_GATEWAY,
        ResolveError::StorageUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        ResolveError::ExpiredResolution => StatusCode::GONE,
        ResolveError::UnsupportedCoverage
        | ResolveError::NoRoutableAccess
        | ResolveError::SnapTooFar => StatusCode::UNPROCESSABLE_ENTITY,
        ResolveError::ProfileChanged
        | ResolveError::CoveragePolicyChanged
        | ResolveError::DataVersionChanged => StatusCode::CONFLICT,
    };
    (
        status,
        Json(ErrorBody {
            outcome: ErrorOutcome { code: error.code() },
        }),
    )
        .into_response()
}

async fn coverage(State(state): State<AppState>) -> impl IntoResponse {
    (StatusCode::OK, Json(state.starts.coverage()))
}

async fn resolve_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Json<ResolveStartBody>, JsonRejection>,
) -> axum::response::Response {
    let Json(body) = match body {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody {
                    outcome: ErrorOutcome {
                        code: "invalid_request",
                    },
                }),
            )
                .into_response();
        }
    };
    let guest = match start_session_identity(&state, &headers).await {
        Ok(value) => value,
        Err(response) => return *response,
    };
    let requested = match Point::new(
        body.requested_point.longitude,
        body.requested_point.latitude,
    ) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    match state.starts.resolve(&guest, requested, body.profile).await {
        Ok(value) => (StatusCode::OK, Json(value)).into_response(),
        Err(error) => error_response(error),
    }
}

async fn confirm_start(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<ResolutionId>,
    body: Result<Json<ConfirmStartBody>, JsonRejection>,
) -> axum::response::Response {
    let Json(body) = match body {
        Ok(value) => value,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody {
                    outcome: ErrorOutcome {
                        code: "invalid_request",
                    },
                }),
            )
                .into_response();
        }
    };
    let guest = match start_session_identity(&state, &headers).await {
        Ok(value) => value,
        Err(response) => return *response,
    };
    match state.starts.confirm(&guest, id, body.profile).await {
        Ok(confirmed_start) => (
            StatusCode::OK,
            Json(serde_json::json!({ "confirmed_start": confirmed_start })),
        )
            .into_response(),
        Err(error) => error_response(error),
    }
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CreateRouteJobBody {
    resolved_start_id: uuid::Uuid,
    revision: i32,
    idempotency_key: String,
    requested_distance_metres: i32,
    distance_tolerance_metres: i32,
    return_to_start: bool,
    profile: RouteRequestProfile,
    coverage_policy_version: String,
    template_id: String,
    template_version: String,
    map_data_version: String,
    algorithm_version: String,
}

#[derive(Serialize)]
struct RouteJobAccepted {
    job_id: uuid::Uuid,
    request_revision: i32,
}

async fn create_route_job(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
    body: Result<Json<CreateRouteJobBody>, JsonRejection>,
) -> axum::response::Response {
    let session = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(value) => value,
        Err(error) => return auth_error(error),
    };
    if let Err(error) = enforce_request_protection(&state.session_policy, &method, &headers) {
        return auth_error(error);
    }
    let Json(body) = match body {
        Ok(value) => value,
        Err(error) if error.status() == StatusCode::PAYLOAD_TOO_LARGE => {
            return job_error_response(JobError::PayloadTooLarge);
        }
        Err(_) => return job_error_response(JobError::InvalidInput),
    };
    let limits = &state.workload_limits;
    if body.revision <= 0
        || body.idempotency_key.is_empty()
        || body.idempotency_key.len() > 200
        || !(1_000..=limits.max_distance_metres).contains(&body.requested_distance_metres)
        || body.distance_tolerance_metres < 0
        || !limits.allowed_tolerance_percent.iter().any(|percent| {
            (body.distance_tolerance_metres * 100
                - body.requested_distance_metres * i32::from(*percent))
            .abs()
                <= 50
        })
        || body.algorithm_version != SUPPORTED_ALGORITHM_VERSION
        || [
            &body.template_id,
            &body.template_version,
            &body.coverage_policy_version,
            &body.map_data_version,
            &body.algorithm_version,
        ]
        .iter()
        .any(|value| value.is_empty())
    {
        return job_error_response(JobError::InvalidInput);
    }
    let template_version = match body.template_version.parse::<u32>() {
        Ok(value) => value,
        Err(_) => return job_error_response(JobError::InvalidInput),
    };
    let template = embedded_catalogue().ok().and_then(|catalogue| {
        catalogue
            .published(None)
            .into_iter()
            .find(|template| {
                template.definition.id == body.template_id
                    && template.definition.version == template_version
            })
            .cloned()
    });
    let template_within_limits = template.as_ref().is_some_and(|template| {
        let points = template
            .definition
            .strokes
            .iter()
            .map(|stroke| stroke.points.len())
            .sum::<usize>()
            + template
                .definition
                .transitions
                .iter()
                .map(|transition| transition.points.len())
                .sum::<usize>();
        points <= limits.max_template_points
            && template.complete_traversal().len() <= limits.max_anchors
    });
    if !template_within_limits {
        return job_error_response(JobError::InvalidInput);
    }
    let canonical = match serde_json::to_vec(&body) {
        Ok(value) => value,
        Err(_) => return job_error_response(JobError::InvalidInput),
    };
    let now = SystemTime::now();
    let activity_profile_json = match serde_json::to_string(&body.profile) {
        Ok(value) => value,
        Err(_) => return job_error_response(JobError::InvalidInput),
    };
    let submission = RouteJobSubmission {
        resolved_start_id: body.resolved_start_id,
        revision: body.revision,
        idempotency_key: body.idempotency_key,
        input_hash: Sha256::digest(canonical).into(),
        requested_distance_metres: body.requested_distance_metres,
        distance_tolerance_metres: body.distance_tolerance_metres,
        return_to_start: body.return_to_start,
        activity_profile_json,
        activity_profile_version: body.profile.version().to_string(),
        coverage_policy_version: body.coverage_policy_version,
        template_id: body.template_id,
        template_version: body.template_version,
        map_data_version: body.map_data_version,
        algorithm_version: body.algorithm_version,
        expires_at: now + limits.guest_retention,
        deadline_at: now + limits.max_runtime,
        max_attempts: limits.max_candidate_attempts,
    };
    match state.jobs.submit(session.id, submission).await {
        Ok(job_id) => (
            StatusCode::ACCEPTED,
            Json(RouteJobAccepted {
                job_id,
                request_revision: body.revision,
            }),
        )
            .into_response(),
        Err(error) => job_error_response(error),
    }
}

#[derive(Serialize)]
struct RouteJobStatusBody {
    job_id: uuid::Uuid,
    request_revision: i32,
    state: &'static str,
    stage: &'static str,
    status_version: i64,
    queued_at_epoch_seconds: u64,
    started_at_epoch_seconds: Option<u64>,
    completed_at_epoch_seconds: Option<u64>,
    deadline_at_epoch_seconds: u64,
    expires_at_epoch_seconds: u64,
    outcome: Option<RouteJobOutcomeBody>,
    result_ids: Vec<uuid::Uuid>,
    poll_after_seconds: Option<u64>,
}

#[derive(Serialize)]
struct RouteJobOutcomeBody {
    code: &'static str,
    budget_exhausted: bool,
    retryable: bool,
    retry_after_seconds: Option<u64>,
}

async fn get_route_job(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> axum::response::Response {
    let session = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(value) => value,
        Err(error) => return auth_error(error),
    };
    let snapshot = match state.jobs.status(session.id, id).await {
        Ok(value) => value,
        Err(error) => return job_error_response(error),
    };
    let etag = format!("\"{}\"", snapshot.status_version);
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(',')
                .any(|candidate| matches!(candidate.trim(), "*") || candidate.trim() == etag)
        })
    {
        let mut response = StatusCode::NOT_MODIFIED.into_response();
        set_job_poll_headers(response.headers_mut(), &etag);
        return response;
    }
    let body = match status_body(snapshot) {
        Ok(value) => value,
        Err(error) => return job_error_response(error),
    };
    let mut response = (StatusCode::OK, Json(body)).into_response();
    set_job_poll_headers(response.headers_mut(), &etag);
    response
}

fn set_job_poll_headers(headers: &mut HeaderMap, etag: &str) {
    if let Ok(value) = HeaderValue::from_str(etag) {
        headers.insert(header::ETAG, value);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-cache"),
    );
}

#[derive(Serialize)]
struct RouteSnapshotBody {
    #[serde(flatten)]
    snapshot: CandidateSnapshot,
    start: [f64; 2],
    finish: [f64; 2],
    direction_markers: Vec<DirectionMarker>,
}

async fn get_route_snapshot(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> axum::response::Response {
    let session = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(value) => value,
        Err(error) => return auth_error(error),
    };
    let snapshot = match state.routes.get(session.id, id).await {
        Ok(value) => value,
        Err(error) => return route_snapshot_error_response(error),
    };
    if snapshot.validate().is_err() {
        return route_snapshot_error_response(RouteSnapshotError::InvalidSnapshot);
    }
    let body = RouteSnapshotBody {
        start: snapshot.start(),
        finish: snapshot.finish(),
        direction_markers: snapshot.direction_markers(),
        snapshot,
    };
    if serde_json::to_vec(&body).map_or(true, |encoded| {
        encoded.len() > state.workload_limits.max_response_bytes
    }) {
        return route_snapshot_error_response(RouteSnapshotError::InvalidSnapshot);
    }
    let mut response = (StatusCode::OK, Json(body)).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response
}

async fn export_route_gpx(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    headers: HeaderMap,
) -> axum::response::Response {
    let session = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(value) => value,
        Err(error) => return auth_error(error),
    };
    let snapshot = match state.routes.get(session.id, id).await {
        Ok(value) => value,
        Err(error) => return route_snapshot_error_response(error),
    };
    if snapshot.validate().is_err() {
        return route_snapshot_error_response(RouteSnapshotError::InvalidSnapshot);
    }
    let filename = gpx_export::filename(&snapshot);
    let disposition = match HeaderValue::from_str(&format!("attachment; filename=\"{filename}\"")) {
        Ok(value) => value,
        Err(_) => return route_snapshot_error_response(RouteSnapshotError::InvalidSnapshot),
    };
    let mut response = (StatusCode::OK, gpx_export::serialize(&snapshot)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/gpx+xml; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn route_snapshot_error_response(error: RouteSnapshotError) -> axum::response::Response {
    let (status, code) = match error {
        RouteSnapshotError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        RouteSnapshotError::Unavailable | RouteSnapshotError::InvalidSnapshot => {
            (StatusCode::SERVICE_UNAVAILABLE, "route_storage_unavailable")
        }
    };
    (status, Json(serde_json::json!({ "code": code }))).into_response()
}

fn status_body(snapshot: JobStatusSnapshot) -> Result<RouteJobStatusBody, JobError> {
    let timestamp = |value: SystemTime| {
        value
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .map_err(|_| JobError::Unavailable)
    };
    let outcome = snapshot.outcome.map(|value| {
        let retryable = value.code == JobOutcomeCode::ProviderUnavailable;
        RouteJobOutcomeBody {
            code: value.code.as_str(),
            budget_exhausted: value.budget_exhausted,
            retryable,
            retry_after_seconds: retryable.then_some(30),
        }
    });
    Ok(RouteJobStatusBody {
        job_id: snapshot.job_id,
        request_revision: snapshot.request_revision,
        state: job_state_name(snapshot.state),
        stage: snapshot.stage.as_str(),
        status_version: snapshot.status_version,
        queued_at_epoch_seconds: timestamp(snapshot.queued_at)?,
        started_at_epoch_seconds: snapshot.started_at.map(timestamp).transpose()?,
        completed_at_epoch_seconds: snapshot.completed_at.map(timestamp).transpose()?,
        deadline_at_epoch_seconds: timestamp(snapshot.deadline_at)?,
        expires_at_epoch_seconds: timestamp(snapshot.expires_at)?,
        outcome,
        result_ids: snapshot.result_ids,
        poll_after_seconds: (!snapshot.state.is_terminal()).then_some(1),
    })
}

async fn cancel_route_job(
    State(state): State<AppState>,
    Path(id): Path<uuid::Uuid>,
    method: Method,
    headers: HeaderMap,
) -> axum::response::Response {
    let session = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(value) => value,
        Err(error) => return auth_error(error),
    };
    if let Err(error) = enforce_request_protection(&state.session_policy, &method, &headers) {
        return auth_error(error);
    }
    match state.jobs.cancel(session.id, id).await {
        Ok(state) => (
            StatusCode::OK,
            Json(serde_json::json!({ "state": job_state_name(state) })),
        )
            .into_response(),
        Err(error) => job_error_response(error),
    }
}

fn job_state_name(state: JobState) -> &'static str {
    match state {
        JobState::Queued => "queued",
        JobState::Running => "running",
        JobState::Succeeded => "succeeded",
        JobState::NoMatch => "no_match",
        JobState::Failed => "failed",
        JobState::Cancelled => "cancelled",
        JobState::TimedOut => "timed_out",
    }
}

fn job_error_response(error: JobError) -> axum::response::Response {
    if let JobError::RateLimited {
        retry_after_seconds,
    } = error
    {
        let mut response = (
            StatusCode::TOO_MANY_REQUESTS,
            Json(serde_json::json!({
                "code": "rate_limited",
                "retry_after_seconds": retry_after_seconds,
            })),
        )
            .into_response();
        if let Ok(value) = HeaderValue::from_str(&retry_after_seconds.to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        return response;
    }
    let (status, code) = match error {
        JobError::InvalidInput => (StatusCode::BAD_REQUEST, "invalid_request"),
        JobError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
        JobError::StartNotFound | JobError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        JobError::IdempotencyConflict => (StatusCode::CONFLICT, "idempotency_conflict"),
        JobError::LeaseLost | JobError::Terminal => (StatusCode::CONFLICT, "job_state_conflict"),
        JobError::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "job_storage_unavailable"),
        JobError::RateLimited { .. } => unreachable!("handled above"),
    };
    (status, Json(serde_json::json!({ "code": code }))).into_response()
}

#[derive(Serialize)]
struct SessionBody {
    expires_at: u64,
    rotate_at: u64,
    rotate_after: u64,
}

fn session_body(issued: &auth::IssuedSession, policy: &SessionPolicy) -> SessionBody {
    SessionBody {
        expires_at: issued
            .record
            .expires_at
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        rotate_at: (issued.record.created_at + policy.rotate_after)
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        rotate_after: policy.rotate_after.as_secs(),
    }
}

async fn create_guest_session(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> impl IntoResponse {
    if headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        != Some(state.session_policy.allowed_origin.as_str())
    {
        return auth_error(AuthError::Forbidden);
    }
    match issue_session(
        state.sessions.as_ref(),
        &state.session_policy,
        SystemTime::now(),
    )
    .await
    {
        Ok(issued) => {
            let mut response = (
                StatusCode::CREATED,
                Json(session_body(&issued, &state.session_policy)),
            )
                .into_response();
            if let Ok(value) =
                HeaderValue::from_str(&session_cookie(&issued.token, &state.session_policy))
            {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
            if let Ok(value) =
                HeaderValue::from_str(&csrf_cookie(&issued.csrf_token, &state.session_policy))
            {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
            response
        }
        Err(error) => auth_error(error),
    }
}

async fn revoke_guest_session(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
) -> impl IntoResponse {
    let session = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(session) => session,
        Err(error) => return auth_error(error),
    };
    if let Err(error) = enforce_request_protection(&state.session_policy, &method, &headers) {
        return auth_error(error);
    }
    if state.sessions.revoke_session(session.id).await.is_err() {
        return auth_error(AuthError::Unavailable);
    }
    let mut response = StatusCode::NO_CONTENT.into_response();
    for cookie in expired_cookies(&state.session_policy) {
        if let Ok(value) = HeaderValue::from_str(&cookie) {
            response.headers_mut().append(header::SET_COOKIE, value);
        }
    }
    response
}

async fn rotate_guest_session(
    State(state): State<AppState>,
    method: Method,
    headers: HeaderMap,
) -> impl IntoResponse {
    let current = match authenticate(state.sessions.as_ref(), &headers, SystemTime::now()).await {
        Ok(session) => session,
        Err(error) => return auth_error(error),
    };
    if let Err(error) = enforce_request_protection(&state.session_policy, &method, &headers) {
        return auth_error(error);
    }
    if !state
        .session_policy
        .rotation_due(&current, SystemTime::now())
    {
        return auth_error(AuthError::RotationNotDue);
    }
    let issued = match rotate_session(
        state.sessions.as_ref(),
        current.id,
        &state.session_policy,
        SystemTime::now(),
    )
    .await
    {
        Ok(value) => value,
        Err(error) => return auth_error(error),
    };
    let mut response = (
        StatusCode::OK,
        Json(session_body(&issued, &state.session_policy)),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(&session_cookie(&issued.token, &state.session_policy))
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    if let Ok(value) =
        HeaderValue::from_str(&csrf_cookie(&issued.csrf_token, &state.session_policy))
    {
        response.headers_mut().append(header::SET_COOKIE, value);
    }
    response
}

fn auth_error(error: AuthError) -> axum::response::Response {
    #[derive(Serialize)]
    struct ErrorBody {
        code: &'static str,
    }
    let (status, code) = match error {
        AuthError::Unauthenticated => (StatusCode::UNAUTHORIZED, "authentication_required"),
        AuthError::Forbidden | AuthError::InvalidPolicy => {
            (StatusCode::FORBIDDEN, "request_forbidden")
        }
        AuthError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
        AuthError::RotationNotDue => (StatusCode::CONFLICT, "rotation_not_due"),
        AuthError::Unavailable => (
            StatusCode::SERVICE_UNAVAILABLE,
            "authentication_unavailable",
        ),
    };
    (status, Json(ErrorBody { code })).into_response()
}

async fn designs(Query(query): Query<DesignsQuery>) -> impl IntoResponse {
    match embedded_catalogue() {
        Ok(catalogue) => {
            let designs = catalogue
                .published(query.category)
                .into_iter()
                .map(|template| DesignSummary {
                    id: template.definition.id.clone(),
                    version: template.definition.version,
                    status: template.definition.status.as_str(),
                    category: template.definition.category,
                    name: template.definition.name.clone(),
                    accessible_name: match template.definition.category {
                        Category::Shape => format!("{} route design", template.definition.name),
                        Category::Text => format!("{} text route design", template.definition.name),
                    },
                    complexity_hint: template.definition.complexity.hint.clone(),
                    preview_svg: render_svg(&template.definition),
                    definition_hash: template.definition_hash.clone(),
                })
                .collect();
            (StatusCode::OK, Json(DesignsBody { designs })).into_response()
        }
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(DesignsBody {
                designs: Vec::new(),
            }),
        )
            .into_response(),
    }
}

pub async fn serve(
    listener: tokio::net::TcpListener,
    readiness: impl Readiness,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, app(readiness))
        .with_graceful_shutdown(shutdown)
        .await
}

pub async fn serve_with_starts(
    listener: tokio::net::TcpListener,
    readiness: impl Readiness,
    starts: impl StartService,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, app_with_starts(readiness, starts))
        .with_graceful_shutdown(shutdown)
        .await
}

pub async fn serve_with_auth(
    listener: tokio::net::TcpListener,
    readiness: impl Readiness,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(listener, app_with_auth(readiness, sessions, session_policy))
        .with_graceful_shutdown(shutdown)
        .await
}

pub async fn serve_with_services(
    listener: tokio::net::TcpListener,
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        app_with_services(readiness, starts, sessions, session_policy),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

pub async fn serve_with_all_services(
    listener: tokio::net::TcpListener,
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    jobs: impl RouteJobService,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    axum::serve(
        listener,
        app_with_all_services(readiness, starts, sessions, session_policy, jobs),
    )
    .with_graceful_shutdown(shutdown)
    .await
}

async fn live() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(HealthBody {
            status: "ok",
            code: "live",
        }),
    )
}

async fn ready(State(state): State<AppState>) -> impl IntoResponse {
    match state.readiness.check().await {
        Ok(()) => (
            StatusCode::OK,
            Json(HealthBody {
                status: "ok",
                code: "ready",
            }),
        ),
        Err(code) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthBody {
                status: "unavailable",
                code,
            }),
        ),
    }
}
