use gpsart_domain::{
    ActivityProfile, Metres, RouteCandidateProfile, RouteRequestProfile,
    templates::embedded_catalogue,
};
use gpsart_routing::{
    CandidatePart, CandidateQualityConfig, FeatureCheckOutcome, GeoPointE7, HeartCandidate,
    QualityBand, QualityError, RouteProvenance, TraversalDirection, TraversalOccurrence,
    rank_and_deduplicate, score_candidate,
};

fn point(x: i32, y: i32) -> GeoPointE7 {
    GeoPointE7::new(y, x)
}

fn occurrence(
    id: &str,
    part: CandidatePart,
    retraced: bool,
    begin: usize,
    end: usize,
) -> TraversalOccurrence {
    TraversalOccurrence {
        edge_id: id.into(),
        direction: TraversalDirection::Forward,
        part,
        retraced,
        begin_geometry_index: begin,
        end_geometry_index: end,
        distance: Metres::new(100).unwrap(),
    }
}

fn candidate() -> HeartCandidate {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    HeartCandidate {
        template_id: "heart".into(),
        template_version: 1,
        template_hash: template.definition_hash,
        profile: RouteCandidateProfile::from_request(&RouteRequestProfile::new(
            ActivityProfile::Walking,
        )),
        geometry: vec![
            point(0, 0),
            point(10_000, 0),
            point(10_000, 10_000),
            point(0, 0),
        ],
        ordered_design_anchors: vec![
            point(0, 0),
            point(2_500, 0),
            point(5_000, 0),
            point(7_500, 0),
            point(10_000, 0),
            point(10_000, 5_000),
            point(10_000, 10_000),
            point(5_000, 5_000),
            point(0, 0),
        ],
        occurrences: vec![
            occurrence("access", CandidatePart::Access, false, 0, 1),
            occurrence("art", CandidatePart::Artwork, false, 1, 2),
            occurrence("access", CandidatePart::Return, true, 2, 3),
        ],
        distance: Metres::new(300).unwrap(),
        connector_distance_metres: 0,
        repeated_distance_metres: 100,
        closure_distance_metres: 0,
        provenance: RouteProvenance {
            provider: "controlled-graph".into(),
            data_version: Some("quality-fixture-v1".into()),
            attribution: None,
        },
    }
}

fn config() -> CandidateQualityConfig {
    CandidateQualityConfig::v1(
        Metres::new(300).unwrap(),
        Metres::new(250).unwrap(),
        Metres::new(350).unwrap(),
    )
}

#[test]
fn records_versioned_full_trace_metrics_and_exposes_only_a_quality_band() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let scored = score_candidate(candidate(), &template, &config()).unwrap();

    assert_eq!(scored.metrics.version, "candidate-quality-v1");
    assert_eq!(
        scored.metrics.weights.version,
        "candidate-quality-weights-v1"
    );
    assert_eq!(scored.metrics.distance_error_metres, 0);
    assert_eq!(scored.metrics.repeated_ratio_basis_points, 3_333);
    assert_eq!(scored.metrics.connector_ratio_basis_points, 0);
    assert_eq!(scored.metrics.full_trace_error_metres, 0);
    assert!(scored.metrics.manoeuvre_complexity > 0);
    assert_eq!(scored.metrics.identifying_feature_checks.len(), 3);
    assert!(
        scored
            .metrics
            .identifying_feature_checks
            .iter()
            .all(|check| check.outcome == FeatureCheckOutcome::Unknown)
    );
    assert_eq!(scored.quality_band, QualityBand::ApproximateMatch);

    let user_json = serde_json::to_value(scored.user_quality()).unwrap();
    assert!(user_json.get("quality_band").is_some());
    assert!(user_json.get("score").is_none());
    assert!(user_json.get("percentage").is_none());
}

#[test]
fn distance_and_structural_validity_are_hard_gates_before_resemblance_scoring() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let mut out_of_range = candidate();
    out_of_range.distance = Metres::new(400).unwrap();
    out_of_range.occurrences[2].distance = Metres::new(200).unwrap();
    assert_eq!(
        score_candidate(out_of_range, &template, &config()),
        Err(QualityError::DistanceOutOfRange)
    );

    let mut discontinuous = candidate();
    discontinuous.occurrences[1].begin_geometry_index = 0;
    assert_eq!(
        score_candidate(discontinuous, &template, &config()),
        Err(QualityError::InvalidCandidate)
    );
}

#[test]
fn uncalibrated_feature_checks_stay_unknown_and_cannot_receive_the_strong_band() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let mut retraced = candidate();
    retraced.occurrences[1].edge_id = "access".into();
    retraced.occurrences[1].retraced = true;
    retraced.occurrences[2].retraced = true;
    retraced.repeated_distance_metres = 200;

    let scored = score_candidate(retraced, &template, &config()).unwrap();
    assert_eq!(scored.quality_band, QualityBand::ApproximateMatch);
    assert!(
        scored
            .metrics
            .identifying_feature_checks
            .iter()
            .all(|check| check.outcome == FeatureCheckOutcome::Unknown)
    );
}

#[test]
fn ranking_is_deterministic_and_deduplicates_traversal_or_point_density_only_variants() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let original = score_candidate(candidate(), &template, &config()).unwrap();

    let mut density_variant = candidate();
    density_variant.geometry.insert(1, point(5_000, 0));
    density_variant.occurrences[0].end_geometry_index = 2;
    density_variant.occurrences[1].begin_geometry_index = 2;
    density_variant.occurrences[1].end_geometry_index = 3;
    density_variant.occurrences[2].begin_geometry_index = 3;
    density_variant.occurrences[2].end_geometry_index = 4;
    let density_variant = score_candidate(density_variant, &template, &config()).unwrap();

    let mut distinct = candidate();
    distinct.occurrences[1].edge_id = "different-art".into();
    distinct.geometry[2] = point(20_000, 10_000);
    distinct.ordered_design_anchors[1] = distinct.geometry[2];
    let distinct = score_candidate(distinct, &template, &config()).unwrap();

    let ranked = rank_and_deduplicate(vec![distinct.clone(), density_variant, original], 3);
    assert_eq!(ranked.len(), 2);
    assert!(
        ranked
            .iter()
            .any(|item| item.candidate.occurrences[1].edge_id == "art")
    );
    assert!(
        ranked
            .iter()
            .any(|item| item.candidate.occurrences[1].edge_id == "different-art")
    );

    let reversed = rank_and_deduplicate(
        vec![
            distinct,
            score_candidate(candidate(), &template, &config()).unwrap(),
        ],
        3,
    );
    assert_eq!(
        ranked
            .iter()
            .map(|item| &item.candidate.geometry)
            .collect::<Vec<_>>(),
        reversed
            .iter()
            .map(|item| &item.candidate.geometry)
            .collect::<Vec<_>>()
    );
}

#[test]
fn geometric_deduplication_never_erases_a_collinear_reversal() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let mut direct = candidate();
    direct.geometry = vec![point(0, 0), point(5_000, 0), point(10_000, 0), point(0, 0)];
    let direct = score_candidate(direct, &template, &config()).unwrap();

    let mut reversed = candidate();
    reversed.geometry = vec![
        point(0, 0),
        point(5_000, 0),
        point(0, 0),
        point(5_000, 0),
        point(10_000, 0),
        point(0, 0),
    ];
    reversed.occurrences[0].edge_id = "reversing-access".into();
    reversed.occurrences[0].end_geometry_index = 3;
    reversed.occurrences[1].edge_id = "reversing-art".into();
    reversed.occurrences[1].begin_geometry_index = 3;
    reversed.occurrences[1].end_geometry_index = 4;
    reversed.occurrences[2].edge_id = "reversing-return".into();
    reversed.occurrences[2].retraced = false;
    reversed.occurrences[2].begin_geometry_index = 4;
    reversed.occurrences[2].end_geometry_index = 5;
    reversed.repeated_distance_metres = 0;
    let reversed = score_candidate(reversed, &template, &config()).unwrap();

    assert_eq!(rank_and_deduplicate(vec![direct, reversed], 3).len(), 2);
}

#[test]
fn exact_traversal_identity_includes_occurrence_direction() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let forward = score_candidate(candidate(), &template, &config()).unwrap();
    let mut reverse = candidate();
    reverse.geometry = reverse.geometry.into_iter().rev().collect();
    reverse.occurrences.reverse();
    for (index, occurrence) in reverse.occurrences.iter_mut().enumerate() {
        occurrence.direction = TraversalDirection::Reverse;
        occurrence.retraced = index == 2;
        occurrence.begin_geometry_index = index;
        occurrence.end_geometry_index = index + 1;
    }
    reverse.repeated_distance_metres = 100;
    let reverse = score_candidate(reverse, &template, &config()).unwrap();

    assert_eq!(rank_and_deduplicate(vec![forward, reverse], 3).len(), 2);
}

#[test]
fn duplicate_representative_and_zero_limit_are_deterministic() {
    let template = embedded_catalogue()
        .unwrap()
        .get("heart", 1)
        .unwrap()
        .clone();
    let first = score_candidate(candidate(), &template, &config()).unwrap();
    let mut shifted = candidate();
    shifted.geometry[1] = point(7_500, 0);
    let shifted = score_candidate(shifted, &template, &config()).unwrap();

    let left = rank_and_deduplicate(vec![first.clone(), shifted.clone()], 1);
    let right = rank_and_deduplicate(vec![shifted, first], 1);
    assert_eq!(left[0].candidate.geometry, right[0].candidate.geometry);
    assert!(rank_and_deduplicate(left, 0).is_empty());
}
