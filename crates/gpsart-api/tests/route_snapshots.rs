use async_trait::async_trait;
use axum::{body::Body, http::Request};
use gpsart_api::auth::{InMemorySessionStore, SessionPolicy};
use gpsart_api::{Readiness, RouteSnapshotService, app_with_routes};
use gpsart_persistence::routes::{
    CandidateProvenance, CandidateQualifications, CandidateSnapshot, DISTANCE_TOLERANCE_METRES,
    DistanceSummary, GeoJsonLineString, InstructionKind, QualityBand, RouteInstruction,
    RouteSnapshotError, RouteTraversal, SnapshotVersions, TraversalClass, TraversalDirection,
};
use http_body_util::BodyExt;
use std::io::Cursor;
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

#[derive(Clone)]
struct Routes(Arc<Mutex<Option<(Uuid, CandidateSnapshot)>>>);

#[async_trait]
impl RouteSnapshotService for Routes {
    async fn get(&self, owner: Uuid, id: Uuid) -> Result<CandidateSnapshot, RouteSnapshotError> {
        let mut slot = self.0.lock().unwrap();
        let (bound_owner, snapshot) = slot.as_mut().ok_or(RouteSnapshotError::NotFound)?;
        if snapshot.id != id {
            return Err(RouteSnapshotError::NotFound);
        }
        if *bound_owner == Uuid::nil() {
            *bound_owner = owner;
        }
        if *bound_owner != owner {
            return Err(RouteSnapshotError::NotFound);
        }
        Ok(snapshot.clone())
    }
}

fn snapshot() -> CandidateSnapshot {
    CandidateSnapshot {
        id: Uuid::parse_str("40000000-0000-4000-8000-000000000001").unwrap(),
        generation_job_id: Uuid::parse_str("30000000-0000-4000-8000-000000000001").unwrap(),
        candidate_version: 1,
        request_revision: 4,
        geometry: GeoJsonLineString {
            coordinates: vec![
                [77.6000, 12.9000],
                [77.6010, 12.9010],
                [77.6000, 12.9010],
                [77.6010, 12.9000],
                [77.6000, 12.9000],
            ],
        },
        traversals: vec![
            RouteTraversal {
                edge_id: "art".into(),
                class: TraversalClass::Artwork,
                direction: TraversalDirection::Forward,
                begin_geometry_index: 0,
                end_geometry_index: 2,
                distance_metres: 600,
                retraced: false,
            },
            RouteTraversal {
                edge_id: "connector".into(),
                class: TraversalClass::Connector,
                direction: TraversalDirection::Reverse,
                begin_geometry_index: 2,
                end_geometry_index: 4,
                distance_metres: 400,
                retraced: true,
            },
        ],
        instructions: vec![
            RouteInstruction {
                kind: InstructionKind::Depart,
                geometry_index: 0,
                provider_code: None,
            },
            RouteInstruction {
                kind: InstructionKind::Via,
                geometry_index: 2,
                provider_code: None,
            },
            RouteInstruction {
                kind: InstructionKind::Arrive,
                geometry_index: 4,
                provider_code: None,
            },
        ],
        duration_seconds: 720,
        distances: DistanceSummary {
            artwork_metres: 600,
            connector_metres: 400,
            total_metres: 1_000,
            repeated_metres: 400,
            repeated_included_in_total: true,
            equality_tolerance_metres: DISTANCE_TOLERANCE_METRES,
        },
        quality_band: QualityBand::ApproximateMatch,
        qualifications: CandidateQualifications {
            legality_version: "candidate-legality-v1".into(),
            route_use_advisory_version: "route-use-advisory-v1".into(),
            access_verified: true,
            direction_verified: true,
            turns_verified: true,
        },
        versions: SnapshotVersions {
            snapshot: "route-candidate-snapshot-v1".into(),
            request: "route-request-v1".into(),
            profile: "walking-v1".into(),
            template_id: "heart".into(),
            template: 1,
            template_hash: "f".repeat(64),
            algorithm: "algorithm-v1".into(),
            routing_data: "map-v1".into(),
            routing_policy: "osm-pedestrian-policy-v2".into(),
            candidate: 1,
        },
        generated_at_epoch_seconds: 1_800_000_000,
        provenance: CandidateProvenance {
            routing_provider: "controlled-local-osm".into(),
            attribution: Some("OpenStreetMap contributors (ODbL 1.0)".into()),
        },
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

async fn auth(app: &axum::Router) -> String {
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
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .map(|value| value.to_str().unwrap().split(';').next().unwrap())
        .collect::<Vec<_>>()
        .join("; ")
}

#[tokio::test]
async fn owned_route_serves_one_exact_ordered_snapshot_after_job_completion() {
    let expected = snapshot();
    let id = expected.id;
    let app = app_with_routes(
        Healthy,
        Arc::new(InMemorySessionStore::default()),
        policy(),
        Routes(Arc::new(Mutex::new(Some((Uuid::nil(), expected))))),
    );
    let owner = auth(&app).await;
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/routes/{id}"))
                .header("cookie", &owner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    let body: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["geometry"]["type"], "LineString");
    assert_eq!(
        body["geometry"]["coordinates"][1],
        serde_json::json!([77.601, 12.901])
    );
    assert_eq!(
        body["geometry"]["coordinates"][3],
        serde_json::json!([77.601, 12.9])
    );
    assert_eq!(body["start"], serde_json::json!([77.6, 12.9]));
    assert_eq!(body["finish"], serde_json::json!([77.6, 12.9]));
    assert_eq!(body["duration_seconds"], 720);
    assert_eq!(body["instructions"].as_array().unwrap().len(), 3);
    assert_eq!(body["instructions"][0]["kind"], "depart");
    assert_eq!(body["instructions"][2]["kind"], "arrive");
    assert_eq!(
        body["direction_markers"][1]["coordinate"],
        serde_json::json!([77.6, 12.901])
    );
    assert!(body["traversals"][0].get("surface").is_none());
    assert_eq!(body["versions"]["request"], "route-request-v1");
    assert_eq!(body["generated_at_epoch_seconds"], 1_800_000_000_u64);

    let foreign = auth(&app).await;
    let denied = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/routes/{id}"))
                .header("cookie", foreign)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), 404);
}

#[tokio::test]
async fn owned_route_exports_exact_gpx_with_safe_download_headers() {
    let expected = snapshot();
    let expected_points = expected.geometry.coordinates.clone();
    let id = expected.id;
    let app = app_with_routes(
        Healthy,
        Arc::new(InMemorySessionStore::default()),
        policy(),
        Routes(Arc::new(Mutex::new(Some((Uuid::nil(), expected))))),
    );
    let unauthenticated = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/routes/{id}/export.gpx"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(unauthenticated.status(), 401);
    let owner = auth(&app).await;
    let response = app
        .clone()
        .oneshot(
            Request::get(format!("/api/v1/routes/{id}/export.gpx"))
                .header("cookie", &owner)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers()["content-type"],
        "application/gpx+xml; charset=utf-8"
    );
    assert_eq!(response.headers()["x-content-type-options"], "nosniff");
    assert_eq!(response.headers()["cache-control"], "private, no-store");
    assert_eq!(
        response.headers()["content-disposition"],
        format!("attachment; filename=\"gpsart-heart-{id}-v1.gpx\"")
    );
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let exported = std::str::from_utf8(&bytes).expect("GPX export is UTF-8");
    assert!(exported.contains("OpenStreetMap contributors"));
    assert_eq!(
        bytes.as_ref(),
        include_bytes!("../../../tests/fixtures/route-export.gpx")
    );

    let imported = gpx::read(Cursor::new(bytes)).expect("external GPX importer accepts export");
    assert_eq!(imported.version, gpx::GpxVersion::Gpx11);
    let points = &imported.tracks[0].segments[0].points;
    let coordinates = points
        .iter()
        .map(|point| [point.point().x(), point.point().y()])
        .collect::<Vec<_>>();
    assert_eq!(coordinates, expected_points);
    assert!(points.iter().all(|point| point.time.is_none()));

    let foreign = auth(&app).await;
    let denied = app
        .oneshot(
            Request::get(format!("/api/v1/routes/{id}/export.gpx"))
                .header("cookie", foreign)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(denied.status(), 404);
}
