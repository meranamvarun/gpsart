use async_trait::async_trait;
use gpsart_domain::{ActivityProfile, Metres, RouteRequestProfile, templates::embedded_catalogue};
use gpsart_routing::{
    Access, AnchorMatch, AttemptControl, CandidatePart, CandidateSearch, ControlledEdge,
    ControlledGraph, ControlledNode, Direction, EdgeMetadata, GeoPointE7, HeartSearchConfig,
    HeartSearchRequest, MatchOutcome, Metadata, Qualification, RoutePath, RouteProvider,
    RouteQuery, RoutingError, Surface,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn point(x_metres: i32, y_metres: i32) -> GeoPointE7 {
    GeoPointE7::new(y_metres * 90, x_metres * 90)
}

fn metric_point(x_metres: f64, y_metres: f64) -> GeoPointE7 {
    GeoPointE7::new(
        (y_metres * 10_000_000.0 / 111_320.0).round() as i32,
        (x_metres * 10_000_000.0 / 111_320.0).round() as i32,
    )
}

fn metadata() -> EdgeMetadata {
    EdgeMetadata {
        access: Metadata::Known(Access::Permitted),
        direction: Metadata::Known(Direction::Both),
        surface: Metadata::Known(Surface::Paved),
        elevation_metres: Metadata::Known(0),
        steps: Metadata::Known(false),
        ferry: Metadata::Known(false),
        motorway: Metadata::Known(false),
        motorroad: Metadata::Known(false),
    }
}

fn graph() -> ControlledGraph {
    let nodes = vec![
        ControlledNode::new("start", point(0, 0)),
        ControlledNode::new("access", point(10, 0)),
        ControlledNode::new("tip", metric_point(20.0, 1.6)),
        ControlledNode::new("lower-left", metric_point(12.4, 9.0)),
        ControlledNode::new("upper-left", metric_point(12.4, 14.4)),
        ControlledNode::new("left-lobe", metric_point(15.6, 17.6)),
        ControlledNode::new("cleft", metric_point(20.0, 14.0)),
        ControlledNode::new("right-lobe", metric_point(24.4, 17.6)),
        ControlledNode::new("upper-right", metric_point(27.6, 14.4)),
        ControlledNode::new("lower-right", metric_point(27.6, 9.0)),
    ];
    let links = [
        ("start-access", "start", "access", 10),
        ("access-tip", "access", "tip", 10),
        ("tip-lower-left", "tip", "lower-left", 10),
        ("lower-left-upper-left", "lower-left", "upper-left", 10),
        ("upper-left-left-lobe", "upper-left", "left-lobe", 10),
        ("left-lobe-cleft", "left-lobe", "cleft", 10),
        ("cleft-right-lobe", "cleft", "right-lobe", 10),
        ("right-lobe-upper-right", "right-lobe", "upper-right", 10),
        ("upper-right-lower-right", "upper-right", "lower-right", 10),
        ("lower-right-tip", "lower-right", "tip", 10),
    ];
    ControlledGraph::new(
        "heart-fixture-v1",
        nodes,
        links
            .into_iter()
            .map(|(id, from, to, distance)| ControlledEdge::new(id, from, to, distance, metadata()))
            .collect(),
        vec![],
    )
    .unwrap()
}

fn request() -> HeartSearchRequest {
    HeartSearchRequest {
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        confirmed_start: point(0, 0),
        minimum_distance: Metres::new(100).unwrap(),
        maximum_distance: Metres::new(130).unwrap(),
    }
}

fn config() -> HeartSearchConfig {
    HeartSearchConfig {
        placement_offsets_metres: vec![(20, 10)],
        scales_metres: vec![20],
        rotations_degrees: vec![0],
        max_placements: 1,
        max_scales: 1,
        max_rotations: 1,
        max_anchor_matches: 16,
        max_provider_calls: 1,
        max_retained_points: 100,
        wall_time: Duration::from_secs(1),
        anchor_tolerance_metres: 2,
    }
}

#[tokio::test]
async fn feasible_fixture_is_closed_legal_recognisable_and_complete() {
    let graph = graph();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let outcome = CandidateSearch::new(&graph, &graph)
        .heart(&template, &request(), &config())
        .await
        .unwrap();
    let MatchOutcome::Match(candidate) = outcome else {
        panic!("controlled heart fixture should match")
    };

    assert_eq!(candidate.geometry.first(), Some(&request().confirmed_start));
    assert_eq!(candidate.geometry.last(), Some(&request().confirmed_start));
    assert!(candidate.closure_distance_metres <= 5);
    assert_eq!(candidate.distance.value(), 120);
    assert_eq!(
        candidate
            .occurrences
            .iter()
            .map(|item| item.distance.value())
            .sum::<u32>(),
        candidate.distance.value()
    );
    assert_eq!(candidate.template_id, "heart");
    assert!(
        candidate
            .occurrences
            .iter()
            .any(|item| item.part == CandidatePart::Access)
    );
    assert!(
        candidate
            .occurrences
            .iter()
            .any(|item| item.part == CandidatePart::Artwork)
    );
    assert!(
        candidate
            .occurrences
            .iter()
            .any(|item| item.part == CandidatePart::Return)
    );
    assert!(candidate.occurrences.iter().any(|item| item.retraced));
    assert_eq!(
        candidate.ordered_design_anchors.first(),
        candidate.ordered_design_anchors.last()
    );
    assert_eq!(
        candidate.provenance.data_version.as_deref(),
        Some("heart-fixture-v1")
    );
    assert_eq!(
        candidate.legality_evidence().validation_version,
        "candidate-legality-v1"
    );
    assert_eq!(
        candidate.legality_evidence().routing_policy_version,
        "controlled-graph-policy-v1"
    );
    assert_eq!(candidate.legality_evidence().profile_version, "walking-v1");
    assert!(candidate.legality_evidence().access_verified);
    assert!(candidate.legality_evidence().direction_verified);
    assert!(candidate.legality_evidence().turns_verified);
}

#[tokio::test]
async fn network_that_cannot_realise_the_design_returns_explicit_no_match() {
    let graph = graph();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let mut impossible = config();
    impossible.anchor_tolerance_metres = 1;
    impossible.placement_offsets_metres = vec![(2_000, 2_000)];

    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .heart(&template, &request(), &impossible)
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );
}

#[derive(Clone)]
struct FixedAnchors;

impl AnchorMatch for FixedAnchors {
    fn nearest_anchor(&self, target: GeoPointE7, _tolerance_metres: u32) -> Option<GeoPointE7> {
        Some(target)
    }
}

#[derive(Clone)]
struct RecordingProvider {
    response: RoutePath,
    calls: Arc<Mutex<usize>>,
}

enum FailureKind {
    InvalidRequest,
    Cancelled,
}

struct FailingProvider(FailureKind);

#[async_trait]
impl RouteProvider for FailingProvider {
    async fn route(
        &self,
        _query: &RouteQuery,
        _control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        Err(match self.0 {
            FailureKind::InvalidRequest => RoutingError::InvalidRequest,
            FailureKind::Cancelled => RoutingError::Cancelled,
        })
    }
}

#[async_trait]
impl RouteProvider for RecordingProvider {
    async fn route(
        &self,
        _query: &RouteQuery,
        _control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        *self.calls.lock().unwrap() += 1;
        Ok(self.response.clone())
    }
}

#[tokio::test]
async fn rejects_invalid_or_incomplete_paths_before_returning_a_candidate() {
    let graph = graph();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let valid = graph
        .route(
            &RouteQuery::new(
                RouteRequestProfile::new(ActivityProfile::Walking),
                [
                    "start",
                    "access",
                    "tip",
                    "lower-left",
                    "upper-left",
                    "cleft",
                    "upper-right",
                    "lower-right",
                    "tip",
                    "access",
                    "start",
                ],
            ),
            &AttemptControl::with_timeout(Duration::from_secs(1), 1),
        )
        .await
        .unwrap();
    let anchors = FixedAnchors;

    for mutate in 0..8 {
        let mut invalid = valid.clone();
        match mutate {
            0 => {
                invalid.geometry.pop();
            }
            1 => invalid.edges[1].begin_geometry_index += 1,
            2 => invalid.edges[0].distance = None,
            3 => invalid.distance = Metres::new(valid.distance.value() + 1).unwrap(),
            4 => invalid.edges[0]
                .qualifications
                .push(Qualification::ProviderAccessUnverified),
            5 => invalid.edges[0]
                .qualifications
                .push(Qualification::AccessUnknown),
            6 => invalid.legality.access_verified = false,
            _ => {
                let first_via = invalid.manoeuvres[1].geometry_index;
                invalid.geometry[first_via] = point(99, 99);
            }
        };
        let provider = RecordingProvider {
            response: invalid,
            calls: Arc::new(Mutex::new(0)),
        };
        assert_eq!(
            CandidateSearch::new(&provider, &anchors)
                .heart(&template, &request(), &config())
                .await
                .unwrap(),
            MatchOutcome::NoMatch
        );
        assert_eq!(*provider.calls.lock().unwrap(), 1);
    }
}

#[test]
fn rotations_outside_the_immutable_template_bounds_are_rejected() {
    let graph = graph();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let mut invalid = config();
    invalid.rotations_degrees = vec![181];
    let runtime = tokio::runtime::Runtime::new().unwrap();
    assert_eq!(
        runtime.block_on(CandidateSearch::new(&graph, &graph).heart(
            &template,
            &request(),
            &invalid
        )),
        Err(gpsart_routing::CandidateError::InvalidRequest)
    );
}

struct SlowAnchors;

impl AnchorMatch for SlowAnchors {
    fn nearest_anchor(&self, target: GeoPointE7, _tolerance_metres: u32) -> Option<GeoPointE7> {
        std::thread::sleep(Duration::from_millis(5));
        Some(target)
    }
}

#[tokio::test]
async fn positive_wall_deadline_applies_during_anchor_fitting_before_provider_work() {
    let graph = graph();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let calls = Arc::new(Mutex::new(0));
    let valid = graph
        .route(
            &RouteQuery::new(
                RouteRequestProfile::new(ActivityProfile::Walking),
                ["start", "access"],
            ),
            &AttemptControl::with_timeout(Duration::from_secs(1), 1),
        )
        .await
        .unwrap();
    let provider = RecordingProvider {
        response: valid,
        calls: calls.clone(),
    };
    let mut limited = config();
    limited.wall_time = Duration::from_millis(1);

    assert_eq!(
        CandidateSearch::new(&provider, &SlowAnchors)
            .heart(&template, &request(), &limited)
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );
    assert_eq!(*calls.lock().unwrap(), 0);
}

#[tokio::test]
async fn provider_contract_errors_and_cancellation_are_not_reported_as_no_match() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    assert_eq!(
        CandidateSearch::new(&FailingProvider(FailureKind::InvalidRequest), &FixedAnchors)
            .heart(&template, &request(), &config())
            .await,
        Err(gpsart_routing::CandidateError::Routing(
            RoutingError::InvalidRequest
        ))
    );
    assert_eq!(
        CandidateSearch::new(&FailingProvider(FailureKind::Cancelled), &FixedAnchors)
            .heart(&template, &request(), &config())
            .await,
        Err(gpsart_routing::CandidateError::Cancelled)
    );

    let control = AttemptControl::with_timeout(Duration::from_secs(1), 1);
    control.cancel();
    assert_eq!(
        CandidateSearch::new(&FailingProvider(FailureKind::InvalidRequest), &FixedAnchors)
            .heart_with_control(&template, &request(), &config(), &control)
            .await,
        Err(gpsart_routing::CandidateError::Cancelled)
    );
}

#[tokio::test]
async fn configured_limits_are_hard_and_deadline_exhaustion_never_returns_partial_work() {
    let graph = graph();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let mut limited = config();
    limited.max_provider_calls = 0;
    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .heart(&template, &request(), &limited)
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );

    limited.max_provider_calls = 1;
    limited.max_retained_points = 1;
    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .heart(&template, &request(), &limited)
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );

    limited.max_retained_points = 100;
    limited.wall_time = Duration::ZERO;
    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .heart(&template, &request(), &limited)
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );
}
