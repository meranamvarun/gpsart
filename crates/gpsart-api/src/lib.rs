use async_trait::async_trait;
use axum::{Json, Router, extract::State, http::StatusCode, response::IntoResponse, routing::get};
use gpsart_persistence::{DependencyProbe, PostgresProbe};
use serde::Serialize;
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
}

#[derive(Serialize)]
struct HealthBody {
    status: &'static str,
    code: &'static str,
}

pub fn app(readiness: impl Readiness) -> Router {
    Router::new()
        .route("/health/live", get(live))
        .route("/health/ready", get(ready))
        .with_state(AppState {
            readiness: Arc::new(readiness),
        })
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
