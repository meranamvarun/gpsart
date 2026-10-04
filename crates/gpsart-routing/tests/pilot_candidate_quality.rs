use gpsart_domain::{ActivityProfile, Metres, RouteRequestProfile, templates::embedded_catalogue};
use gpsart_routing::{
    CandidatePart, CandidateQualityConfig, CandidateSearch, GeoPointE7, HeartSearchConfig,
    HeartSearchRequest, LocalOsmGraph, MatchOutcome, QualityBand, ValidatedCandidate,
    score_candidate,
};
use std::time::Duration;

const SNAPSHOT: &[u8] = include_bytes!("../data/bengaluru-central-osm-v3.json");
const SNAPSHOT_HASH: &str = include_str!("../data/bengaluru-central-osm-v3.sha256");

fn request(graph: &LocalOsmGraph, start_node_id: u64) -> HeartSearchRequest {
    HeartSearchRequest {
        profile: RouteRequestProfile::new(ActivityProfile::Walking),
        confirmed_start: graph.point(start_node_id).unwrap(),
        minimum_distance: Metres::new(1).unwrap(),
        maximum_distance: Metres::new(100_000).unwrap(),
    }
}

fn heart_config() -> HeartSearchConfig {
    HeartSearchConfig {
        placement_offsets_metres: (-149..=-143)
            .flat_map(|east| (79..=85).map(move |north| (east, north)))
            .collect(),
        scales_metres: vec![400],
        rotations_degrees: vec![60],
        max_placements: 49,
        max_scales: 1,
        max_rotations: 1,
        max_anchor_matches: 1_000,
        max_provider_calls: 49,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(5),
        anchor_tolerance_metres: 15,
    }
}

fn text_config() -> HeartSearchConfig {
    HeartSearchConfig {
        placement_offsets_metres: vec![(66, 24)],
        scales_metres: vec![160],
        rotations_degrees: vec![-3],
        max_placements: 1,
        max_scales: 1,
        max_rotations: 1,
        max_anchor_matches: 16,
        max_provider_calls: 1,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(5),
        anchor_tolerance_metres: 12,
    }
}

fn point_to_segment_metres(point: GeoPointE7, start: GeoPointE7, end: GeoPointE7) -> f64 {
    let latitude = f64::from(point.latitude_e7) / 10_000_000.0;
    let longitude_scale = 111_320.0 * latitude.to_radians().cos();
    let local = |other: GeoPointE7| {
        (
            f64::from(other.longitude_e7 - point.longitude_e7) / 10_000_000.0 * longitude_scale,
            f64::from(other.latitude_e7 - point.latitude_e7) / 10_000_000.0 * 111_320.0,
        )
    };
    let (start_x, start_y) = local(start);
    let (end_x, end_y) = local(end);
    let dx = end_x - start_x;
    let dy = end_y - start_y;
    let denominator = dx * dx + dy * dy;
    let factor = if denominator <= f64::EPSILON {
        0.0
    } else {
        (-(start_x * dx + start_y * dy) / denominator).clamp(0.0, 1.0)
    };
    (start_x + factor * dx).hypot(start_y + factor * dy)
}

fn vector_metres(start: GeoPointE7, end: GeoPointE7) -> (f64, f64) {
    let latitude = f64::from(start.latitude_e7 + end.latitude_e7) / 20_000_000.0;
    (
        f64::from(end.longitude_e7 - start.longitude_e7) / 10_000_000.0
            * 111_320.0
            * latitude.to_radians().cos(),
        f64::from(end.latitude_e7 - start.latitude_e7) / 10_000_000.0 * 111_320.0,
    )
}

fn direction(start: GeoPointE7, end: GeoPointE7) -> (f64, f64) {
    let (east, north) = vector_metres(start, end);
    let length = east.hypot(north);
    (east / length, north / length)
}

fn dot(left: (f64, f64), right: (f64, f64)) -> f64 {
    left.0 * right.0 + left.1 * right.1
}

fn assert_reviewable_hi_geometry(candidate: &ValidatedCandidate, scale_metres: u32) {
    let artwork_runs = candidate
        .occurrences
        .chunk_by(|left, right| left.part == right.part)
        .filter(|run| run[0].part == CandidatePart::Artwork)
        .collect::<Vec<_>>();
    assert_eq!(artwork_runs.len(), 4, "HI must retain four visible strokes");

    let maximum_deviation = artwork_runs
        .iter()
        .flat_map(|run| {
            let begin = run[0].begin_geometry_index;
            let end = run.last().unwrap().end_geometry_index;
            let start = candidate.geometry[begin];
            let finish = candidate.geometry[end];
            candidate.geometry[begin..=end]
                .iter()
                .copied()
                .map(move |point| point_to_segment_metres(point, start, finish))
        })
        .fold(0.0, f64::max);
    assert!(
        maximum_deviation <= f64::from(scale_metres) * 0.10,
        "HI artwork deviates {maximum_deviation:.1}m from its four intended strokes"
    );

    let endpoints = artwork_runs
        .iter()
        .map(|run| {
            (
                candidate.geometry[run[0].begin_geometry_index],
                candidate.geometry[run.last().unwrap().end_geometry_index],
            )
        })
        .collect::<Vec<_>>();
    let left = direction(endpoints[0].0, endpoints[0].1);
    let bar = direction(endpoints[1].0, endpoints[1].1);
    let right = direction(endpoints[2].0, endpoints[2].1);
    let i_stem = direction(endpoints[3].0, endpoints[3].1);
    assert!(dot(left, right).abs() >= 0.95);
    assert!(dot(left, i_stem).abs() >= 0.95);
    assert!(dot(left, bar).abs() <= 0.25);
    assert!(
        point_to_segment_metres(endpoints[1].0, endpoints[0].0, endpoints[0].1)
            <= f64::from(scale_metres) * 0.12
    );
    assert!(
        point_to_segment_metres(endpoints[1].1, endpoints[2].0, endpoints[2].1)
            <= f64::from(scale_metres) * 0.12
    );
    assert!(
        point_to_segment_metres(endpoints[3].0, endpoints[2].0, endpoints[2].1)
            >= f64::from(scale_metres) * 0.12,
        "I stem must remain visibly separate from the H"
    );
    assert!(
        u64::from(candidate.repeated_distance_metres) * 3 < u64::from(candidate.distance.value()),
        "HI must not spend one third or more of its distance retracing"
    );
}

#[tokio::test]
async fn pinned_pedestrian_pilot_produces_distinct_verified_approximate_traversals() {
    let graph = LocalOsmGraph::from_json(SNAPSHOT, SNAPSHOT_HASH.trim()).unwrap();
    let catalogue = embedded_catalogue().unwrap();
    let heart_template = catalogue.get("heart", 1).unwrap().clone();
    let text_template = catalogue.get("HI", 1).unwrap().clone();
    let heart_request = request(&graph, 13_459_508_937);
    let text_request = request(&graph, 10_238_269_050);

    let MatchOutcome::Match(heart) = CandidateSearch::new(&graph, &graph)
        .heart(&heart_template, &heart_request, &heart_config())
        .await
        .unwrap()
    else {
        panic!("pinned heart traversal must remain feasible")
    };
    let MatchOutcome::Match(text) = CandidateSearch::new(&graph, &graph)
        .curated_text(&text_template, &text_request, &text_config())
        .await
        .unwrap()
    else {
        panic!("pinned HI traversal must remain feasible")
    };

    for candidate in [&heart, &text] {
        assert_eq!(
            candidate.legality_evidence().routing_policy_version,
            "osm-pedestrian-policy-v2"
        );
        assert!(candidate.legality_evidence().access_verified);
        assert!(candidate.legality_evidence().direction_verified);
        assert!(candidate.legality_evidence().turns_verified);
    }
    assert_ne!(heart.geometry, text.geometry);
    assert_reviewable_hi_geometry(&text, text_config().scales_metres[0]);

    let heart = score_candidate(
        *heart,
        &heart_template,
        &CandidateQualityConfig::v1(
            Metres::new(1_300).unwrap(),
            heart_request.minimum_distance,
            heart_request.maximum_distance,
        ),
    )
    .unwrap();
    let text = score_candidate(
        *text,
        &text_template,
        &CandidateQualityConfig::v1(
            Metres::new(1_000).unwrap(),
            text_request.minimum_distance,
            text_request.maximum_distance,
        ),
    )
    .unwrap();
    assert_eq!(heart.quality_band, QualityBand::ApproximateMatch);
    assert_eq!(text.quality_band, QualityBand::ApproximateMatch);
    assert!(heart.metrics.full_trace_error_metres <= 70);
    assert!(heart.metrics.repeated_ratio_basis_points <= 500);
    assert!(text.metrics.full_trace_error_metres <= 20);
}
