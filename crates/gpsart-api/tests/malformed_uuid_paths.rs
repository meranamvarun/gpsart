use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::{
    CoveragePayload, Readiness, RouteJobService, RouteSnapshotService, StartService,
    app_with_all_services_and_routes,
    auth::{InMemorySessionStore, SessionPolicy},
};
use gpsart_domain::{
    RouteRequestProfile,
    starts::{Point, ResolutionId, ResolveError, StartResolution},
};
use gpsart_persistence::{
    jobs::{JobError, JobState, JobStatusSnapshot, RouteJobSubmission},
    routes::{CandidateSnapshot, RouteSnapshotError},
};
use http_body_util::BodyExt;
use std::{sync::Arc, time::Duration};
use tower::ServiceExt;
use uuid::Uuid;

#[derive(Clone)]
struct Healthy;

#[async_trait]
impl Readiness for Healthy {
    async fn check(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

struct UnusedStarts;

#[async_trait]
impl StartService for UnusedStarts {
    fn coverage(&self) -> CoveragePayload {
        CoveragePayload {
            version: "unused".into(),
            geojson: serde_json::json!({}),
            local_route_feasibility: "not_evaluated",
        }
    }

    async fn resolve(
        &self,
        _: &str,
        _: Point,
        _: RouteRequestProfile,
    ) -> Result<StartResolution, ResolveError> {
        panic!("malformed paths must be rejected before the service is called")
    }

    async fn confirm(
        &self,
        _: &str,
        _: ResolutionId,
        _: RouteRequestProfile,
    ) -> Result<Point, ResolveError> {
        panic!("malformed paths must be rejected before the service is called")
    }
}

struct UnusedJobs;

#[async_trait]
impl RouteJobService for UnusedJobs {
    async fn submit(&self, _: Uuid, _: RouteJobSubmission) -> Result<Uuid, JobError> {
        panic!("not used by malformed-path contract tests")
    }

    async fn cancel(&self, _: Uuid, _: Uuid) -> Result<JobState, JobError> {
        panic!("malformed paths must be rejected before the service is called")
    }

    async fn status(&self, _: Uuid, _: Uuid) -> Result<JobStatusSnapshot, JobError> {
        panic!("malformed paths must be rejected before the service is called")
    }
}

struct UnusedRoutes;

#[async_trait]
impl RouteSnapshotService for UnusedRoutes {
    async fn get(&self, _: Uuid, _: Uuid) -> Result<CandidateSnapshot, RouteSnapshotError> {
        panic!("malformed paths must be rejected before the service is called")
    }
}

fn policy() -> SessionPolicy {
    SessionPolicy::new(
        "https://app.test",
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )
    .unwrap()
}

async fn credentials(app: &axum::Router) -> (String, String) {
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/v1/guest-sessions")
                .header("origin", "https://app.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookies = response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|value| {
            value
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned()
        })
        .collect::<Vec<_>>();
    let csrf = cookies
        .iter()
        .find_map(|cookie| cookie.strip_prefix("gpsart_csrf="))
        .unwrap()
        .to_owned();
    (cookies.join("; "), csrf)
}

#[tokio::test]
async fn every_public_uuid_path_returns_the_documented_json_error() {
    let app = app_with_all_services_and_routes(
        Healthy,
        UnusedStarts,
        Arc::new(InMemorySessionStore::default()),
        policy(),
        UnusedJobs,
        UnusedRoutes,
    );
    let (cookie, csrf) = credentials(&app).await;
    let cases = [
        ("GET", "/api/v1/route-jobs/not-a-uuid"),
        ("GET", "/api/v1/routes/not-a-uuid"),
        ("GET", "/api/v1/routes/not-a-uuid/export.gpx"),
        ("DELETE", "/api/v1/route-jobs/not-a-uuid"),
        ("POST", "/api/v1/starts/not-a-uuid/confirm"),
    ];

    for (method, uri) in cases {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header("cookie", &cookie)
            .header("origin", "https://app.test")
            .header("x-csrf-token", &csrf);
        if method == "POST" {
            request = request.header("content-type", "application/json");
        }
        let body = if method == "POST" {
            Body::from(r#"{"profile":{"activity":"walking","profile_version":"walking-v1"}}"#)
        } else {
            Body::empty()
        };
        let response = app
            .clone()
            .oneshot(request.body(body).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), 400, "{method} {uri}");
        assert_eq!(
            response.headers()["content-type"],
            "application/json",
            "{method} {uri}"
        );
        let payload: serde_json::Value =
            serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes())
                .unwrap();
        assert_eq!(
            payload,
            serde_json::json!({ "code": "invalid_path_parameter" }),
            "{method} {uri}"
        );
    }
}
