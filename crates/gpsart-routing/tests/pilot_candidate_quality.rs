use gpsart_domain::{ActivityProfile, Metres, RouteRequestProfile, templates::embedded_catalogue};
use gpsart_routing::{
    CandidateQualityConfig, CandidateSearch, HeartSearchConfig, HeartSearchRequest, LocalOsmGraph,
    MatchOutcome, QualityBand, score_candidate,
};
use std::time::Duration;

const SNAPSHOT: &[u8] = include_bytes!("../data/cubbon-park-osm-v2.json");
const SNAPSHOT_HASH: &str = include_str!("../data/cubbon-park-osm-v2.sha256");

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
        placement_offsets_metres: vec![(28, 10)],
        scales_metres: vec![40],
        rotations_degrees: vec![-15],
        max_placements: 1,
        max_scales: 1,
        max_rotations: 1,
        max_anchor_matches: 16,
        max_provider_calls: 1,
        max_retained_points: 10_000,
        wall_time: Duration::from_secs(5),
        anchor_tolerance_metres: 8,
    }
}

#[tokio::test]
async fn pinned_pedestrian_pilot_produces_distinct_verified_approximate_traversals() {
    let graph = LocalOsmGraph::from_json(SNAPSHOT, SNAPSHOT_HASH.trim()).unwrap();
    let catalogue = embedded_catalogue().unwrap();
    let heart_template = catalogue.get("heart", 1).unwrap().clone();
    let text_template = catalogue.get("HI", 1).unwrap().clone();
    let heart_request = request(&graph, 13_459_508_937);
    let text_request = request(&graph, 665_065_740);

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
            Metres::new(400).unwrap(),
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
