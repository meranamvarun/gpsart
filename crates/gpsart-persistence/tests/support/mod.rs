use gpsart_domain::{ActivityProfile, Metres, RouteRequestProfile, templates::embedded_catalogue};
use gpsart_routing::{
    Access, CandidateQualityConfig, CandidateSearch, ControlledEdge, ControlledGraph,
    ControlledNode, Direction, EdgeMetadata, GeoPointE7, HeartSearchConfig, HeartSearchRequest,
    MatchOutcome, Metadata, ScoredCandidate, Surface, score_candidate,
};
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

pub async fn scored_candidate(map_version: &str, edge_distance_metres: u32) -> ScoredCandidate {
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
        ("start-access", "start", "access"),
        ("access-tip", "access", "tip"),
        ("tip-lower-left", "tip", "lower-left"),
        ("lower-left-upper-left", "lower-left", "upper-left"),
        ("upper-left-left-lobe", "upper-left", "left-lobe"),
        ("left-lobe-cleft", "left-lobe", "cleft"),
        ("cleft-right-lobe", "cleft", "right-lobe"),
        ("right-lobe-upper-right", "right-lobe", "upper-right"),
        ("upper-right-lower-right", "upper-right", "lower-right"),
        ("lower-right-tip", "lower-right", "tip"),
    ];
    let graph = ControlledGraph::new(
        map_version,
        nodes,
        links
            .into_iter()
            .map(|(id, from, to)| {
                ControlledEdge::new(id, from, to, edge_distance_metres, metadata())
            })
            .collect(),
        vec![],
    )
    .unwrap();
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let expected_distance = edge_distance_metres * 12;
    let request = HeartSearchRequest {
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        confirmed_start: point(0, 0),
        minimum_distance: Metres::new(expected_distance.saturating_sub(10)).unwrap(),
        maximum_distance: Metres::new(expected_distance + 10).unwrap(),
    };
    let config = HeartSearchConfig {
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
    };
    let MatchOutcome::Match(candidate) = CandidateSearch::new(&graph, &graph)
        .heart(&template, &request, &config)
        .await
        .unwrap()
    else {
        panic!("controlled snapshot source must match")
    };
    score_candidate(
        *candidate,
        &template,
        &CandidateQualityConfig::v1(
            Metres::new(expected_distance).unwrap(),
            request.minimum_distance,
            request.maximum_distance,
        ),
    )
    .unwrap()
}
