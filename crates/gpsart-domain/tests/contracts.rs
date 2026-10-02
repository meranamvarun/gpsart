use gpsart_domain::{ActivityProfile, Metres, PlanningRequestId};

#[test]
fn canonical_distance_is_metres_and_ids_are_typed() {
    assert_eq!(Metres::new(1_500).expect("valid metres").value(), 1_500);
    assert!(Metres::new(0).is_err());
    let id = PlanningRequestId::new();
    assert_eq!(id.to_string().len(), 36);
}

#[test]
fn activity_profiles_are_explicit_domain_values() {
    assert_eq!(ActivityProfile::Walking.as_str(), "walking");
}
