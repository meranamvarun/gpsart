use async_trait::async_trait;
use gpsart_domain::{
    ActivityProfile, Metres, RouteRequestProfile,
    templates::{Category, embedded_catalogue},
};
use gpsart_routing::{
    Access, AnchorMatch, AttemptControl, CandidateError, CandidatePart, CandidateSearch,
    ControlledEdge, ControlledGraph, ControlledNode, CuratedTextSearchConfig,
    CuratedTextSearchRequest, Direction, EdgeMetadata, GeoPointE7, MatchOutcome, Metadata,
    RoutePath, RouteProvider, RouteQuery, RoutingError, Surface,
};
use std::time::Duration;

fn metric_point(east_metres: f64, north_metres: f64) -> GeoPointE7 {
    GeoPointE7::new(
        (north_metres * 10_000_000.0 / 111_320.0).round() as i32,
        (east_metres * 10_000_000.0 / 111_320.0).round() as i32,
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

fn graph(include_letter_connector: bool) -> ControlledGraph {
    let nodes = vec![
        ControlledNode::new("start", metric_point(0.0, 0.0)),
        ControlledNode::new("access", metric_point(30.0, 30.0)),
        ControlledNode::new("h-left-top", metric_point(62.0, 65.0)),
        ControlledNode::new("h-left-mid", metric_point(62.0, 100.0)),
        ControlledNode::new("h-left-bottom", metric_point(62.0, 135.0)),
        ControlledNode::new("h-right-mid", metric_point(98.0, 100.0)),
        ControlledNode::new("h-right-bottom", metric_point(98.0, 135.0)),
        ControlledNode::new("h-right-top", metric_point(98.0, 65.0)),
        ControlledNode::new("i-bottom", metric_point(128.0, 135.0)),
        ControlledNode::new("i-top", metric_point(128.0, 65.0)),
    ];
    let mut links = vec![
        ("start-access", "start", "access", 42),
        ("access-h", "access", "h-left-top", 48),
        ("h-left-upper", "h-left-top", "h-left-mid", 35),
        ("h-left-lower", "h-left-mid", "h-left-bottom", 35),
        ("h-bar", "h-left-mid", "h-right-mid", 36),
        ("h-right-lower", "h-right-mid", "h-right-bottom", 35),
        ("h-right-upper", "h-right-mid", "h-right-top", 35),
        ("i-stem", "i-bottom", "i-top", 70),
        ("return", "i-top", "start", 144),
    ];
    if include_letter_connector {
        links.push(("letter-connector", "h-right-top", "i-bottom", 76));
    }
    ControlledGraph::new(
        "hi-fixture-v1",
        nodes,
        links
            .into_iter()
            .map(|(id, from, to, distance)| ControlledEdge::new(id, from, to, distance, metadata()))
            .collect(),
        vec![],
    )
    .unwrap()
}

fn request() -> CuratedTextSearchRequest {
    CuratedTextSearchRequest {
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        confirmed_start: metric_point(0.0, 0.0),
        minimum_distance: Metres::new(600).unwrap(),
        maximum_distance: Metres::new(650).unwrap(),
    }
}

fn config() -> CuratedTextSearchConfig {
    CuratedTextSearchConfig {
        placement_offsets_metres: vec![(100, 100)],
        scales_metres: vec![100],
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
async fn feasible_hi_is_one_complete_ordered_full_trace_with_honest_diagnostics() {
    let graph = graph(true);
    let template = embedded_catalogue().unwrap().get("HI", 1).unwrap().clone();
    let MatchOutcome::Match(candidate) = CandidateSearch::new(&graph, &graph)
        .curated_text(&template, &request(), &config())
        .await
        .unwrap()
    else {
        panic!("controlled HI fixture should match")
    };

    assert_eq!(candidate.template_id, "HI");
    assert_eq!(candidate.geometry.first(), Some(&request().confirmed_start));
    assert_eq!(candidate.geometry.last(), Some(&request().confirmed_start));
    assert_eq!(candidate.ordered_design_anchors.len(), 8);
    assert_ne!(
        candidate.ordered_design_anchors.first(),
        candidate.ordered_design_anchors.last()
    );
    assert_eq!(candidate.resemblance_trace(), candidate.geometry.as_slice());
    assert_eq!(candidate.distance.value(), 626);
    assert_eq!(
        candidate
            .occurrences
            .iter()
            .map(|occurrence| occurrence.distance.value())
            .sum::<u32>(),
        candidate.distance.value()
    );
    assert_eq!(candidate.connector_distance_metres, 146);
    assert_eq!(candidate.repeated_distance_metres, 70);
    assert!(candidate.occurrences.iter().any(|occurrence| {
        occurrence.part == CandidatePart::Connector && occurrence.edge_id == "letter-connector"
    }));
    assert!(candidate.occurrences.iter().any(|occurrence| {
        occurrence.part == CandidatePart::Artwork
            && occurrence.edge_id == "h-right-lower"
            && occurrence.retraced
    }));
    assert_eq!(
        candidate.provenance.data_version.as_deref(),
        Some("hi-fixture-v1")
    );
}

#[tokio::test]
async fn disconnected_hi_returns_classified_no_match_without_fabricated_geometry() {
    let graph = graph(false);
    let template = embedded_catalogue().unwrap().get("HI", 1).unwrap().clone();

    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .curated_text(&template, &request(), &config())
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );
}

#[test]
fn mirrored_reordered_and_disallowed_rotation_inputs_fail_hard_validation() {
    let graph = graph(true);
    let template = embedded_catalogue().unwrap().get("HI", 1).unwrap().clone();
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let mut mirrored = template.clone();
    mirrored.definition.orientation.mirror = true;
    assert_eq!(
        runtime.block_on(CandidateSearch::new(&graph, &graph).curated_text(
            &mirrored,
            &request(),
            &config()
        )),
        Err(CandidateError::InvalidRequest)
    );

    let mut reordered = template.clone();
    reordered.definition.strokes.swap(0, 1);
    assert_eq!(
        runtime.block_on(CandidateSearch::new(&graph, &graph).curated_text(
            &reordered,
            &request(),
            &config()
        )),
        Err(CandidateError::InvalidRequest)
    );

    let mut rotated = config();
    rotated.rotations_degrees = vec![16];
    assert_eq!(
        runtime.block_on(CandidateSearch::new(&graph, &graph).curated_text(
            &template,
            &request(),
            &rotated
        )),
        Err(CandidateError::InvalidRequest)
    );
}

#[tokio::test]
async fn collapsed_anchors_limits_and_cancellation_never_return_partial_text() {
    struct CollapsedAnchors;

    impl AnchorMatch for CollapsedAnchors {
        fn nearest_anchor(
            &self,
            _target: GeoPointE7,
            _tolerance_metres: u32,
        ) -> Option<GeoPointE7> {
            Some(metric_point(50.0, 50.0))
        }
    }

    let graph = graph(true);
    let template = embedded_catalogue().unwrap().get("HI", 1).unwrap().clone();
    assert_eq!(
        CandidateSearch::new(&graph, &CollapsedAnchors)
            .curated_text(&template, &request(), &config())
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );

    let mut exhausted = config();
    exhausted.max_provider_calls = 0;
    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .curated_text(&template, &request(), &exhausted)
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );

    let control = AttemptControl::with_timeout(Duration::from_secs(1), 1);
    control.cancel();
    assert_eq!(
        CandidateSearch::new(&graph, &graph)
            .curated_text_with_control(&template, &request(), &config(), &control)
            .await,
        Err(CandidateError::Cancelled)
    );
}

struct ExactAnchors;

impl AnchorMatch for ExactAnchors {
    fn nearest_anchor(&self, target: GeoPointE7, _tolerance_metres: u32) -> Option<GeoPointE7> {
        Some(target)
    }
}

struct FixedProvider(RoutePath);

#[async_trait]
impl RouteProvider for FixedProvider {
    async fn route(
        &self,
        _query: &RouteQuery,
        _control: &AttemptControl,
    ) -> Result<RoutePath, RoutingError> {
        Ok(self.0.clone())
    }
}

#[tokio::test]
async fn visually_incomplete_or_reordered_provider_trace_is_rejected() {
    let graph = graph(true);
    let profile = RouteRequestProfile::new(ActivityProfile::Walking);
    let anchors = [
        "start",
        "h-left-top",
        "h-left-bottom",
        "h-left-mid",
        "h-right-mid",
        "h-right-bottom",
        "h-right-top",
        "i-bottom",
        "i-top",
        "start",
    ];
    let valid = graph
        .route(
            &RouteQuery::new(profile, anchors),
            &AttemptControl::with_timeout(Duration::from_secs(1), 1),
        )
        .await
        .unwrap();
    let template = embedded_catalogue().unwrap().get("HI", 1).unwrap().clone();

    let mut incomplete = valid.clone();
    incomplete.manoeuvres.remove(4);
    assert_eq!(
        CandidateSearch::new(&FixedProvider(incomplete), &ExactAnchors)
            .curated_text(&template, &request(), &config())
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );

    let mut reordered = valid;
    let first = reordered.manoeuvres[2].geometry_index;
    reordered.manoeuvres[2].geometry_index = reordered.manoeuvres[3].geometry_index;
    reordered.manoeuvres[3].geometry_index = first;
    assert_eq!(
        CandidateSearch::new(&FixedProvider(reordered), &ExactAnchors)
            .curated_text(&template, &request(), &config())
            .await
            .unwrap(),
        MatchOutcome::NoMatch
    );
}

#[test]
fn fixture_is_the_published_curated_text_template() {
    let template = embedded_catalogue().unwrap().get("HI", 1).unwrap().clone();
    assert_eq!(template.definition.category, Category::Text);
}
