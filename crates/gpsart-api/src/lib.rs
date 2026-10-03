use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::rejection::JsonRejection,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
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
use gpsart_persistence::{DependencyProbe, PostgresProbe};
use serde::{Deserialize, Serialize};
use std::{future::Future, sync::Arc};

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
    app_with_starts(readiness, UnavailableStartService)
}

pub fn app_with_starts(readiness: impl Readiness, starts: impl StartService) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/designs", get(designs))
        .route("/api/v1/coverage", get(coverage))
        .route("/api/v1/starts/resolve", post(resolve_start))
        .route("/api/v1/starts/{id}/confirm", post(confirm_start))
        .with_state(AppState {
            readiness: Arc::new(readiness),
            starts: Arc::new(starts),
        })
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
    let guest = match guest_session(&headers) {
        Ok(value) => value,
        Err(value) => return value.into_response(),
    };
    let requested = match Point::new(
        body.requested_point.longitude,
        body.requested_point.latitude,
    ) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    match state.starts.resolve(guest, requested, body.profile).await {
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
    let guest = match guest_session(&headers) {
        Ok(value) => value,
        Err(value) => return value.into_response(),
    };
    match state.starts.confirm(guest, id, body.profile).await {
        Ok(confirmed_start) => (
            StatusCode::OK,
            Json(serde_json::json!({ "confirmed_start": confirmed_start })),
        )
            .into_response(),
        Err(error) => error_response(error),
    }
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
