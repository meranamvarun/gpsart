use async_trait::async_trait;
use gpsart_domain::{ActivityProfile, RouteRequestProfile};
use gpsart_routing::{
    AttemptControl, GeoPointE7, Qualification, RouteProvider, RouteQuery, RoutingError,
    ValhallaAdapter, ValhallaTransport,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Default)]
struct FixtureTransport {
    requests: Arc<Mutex<Vec<Value>>>,
}

#[async_trait]
impl ValhallaTransport for FixtureTransport {
    async fn post_route(&self, request: Value) -> Result<Value, RoutingError> {
        self.requests.lock().unwrap().push(request);
        Ok(json!({
            "trip": {
                "summary": {"length": 0.024},
                "legs": [{
                    "shape": "??A?A?A?",
                    "maneuvers": [
                        {"type": 1, "begin_shape_index": 0},
                        {"type": 10, "begin_shape_index": 1},
                        {"type": 4, "begin_shape_index": 3}
                    ]
                }]
            }
        }))
    }

    async fn post_trace_attributes(&self, request: Value) -> Result<Value, RoutingError> {
        self.requests.lock().unwrap().push(request);
        Ok(json!({
            "edges": [
                {"id": 42, "length": 0.008, "begin_shape_index": 0, "end_shape_index": 1},
                {"id": 43, "length": 0.008, "begin_shape_index": 1, "end_shape_index": 2},
                {"id": 42, "length": 0.008, "begin_shape_index": 2, "end_shape_index": 3}
            ]
        }))
    }
}

fn point(latitude_e7: i32, longitude_e7: i32) -> GeoPointE7 {
    GeoPointE7::new(latitude_e7, longitude_e7)
}

#[tokio::test]
async fn valhalla_adapter_preserves_anchor_order_edges_geometry_and_repetition() {
    let transport = FixtureTransport::default();
    let adapter = ValhallaAdapter::new(
        transport.clone(),
        Some("tileset-2026-10-02".into()),
        "OpenStreetMap contributors",
    );
    let query = RouteQuery::from_points(
        RouteRequestProfile::new(ActivityProfile::Cycling),
        [point(0, 0), point(0, 10_000), point(0, 30_000)],
    );
    let route = adapter
        .route(
            &query,
            &AttemptControl::with_timeout(Duration::from_secs(1), 1),
        )
        .await
        .expect("captured provider response");

    assert_eq!(
        route
            .edges
            .iter()
            .map(|edge| edge.edge_id.as_str())
            .collect::<Vec<_>>(),
        ["42", "43", "42"]
    );
    assert_eq!(
        route.geometry,
        [point(0, 0), point(10, 0), point(20, 0), point(30, 0)]
    );
    assert_eq!(route.distance.value(), 24);
    assert!(
        route
            .edges
            .iter()
            .all(|edge| edge.qualifications == [Qualification::ProviderAccessUnverified])
    );
    assert_eq!(
        route.provenance.data_version.as_deref(),
        Some("tileset-2026-10-02")
    );

    let requests = transport.requests.lock().unwrap();
    assert_eq!(requests[0]["costing"], "bicycle");
    assert_eq!(requests[0]["locations"][1]["type"], "through");
    assert_eq!(requests[1]["shape_match"], "edge_walk");
}

#[tokio::test]
async fn walking_and_running_use_pedestrian_not_car_costing() {
    for activity in [ActivityProfile::Walking, ActivityProfile::Running] {
        let transport = FixtureTransport::default();
        let adapter = ValhallaAdapter::new(transport.clone(), None, "OpenStreetMap contributors");
        let query = RouteQuery::from_points(
            RouteRequestProfile::new(activity),
            [point(0, 0), point(0, 30_000)],
        );
        adapter
            .route(
                &query,
                &AttemptControl::with_timeout(Duration::from_secs(1), 1),
            )
            .await
            .unwrap();
        assert_eq!(
            transport.requests.lock().unwrap()[0]["costing"],
            "pedestrian"
        );
    }
}

#[derive(Clone)]
struct MissingEdgesTransport;

#[async_trait]
impl ValhallaTransport for MissingEdgesTransport {
    async fn post_route(&self, _request: Value) -> Result<Value, RoutingError> {
        FixtureTransport::default().post_route(json!({})).await
    }

    async fn post_trace_attributes(&self, _request: Value) -> Result<Value, RoutingError> {
        Ok(json!({"edges": []}))
    }
}

#[derive(Clone)]
struct ResponseTransport {
    route: Value,
    trace: Value,
}

#[async_trait]
impl ValhallaTransport for ResponseTransport {
    async fn post_route(&self, _request: Value) -> Result<Value, RoutingError> {
        Ok(self.route.clone())
    }

    async fn post_trace_attributes(&self, _request: Value) -> Result<Value, RoutingError> {
        Ok(self.trace.clone())
    }
}

fn valid_route_response() -> Value {
    json!({
        "trip": {
            "summary": {"length": 0.024},
            "legs": [{
                "shape": "??A?A?A?",
                "maneuvers": [
                    {"type": 1, "begin_shape_index": 0},
                    {"type": 10, "begin_shape_index": 1},
                    {"type": 4, "begin_shape_index": 3}
                ]
            }]
        }
    })
}

fn valid_trace_response() -> Value {
    json!({
        "edges": [
            {"id": 42, "length": 0.008, "begin_shape_index": 0, "end_shape_index": 1},
            {"id": 43, "length": 0.008, "begin_shape_index": 1, "end_shape_index": 2},
            {"id": 42, "length": 0.008, "begin_shape_index": 2, "end_shape_index": 3}
        ]
    })
}

async fn assert_provider_response_is_rejected(route: Value, trace: Value) {
    let adapter = ValhallaAdapter::new(
        ResponseTransport { route, trace },
        None,
        "OpenStreetMap contributors",
    );
    let query = RouteQuery::from_points(
        RouteRequestProfile::new(ActivityProfile::Walking),
        [point(0, 0), point(0, 30_000)],
    );
    assert_eq!(
        adapter
            .route(
                &query,
                &AttemptControl::with_timeout(Duration::from_secs(1), 1),
            )
            .await,
        Err(RoutingError::InvalidProviderResponse),
    );
}

#[tokio::test]
async fn missing_provider_edges_is_an_error_not_fabricated_connectivity() {
    let adapter = ValhallaAdapter::new(MissingEdgesTransport, None, "OpenStreetMap contributors");
    let query = RouteQuery::from_points(
        RouteRequestProfile::new(ActivityProfile::Walking),
        [point(0, 0), point(0, 30_000)],
    );
    assert_eq!(
        adapter
            .route(
                &query,
                &AttemptControl::with_timeout(Duration::from_secs(1), 1)
            )
            .await,
        Err(RoutingError::InvalidProviderResponse),
    );
}

#[tokio::test]
async fn inconsistent_edge_coverage_and_distance_are_rejected() {
    let invalid_traces = [
        json!({"edges": [
            {"id": 42, "length": 0.008, "begin_shape_index": 1, "end_shape_index": 2},
            {"id": 43, "length": 0.008, "begin_shape_index": 2, "end_shape_index": 3}
        ]}),
        json!({"edges": [
            {"id": 42, "length": 0.008, "begin_shape_index": 0, "end_shape_index": 1},
            {"id": 43, "length": 0.008, "begin_shape_index": 2, "end_shape_index": 3}
        ]}),
        json!({"edges": [
            {"id": 42, "length": 0.008, "begin_shape_index": 0, "end_shape_index": 2},
            {"id": 43, "length": 0.008, "begin_shape_index": 1, "end_shape_index": 3}
        ]}),
        json!({"edges": [
            {"id": 42, "length": 0.100, "begin_shape_index": 0, "end_shape_index": 1},
            {"id": 43, "length": 0.100, "begin_shape_index": 1, "end_shape_index": 2},
            {"id": 44, "length": 0.100, "begin_shape_index": 2, "end_shape_index": 3}
        ]}),
    ];
    for trace in invalid_traces {
        assert_provider_response_is_rejected(valid_route_response(), trace).await;
    }
}

#[tokio::test]
async fn missing_out_of_bounds_or_reordered_manoeuvres_are_rejected() {
    let invalid_manoeuvres = [
        json!([]),
        json!([
            {"type": 1, "begin_shape_index": 0},
            {"type": 4, "begin_shape_index": 4}
        ]),
        json!([
            {"type": 1, "begin_shape_index": 0},
            {"type": 10, "begin_shape_index": 2},
            {"type": 10, "begin_shape_index": 1},
            {"type": 4, "begin_shape_index": 3}
        ]),
        json!([
            {"type": 1, "begin_shape_index": 1},
            {"type": 4, "begin_shape_index": 3}
        ]),
        json!([
            {"type": 1, "begin_shape_index": 0},
            {"type": 4, "begin_shape_index": 2}
        ]),
    ];
    for manoeuvres in invalid_manoeuvres {
        let mut route = valid_route_response();
        route["trip"]["legs"][0]["maneuvers"] = manoeuvres;
        assert_provider_response_is_rejected(route, valid_trace_response()).await;
    }
}
