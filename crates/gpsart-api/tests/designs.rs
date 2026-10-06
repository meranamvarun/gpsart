use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::{Readiness, app};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[derive(Clone)]
struct Healthy;

#[async_trait]
impl Readiness for Healthy {
    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

#[tokio::test]
async fn designs_returns_only_published_templates_and_filters_category() {
    let response = app(Healthy)
        .oneshot(
            Request::get("/api/v1/designs?category=text")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["designs"].as_array().unwrap().len(), 1);
    assert_eq!(json["designs"][0]["id"], "HI");
    assert_eq!(json["designs"][0]["version"], 1);
    assert_eq!(json["designs"][0]["status"], "published");
    assert_eq!(
        json["designs"][0]["accessible_name"],
        "HI text route design"
    );
    assert!(
        json["designs"][0]["preview_svg"]
            .as_str()
            .unwrap()
            .starts_with("<svg")
    );
}

#[tokio::test]
async fn designs_rejects_unknown_category_without_guessing() {
    let response = app(Healthy)
        .oneshot(
            Request::get("/api/v1/designs?category=words")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
