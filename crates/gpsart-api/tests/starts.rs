use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::{
    ConfiguredStartService, CoveragePayload, Readiness, StartService, app_with_services,
    app_with_starts,
    auth::{InMemorySessionStore, SessionPolicy},
};
use gpsart_domain::starts::{
    Coverage, NetworkPoint, Point, ProviderError, ResolutionId, ResolveError, StartResolution,
    StartResolver, StartResolverProvider,
};
use gpsart_domain::{ActivityProfile, RouteRequestProfile};
use gpsart_persistence::{ResolvedStartRecord, ResolvedStartStore, ResolvedStartStoreError};
use http_body_util::BodyExt;
use std::{
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

#[derive(Default)]
struct RecordingStartStore(Mutex<Vec<ResolvedStartRecord>>);

#[async_trait]
impl ResolvedStartStore for RecordingStartStore {
    async fn insert_resolved_start(
        &self,
        record: ResolvedStartRecord,
    ) -> Result<(), ResolvedStartStoreError> {
        self.0.lock().unwrap().push(record);
        Ok(())
    }
}

struct FailingStartStore;

#[async_trait]
impl ResolvedStartStore for FailingStartStore {
    async fn insert_resolved_start(
        &self,
        _: ResolvedStartRecord,
    ) -> Result<(), ResolvedStartStoreError> {
        Err(ResolvedStartStoreError::Unavailable)
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

struct OutcomeService;

#[async_trait]
impl StartService for OutcomeService {
    fn coverage(&self) -> CoveragePayload {
        CoveragePayload {
            version: "coverage-v1".into(),
            geojson: serde_json::json!({"type":"Feature"}),
            local_route_feasibility: "not_evaluated",
        }
    }

    async fn resolve(
        &self,
        _: &str,
        requested: Point,
        _: RouteRequestProfile,
    ) -> Result<StartResolution, ResolveError> {
        Err(match requested.longitude as i32 {
            70 => ResolveError::UnsupportedCoverage,
            71 => ResolveError::NoRoutableAccess,
            _ => ResolveError::ProviderFailure,
        })
    }

    async fn confirm(
        &self,
        guest: &str,
        _: ResolutionId,
        _: RouteRequestProfile,
    ) -> Result<Point, ResolveError> {
        Err(match guest {
            "expired" => ResolveError::ExpiredResolution,
            "wrong" => ResolveError::WrongSession,
            "profile" => ResolveError::ProfileChanged,
            "policy" => ResolveError::CoveragePolicyChanged,
            _ => ResolveError::DataVersionChanged,
        })
    }
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
async fn confirmation_persists_the_exact_owned_versioned_start_for_job_submission() {
    let coverage = Coverage::rectangle("coverage-v1", 77.0, 28.0, 78.0, 29.0).unwrap();
    let resolver = StartResolver::new(
        coverage,
        FixedProvider(Some(NetworkPoint {
            point: Point::new(77.2001, 28.6001).unwrap(),
            snap_distance_metres: 15,
        })),
        Duration::from_secs(300),
    );
    let store = Arc::new(RecordingStartStore::default());
    let service = ConfiguredStartService::with_store(resolver, "map-v1", store.clone());
    let owner = Uuid::new_v4();
    let profile = RouteRequestProfile::new(ActivityProfile::Walking);
    let resolution = service
        .resolve(&owner.to_string(), Point::new(77.2, 28.6).unwrap(), profile)
        .await
        .unwrap();

    service
        .confirm(&owner.to_string(), resolution.id, profile)
        .await
        .unwrap();

    let records = store.0.lock().unwrap();
    assert_eq!(records.len(), 1);
    let record = &records[0];
    assert_eq!(record.id, resolution.id.as_uuid());
    assert_eq!(record.guest_session_id, owner);
    assert_eq!(record.requested_longitude, 77.2);
    assert_eq!(record.requested_latitude, 28.6);
    assert_eq!(record.confirmed_longitude, 77.2001);
    assert_eq!(record.confirmed_latitude, 28.6001);
    assert_eq!(record.profile_version, "walking-v1");
    assert_eq!(record.coverage_policy_version, "coverage-v1");
    assert_eq!(record.map_data_version, "map-v1");
}

#[tokio::test]
async fn confirmation_reports_storage_unavailable_without_blaming_the_route_provider() {
    let coverage = Coverage::rectangle("coverage-v1", 77.0, 28.0, 78.0, 29.0).unwrap();
    let resolver = StartResolver::new(
        coverage,
        FixedProvider(Some(NetworkPoint {
            point: Point::new(77.2001, 28.6001).unwrap(),
            snap_distance_metres: 15,
        })),
        Duration::from_secs(300),
    );
    let app = app_with_starts(
        Healthy,
        ConfiguredStartService::with_store(resolver, "map-v1", Arc::new(FailingStartStore)),
    );
    let owner = Uuid::new_v4().to_string();
    let resolved = app
        .clone()
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", &owner)
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
                .header("x-gpsart-guest-session", owner)
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
    assert_eq!(response.status(), 503);
    assert_eq!(
        json(response).await["outcome"]["code"],
        "storage_unavailable"
    );
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

#[tokio::test]
async fn every_resolution_failure_has_a_stable_http_status_and_code() {
    let malformed = app_with_starts(Healthy, OutcomeService)
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "guest-a")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(malformed.status(), 400);
    assert_eq!(json(malformed).await["outcome"]["code"], "invalid_request");

    let unauthorized = app_with_starts(Healthy, OutcomeService)
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::json!({
                        "requested_point": {"longitude": 70.0, "latitude": 28.6},
                        "profile": {"activity": "walking", "profile_version": "walking-v1"}
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), 401);
    assert_eq!(
        json(unauthorized).await["outcome"]["code"],
        "authorization_required"
    );

    for (longitude, status, code) in [
        (70.0, 422, "unsupported_coverage"),
        (71.0, 422, "no_routable_access"),
        (72.0, 502, "provider_failure"),
    ] {
        let response = app_with_starts(Healthy, OutcomeService)
            .oneshot(
                Request::post("/api/v1/starts/resolve")
                    .header("content-type", "application/json")
                    .header("x-gpsart-guest-session", "guest-a")
                    .body(Body::from(
                        serde_json::json!({
                            "requested_point": {"longitude": longitude, "latitude": 28.6},
                            "profile": {"activity": "walking", "profile_version": "walking-v1"}
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), status);
        assert_eq!(json(response).await["outcome"]["code"], code);
    }

    let id = "7bf80ccf-68e8-49ec-a3e8-cad667bb95a0";
    for (guest, status, code) in [
        ("expired", 410, "expired_resolution"),
        ("wrong", 404, "wrong_session"),
        ("profile", 409, "profile_changed"),
        ("policy", 409, "coverage_policy_changed"),
        ("data", 409, "data_version_changed"),
    ] {
        let response = app_with_starts(Healthy, OutcomeService)
            .oneshot(
                Request::post(format!("/api/v1/starts/{id}/confirm"))
                    .header("content-type", "application/json")
                    .header("x-gpsart-guest-session", guest)
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
        assert_eq!(response.status(), status);
        assert_eq!(json(response).await["outcome"]["code"], code);
    }
}

#[tokio::test]
async fn integrated_start_mutations_require_private_cookie_and_csrf() {
    let policy = SessionPolicy::new(
        "https://app.gpsart.test",
        true,
        Duration::from_secs(86_400),
        Duration::from_secs(43_200),
    )
    .unwrap();
    let app = app_with_services(
        Healthy,
        OutcomeService,
        Arc::new(InMemorySessionStore::default()),
        policy,
    );
    let issued = app
        .clone()
        .oneshot(
            Request::post("/api/v1/guest-sessions")
                .header("origin", "https://app.gpsart.test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookies = issued
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
        .unwrap();
    let body = serde_json::json!({
        "requested_point": {"longitude": 70.0, "latitude": 28.6},
        "profile": {"activity": "walking", "profile_version": "walking-v1"}
    });
    let response = app
        .clone()
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("cookie", cookies.join("; "))
                .header("origin", "https://app.gpsart.test")
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 422);

    let legacy_header_only = app
        .oneshot(
            Request::post("/api/v1/starts/resolve")
                .header("content-type", "application/json")
                .header("x-gpsart-guest-session", "bypass")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(legacy_header_only.status(), 401);
}
