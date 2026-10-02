use gpsart_domain::{
    ActivityProfile, Metres, ProfileVersion, RouteCandidateProfile, RouteRequestProfile,
    profile_for,
};

#[test]
fn every_activity_has_a_distinct_versioned_profile() {
    let walking = profile_for(ActivityProfile::Walking);
    let running = profile_for(ActivityProfile::Running);
    let cycling = profile_for(ActivityProfile::Cycling);

    assert_eq!(walking.version, ProfileVersion::WalkingV1);
    assert_eq!(running.version, ProfileVersion::RunningV1);
    assert_eq!(cycling.version, ProfileVersion::CyclingV1);
    assert_ne!(walking.version, running.version);
    assert_ne!(
        running.default_pace_seconds_per_km,
        walking.default_pace_seconds_per_km
    );
}

#[test]
fn request_and_candidate_record_the_exact_profile_version() {
    let request = RouteRequestProfile::new(ActivityProfile::Running);
    let candidate = RouteCandidateProfile::from_request(&request);

    assert_eq!(request.version(), ProfileVersion::RunningV1);
    assert_eq!(candidate.version(), request.version());
    assert_eq!(candidate.activity(), request.activity());
}

#[test]
fn recorded_profiles_serialize_exactly_and_reject_mismatches() {
    let request = RouteRequestProfile::new(ActivityProfile::Cycling);
    assert_eq!(
        serde_json::to_value(request).expect("serialize profile"),
        serde_json::json!({"activity": "cycling", "profile_version": "cycling-v1"}),
    );
    assert!(
        serde_json::from_value::<RouteRequestProfile>(serde_json::json!({
            "activity": "cycling",
            "profile_version": "walking-v1"
        }))
        .is_err()
    );
}

#[test]
fn pace_changes_only_duration_not_distance_or_profile_identity() {
    let distance = Metres::new(1_500).expect("valid fixture distance");
    let walking = profile_for(ActivityProfile::Walking);
    let running = profile_for(ActivityProfile::Running);

    assert_eq!(walking.canonical_distance(distance), distance);
    assert_eq!(running.canonical_distance(distance), distance);
    assert_eq!(walking.access_policy, running.access_policy);
    assert_ne!(
        walking.estimated_duration(distance),
        running.estimated_duration(distance)
    );
}
