use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    extract::{Path, Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};
use gpsart_domain::templates::{Category, embedded_catalogue, render_svg};
use gpsart_domain::{
    RouteRequestProfile,
    starts::{
        Point, ResolutionContext, ResolutionId, ResolveError, StartResolution, StartResolver,
        StartResolverProvider,
    },
};
use gpsart_persistence::{DependencyProbe, GuestSessionStore, PostgresProbe};
use serde::{Deserialize, Serialize};
use std::{
    future::Future,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub mod auth;
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
    )
}

pub fn app_with_starts(readiness: impl Readiness, starts: impl StartService) -> Router {
    build_app(
        readiness,
        starts,
        Arc::new(InMemorySessionStore::default()),
        default_session_policy(),
        false,
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
    )
}

pub fn app_with_services(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
) -> Router {
    build_app(readiness, starts, sessions, session_policy, true)
}

fn build_app(
    readiness: impl Readiness,
    starts: impl StartService,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
    authenticate_starts: bool,
) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/designs", get(designs))
        .route("/api/v1/coverage", get(coverage))
        .route("/api/v1/starts/resolve", post(resolve_start))
        .route("/api/v1/starts/{id}/confirm", post(confirm_start))
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
        .with_state(AppState {
            readiness: Arc::new(readiness),
            starts: Arc::new(starts),
            sessions,
            session_policy,
            authenticate_starts,
        })
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

pub struct ConfiguredStartService<P> {
    resolver: StartResolver<P>,
    data_version: String,
}

impl<P: StartResolverProvider> ConfiguredStartService<P> {
    pub fn new(resolver: StartResolver<P>, data_version: impl Into<String>) -> Self {
        Self {
            resolver,
            data_version: data_version.into(),
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
        self.resolver.confirm(
            id,
            &self.context(guest_session_id, profile),
            std::time::SystemTime::now(),
        )
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
) -> Result<String, axum::response::Response> {
    if state.authenticate_starts {
        let session = authenticate(state.sessions.as_ref(), headers, SystemTime::now())
            .await
            .map_err(auth_error)?;
        enforce_request_protection(&state.session_policy, &Method::POST, headers)
            .map_err(auth_error)?;
        Ok(session.id.to_string())
    } else {
        guest_session(headers)
            .map(str::to_owned)
            .map_err(|error| error.into_response())
    }
}

fn error_response(error: ResolveError) -> axum::response::Response {
    let status = match error {
        ResolveError::InvalidCoordinates => StatusCode::BAD_REQUEST,
        ResolveError::WrongSession => StatusCode::NOT_FOUND,
        ResolveError::ProviderFailure => StatusCode::BAD_GATEWAY,
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
        Err(response) => return response,
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
        Err(response) => return response,
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
