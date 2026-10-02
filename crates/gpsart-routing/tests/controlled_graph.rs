use async_trait::async_trait;
use gpsart_domain::{ActivityProfile, RouteRequestProfile};
use gpsart_routing::{
    Access, AttemptControl, ControlledEdge, ControlledGraph, ControlledNode, Direction,
    EdgeMetadata, GeoPointE7, Metadata, RoutePath, RouteProvider, RouteQuery, RoutingError,
    Surface, TurnRule, route_with_retries,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

#[derive(Default)]
struct UnavailableProvider(AtomicUsize);

impl UnavailableProvider {
    fn calls(&self) -> usize {
        self.0.load(Ordering::Acquire)
    }
}

struct SlowProvider;

#[async_trait]
impl RouteProvider for SlowProvider {
    async fn route(
        &self,
        _query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        control
            .run(async {
                tokio::time::sleep(Duration::from_secs(5)).await;
                Err(RoutingError::Unavailable)
            })
            .await
    }
}

#[async_trait]
impl RouteProvider for UnavailableProvider {
    async fn route(
        &self,
        _query: &RouteQuery,
        control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        control
            .run(async {
                self.0.fetch_add(1, Ordering::AcqRel);
                Err(RoutingError::Unavailable)
            })
            .await
    }
}

fn metadata(access: Access, direction: Direction) -> EdgeMetadata {
    EdgeMetadata {
        access: Metadata::Known(access),
        direction: Metadata::Known(direction),
        surface: Metadata::Known(Surface::Paved),
        elevation_metres: Metadata::Unknown,
        steps: Metadata::Known(false),
        ferry: Metadata::Known(false),
        motorway: Metadata::Known(false),
        motorroad: Metadata::Known(false),
    }
}

fn node(id: &str, x: i32, y: i32) -> ControlledNode {
    ControlledNode::new(id, GeoPointE7::new(y, x))
}

fn edge(
    id: &str,
    from: &str,
    to: &str,
    distance: u32,
    access: Access,
    direction: Direction,
) -> ControlledEdge {
    ControlledEdge::new(id, from, to, distance, metadata(access, direction))
}

fn query(activity: ActivityProfile, anchors: &[&str]) -> RouteQuery {
    RouteQuery::new(RouteRequestProfile::new(activity), anchors.iter().copied())
}

#[tokio::test]
async fn returns_ordered_edges_geometry_distance_manoeuvres_and_provenance() {
    let graph = ControlledGraph::new(
        "synthetic-v1",
        vec![node("a", 0, 0), node("b", 10, 0), node("c", 10, 10)],
        vec![
            edge("ab", "a", "b", 11, Access::Permitted, Direction::Both),
            edge("bc", "b", "c", 13, Access::Permitted, Direction::Both),
        ],
        vec![],
    )
    .expect("valid controlled graph");

    let route = graph
        .route(
            &query(ActivityProfile::Walking, &["a", "b", "c"]),
            &AttemptControl::with_timeout(Duration::from_secs(1), 1),
        )
        .await
        .expect("route");

    assert_eq!(
        route
            .edges
            .iter()
            .map(|item| item.edge_id.as_str())
            .collect::<Vec<_>>(),
        ["ab", "bc"]
    );
    assert_eq!(
        route.geometry,
        [
            GeoPointE7::new(0, 0),
            GeoPointE7::new(0, 10),
            GeoPointE7::new(10, 10)
        ]
    );
    assert_eq!(route.distance.value(), 24);
    assert_eq!(route.manoeuvres.len(), 3);
    assert_eq!(route.provenance.provider, "controlled-graph");
    assert_eq!(route.provenance.data_version, Some("synthetic-v1".into()));
}

#[tokio::test]
async fn enforces_one_way_forbidden_turn_and_activity_access() {
    let one_way = ControlledGraph::new(
        "synthetic-v1",
        vec![node("a", 0, 0), node("b", 10, 0)],
        vec![edge(
            "ab",
            "a",
            "b",
            10,
            Access::Permitted,
            Direction::ForwardOnly,
        )],
        vec![],
    )
    .unwrap();
    let control = || AttemptControl::with_timeout(Duration::from_secs(1), 1);
    assert!(
        one_way
            .route(&query(ActivityProfile::Walking, &["a", "b"]), &control())
            .await
            .is_ok()
    );
    assert_eq!(
        one_way
            .route(&query(ActivityProfile::Walking, &["b", "a"]), &control())
            .await,
        Err(RoutingError::NoPath)
    );

    let restricted = ControlledGraph::new(
        "synthetic-v1",
        vec![node("a", 0, 0), node("b", 10, 0), node("c", 20, 0)],
        vec![
            edge("ab", "a", "b", 10, Access::Permitted, Direction::Both),
            edge("bc", "b", "c", 10, Access::Permitted, Direction::Both),
        ],
        vec![TurnRule::forbidden("ab", "bc")],
    )
    .unwrap();
    assert_eq!(
        restricted
            .route(&query(ActivityProfile::Walking, &["a", "c"]), &control())
            .await,
        Err(RoutingError::NoPath)
    );

    let foot_only = ControlledGraph::new(
        "synthetic-v1",
        vec![node("a", 0, 0), node("b", 10, 0)],
        vec![edge(
            "foot",
            "a",
            "b",
            10,
            Access::FootOnly,
            Direction::Both,
        )],
        vec![],
    )
    .unwrap();
    assert!(
        foot_only
            .route(&query(ActivityProfile::Walking, &["a", "b"]), &control())
            .await
            .is_ok()
    );
    assert_eq!(
        foot_only
            .route(&query(ActivityProfile::Cycling, &["a", "b"]), &control())
            .await,
        Err(RoutingError::NoPath)
    );
}

#[tokio::test]
async fn preserves_repeated_edges_and_does_not_connect_grade_separated_crossings() {
    let graph = ControlledGraph::new(
        "synthetic-v1",
        vec![
            node("a", 0, 0),
            node("b", 10, 0),
            node("bridge-west", 0, 10),
            node("bridge-east", 10, 10),
            node("under-south", 5, 5),
            node("under-north", 5, 15),
        ],
        vec![
            edge("ab", "a", "b", 10, Access::Permitted, Direction::Both),
            edge(
                "bridge",
                "bridge-west",
                "bridge-east",
                10,
                Access::Permitted,
                Direction::Both,
            ),
            edge(
                "under",
                "under-south",
                "under-north",
                10,
                Access::Permitted,
                Direction::Both,
            ),
        ],
        vec![],
    )
    .unwrap();
    let control = || AttemptControl::with_timeout(Duration::from_secs(1), 1);
    let repeated = graph
        .route(
            &query(ActivityProfile::Walking, &["a", "b", "a"]),
            &control(),
        )
        .await
        .unwrap();
    assert_eq!(
        repeated
            .edges
            .iter()
            .map(|item| item.edge_id.as_str())
            .collect::<Vec<_>>(),
        ["ab", "ab"]
    );
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["bridge-west", "under-north"]),
                &control()
            )
            .await,
        Err(RoutingError::NoPath)
    );
    assert_eq!(
        graph
            .route(
                &query(ActivityProfile::Walking, &["a", "bridge-west"]),
                &control()
            )
            .await,
        Err(RoutingError::NoPath)
    );
}

#[tokio::test]
async fn cancellation_deadline_and_retry_budget_are_hard_limits() {
    let graph = ControlledGraph::new(
        "synthetic-v1",
        vec![node("a", 0, 0), node("b", 10, 0)],
        vec![edge("ab", "a", "b", 10, Access::Permitted, Direction::Both)],
        vec![],
    )
    .unwrap();
    let route_query = query(ActivityProfile::Walking, &["a", "b"]);

    let cancelled = AttemptControl::with_timeout(Duration::from_secs(1), 1);
    cancelled.cancel();
    assert_eq!(
        graph.route(&route_query, &cancelled).await,
        Err(RoutingError::Cancelled)
    );

    let expired = AttemptControl::with_timeout(Duration::ZERO, 1);
    assert_eq!(
        graph.route(&route_query, &expired).await,
        Err(RoutingError::DeadlineExceeded)
    );

    let unavailable = UnavailableProvider::default();
    let budget = AttemptControl::with_timeout(Duration::from_secs(1), 2);
    assert_eq!(
        route_with_retries(&unavailable, &route_query, &budget, 5).await,
        Err(RoutingError::BudgetExhausted)
    );
    assert_eq!(unavailable.calls(), 2);

    let in_flight = AttemptControl::with_timeout(Duration::from_secs(1), 1);
    let cancellation = in_flight.clone();
    let cancel_after_start = async move {
        tokio::time::sleep(Duration::from_millis(5)).await;
        cancellation.cancel();
    };
    let ((), result) = tokio::join!(
        cancel_after_start,
        SlowProvider.route(&route_query, &in_flight)
    );
    assert_eq!(result, Err(RoutingError::Cancelled));

    let short_deadline = AttemptControl::with_timeout(Duration::from_millis(5), 1);
    assert_eq!(
        SlowProvider.route(&route_query, &short_deadline).await,
        Err(RoutingError::DeadlineExceeded)
    );
}
