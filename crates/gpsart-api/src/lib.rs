use async_trait::async_trait;
use axum::{
    Json, Router,
    extract::{Query, State},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    response::IntoResponse,
    routing::get,
};
use gpsart_domain::templates::{Category, embedded_catalogue, render_svg};
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
    sessions: Arc<dyn GuestSessionStore>,
    session_policy: SessionPolicy,
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
    app_with_auth(
        readiness,
        Arc::new(InMemorySessionStore::default()),
        SessionPolicy::new(
            "https://localhost",
            true,
            Duration::from_secs(86_400),
            Duration::from_secs(43_200),
        )
        .expect("default session policy"),
    )
}

pub fn app_with_auth(
    readiness: impl Readiness,
    sessions: Arc<impl GuestSessionStore>,
    session_policy: SessionPolicy,
) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .route("/api/v1/designs", get(designs))
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
            sessions,
            session_policy,
        })
}

#[derive(Serialize)]
struct SessionBody {
    expires_at: u64,
    rotate_after: u64,
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
                Json(SessionBody {
                    expires_at: issued
                        .record
                        .expires_at
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs(),
                    rotate_after: state.session_policy.rotate_after.as_secs(),
                }),
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
        Json(SessionBody {
            expires_at: issued
                .record
                .expires_at
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            rotate_after: state.session_policy.rotate_after.as_secs(),
        }),
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
