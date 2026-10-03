use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::{
    CoveragePayload, Readiness, RouteJobService, StartService, app_with_all_services,
    auth::{InMemorySessionStore, SessionPolicy},
};
use gpsart_domain::{
    RouteRequestProfile,
    starts::{Point, ResolutionId, ResolveError, StartResolution},
};
use gpsart_persistence::jobs::{JobError, JobState, JobStatusSnapshot, RouteJobSubmission};
use http_body_util::BodyExt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
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
struct NoStarts;
#[async_trait]
impl StartService for NoStarts {
    fn coverage(&self) -> CoveragePayload {
        CoveragePayload {
            version: "v".into(),
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

#[derive(Default)]
struct Jobs {
    keys: Mutex<HashMap<String, ([u8; 32], Uuid)>>,
    states: Mutex<HashMap<Uuid, JobState>>,
}
#[async_trait]
impl RouteJobService for Jobs {
    async fn submit(&self, _: Uuid, input: RouteJobSubmission) -> Result<Uuid, JobError> {
        if input.resolved_start_id == Uuid::nil() {
            return Err(JobError::StartNotFound);
        }
        let mut keys = self.keys.lock().unwrap();
        if let Some((hash, id)) = keys.get(&input.idempotency_key) {
            return if hash == &input.input_hash {
                Ok(*id)
            } else {
                Err(JobError::IdempotencyConflict)
            };
        }
        let id = Uuid::new_v4();
        keys.insert(input.idempotency_key, (input.input_hash, id));
        self.states.lock().unwrap().insert(id, JobState::Queued);
        Ok(id)
    }
    async fn cancel(&self, _: Uuid, id: Uuid) -> Result<JobState, JobError> {
        let mut states = self.states.lock().unwrap();
        let state = states.get_mut(&id).ok_or(JobError::NotFound)?;
        if !state.is_terminal() {
            *state = JobState::Cancelled;
        }
        Ok(*state)
    }
    async fn status(&self, _: Uuid, _: Uuid) -> Result<JobStatusSnapshot, JobError> {
        Err(JobError::Unavailable)
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

async fn auth(app: &axum::Router) -> (String, String) {
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
        .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_owned())
        .collect::<Vec<_>>();
    let csrf = cookies
        .iter()
        .find_map(|v| v.strip_prefix("gpsart_csrf="))
        .unwrap()
        .to_owned();
    (cookies.join("; "), csrf)
}

fn body(key: &str, distance: i32, start: Uuid) -> String {
    serde_json::json!({
    "resolved_start_id": start, "revision": 1, "idempotency_key": key,
    "requested_distance_metres": distance, "distance_tolerance_metres": 250,
        "profile": {"activity":"walking","profile_version":"walking-v1"},
        "coverage_policy_version":"coverage-v1",
    "template_id":"heart", "template_version":"1", "map_data_version":"map-v1", "algorithm_version":"algorithm-v1"
}).to_string()
}

async fn post(
    app: &axum::Router,
    cookie: &str,
    csrf: &str,
    payload: String,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::post("/api/v1/route-jobs")
                .header("content-type", "application/json")
                .header("cookie", cookie)
                .header("origin", "https://app.test")
                .header("x-csrf-token", csrf)
                .body(Body::from(payload))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn creation_is_validated_owned_and_idempotent() {
    let app = app_with_all_services(
        Healthy,
        NoStarts,
        Arc::new(InMemorySessionStore::default()),
        policy(),
        Jobs::default(),
    );
    let (cookie, csrf) = auth(&app).await;
    let start = Uuid::new_v4();
    let first = post(&app, &cookie, &csrf, body("key", 5_000, start)).await;
    assert_eq!(first.status(), 202);
    let first_body: serde_json::Value =
        serde_json::from_slice(&first.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let repeat = post(&app, &cookie, &csrf, body("key", 5_000, start)).await;
    let repeat_body: serde_json::Value =
        serde_json::from_slice(&repeat.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(first_body["job_id"], repeat_body["job_id"]);
    assert_eq!(
        post(&app, &cookie, &csrf, body("key", 6_000, start))
            .await
            .status(),
        409
    );
    assert_eq!(
        post(&app, &cookie, &csrf, body("missing", 5_000, Uuid::nil()))
            .await
            .status(),
        404
    );
    assert_eq!(
        post(&app, &cookie, &csrf, body("bad", 0, start))
            .await
            .status(),
        400
    );
    assert_eq!(
        post(&app, &cookie, &csrf, body("too-far", 100_001, start))
            .await
            .status(),
        400
    );
    let mut unknown_top: serde_json::Value =
        serde_json::from_str(&body("unknown-top", 5_000, start)).unwrap();
    unknown_top["invented"] = serde_json::json!(true);
    assert_eq!(
        post(&app, &cookie, &csrf, unknown_top.to_string())
            .await
            .status(),
        400
    );
    let mut unknown_profile: serde_json::Value =
        serde_json::from_str(&body("unknown-profile", 5_000, start)).unwrap();
    unknown_profile["profile"]["invented"] = serde_json::json!(true);
    assert_eq!(
        post(&app, &cookie, &csrf, unknown_profile.to_string())
            .await
            .status(),
        400
    );
    let mut unpublished: serde_json::Value =
        serde_json::from_str(&body("unknown-template", 5_000, start)).unwrap();
    unpublished["template_id"] = serde_json::json!("not-published");
    assert_eq!(
        post(&app, &cookie, &csrf, unpublished.to_string())
            .await
            .status(),
        400
    );
    let mut unsupported_algorithm: serde_json::Value =
        serde_json::from_str(&body("unknown-algorithm", 5_000, start)).unwrap();
    unsupported_algorithm["algorithm_version"] = serde_json::json!("client-invented-v99");
    assert_eq!(
        post(&app, &cookie, &csrf, unsupported_algorithm.to_string())
            .await
            .status(),
        400
    );
    let job_id = first_body["job_id"].as_str().unwrap();
    for _ in 0..2 {
        let cancelled = app
            .clone()
            .oneshot(
                Request::delete(format!("/api/v1/route-jobs/{job_id}"))
                    .header("cookie", &cookie)
                    .header("origin", "https://app.test")
                    .header("x-csrf-token", &csrf)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cancelled.status(), 200);
    }
}
