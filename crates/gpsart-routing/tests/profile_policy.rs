use gpsart_domain::{ActivityProfile, profile_for};
use gpsart_routing::{
    Access, Direction, EdgeMetadata, Eligibility, IneligibleReason, Metadata, Qualification,
    Surface, TraversalDirection, TurnMetadata, evaluate_edge, evaluate_turn,
};

fn ordinary_edge() -> EdgeMetadata {
    EdgeMetadata {
        access: Metadata::Known(Access::Permitted),
        direction: Metadata::Known(Direction::Both),
        surface: Metadata::Known(Surface::Paved),
        elevation_metres: Metadata::Known(12),
        steps: Metadata::Known(false),
        ferry: Metadata::Known(false),
        motorway: Metadata::Known(false),
        motorroad: Metadata::Known(false),
    }
}

#[test]
fn cycling_hard_exclusions_are_table_driven() {
    let cycling = profile_for(ActivityProfile::Cycling);
    let cases = [
        ("steps", IneligibleReason::Steps),
        ("ferry", IneligibleReason::Ferry),
        ("motorway", IneligibleReason::Motorway),
        ("motorroad", IneligibleReason::Motorroad),
        ("forbidden access", IneligibleReason::AccessForbidden),
        ("foot-only access", IneligibleReason::AccessForbidden),
    ];

    for (name, expected) in cases {
        let mut edge = ordinary_edge();
        match name {
            "steps" => edge.steps = Metadata::Known(true),
            "ferry" => edge.ferry = Metadata::Known(true),
            "motorway" => edge.motorway = Metadata::Known(true),
            "motorroad" => edge.motorroad = Metadata::Known(true),
            "forbidden access" => edge.access = Metadata::Known(Access::Forbidden),
            "foot-only access" => edge.access = Metadata::Known(Access::FootOnly),
            _ => unreachable!("controlled fixture name"),
        }
        assert_eq!(
            evaluate_edge(cycling, &edge, TraversalDirection::Forward),
            Eligibility::Ineligible(expected),
            "{name}"
        );
    }
}

#[test]
fn foot_only_edge_differs_between_profiles() {
    let mut edge = ordinary_edge();
    edge.access = Metadata::Known(Access::FootOnly);

    for activity in [ActivityProfile::Walking, ActivityProfile::Running] {
        assert!(
            evaluate_edge(profile_for(activity), &edge, TraversalDirection::Forward).is_eligible()
        );
    }
    assert_eq!(
        evaluate_edge(
            profile_for(ActivityProfile::Cycling),
            &edge,
            TraversalDirection::Forward
        ),
        Eligibility::Ineligible(IneligibleReason::AccessForbidden),
    );
}

#[test]
fn activity_access_rules_are_table_driven() {
    let cases = [
        (ActivityProfile::Walking, Access::Permitted, true),
        (ActivityProfile::Walking, Access::FootOnly, true),
        (ActivityProfile::Walking, Access::BicycleOnly, false),
        (ActivityProfile::Walking, Access::Forbidden, false),
        (ActivityProfile::Running, Access::Permitted, true),
        (ActivityProfile::Running, Access::FootOnly, true),
        (ActivityProfile::Running, Access::BicycleOnly, false),
        (ActivityProfile::Running, Access::Forbidden, false),
        (ActivityProfile::Cycling, Access::Permitted, true),
        (ActivityProfile::Cycling, Access::FootOnly, false),
        (ActivityProfile::Cycling, Access::BicycleOnly, true),
        (ActivityProfile::Cycling, Access::Forbidden, false),
    ];
    for (activity, access, expected) in cases {
        let mut edge = ordinary_edge();
        edge.access = Metadata::Known(access);
        assert_eq!(
            evaluate_edge(profile_for(activity), &edge, TraversalDirection::Forward).is_eligible(),
            expected,
            "{activity:?} with {access:?}",
        );
    }
}

#[test]
fn direction_and_turn_restrictions_are_enforced() {
    let profile = profile_for(ActivityProfile::Walking);
    let direction_cases = [
        (Direction::Both, TraversalDirection::Forward, true),
        (Direction::Both, TraversalDirection::Reverse, true),
        (Direction::ForwardOnly, TraversalDirection::Forward, true),
        (Direction::ForwardOnly, TraversalDirection::Reverse, false),
        (Direction::ReverseOnly, TraversalDirection::Forward, false),
        (Direction::ReverseOnly, TraversalDirection::Reverse, true),
    ];
    for (direction, traversal, expected) in direction_cases {
        let mut edge = ordinary_edge();
        edge.direction = Metadata::Known(direction);
        assert_eq!(
            evaluate_edge(profile, &edge, traversal).is_eligible(),
            expected
        );
    }

    assert!(
        evaluate_turn(
            profile,
            &TurnMetadata {
                permitted: Metadata::Known(true)
            }
        )
        .is_eligible()
    );
    assert_eq!(
        evaluate_turn(
            profile,
            &TurnMetadata {
                permitted: Metadata::Known(false)
            }
        ),
        Eligibility::Ineligible(IneligibleReason::TurnForbidden),
    );
    assert_eq!(
        evaluate_turn(
            profile,
            &TurnMetadata {
                permitted: Metadata::Unknown
            }
        ),
        Eligibility::Ineligible(IneligibleReason::TurnUnknown),
    );
}

#[test]
fn missing_metadata_stays_unknown_and_is_never_invented() {
    let profile = profile_for(ActivityProfile::Cycling);
    let edge = EdgeMetadata {
        access: Metadata::Unknown,
        direction: Metadata::Known(Direction::Both),
        surface: Metadata::Unknown,
        elevation_metres: Metadata::Unknown,
        steps: Metadata::Known(false),
        ferry: Metadata::Known(false),
        motorway: Metadata::Known(false),
        motorroad: Metadata::Known(false),
    };

    assert_eq!(edge.access, Metadata::Unknown);
    assert_eq!(edge.surface, Metadata::Unknown);
    assert_eq!(edge.elevation_metres, Metadata::Unknown);
    assert_eq!(
        evaluate_edge(profile, &edge, TraversalDirection::Forward),
        Eligibility::Eligible {
            qualifications: vec![
                Qualification::AccessUnknown,
                Qualification::SurfaceUnknown,
                Qualification::ElevationUnknown,
            ],
        },
    );
}

#[test]
fn missing_direction_is_not_guessed_as_bidirectional() {
    let mut edge = ordinary_edge();
    edge.direction = Metadata::Unknown;
    assert_eq!(
        evaluate_edge(
            profile_for(ActivityProfile::Walking),
            &edge,
            TraversalDirection::Forward
        ),
        Eligibility::Ineligible(IneligibleReason::DirectionUnknown),
    );
}

#[test]
fn surface_does_not_change_legality() {
    let profile = profile_for(ActivityProfile::Cycling);
    for surface in [Surface::Paved, Surface::Unpaved] {
        let mut edge = ordinary_edge();
        edge.surface = Metadata::Known(surface);
        assert!(evaluate_edge(profile, &edge, TraversalDirection::Forward).is_eligible());
    }
}
