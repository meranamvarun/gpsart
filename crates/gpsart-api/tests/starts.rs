use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::{ConfiguredStartService, Readiness, app_with_starts};
use gpsart_domain::starts::{
    Coverage, NetworkPoint, Point, ProviderError, StartResolver, StartResolverProvider,
};
use gpsart_domain::{ActivityProfile, RouteRequestProfile};
use http_body_util::BodyExt;
use std::time::Duration;
use tower::ServiceExt;

#[derive(Clone)]
struct Healthy;

#[async_trait]
impl Readiness for Healthy {
    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

struct FixedProvider(Option<NetworkPoint>);

impl StartResolverProvider for FixedProvider {
    fn nearest_routable(
        &self,
        _: Point,
        _: RouteRequestProfile,
    ) -> Result<Option<NetworkPoint>, ProviderError> {
        Ok(self.0)
    }
}

fn router(snap_distance_metres: u32) -> axum::Router {
    let coverage = Coverage::rectangle("coverage-v1", 77.0, 28.0, 78.0, 29.0).unwrap();
    let resolver = StartResolver::new(
        coverage,
        FixedProvider(Some(NetworkPoint {
            point: Point::new(77.2001, 28.6001).unwrap(),
            snap_distance_metres,
        })),
        Duration::from_secs(300),
    );
    app_with_starts(Healthy, ConfiguredStartService::new(resolver, "map-v1"))
}

async fn json(response: axum::response::Response) -> serde_json::Value {
    serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

#[tokio::test]
async fn coverage_is_versioned_geojson_without_a_feasibility_claim() {
    let response = router(10)
        .oneshot(
            Request::get("/api/v1/coverage")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = json(response).await;
    assert_eq!(body["version"], "coverage-v1");
    assert_eq!(body["geojson"]["type"], "Feature");
    assert_eq!(body["local_route_feasibility"], "not_evaluated");
}

#[tokio::test]
async fn resolution_and_confirmation_keep_coordinates_in_private_post_bodies() {
    let app = router(30);
    let request_body = serde_json::json!({
        "requested_point": {"longitude": 77.2, "latitude": 28.6},
        "profile": {"activity": "walking", "profile_version": "walking-v1"}
    });
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "guest-a")
                .body(Body::from(request_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let body = json(response).await;
    assert_eq!(body["requested_point"]["longitude"], 77.2);
    assert_eq!(body["candidate_network_point"]["longitude"], 77.2001);
    assert_eq!(body["snap_distance_metres"], 30);
    assert_eq!(body["profile_version"], "walking-v1");
    assert_eq!(body["data_version"], "map-v1");
    assert_eq!(body["confirmation_required"], true);

    let id = body["id"].as_str().unwrap();
    let confirm_uri = format!("/api/v1/starts/{id}/confirm");
    assert!(!confirm_uri.contains("77.2"));
    let response = app
        .oneshot(
            Request::post(confirm_uri)
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "guest-a")
                .body(Body::from(
                    serde_json::json!({
                        "profile": {"activity": "walking", "profile_version": "walking-v1"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
}

#[tokio::test]
async fn invalid_coordinates_and_long_snap_are_machine_readable() {
    let invalid = router(5)
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "guest-a")
                .body(Body::from(
                    serde_json::json!({
                        "requested_point": {"longitude": 181.0, "latitude": 28.6},
                        "profile": {"activity": "walking", "profile_version": "walking-v1"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(invalid.status(), 400);
    assert_eq!(
        json(invalid).await["outcome"]["code"],
        "invalid_coordinates"
    );

    let app = router(31);
    let resolved = app
        .clone()
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "guest-a")
                .body(Body::from(
                    serde_json::json!({
                        "requested_point": {"longitude": 77.2, "latitude": 28.6},
                        "profile": {"activity": "walking", "profile_version": "walking-v1"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let id = json(resolved).await["id"].as_str().unwrap().to_owned();
    let response = app
        .oneshot(
            Request::post(format!("/api/v1/starts/{id}/confirm"))
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "guest-a")
                .body(Body::from(
                    serde_json::json!({
                        "profile": {"activity": ActivityProfile::Walking, "profile_version": "walking-v1"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 422);
    assert_eq!(json(response).await["outcome"]["code"], "snap_too_far");
}
