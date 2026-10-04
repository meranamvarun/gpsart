use gpsart_persistence::routes::{
    CandidateProvenance, CandidateQualifications, CandidateSnapshot, DISTANCE_TOLERANCE_METRES,
    DistanceSummary, GeoJsonLineString, InstructionKind, QualityBand, RouteInstruction,
    RouteTraversal, SnapshotError, SnapshotIdentity, SnapshotVersions, TraversalClass,
    TraversalDirection,
};
use uuid::Uuid;

mod support;

fn snapshot() -> CandidateSnapshot {
    CandidateSnapshot {
        id: Uuid::parse_str("40000000-0000-4000-8000-000000000001").unwrap(),
        generation_job_id: Uuid::parse_str("30000000-0000-4000-8000-000000000001").unwrap(),
        candidate_version: 1,
        request_revision: 3,
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
                edge_id: "art-a".into(),
                class: TraversalClass::Artwork,
                direction: TraversalDirection::Forward,
                begin_geometry_index: 0,
                end_geometry_index: 2,
                distance_metres: 600,
                retraced: false,
            },
            RouteTraversal {
                edge_id: "connector-a".into(),
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
            attribution: Some("OpenStreetMap contributors".into()),
        },
    }
}

#[test]
fn validated_snapshot_preserves_ordered_self_intersection_and_derives_views() {
    let candidate = snapshot();
    candidate.validate().unwrap();

    assert_eq!(candidate.start(), [77.6000, 12.9000]);
    assert_eq!(candidate.finish(), [77.6000, 12.9000]);
    assert_eq!(candidate.duration_seconds, 720);
    assert_eq!(candidate.instructions.len(), 3);
    assert_eq!(candidate.direction_markers().len(), 2);
    assert_eq!(
        candidate.direction_markers()[1].coordinate,
        [77.6000, 12.9010]
    );
    assert_eq!(candidate.geometry.coordinates[1], [77.6010, 12.9010]);
    assert_eq!(candidate.geometry.coordinates[3], [77.6010, 12.9000]);
}

#[test]
fn rejects_distance_or_trace_classification_contradictions() {
    let mut inconsistent = snapshot();
    inconsistent.distances.connector_metres += DISTANCE_TOLERANCE_METRES + 1;
    assert_eq!(
        inconsistent.validate(),
        Err(SnapshotError::DistanceMismatch)
    );

    let mut gap = snapshot();
    gap.traversals[1].begin_geometry_index = 1;
    assert_eq!(gap.validate(), Err(SnapshotError::InvalidTraversal));

    let mut added_twice = snapshot();
    added_twice.distances.repeated_included_in_total = false;
    assert_eq!(
        added_twice.validate(),
        Err(SnapshotError::RepeatedDistanceNotIncluded)
    );

    let mut unverified_access = snapshot();
    unverified_access.qualifications.access_verified = false;
    assert_eq!(
        unverified_access.validate(),
        Err(SnapshotError::IncompleteEvidence)
    );

    let mut wrong_request_contract = snapshot();
    wrong_request_contract.versions.request = "invented-request-v9".into();
    assert_eq!(
        wrong_request_contract.validate(),
        Err(SnapshotError::InvalidVersion)
    );

    let mut wrong_algorithm = snapshot();
    wrong_algorithm.versions.algorithm = "invented-algorithm".into();
    assert_eq!(
        wrong_algorithm.validate(),
        Err(SnapshotError::InvalidVersion)
    );
}

#[test]
fn unknown_metadata_is_absent_instead_of_invented() {
    let candidate = snapshot();
    let encoded = serde_json::to_value(candidate).unwrap();
    let first = &encoded["traversals"][0];
    assert!(first.get("surface").is_none());
    assert!(first.get("access").is_none());
    assert!(first.get("elevation_gain_metres").is_none());
    assert!(first.get("provider_metadata").is_none());
    let mut invented_provider_instruction = snapshot();
    invented_provider_instruction.instructions[0].provider_code = Some(9);
    assert_eq!(
        invented_provider_instruction.validate(),
        Err(SnapshotError::InvalidTraversal)
    );

    let mut missing_departure = snapshot();
    missing_departure.instructions[0].kind = InstructionKind::Via;
    assert_eq!(
        missing_departure.validate(),
        Err(SnapshotError::InvalidTraversal)
    );

    let mut zero_duration = snapshot();
    zero_duration.duration_seconds = 0;
    assert_eq!(
        zero_duration.validate(),
        Err(SnapshotError::InvalidTraversal)
    );

    let mut empty_attribution = snapshot();
    empty_attribution.provenance.attribution = Some(String::new());
    assert_eq!(
        empty_attribution.validate(),
        Err(SnapshotError::IncompleteEvidence)
    );
}

#[tokio::test]
async fn durable_snapshot_fields_are_derived_from_the_sealed_scored_candidate() {
    let source = support::scored_candidate("map-v1", 10).await;
    let identity = SnapshotIdentity {
        id: Uuid::parse_str("40000000-0000-4000-8000-000000000002").unwrap(),
        generation_job_id: Uuid::parse_str("30000000-0000-4000-8000-000000000001").unwrap(),
        candidate_version: 1,
        request_revision: 1,
        generated_at_epoch_seconds: 1_800_000_000,
    };
    let snapshot = CandidateSnapshot::from_scored(&source, identity).unwrap();
    snapshot.validate_source(&source).unwrap();
    assert_eq!(snapshot.duration_seconds, 87);
    assert_eq!(
        snapshot.instructions.first().unwrap().kind,
        InstructionKind::Depart
    );
    assert_eq!(
        snapshot.instructions.last().unwrap().kind,
        InstructionKind::Arrive
    );

    let mut invented_instruction = snapshot.clone();
    invented_instruction.instructions[0].kind = InstructionKind::Via;
    assert_eq!(
        invented_instruction.validate_source(&source),
        Err(SnapshotError::InvalidTraversal)
    );

    let mut invented_duration = snapshot.clone();
    invented_duration.duration_seconds += 1;
    assert_eq!(
        invented_duration.validate_source(&source),
        Err(SnapshotError::IncompleteEvidence)
    );
}
