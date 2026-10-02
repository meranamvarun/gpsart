use async_trait::async_trait;
use axum::body::Body;
use gpsart_api::{Readiness, app, serve};
use http::{Request, StatusCode};
use http_body_util::BodyExt;
use tower::ServiceExt;

struct Unavailable;

#[async_trait]
impl Readiness for Unavailable {
    async fn check(&self) -> Result<(), &'static str> {
        Err("postgres_unavailable")
    }
}

#[tokio::test]
async fn server_stops_when_the_shutdown_signal_completes() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(serve(listener, Unavailable, async move {
        let _ = shutdown_rx.await;
    }));
    shutdown_tx.send(()).unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(1), server)
        .await
        .expect("server stops before timeout")
        .expect("server task")
        .expect("clean shutdown");
}

#[tokio::test]
async fn liveness_is_independent_but_readiness_reflects_postgres() {
    let service = app(Unavailable);
    let live = service
        .clone()
        .oneshot(Request::get("/health/live").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(live.status(), StatusCode::OK);
    let ready = service
        .oneshot(Request::get("/health/ready").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = ready.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap()["code"],
        "postgres_unavailable"
    );
}
